//! The reader's own notifications, exactly as this instance words them.
//!
//! The shared page store keeps a [`nodus_client_core::model::Notification`], which is the
//! Mastodon shape: who, what kind, and the status. This instance also sends a moderation
//! notice — the action, the reason, the rule, whether it can still be appealed — and the
//! grouping key that folds twenty favourites on one post into one row. None of that fits
//! the SDK type, so the page reads the endpoint itself and keeps the extra fields.
//!
//! Availability is part of the state, not an error: an instance with no notifications API
//! answers 404, and the rail is supposed to disappear rather than show a door that opens
//! onto nothing.

use leptos::prelude::*;
use nodus_client_core::model::{Account, Status};
use serde::Deserialize;
use std::collections::HashSet;
use wasm_bindgen_futures::spawn_local;

use super::session_token;
use crate::platform::storage;

/// Where the ids already read are remembered, per browser rather than per account: a
/// notification id names one row on one instance, so two accounts cannot collide on one.
const SEEN_KEY: &str = "nodus.notifications/seen/1";

/// Page size. Enough to fill a phone screen twice, few enough to stay one request.
const LIMIT: usize = 40;

/// One row as `/api/v1/notifications` sends it.
#[derive(Debug, Clone, Deserialize)]
pub struct Notice {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub created_at: String,
    pub account: Account,
    #[serde(default)]
    pub status: Option<Status>,
    /// The sentence the instance wrote when there is no status to inline.
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub moderation: Option<Moderation>,
    #[serde(default)]
    pub group_key: String,
    #[serde(default = "one")]
    pub group_count: i64,
}

fn one() -> i64 {
    1
}

/// What a moderator did, and what the reader can still do about it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Moderation {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub rule: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub can_appeal: bool,
    #[serde(default)]
    pub appeal: Option<Appeal>,
    /// Who decided the appeal. The reader's `AppealDto` does not send it yet; parsing it
    /// here means the sentence appears the day the server starts including it.
    #[serde(default)]
    pub decided_by: Option<String>,
}

/// An appeal against a restriction, as its author sees it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Appeal {
    // The instance sends all three; the page reads only the decision fields today.
    #[expect(dead_code, reason = "part of the wire shape, not of the sentence yet")]
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub action: String,
    #[expect(dead_code, reason = "part of the wire shape, not of the sentence yet")]
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub decision_note: Option<String>,
    #[expect(dead_code, reason = "part of the wire shape, not of the sentence yet")]
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub decided_at: Option<String>,
    #[serde(default)]
    pub decided_by: Option<String>,
}

impl Appeal {
    /// Whether a moderator has answered. `pending` is the only open status the store writes.
    #[must_use]
    pub fn decided(&self) -> bool {
        !self.status.is_empty() && self.status != "pending"
    }
}

thread_local! {
    /// Newest first, as the instance ordered them.
    static ITEMS: RwSignal<Vec<Notice>> = RwSignal::new(Vec::new());
    /// Notification ids already read on this browser.
    static SEEN: RwSignal<HashSet<String>> = RwSignal::new(load_seen());
    /// `None` until asked; `Some(false)` once the instance has answered 404.
    static AVAILABLE: RwSignal<Option<bool>> = RwSignal::new(None);
    static LOADED: RwSignal<bool> = RwSignal::new(false);
    static LOADING: RwSignal<bool> = RwSignal::new(false);
    static ERROR: RwSignal<Option<String>> = RwSignal::new(None);
    /// The `max_id` for the next page, when the instance sent a `Link`.
    static CURSOR: RwSignal<Option<String>> = RwSignal::new(None);
    /// Bumped after an appeal, so a mounted page re-reads the rows it holds.
    static REVISION: RwSignal<u64> = RwSignal::new(0);
}

/// Creates the signals with no owner, before the first mount (see `boot::start`).
///
/// Each `thread_local` is a lazy initialiser: the signal really comes into existence on the
/// first `with`, not when a derived `Signal` around it is built. Touching the signals here
/// is what puts them outside every component's owner. A signal created on first read
/// belongs to whatever scope happened to read it first and dies with it, and the next read
/// then panics with "a reactive value that has already been disposed".
pub fn install() {
    let _ = items_signal();
    let _ = available_signal();
    let _ = seen_signal();
    let _ = loaded_signal();
    let _ = loading_signal();
    let _ = error_signal();
    let _ = cursor_signal();
    let _ = revision_signal();
}

// A `thread_local` is not the `RwSignal`; every read and write goes through `with`.
fn items_signal() -> RwSignal<Vec<Notice>> {
    ITEMS.with(|signal| *signal)
}

fn seen_signal() -> RwSignal<HashSet<String>> {
    SEEN.with(|signal| *signal)
}

fn available_signal() -> RwSignal<Option<bool>> {
    AVAILABLE.with(|signal| *signal)
}

fn loaded_signal() -> RwSignal<bool> {
    LOADED.with(|signal| *signal)
}

fn loading_signal() -> RwSignal<bool> {
    LOADING.with(|signal| *signal)
}

fn error_signal() -> RwSignal<Option<String>> {
    ERROR.with(|signal| *signal)
}

fn cursor_signal() -> RwSignal<Option<String>> {
    CURSOR.with(|signal| *signal)
}

fn revision_signal() -> RwSignal<u64> {
    REVISION.with(|signal| *signal)
}

pub fn items() -> Signal<Vec<Notice>> {
    Signal::derive(move || items_signal().get())
}

pub fn loading() -> Signal<bool> {
    Signal::derive(move || loading_signal().get())
}

pub fn loaded() -> Signal<bool> {
    Signal::derive(move || loaded_signal().get())
}

pub fn error() -> Signal<Option<String>> {
    Signal::derive(move || error_signal().get())
}

pub fn cursor() -> Signal<Option<String>> {
    Signal::derive(move || cursor_signal().get())
}

pub fn available() -> Signal<Option<bool>> {
    Signal::derive(move || available_signal().get())
}

pub fn revision() -> Signal<u64> {
    Signal::derive(move || revision_signal().get())
}

/// The ids already read on this browser.
pub fn seen() -> Signal<HashSet<String>> {
    Signal::derive(move || seen_signal().get())
}

/// How many rows have not been read yet.
pub fn unread() -> Signal<usize> {
    Signal::derive(move || {
        let seen = seen_signal().get();
        items_signal()
            .get()
            .iter()
            .filter(|notice| !seen.contains(&notice.id))
            .count()
    })
}

/// Fetches the first page exactly once per session.
pub fn ensure_loaded() {
    if loaded_signal().get_untracked() || loading_signal().get_untracked() {
        return;
    }
    reload();
}

/// Re-reads the newest page, keeping the rows already shown until the new ones arrive.
pub fn reload() {
    if loading_signal().get_untracked() {
        return;
    }
    loading_signal().set(true);
    error_signal().set(None);
    spawn_local(async move {
        match fetch(None).await {
            Ok(Some(page)) => {
                available_signal().set(Some(true));
                items_signal().set(page.notices);
                cursor_signal().set(page.cursor);
                loaded_signal().set(true);
            }
            Ok(None) => {
                available_signal().set(Some(false));
                items_signal().set(Vec::new());
                cursor_signal().set(None);
                loaded_signal().set(true);
            }
            Err(message) => error_signal().set(Some(message)),
        }
        loading_signal().set(false);
    });
}

/// Adds a row the live stream just pushed, or refreshes the row already holding that id.
///
/// Deduped by id, not by content: a reconnecting stream replays what was missed, and a
/// list that appended the repeat would count one favourite twice in the rail's badge. A
/// repeat replaces the row in place — the instance's copy is the newer one — and that is
/// also the path an `update` for the row's status takes.
pub fn push(notice: Notice) {
    // A push is proof the endpoint exists, whatever the rail was told before: the stream
    // only sends notifications the instance can also list.
    available_signal().set(Some(true));
    items_signal().update(|rows| match rows.iter().position(|row| row.id == notice.id) {
        Some(index) => rows[index] = notice,
        None => rows.insert(0, notice),
    });
}

/// Replaces the inline copy of a status the stream says changed.
///
/// The row itself is not touched: an edited post still is the post that was favourited, and
/// the notice is a record of the event, not a snapshot of the status.
pub fn refresh_status(status: &Status) {
    items_signal().update(|rows| {
        for row in rows.iter_mut() {
            if row.status.as_ref().is_some_and(|held| held.id == status.id) {
                row.status = Some(status.clone());
            }
        }
    });
}

/// Drops the inline copy of a status the stream says was deleted.
///
/// The row stays, with no status: the notification is still something that happened, and
/// the instance keeps sending it (with `status: null`) for exactly that reason. What goes
/// is the post body, which would otherwise be rendered from a copy nothing can serve.
pub fn forget_status(id: &str) {
    items_signal().update(|rows| {
        for row in rows.iter_mut() {
            if row.status.as_ref().is_some_and(|held| held.id == id) {
                row.status = None;
            }
        }
    });
    revision_signal().update(|revision| *revision += 1);
}

/// Appends the next page, when the instance offered one.
pub fn load_more() {
    let Some(cursor) = cursor_signal().get_untracked() else {
        return;
    };
    if loading_signal().get_untracked() {
        return;
    }
    loading_signal().set(true);
    error_signal().set(None);
    spawn_local(async move {
        match fetch(Some(cursor)).await {
            Ok(Some(mut page)) => {
                items_signal().update(|rows| rows.append(&mut page.notices));
                cursor_signal().set(page.cursor);
            }
            Ok(None) => available_signal().set(Some(false)),
            Err(message) => error_signal().set(Some(message)),
        }
        loading_signal().set(false);
    });
}

/// Remembers every row currently on the page as read.
pub fn mark_all_read() {
    let ids: Vec<String> = items_signal()
        .get_untracked()
        .iter()
        .map(|row| row.id.clone())
        .collect();
    if ids.is_empty() {
        return;
    }
    seen_signal().update(|seen| {
        for id in &ids {
            seen.insert(id.clone());
        }
    });
    save_seen();
}

/// Remembers one row as read, and tells the instance when it can.
pub fn mark_seen(id: &str) {
    seen_signal().update(|seen| {
        seen.insert(id.to_owned());
    });
    save_seen();
    let Some(token) = session_token() else {
        return;
    };
    let path = format!("/api/v1/notifications/{id}");
    spawn_local(async move {
        let Ok(request) = gloo_net::http::Request::post(&path)
            .header("authorization", &format!("Bearer {token}"))
            .build()
        else {
            return;
        };
        let _ = request.send().await;
    });
}

/// Posts an appeal against the restriction this notice announced.
pub fn submit_appeal(notice_id: String, reason: String, done: Callback<Result<(), String>>) {
    let Some(token) = session_token() else {
        done.run(Err("Sign in again to appeal.".to_owned()));
        return;
    };
    let body = serde_json::json!({ "reason": reason });
    let Ok(request) = gloo_net::http::Request::post("/api/v1/appeals")
        .header("authorization", &format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(body.to_string())
    else {
        done.run(Err("The appeal could not be sent.".to_owned()));
        return;
    };
    spawn_local(async move {
        match request.send().await {
            Ok(response) if response.ok() => {
                let appeal = response.json::<Appeal>().await.unwrap_or_else(|_| Appeal {
                    status: "pending".to_owned(),
                    ..Appeal::default()
                });
                items_signal().update(|rows| {
                    if let Some(row) = rows.iter_mut().find(|row| row.id == notice_id) {
                        if let Some(moderation) = row.moderation.as_mut() {
                            moderation.can_appeal = false;
                            moderation.appeal = Some(appeal);
                        }
                    }
                });
                revision_signal().update(|revision| *revision += 1);
                done.run(Ok(()));
            }
            Ok(response) => {
                done.run(Err(format!(
                    "The instance refused the appeal ({}).",
                    response.status()
                )));
            }
            Err(error) => done.run(Err(format!("The appeal could not be sent: {error}"))),
        }
    });
}

struct Page {
    notices: Vec<Notice>,
    cursor: Option<String>,
}

/// One request; `None` means the instance has no notifications API (404).
async fn fetch(cursor: Option<String>) -> Result<Option<Page>, String> {
    let Some(token) = session_token() else {
        return Err("Sign in to read notifications.".to_owned());
    };
    let mut path = format!("/api/v1/notifications?limit={LIMIT}");
    if let Some(cursor) = cursor {
        path.push_str("&max_id=");
        path.push_str(&cursor);
    }
    let request = gloo_net::http::Request::get(&path)
        .header("authorization", &format!("Bearer {token}"))
        .build()
        .map_err(|error| error.to_string())?;
    let response = request.send().await.map_err(|error| error.to_string())?;
    if response.status() == 404 {
        return Ok(None);
    }
    if !response.ok() {
        return Err(format!("The instance answered {}.", response.status()));
    }
    let cursor = next_max_id(response.headers().get("link"));
    let notices = response.json::<Vec<Notice>>().await.map_err(|error| {
        format!("The instance sent notifications this client cannot read: {error}")
    })?;
    Ok(Some(Page { notices, cursor }))
}

/// The `max_id` in a `Link: <...?max_id=123>; rel="next"` header.
fn next_max_id(link: Option<String>) -> Option<String> {
    let link = link?;
    let start = link.find("max_id=")? + "max_id=".len();
    let rest = &link[start..];
    let end = rest
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(rest.len());
    (!rest[..end].is_empty()).then(|| rest[..end].to_owned())
}

fn load_seen() -> HashSet<String> {
    storage::get(SEEN_KEY)
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .map(|ids| ids.into_iter().collect())
        .unwrap_or_default()
}

fn save_seen() {
    let ids: Vec<String> = seen_signal().get_untracked().into_iter().collect();
    if let Ok(raw) = serde_json::to_string(&ids) {
        storage::set(SEEN_KEY, &raw);
    }
}
