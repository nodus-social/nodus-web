//! The bridge against a fake instance, in the style the SDK's own tests use.
//!
//! The console's transport is `gloo-net`, so these run where the crate is built: the target
//! is wasm and the transport here answers from a queue instead of the network. What they pin
//! down is the part this crate adds — which verbs Misskey does not have, which wire shape
//! each one posts, and how the two "I like this" channels map onto the shared star.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::rc::Rc;
use std::task::Poll;

use nodus_client_core::actions::StatusActions;
use nodus_client_core::rest::{HttpRequest, HttpResponse, Transport};
use nodus_client_core::{Error, Instance, InstanceApi};
use serde_json::{Value, json};

use super::MisskeyBridge;

/// One request the bridge made, kept for the assertions.
#[derive(Debug, Clone)]
struct Sent {
    url: String,
    body: Value,
}

/// An instance that answers from a queue and records what it was asked.
#[derive(Clone, Default)]
struct FakeTransport {
    sent: Rc<RefCell<Vec<Sent>>>,
    answers: Rc<RefCell<VecDeque<HttpResponse>>>,
}

impl FakeTransport {
    fn with(answers: Vec<Value>) -> Self {
        Self {
            sent: Rc::new(RefCell::new(Vec::new())),
            answers: Rc::new(RefCell::new(answers.iter().map(ok_json).collect())),
        }
    }

    fn sent(&self) -> Vec<Sent> {
        self.sent.borrow().clone()
    }
}

impl Transport for FakeTransport {
    fn send(
        &self,
        request: HttpRequest,
    ) -> impl Future<Output = nodus_client_core::Result<HttpResponse>> {
        let body = request
            .body
            .as_deref()
            .and_then(|bytes| serde_json::from_slice(bytes).ok())
            .unwrap_or(Value::Null);
        self.sent.borrow_mut().push(Sent {
            url: request.url.to_string(),
            body,
        });
        let answer = self
            .answers
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| ok_json(&Value::Array(Vec::new())));
        core::future::ready(Ok(answer))
    }
}

fn ok_json(body: &Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        retry_after_s: None,
        body: serde_json::to_vec(body).unwrap_or_default(),
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

fn bridge(answers: Vec<Value>) -> (MisskeyBridge<FakeTransport>, FakeTransport) {
    let transport = FakeTransport::with(answers);
    let instance = Instance::parse("https://stub.instance").expect("a valid instance");
    (
        MisskeyBridge::with_transport(&instance, "token", transport.clone()),
        transport,
    )
}

/// A note with a private favourite, a public reaction and two keys in the tally.
fn note(is_favorited: bool) -> Value {
    json!({
        "id": "n1",
        "createdAt": "2026-01-01T00:00:00Z",
        "text": "hello",
        "user": { "id": "u1", "username": "alice" },
        "reactions": { "❤️": 2, ":blob:": 1 },
        "myReaction": "❤️",
        "isFavorited": is_favorited
    })
}

#[test]
fn the_private_favourite_becomes_the_star_and_the_reaction_stays_out_of_it() {
    let (bridge, _transport) = bridge(vec![note(true)]);
    let status = block_on(bridge.status("n1")).expect("the fake instance answers");

    // Misskey's star verb is the favourite, so the star is lit by `isFavorited`; the
    // reaction reached `favourited` in the SDK's conversion and must not light it twice.
    assert!(status.favourited, "{status:?}");
    assert!(!status.bookmarked, "Misskey has no bookmark verb to draw");
}

#[test]
fn a_note_read_reports_the_tally_and_whether_the_account_already_reacted() {
    let (bridge, transport) = bridge(vec![note(false), note(false)]);
    let read = block_on(bridge.note("n1")).expect("the fake instance answers");

    assert!(read.reacted, "myReaction names a reaction of this account");
    assert_eq!(read.reactions.count("❤️"), 2);
    assert_eq!(read.reactions.count(":blob:"), 1);
    assert_eq!(
        transport.sent().len(),
        2,
        "the note and its tally are two reads of a note addressed by id"
    );
}

#[test]
fn favouriting_runs_the_favourite_verb_and_answers_with_the_star_set() {
    let (bridge, transport) = bridge(vec![json!({ "isFavorited": true }), note(true)]);
    let status = block_on(StatusActions::favourite(&bridge, "n1")).expect("the verb is served");

    let sent = transport.sent();
    assert!(
        sent[0].url.ends_with("/api/notes/favorites/create"),
        "{:?}",
        sent[0].url
    );
    assert_eq!(sent[0].body["noteId"], "n1");
    assert!(status.favourited, "{status:?}");
    assert!(!status.bookmarked, "{status:?}");
}

#[test]
fn a_verb_misskey_has_no_wire_for_answers_unsupported_and_posts_nothing() {
    let (bridge, transport) = bridge(Vec::new());
    for outcome in [
        block_on(StatusActions::reblog(&bridge, "n1")).map(drop),
        block_on(StatusActions::bookmark(&bridge, "n1")).map(drop),
        block_on(StatusActions::pin(&bridge, "n1")).map(drop),
    ] {
        match outcome {
            Err(Error::Unsupported(_)) => {}
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }
    assert!(transport.sent().is_empty(), "no verb is posted blind");
}

#[test]
fn removing_a_reaction_names_the_note_and_not_the_key() {
    let (bridge, transport) = bridge(vec![json!({}), note(false)]);
    let tally = block_on(bridge.unreact("n1")).expect("the verb is served");

    let sent = transport.sent();
    assert!(
        sent[0].url.ends_with("/api/notes/reactions/delete"),
        "{:?}",
        sent[0].url
    );
    assert_eq!(sent[0].body, json!({ "noteId": "n1" }));
    assert_eq!(tally.total(), 3, "the tally is read back, not guessed");
}
