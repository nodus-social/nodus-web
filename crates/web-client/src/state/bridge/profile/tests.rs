//! The save against a fake instance: the body the SDK puts on the wire, and the 422 path.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::rc::Rc;
use std::task::Poll;

use nodus_client_core::Instance;
use nodus_client_core::rest::{HttpRequest, HttpResponse, Transport};
use nodus_mastodon::rest::RestInstance;
use nodus_pages::pages::settings::form::ProfileField;
use serde_json::{Value, json};

use super::{SaveError, write};

const ACCOUNT_JSON: &str = r#"{"id":"1","acct":"alice","display_name":"Alice"}"#;

/// The credential read's answer: the rows as the instance stored and verified them.
const ROWS_JSON: &str = r#"{
    "id": "1",
    "acct": "alice",
    "display_name": "Alice",
    "fields": [
        {"name": "Blog", "value": "https://example.test", "verified_at": "2026-01-01T00:00:00Z"},
        {"name": "Pronouns", "value": "she/her", "verified_at": null}
    ]
}"#;

/// One request the save made, kept for the assertions.
#[derive(Debug, Clone)]
struct Sent {
    method: String,
    url: String,
    content_type: Option<String>,
    body: Value,
}

/// An instance that answers from a queue and records what it was asked.
#[derive(Clone, Default)]
struct FakeTransport {
    sent: Rc<RefCell<Vec<Sent>>>,
    answers: Rc<RefCell<VecDeque<HttpResponse>>>,
}

impl FakeTransport {
    fn with(answers: Vec<HttpResponse>) -> Self {
        Self {
            sent: Rc::new(RefCell::new(Vec::new())),
            answers: Rc::new(RefCell::new(answers.into())),
        }
    }
}

impl Transport for FakeTransport {
    fn send(
        &self,
        request: HttpRequest,
    ) -> impl Future<Output = nodus_client_core::Result<HttpResponse>> {
        let content_type = request
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.clone());
        let body = request
            .body
            .as_deref()
            .and_then(|bytes| serde_json::from_slice(bytes).ok())
            .unwrap_or(Value::Null);
        self.sent.borrow_mut().push(Sent {
            method: format!("{:?}", request.method),
            url: request.url.to_string(),
            content_type,
            body,
        });
        let answer = self
            .answers
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| ok_json(ACCOUNT_JSON));
        core::future::ready(Ok(answer))
    }
}

fn ok_json(body: &str) -> HttpResponse {
    HttpResponse {
        status: 200,
        retry_after_s: None,
        body: body.as_bytes().to_vec(),
    }
}

fn answer_response(status: u16, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        retry_after_s: None,
        body: body.as_bytes().to_vec(),
    }
}

/// Runs a future the fake transport never pends on to completion.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = core::pin::pin!(future);
    let waker = core::task::Waker::noop();
    let mut context = core::task::Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("the fake transport never pends"),
    }
}

fn field(name: &str, value: &str) -> ProfileField {
    ProfileField {
        name: name.to_owned(),
        value: value.to_owned(),
        verified_at: None,
    }
}

fn instance(answers: Vec<HttpResponse>) -> (RestInstance<FakeTransport>, FakeTransport) {
    let transport = FakeTransport::with(answers);
    let base = Instance::parse("https://stub.instance").expect("a valid instance");
    (
        RestInstance::with_transport(&base, "token", transport.clone()),
        transport,
    )
}

#[test]
fn a_two_row_save_sends_the_rows_through_the_sdk() {
    let (instance, transport) = instance(vec![ok_json(ACCOUNT_JSON), ok_json(ROWS_JSON)]);
    let fields = vec![
        field(" Blog ", " https://example.test "),
        field("Pronouns", "she/her"),
    ];

    let stored = block_on(write(
        &instance,
        "Alice".to_owned(),
        "Hello".to_owned(),
        fields,
    ))
    .expect("the fake instance answers");

    let sent = transport.sent.borrow();
    assert_eq!(sent.len(), 2, "the save and the read that returns its rows");
    let request = &sent[0];
    assert_eq!(request.method, "Patch");
    assert!(
        request.url.ends_with("/api/v1/accounts/update_credentials"),
        "{}",
        request.url
    );
    assert_eq!(request.content_type.as_deref(), Some("application/json"));
    let rows = json!([
        { "name": "Blog", "value": "https://example.test" },
        { "name": "Pronouns", "value": "she/her" },
    ]);
    assert_eq!(request.body["display_name"], "Alice");
    assert_eq!(request.body["note"], "Hello");
    assert_eq!(request.body["fields"], rows);
    assert_eq!(request.body["fields_attributes"], rows);
    assert_eq!(sent[1].method, "Get");
    assert!(
        sent[1].url.ends_with("/api/v1/accounts/verify_credentials"),
        "{}",
        sent[1].url
    );
    // The rows the editor paints come from the instance's answer, not from the input: a
    // verified link keeps its stamp and the sent whitespace is not shown back.
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].name, "Blog");
    assert_eq!(
        stored[0].verified_at.as_deref(),
        Some("2026-01-01T00:00:00Z")
    );
    assert_eq!(stored[1].verified_at, None);
}

#[test]
fn removing_every_row_still_sends_an_empty_list() {
    let (instance, transport) = instance(vec![
        ok_json(ACCOUNT_JSON),
        ok_json(r#"{"id":"1","acct":"alice","fields":[]}"#),
    ]);

    block_on(write(
        &instance,
        "Alice".to_owned(),
        String::new(),
        Vec::new(),
    ))
    .expect("the fake instance answers");

    let sent = transport.sent.borrow();
    assert_eq!(sent[0].body["fields"], json!([]));
    assert_eq!(sent[0].body["fields_attributes"], json!([]));
}

#[test]
fn a_422_that_names_a_row_is_placed_beside_that_row() {
    let answer = r#"{"error":"profile field 2 needs a name"}"#;
    let (instance, _transport) = instance(vec![answer_response(422, answer)]);

    let error = block_on(write(
        &instance,
        "Alice".to_owned(),
        String::new(),
        vec![field("Blog", ""), field("", "value")],
    ))
    .expect_err("a 422 is a failed save");

    match error {
        SaveError::Row { row, message } => {
            assert_eq!(row, 1, "the server counts rows from one");
            assert!(message.contains("needs a name"), "{message}");
        }
        other => panic!("a named row must be a row error, got {other:?}"),
    }
}

#[test]
fn a_422_that_names_no_row_stays_at_form_level() {
    let body = r#"{"error":"a profile has at most 6 fields; 7 were sent"}"#;
    let (instance, _transport) = instance(vec![answer_response(422, body)]);

    let error = block_on(write(
        &instance,
        "Alice".to_owned(),
        String::new(),
        Vec::new(),
    ))
    .expect_err("a 422 is a failed save");

    assert!(matches!(error, SaveError::Form(_)), "{error:?}");
}
