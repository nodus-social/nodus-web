//! The account's live stream: what the instance pushes, applied to the stores the screens read.
//!
//! `GET /api/v1/streaming?stream=user` is server-sent events, so the browser's own parser is
//! the right client: an [`EventSource`] reads the frames, dispatches the `event:` names and
//! keeps the connection. The page used to learn about a new notification only when it next
//! fetched the list, which is why the rail's badge was always one action behind.
//!
//! Three events matter here, and each one takes the path the rest of the app already uses
//! for the same fact — the stream must not become a second, quieter store:
//!
//! * `notification` → [`notifications::push`], deduped by id;
//! * `update` → [`timeline::upsert`], the status-upsert path a publish or an edit uses;
//! * `delete` → `Command::StatusRemoved`, which drops the id from the entity store and from
//!   every page holding it, plus [`notifications::forget_status`] for the inline copy.
//!
//! # Why the token is in the query string
//!
//! `EventSource` cannot set request headers: the constructor takes a URL and an options bag
//! whose only switch is credentials, and there is no `Request` to hand it. A bearer token
//! therefore has nowhere to go but the URL, which is exactly why the endpoint accepts
//! `?access_token=…` as well as the header. The page is same-origin with the API in this
//! deployment (Trunk proxies `/api` in development, Caddy serves it in production), so the
//! token is not sent to a third party, and the server answers `cache-control: no-cache` so
//! no intermediary keeps the URL as a cache key.
//!
//! # Why only our own instance
//!
//! A Mastodon server pushes over a WebSocket on the same path. Pointing an `EventSource` at it
//! would open a connection nobody upgrades, and the page would sit in silence while pretending
//! to be live — the precise lie this codebase keeps removing. So the caller gates on
//! `supports_extensions()`, and a Mastodon account keeps polling, which works.

use std::cell::{Cell, RefCell};

use gloo_timers::future::TimeoutFuture;
use nodus_client_core::model::Status;
use nodus_client_sdk::Command;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::spawn_local;

use super::notifications::{self, Notice};
use super::{active_cache_key, dispatch, session_token, timeline};

/// The endpoint, relative to this origin: the same base every other call in the bridge uses.
const PATH: &str = "/api/v1/streaming";
/// The first retry waits about a second, the last about half a minute.
const MIN_RETRY_MS: u32 = 1_000;
const MAX_RETRY_MS: u32 = 30_000;

thread_local! {
    /// The account that owns a stream, whether the connection is open or waiting to retry.
    ///
    /// One slot, one account: a second mount, a navigation and a repaint all find the same
    /// entry and reuse it. Replacing the entry is the only way a stream is opened, so two
    /// streams for one account cannot exist.
    static STREAM: RefCell<Option<Stream>> = const { RefCell::new(None) };
    /// Identifies one stream against the retry of a previous one. A retry that wakes up
    /// after `stop` or an account switch sees a different number and does nothing.
    static NEXT: Cell<u64> = const { Cell::new(0) };
    /// Consecutive failures, reset by a successful open.
    static ATTEMPT: Cell<u32> = const { Cell::new(0) };
}

/// One named-event listener, with the name needed to detach it again.
type Listener = (&'static str, Closure<dyn FnMut(web_sys::Event)>);

struct Stream {
    /// The `(host, account id)` this stream belongs to.
    key: String,
    /// The open connection, when there is one; `None` while waiting to retry.
    source: Option<web_sys::EventSource>,
    listeners: Vec<Listener>,
    opened: Option<Closure<dyn FnMut()>>,
    failed: Option<Closure<dyn FnMut(web_sys::Event)>>,
    /// The identity a pending retry checks before it connects again.
    generation: u64,
}

impl Stream {
    fn waiting(key: &str, generation: u64) -> Self {
        Self {
            key: key.to_owned(),
            source: None,
            listeners: Vec::new(),
            opened: None,
            failed: None,
            generation,
        }
    }

    /// Detaches every handler and closes the connection.
    ///
    /// Closing matters even while nothing is displayed: an `EventSource` whose connection
    /// drops retries by itself, on a delay of its own and with no way to tell the page
    /// whether it is still trying. Detaching the listeners first keeps a frame that was
    /// already in flight from reaching a store this stream no longer belongs to.
    fn close(&mut self) {
        if let Some(source) = self.source.take() {
            source.set_onopen(None);
            source.set_onerror(None);
            for (event, callback) in &self.listeners {
                let _ = source
                    .remove_event_listener_with_callback(event, callback.as_ref().unchecked_ref());
            }
            source.close();
        }
        self.listeners.clear();
        self.opened = None;
        self.failed = None;
    }
}

/// Opens the account's stream, or keeps the one already open.
///
/// Idempotent for the same account and idempotent for a retry already in flight: the
/// caller may run this on every navigation and every change of the extension answer.
pub fn start() {
    let Some(key) = stream_key() else {
        stop();
        return;
    };
    // Reuse, not reopen: this is the one-stream-per-account guarantee, and it also means a
    // navigation during a backoff wait does not restart the wait at one second.
    if STREAM.with_borrow(|stream| stream.as_ref().is_some_and(|stream| stream.key == key)) {
        return;
    }
    let Some(token) = session_token() else {
        stop();
        return;
    };
    let generation = NEXT.get().wrapping_add(1);
    NEXT.set(generation);
    ATTEMPT.set(0);
    STREAM.with_borrow_mut(|slot| {
        if let Some(mut previous) = slot.replace(Stream::waiting(&key, generation)) {
            // Another account was on screen: its stream goes with it.
            previous.close();
        }
    });
    connect(&key, &token, generation);
}

/// Closes the stream and cancels any retry. Safe to call when nothing is open.
pub fn stop() {
    STREAM.with_borrow_mut(|slot| {
        if let Some(mut stream) = slot.take() {
            stream.close();
        }
    });
}

/// Builds the connection and attaches the handlers for one generation.
fn connect(key: &str, token: &str, generation: u64) {
    // The token has to be in the URL: see the module note. Percent-encoded because a token
    // is opaque and one `+` or `&` would otherwise turn the rest of it into query keys.
    let token = js_sys::encode_uri_component(token);
    let url = format!("{PATH}?stream=user&access_token={token}");
    let Ok(source) = web_sys::EventSource::new(&url) else {
        retry(generation);
        return;
    };

    let mut stream = Stream::waiting(key, generation);

    stream.listeners.push((
        "notification",
        listen(&source, "notification", move |event| {
            if current(generation) {
                on_notification(&event);
            }
        }),
    ));
    stream.listeners.push((
        "update",
        listen(&source, "update", move |event| {
            if current(generation) {
                on_update(&event);
            }
        }),
    ));
    stream.listeners.push((
        "delete",
        listen(&source, "delete", move |event| {
            if current(generation) {
                on_delete(&event);
            }
        }),
    ));

    let opened = Closure::<dyn FnMut()>::new(move || {
        if current(generation) {
            ATTEMPT.set(0);
        }
    });
    source.set_onopen(Some(opened.as_ref().unchecked_ref()));
    stream.opened = Some(opened);

    let failed = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| retry(generation));
    source.set_onerror(Some(failed.as_ref().unchecked_ref()));
    stream.failed = Some(failed);

    stream.source = Some(source);
    install(stream, generation);
}

/// Puts a freshly built connection in the slot, when it is still the current generation.
fn install(stream: Stream, generation: u64) {
    let wanted = STREAM.with_borrow(|slot| {
        slot.as_ref()
            .is_some_and(|held| held.generation == generation && held.key == stream.key)
    });
    if !wanted {
        // Stopped or switched while the connection was being built.
        let mut stream = stream;
        stream.close();
        return;
    }
    STREAM.with_borrow_mut(|slot| {
        if let Some(slot) = slot.as_mut() {
            if slot.generation == generation {
                slot.close();
                *slot = stream;
            }
        }
    });
}

/// Waits, then connects again.
///
/// The wait is only bounded when the instance is reachable: a server that is down is asked
/// again later, a token the instance refuses is not asked at all.
fn retry(generation: u64) {
    if !current(generation) {
        return;
    }
    // The browser would retry on its own; taking that decision back is what makes the
    // backoff ours, and what gives `stop` something to cancel.
    STREAM.with_borrow_mut(|slot| {
        if let Some(stream) = slot.as_mut() {
            if stream.generation == generation {
                stream.close();
            }
        }
    });

    spawn_local(async move {
        let allowed = still_allowed().await;
        if !current(generation) {
            return;
        }
        if allowed == Some(false) {
            // `401` on the probe means the session is gone, not that the network blinked.
            // Retrying would hammer an instance that has already answered, and the next
            // sign-in opens its own stream anyway.
            stop();
            return;
        }
        let delay = backoff(ATTEMPT.get());
        ATTEMPT.set(ATTEMPT.get().saturating_add(1));
        TimeoutFuture::new(delay).await;
        if !current(generation) {
            return;
        }
        let (Some(token), Some(key)) = (session_token(), stream_key()) else {
            stop();
            return;
        };
        if block(|stream| stream.key.clone()).as_deref() != Some(key.as_str()) {
            stop();
            return;
        }
        connect(&key, &token, generation);
    });
}

/// Whether the session is still accepted, as far as one cheap request can tell.
///
/// `None` is "could not ask" — a network failure is not a refusal, so the caller retries.
/// `EventSource` reports a failed connection as one error event with no status, which is
/// exactly why the answer has to be asked for rather than read off the failure.
async fn still_allowed() -> Option<bool> {
    let token = session_token()?;
    let request = gloo_net::http::Request::get("/api/v1/accounts/verify_credentials")
        .header("authorization", &format!("Bearer {token}"))
        .build()
        .ok()?;
    let response = request.send().await.ok()?;
    Some(response.status() != 401)
}

/// 1 s, 2 s, 4 s … capped at 30 s, jittered inside the top quarter of the window.
///
/// The jitter is what stops a hundred tabs that all lost the same server from coming back in
/// the same millisecond, which is a second outage made of retries.
fn backoff(attempt: u32) -> u32 {
    let window = MIN_RETRY_MS
        .saturating_mul(1_u32 << attempt.min(5))
        .min(MAX_RETRY_MS);
    let jitter = 0.75 + 0.25 * js_sys::Math::random();
    (f64::from(window) * jitter).round() as u32
}

/// The `(host, account id)` the stream belongs to, when signed in.
fn stream_key() -> Option<String> {
    active_cache_key().map(|(host, account)| format!("{host}|{account}"))
}

/// Whether the stream in the slot is still the one this generation describes.
fn current(generation: u64) -> bool {
    STREAM.with_borrow(|slot| {
        slot.as_ref()
            .is_some_and(|held| held.generation == generation)
    })
}

/// Reads one field of the current stream, when there is one.
fn block<R>(f: impl FnOnce(&Stream) -> R) -> Option<R> {
    STREAM.with_borrow(|slot| slot.as_ref().map(f))
}

/// Attaches one named-event handler and hands back the closure that keeps it alive.
fn listen(
    source: &web_sys::EventSource,
    event: &'static str,
    handle: impl FnMut(web_sys::Event) + 'static,
) -> Closure<dyn FnMut(web_sys::Event)> {
    let callback = Closure::<dyn FnMut(web_sys::Event)>::new(handle);
    let _ = source.add_event_listener_with_callback(event, callback.as_ref().unchecked_ref());
    callback
}

/// The `data:` text of one frame.
fn payload(event: &web_sys::Event) -> String {
    event
        .dyn_ref::<web_sys::MessageEvent>()
        .map(web_sys::MessageEvent::data)
        .and_then(|data| data.as_string())
        .unwrap_or_default()
}

/// A notification the instance pushed, folded into the store the rail and the page read.
fn on_notification(event: &web_sys::Event) {
    match serde_json::from_str::<Notice>(&payload(event)) {
        Ok(notice) => notifications::push(notice),
        Err(error) => complain("a notification this client cannot read", &error),
    }
}

/// A status that changed: an edit, a new favourite count, a new post.
///
/// The same call the publish and edit paths make, so a status that arrives on the wire lands
/// in the entity store once. A brand-new post from someone you follow does not appear in the
/// home list until the list is fetched again: a list is a page of ids the server ordered, and
/// inserting at the top here would put a post above a page the reader has not reached yet.
///
/// A notification that inlines this status is refreshed too, so a favourite on a post that
/// was just edited renders the edited body without waiting for the next page.
fn on_update(event: &web_sys::Event) {
    match serde_json::from_str::<Status>(&payload(event)) {
        Ok(status) => {
            notifications::refresh_status(&status);
            timeline::upsert(vec![status]);
        }
        Err(error) => complain("a status this client cannot read", &error),
    }
}

/// A status that was deleted: it leaves every list, and stops being inlined in a notice.
fn on_delete(event: &web_sys::Event) {
    let raw = payload(event);
    // The server sends the id as a JSON string; a bare id is accepted too, so a frame from an
    // older server still removes the post instead of being dropped in silence.
    let id = serde_json::from_str::<String>(&raw).unwrap_or(raw);
    if id.is_empty() {
        return;
    }
    dispatch(Command::StatusRemoved(id.clone()));
    notifications::forget_status(&id);
}

fn complain(what: &str, error: &impl std::fmt::Display) {
    web_sys::console::warn_1(&format!("nodus: stream: {what}: {error}").into());
}
