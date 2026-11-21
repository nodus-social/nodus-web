//! Which API dialect the account's instance speaks.
//!
//! The console's screens are one vocabulary over servers that do not share an API: the
//! star, the boost and the bookmark are Mastodon verbs, while a Misskey instance keeps a
//! private favourite beside a public reaction tally. Which of them a screen may offer is a
//! question about the *server*, so it is asked once when the session is installed and
//! recorded here, where every view can read it without waiting on a request.
//!
//! The probe itself is the SDK's ([`nodus_client_sdk::protocol::Probe`]): three documents a
//! server already publishes, compared in a fixed order of precedence. This module owns only
//! the asking, the one-answer-per-host record, and the signal a view depends on.

use std::cell::RefCell;

use leptos::prelude::*;
use nodus_client_core::Instance;
use nodus_client_sdk::protocol::{Probe, Protocol};

/// The dialect assumed when the instance published nothing the SDK recognises.
///
/// Mastodon is the SDK's own compatibility floor and the console's screens were built
/// against it, so an unknown server keeps working instead of blanking the app on a document
/// it did not think to publish.
const FLOOR: Protocol = Protocol::Mastodon;

thread_local! {
    /// The session's server, as the host the answer belongs to and the instance it was
    /// parsed from.
    static CURRENT: RefCell<Option<(String, Instance)>> = const { RefCell::new(None) };

    /// The protocol for one host.
    ///
    /// Keyed by host because the answer belongs to the server, not to the tab: switching
    /// accounts can switch instances, and a stale answer would offer Misskey verbs to a
    /// Mastodon server.
    static ANSWER: RefCell<Option<(String, Protocol)>> = const { RefCell::new(None) };

    /// The probe in flight, as the browser's own promise.
    ///
    /// Held rather than a flag because a read that needs the answer has to *wait* for it:
    /// the thread screen asks for a note in the same turn the session is installed, and a
    /// read sent before the probe landed would go to the wrong API. One promise can be
    /// awaited by every waiter while the instance is asked exactly once.
    #[cfg(target_arch = "wasm32")]
    static PROBING: RefCell<Option<(String, js_sys::Promise)>> = const { RefCell::new(None) };
}

fn revision() -> RwSignal<u64> {
    thread_local! {
        /// A root of its own, deliberately detached from the component tree: the counter
        /// outlives whichever card happens to paint first. Held in an arena owned by the
        /// card that created it, the counter would be disposed with that card, and the
        /// next card to ask which protocol it is reading would panic.
        static ROOT: Owner = Owner::new_root(None);
        static REVISION: RwSignal<u64> = ROOT.with(|root| root.with(|| RwSignal::new(0)));
    }
    REVISION.with(|revision| *revision)
}

/// The protocol the session is talking to, as a signal a view can depend on.
///
/// [`FLOOR`] until the probe answers, which on a first visit is a frame or two.
#[must_use]
pub fn protocol() -> Signal<Protocol> {
    let revision = revision();
    Signal::derive(move || {
        revision.get();
        answer().unwrap_or(FLOOR)
    })
}

/// The session's protocol, awaited.
///
/// A read that picks a transport has to know the dialect before it sends, so the thread
/// screen awaits this first and then uses the instance's answer rather than the floor.
pub async fn ensure() -> Protocol {
    if let Some(known) = answer() {
        return known;
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(known) = wait().await {
        return known;
    }
    FLOOR
}

/// Takes `instance` as the session's server.
///
/// The per-host records are dropped when the host changes: an answer for the previous
/// account's server would otherwise gate the new account's controls.
pub(super) fn watch(instance: &Instance) {
    let host = host_of(instance);
    let moved = CURRENT.with_borrow_mut(|slot| {
        let moved = slot
            .as_ref()
            .is_none_or(|(known, _)| known.as_str() != host.as_str());
        *slot = Some((host.clone(), instance.clone()));
        moved
    });
    if !moved {
        return;
    }
    ANSWER.with_borrow_mut(|slot| *slot = None);
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ask(instance);
    }
}

/// Forgets the answer, for a session that ended.
pub(super) fn forget() {
    CURRENT.with_borrow_mut(|slot| *slot = None);
    ANSWER.with_borrow_mut(|slot| *slot = None);
    #[cfg(target_arch = "wasm32")]
    PROBING.with_borrow_mut(|slot| *slot = None);
    revision().update(|generation| *generation = generation.wrapping_add(1));
}

/// The answer on record, when it belongs to the session's own host.
fn answer() -> Option<Protocol> {
    let current = CURRENT.with_borrow(|slot| slot.as_ref().map(|(host, _)| host.clone()))?;
    ANSWER.with_borrow(|held| {
        held.as_ref()
            .and_then(|(host, protocol)| (host == &current).then_some(*protocol))
    })
}

fn host_of(instance: &Instance) -> String {
    instance.base.host_str().unwrap_or_default().to_owned()
}

/// Records what the probe found, unless the session moved on while it was asking.
///
/// A late answer for a host this tab no longer uses is dropped: it would gate the current
/// account's controls by the previous account's server.
fn settle(host: &str, found: Option<Protocol>) {
    let current = CURRENT.with_borrow(|slot| slot.as_ref().map(|(known, _)| known.clone()));
    if current.as_deref() != Some(host) {
        return;
    }
    ANSWER.with_borrow_mut(|slot| *slot = Some((host.to_owned(), found.unwrap_or(FLOOR))));
    revision().update(|generation| *generation = generation.wrapping_add(1));
}

/// The one probe for `instance`, as a promise every waiter can await.
#[cfg(target_arch = "wasm32")]
fn ask(instance: &Instance) -> js_sys::Promise {
    let host = host_of(instance);
    PROBING.with_borrow_mut(|slot| {
        let held = slot
            .as_ref()
            .and_then(|(known, promise)| (known == &host).then(|| promise.clone()));
        if let Some(promise) = held {
            return promise;
        }
        let base = instance.base.as_str().trim_end_matches('/').to_owned();
        let settling = host.clone();
        let promise = wasm_bindgen_futures::future_to_promise(async move {
            let found = ask_which(&base).await;
            settle(&settling, found);
            Ok(wasm_bindgen::JsValue::UNDEFINED)
        });
        *slot = Some((host, promise.clone()));
        promise
    })
}

/// Waits for the probe the session started, then reports what it settled on.
#[cfg(target_arch = "wasm32")]
async fn wait() -> Option<Protocol> {
    let promise = CURRENT.with_borrow(|slot| slot.as_ref().map(|(_, instance)| ask(instance)))?;
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    answer()
}

/// Which protocol `base` speaks, asked without requesting a document it may not have.
///
/// The capability document under `/api/v1/nodus/` is one a Misskey or a Mastodon server
/// does not serve at all, and a session should not ask a server for a document it knows it
/// does not have. The Nodus surface is recognised by the meta document instead — the same
/// one the extension question reads, and the same rule about what counts as an answer — and
/// what remains, Misskey or Mastodon, is handed to the SDK's own [`Probe`], so which
/// protocol wins stays the SDK's decision rather than this tab's.
#[cfg(target_arch = "wasm32")]
async fn ask_which(base: &str) -> Option<Protocol> {
    let meta = document(&format!("{base}/api/v1/meta")).await;
    if meta.as_deref().and_then(super::features_of).is_some() {
        return Some(Protocol::Nodus);
    }
    let misskey = json(&format!("{base}/api/meta")).await;
    let mastodon = json(&format!("{base}/api/v2/instance")).await;
    Probe {
        nodus: None,
        mastodon: mastodon.as_ref(),
        misskey: misskey.as_ref(),
    }
    .detect()
}

/// One document from `url`, or `None` when the server does not answer with a body.
///
/// A failed request is absence rather than failure, for the same reason the SDK's own probe
/// treats it that way: not having a document is how a server says what it is not.
#[cfg(target_arch = "wasm32")]
async fn document(url: &str) -> Option<Vec<u8>> {
    let response = gloo_net::http::Request::get(url).send().await.ok()?;
    if !response.ok() {
        return None;
    }
    response.binary().await.ok()
}

/// The same document, when it parses as JSON.
#[cfg(target_arch = "wasm32")]
async fn json(url: &str) -> Option<serde_json::Value> {
    let body = document(url).await?;
    serde_json::from_slice(&body).ok()
}
