//! The notification policy: what this instance should do with each kind of notification.
//!
//! The editor is one read and one write, both against `/api/v2/notifications/policy`. The
//! read is also the capability check: an instance that does not publish the surface answers
//! 404, which is a "not advertised" answer rather than a failure, and the caller keeps the
//! explanation it already had. A refused write answers with the instance's own sentence
//! (`422 unknown policy for <kind>: <value>; …`), and that sentence is what the row shows.

use std::collections::BTreeMap;

use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;

use nodus_pages::pages::settings::notifications::{PolicyKind, PolicyRow, PolicyValue};

use super::session_token;

const PATH: &str = "/api/v2/notifications/policy";

/// Reads all eight kinds, in the order the page draws them.
///
/// Missing kinds default to `accept`, which is the endpoint's own default. A value this
/// client does not know is treated as `accept` too: the row still renders, and the next
/// change overwrites it.
pub async fn load() -> Result<Option<Vec<PolicyRow>>, String> {
    let Some(token) = session_token() else {
        return Err("Sign in to read notification settings.".to_owned());
    };
    let request = gloo_net::http::Request::get(PATH)
        .header("authorization", &format!("Bearer {token}"))
        .build()
        .map_err(|error| error.to_string())?;
    let response = request.send().await.map_err(|error| error.to_string())?;
    if response.status() == 404 {
        return Ok(None);
    }
    if !response.ok() {
        return Err(format!(
            "The instance answered {} while reading the notification policy.",
            response.status()
        ));
    }
    let policy = response
        .json::<BTreeMap<String, String>>()
        .await
        .map_err(|error| {
            format!("The instance sent a notification policy this client cannot read: {error}")
        })?;
    let rows = PolicyKind::ALL
        .into_iter()
        .map(|kind| PolicyRow {
            kind,
            value: policy
                .get(kind.wire())
                .and_then(|word| PolicyValue::from_wire(word))
                .unwrap_or(PolicyValue::Accept),
            saving: false,
            error: None,
        })
        .collect();
    Ok(Some(rows))
}

/// Writes one kind and answers once: `Ok` when the instance took it, `Err` with the
/// instance's sentence when it refused.
pub fn save(kind: PolicyKind, value: PolicyValue, done: Callback<Result<(), String>>) {
    let Some(token) = session_token() else {
        done.run(Err("Sign in to change notification settings.".to_owned()));
        return;
    };
    let Ok(request) = gloo_net::http::Request::put(PATH)
        .header("authorization", &format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(PolicyRow::change_body(kind, value))
    else {
        done.run(Err("The change could not be sent.".to_owned()));
        return;
    };
    spawn_local(async move {
        match request.send().await {
            Ok(response) if response.ok() => done.run(Ok(())),
            Ok(response) => done.run(Err(refusal(response).await)),
            Err(error) => done.run(Err(error.to_string())),
        }
    });
}

/// The instance's own sentence when it sends one, and a plain one when it does not.
#[derive(Deserialize)]
struct Refusal {
    error: String,
}

async fn refusal(response: gloo_net::http::Response) -> String {
    let status = response.status();
    match response.json::<Refusal>().await {
        Ok(refusal) => refusal.error,
        Err(_) => format!("The instance refused the change ({status})."),
    }
}
