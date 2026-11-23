//! Server-side reads of a public origin.
//!
//! The browser never talks to a third-party instance: a shared link is resolved by this
//! host and the bytes are handed back same-origin. A signed-in reader's own home instance
//! is asked first to resolve the link, so the home server pulls the remote copy it keeps.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::Value;

use crate::validate::{host_valid, id_valid, public_http_client};
use crate::{App, session_of};

fn local_host() -> Option<String> {
    std::env::var("NODUS_INSTANCE_HOST").ok()
}

/// The host name a value such as `https://nodus.test` or `nodus.test:18443` names.
fn host_of(value: &str) -> &str {
    let authority = value.split_once("://").map_or(value, |(_, rest)| rest);
    let authority = authority.split('/').next().unwrap_or(authority);
    authority
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
        .map_or(authority, |(host, _)| host)
}

/// The host name this shell answers on.
///
/// A deployment sets `NODUS_INSTANCE_HOST` (or at least `NODUS_BASE_URL`); the lab and the
/// dev loop set neither for the web role, so the browser's `Host` header names it instead.
/// Only the name is used: callers go back out over https, where Caddy terminates.
fn local_host_name(headers: &HeaderMap) -> Option<String> {
    if let Ok(host) = std::env::var("NODUS_INSTANCE_HOST")
        && !host.is_empty()
    {
        return Some(host);
    }
    if let Ok(base) = std::env::var("NODUS_BASE_URL")
        && !host_of(&base).is_empty()
    {
        return Some(host_of(&base).to_owned());
    }
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(host_of)
        .filter(|host| !host.is_empty())
        .map(std::borrow::ToOwned::to_owned)
}

/// A read of this origin's own copy of a post or profile that lives on another instance.
///
/// The shell links a remote post by the id this origin stores under the peer's host name,
/// so asking the peer for that id would 404 even when the peer is reachable. The origin
/// already has the row `public_page` serves; `None` means it does not hold this id (the
/// peer's own numbering, an unknown post), and the caller relays to the peer instead of
/// inventing a second lookup.
///
/// The origin answers by id alone: it is never told which host the address claimed, so a
/// caller that serves these bytes must check `answers_for` first.
async fn local_json(headers: &HeaderMap, path: &str, remote_host: &str) -> Option<Value> {
    let local = local_host_name(headers)?;
    if local == remote_host || !host_valid(&local) {
        return None;
    }
    let client = external_client().ok()?;
    let response = client
        .get(format!("https://{local}{path}"))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

/// Whether a row this origin stored is an object that lives on `host`, the instance the
/// address named.
///
/// The id alone cannot answer this, which is why it is checked rather than assumed: this
/// origin numbers its own posts and the ones it copied from peers in one space, so
/// `/status/mastodon.test/<a local id>` resolves locally and would answer 200 with
/// `"uri": "https://<this host>/@ada/..."`. The bytes are public either way, but the link
/// would claim the post lives on `mastodon.test`, and anything keyed on that path segment —
/// a cache entry, a trust ruling, a moderation badge — would read a local row as the
/// peer's. A mismatch therefore falls through to the relay, which 404s or 502s for that id
/// exactly as it would for any id the peer does not have.
///
/// The row's own links are the evidence: `uri` and `url` point at the instance that holds
/// the object, and a row that names no host at all is refused for the same reason.
fn answers_for(row: &Value, host: &str) -> bool {
    let named = ["uri", "url"]
        .into_iter()
        .filter_map(|key| row.get(key).and_then(Value::as_str))
        .map(host_of)
        .find(|named| !named.is_empty());
    match named {
        Some(named) => named == host,
        None => row
            .get("acct")
            .and_then(Value::as_str)
            .and_then(|acct| acct.rsplit_once('@'))
            .is_some_and(|(_, acct_host)| acct_host == host),
    }
}

/// The kind of read a stored row may answer, which decides who it answers for.
///
/// The two differ because their addresses mean different things. A post permalink carries
/// the id this origin minted for its stored copy, so no relay can turn that id into an
/// answer — for a signed-out reader or a signed-in one. A profile address carries the
/// instance's own reference, which a signed-in reader's home instance can still resolve.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Read {
    /// A post, addressed by the id this origin minted for its copy of it.
    Post,
    /// A profile, addressed the way the instance that holds it numbers accounts.
    Profile,
}

/// The response a read may take from this origin's stored row, if any.
///
/// A post is answered for every reader. The id in `/status/{host}/{id}` is the one this
/// origin minted for the row it copied (`rows.rs`, `status_path(Some(&host), &inner.id)`),
/// so the peer never had it: relaying it asks an instance for a post that instance never
/// held. That is wrong in production, not just where a private peer is refused. A token
/// cannot rescue the relay either, because the reader's home instance resolves the peer's
/// own numbering, not this origin's. So a signed-in reader is served the stored row, and a
/// reader who happens to have the post in their home instance sees this origin's public
/// copy of it.
///
/// A profile is answered for a signed-out reader only. A signed-in reader's home instance
/// resolves the profile by url and answers with their relationship to it, so the relay is
/// the better answer there and stays (`8fbabee`).
///
/// Either way the row must name the host the address named (`answers_for`); the reader's
/// session is not the check that keeps a local row from answering under a peer's name.
fn from_origin(
    read: Read,
    token: Option<&str>,
    row: Option<Value>,
    host: &str,
) -> Option<Response> {
    if read == Read::Profile && token.is_some() {
        return None;
    }
    let row = row.filter(|row| answers_for(row, host))?;
    Some(Json(row).into_response())
}

fn hydrate_base() -> Option<String> {
    std::env::var("NODUS_HYDRATE_URL").ok()
}

async fn hydrate(path: &str) -> Option<Value> {
    let base = hydrate_base()?;
    let mut request = reqwest::Client::new()
        .get(format!("{}{path}", base.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(8));
    if let Ok(token) = std::env::var("NODUS_HYDRATE_TOKEN") {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

fn hydrated_account(account: &Value) -> Value {
    let acct = account
        .get("acct")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let username = acct.split('@').next().unwrap_or(acct);
    serde_json::json!({
        "id": account.get("id").cloned().unwrap_or(Value::Null),
        "username": username,
        "acct": acct,
        "display_name": account.get("display_name").cloned().unwrap_or(Value::Null),
        "note": account.get("note").cloned().unwrap_or(Value::Null),
        "url": account.get("url").cloned().unwrap_or(Value::Null),
        "avatar": account.get("avatar").cloned().unwrap_or(Value::Null),
        "header": account.get("header").cloned().unwrap_or(Value::Null),
        "followers_count": account.get("followers_count").cloned().unwrap_or(Value::Null),
        "following_count": account.get("following_count").cloned().unwrap_or(Value::Null),
        "statuses_count": 0,
        "bot": false,
        "locked": false,
        "created_at": "",
        "emojis": [],
        "fields": []
    })
}

fn hydrated_status(post: &Value, account: &Value) -> Value {
    let media: Vec<Value> = post
        .get("media")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|media| {
                    serde_json::json!({
                        "id": media.get("url").and_then(Value::as_str).unwrap_or_default(),
                        "type": media.get("kind").and_then(Value::as_str).unwrap_or("unknown"),
                        "url": media.get("url").cloned().unwrap_or(Value::Null),
                        "preview_url": media.get("preview_url").cloned().unwrap_or(Value::Null),
                        "description": media.get("description").cloned().unwrap_or(Value::Null),
                        "meta": {
                            "original": {
                                "width": media.get("width").cloned().unwrap_or(Value::Null),
                                "height": media.get("height").cloned().unwrap_or(Value::Null)
                            }
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    serde_json::json!({
        "id": post.get("id").cloned().unwrap_or(Value::Null),
        "created_at": post.get("published").cloned().unwrap_or(Value::Null),
        "content": post.get("content").cloned().unwrap_or(Value::Null),
        "url": post.get("url").cloned().unwrap_or(Value::Null),
        "visibility": "public",
        "sensitive": post.get("sensitive").cloned().unwrap_or(Value::Bool(false)),
        "spoiler_text": post.get("spoiler_text").cloned().unwrap_or(Value::Null),
        "favourites_count": 0,
        "reblogs_count": 0,
        "replies_count": post.get("replies_count").cloned().unwrap_or(Value::Null),
        "favourited": false,
        "reblogged": false,
        "bookmarked": false,
        "reblog": null,
        "in_reply_to_id": null,
        "media_attachments": media,
        "emojis": post.get("emojis").cloned().unwrap_or(Value::Array(Vec::new())),
        "mentions": [],
        "tags": [],
        "card": null,
        "poll": null,
        "edited_at": null,
        "account": hydrated_account(account)
    })
}

fn external_client() -> Result<reqwest::Client, reqwest::Error> {
    public_http_client(std::time::Duration::from_secs(12))
}

/// A sentence that names the real reason, not just "error sending request".
///
/// `reqwest::Error`'s own `Display` hides its source, which turns every failure — a DNS
/// refusal, a refused connection, a rejected certificate — into the same useless line in an
/// operator's log and in the reader's error page.
fn error_detail(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut cause = std::error::Error::source(error);
    while let Some(source) = cause {
        message.push_str(": ");
        message.push_str(&source.to_string());
        cause = source.source();
    }
    message
}

fn valid_target(host: &str, id: &str) -> bool {
    host_valid(host) && id_valid(id)
}

/// Whether a path segment is an instance's own numbering rather than a name.
///
/// Mastodon prints decimal ids and nodus prints ULIDs; nothing else is a valid id, so any
/// other segment is treated as the account's name. Names are what the site links with,
/// ids are what the API answers with, and both have to keep working.
pub(crate) fn looks_like_id(segment: &str) -> bool {
    segment.chars().all(|c| c.is_ascii_digit())
        || (segment.len() == 26 && segment.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// The id of the account a path segment names.
///
/// A segment that is already an id is passed straight through; anything else is asked of
/// the instance the address named, through its own `accounts/lookup`, so a name is
/// resolved by the instance that owns it rather than guessed here.
pub(crate) async fn resolve_account_ref(
    host: &str,
    segment: &str,
    token: Option<&str>,
) -> Option<String> {
    if looks_like_id(segment) {
        return Some(segment.to_owned());
    }
    let acct = format!("{segment}@{host}");
    let url = format!(
        "https://{host}/api/v1/accounts/lookup?acct={}",
        urlencoding(&acct)
    );
    let account = json(&url, token).await?;
    account
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(std::borrow::ToOwned::to_owned)
}

async fn json(url: &str, token: Option<&str>) -> Option<Value> {
    let client = external_client().ok()?;
    let mut request = client.get(url);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

async fn relay(url: String, token: Option<&str>) -> Response {
    let client = match external_client() {
        Ok(client) => client,
        Err(error) => {
            return (StatusCode::BAD_GATEWAY, error_detail(&error)).into_response();
        }
    };
    let mut request = client.get(&url);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return (StatusCode::BAD_GATEWAY, error_detail(&error)).into_response();
        }
    };
    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => {
            return (StatusCode::BAD_GATEWAY, error_detail(&error)).into_response();
        }
    };
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// Resolve an origin account through the reader's home instance.
async fn account_target(
    app: &App,
    headers: &HeaderMap,
    host: &str,
    id: &str,
) -> (String, String, Option<String>) {
    let session = session_of(app, headers);
    if session.is_none()
        && local_host().as_deref() == Some(host)
        && let Some(local) = json(&format!("https://{host}/api/v1/accounts/{id}"), None).await
        && let Some(acct) = local.get("acct").and_then(Value::as_str)
        && let Some((remote_user, remote_host)) = acct.split_once('@')
        && !remote_user.is_empty()
        && !remote_host.contains('@')
        && host_valid(remote_host)
    {
        let resolved = hydrate(&format!("/v1/ap/account?acct={}", urlencoding(acct))).await;
        if let Some(remote) = resolved
            .as_ref()
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)
            .filter(|remote| id_valid(remote))
        {
            return (remote_host.to_owned(), remote.to_owned(), None);
        }
    }
    let Some(session) = session.filter(|session| host_valid(&session.host)) else {
        return (host.to_owned(), id.to_owned(), None);
    };
    if session.host == host {
        return (host.to_owned(), id.to_owned(), Some(session.token));
    }
    let origin = json(&format!("https://{host}/api/v1/accounts/{id}"), None).await;
    let Some(url) = origin
        .as_ref()
        .and_then(|value| value.get("url"))
        .and_then(Value::as_str)
    else {
        return (host.to_owned(), id.to_owned(), None);
    };
    let query = urlencoding(url);
    let search = json(
        &format!(
            "https://{}/api/v2/search?q={query}&resolve=true&type=accounts&limit=1",
            session.host
        ),
        Some(&session.token),
    )
    .await;
    let local = search
        .as_ref()
        .and_then(|value| value.get("accounts"))
        .and_then(Value::as_array)
        .and_then(|accounts| accounts.first())
        .and_then(|account| account.get("id"))
        .and_then(Value::as_str)
        .filter(|local| id_valid(local));
    match local {
        Some(local) => (session.host.clone(), local.to_owned(), Some(session.token)),
        None => (host.to_owned(), id.to_owned(), None),
    }
}

fn urlencoding(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The same resolution for a status, using its origin URL.
async fn status_target(
    app: &App,
    headers: &HeaderMap,
    host: &str,
    id: &str,
) -> (String, String, Option<String>) {
    let Some(session) = session_of(app, headers).filter(|session| host_valid(&session.host)) else {
        return (host.to_owned(), id.to_owned(), None);
    };
    if session.host == host {
        return (host.to_owned(), id.to_owned(), Some(session.token));
    }
    let origin = json(&format!("https://{host}/api/v1/statuses/{id}"), None).await;
    let Some(url) = origin
        .as_ref()
        .and_then(|value| value.get("url"))
        .and_then(Value::as_str)
    else {
        return (host.to_owned(), id.to_owned(), None);
    };
    let query = urlencoding(url);
    let search = json(
        &format!(
            "https://{}/api/v2/search?q={query}&resolve=true&type=statuses&limit=1",
            session.host
        ),
        Some(&session.token),
    )
    .await;
    let local = search
        .as_ref()
        .and_then(|value| value.get("statuses"))
        .and_then(Value::as_array)
        .and_then(|statuses| statuses.first())
        .and_then(|status| status.get("id"))
        .and_then(Value::as_str)
        .filter(|local| id_valid(local));
    match local {
        Some(local) => (session.host.clone(), local.to_owned(), Some(session.token)),
        None => (host.to_owned(), id.to_owned(), None),
    }
}

pub async fn status(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path((host, id)): Path<(String, String)>,
) -> Response {
    if !valid_target(&host, &id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    // The store is asked before either remote read: the id is this origin's, so the peer
    // cannot answer for it and the relay is only reached for an id this origin never
    // stored (a fresh link to the peer's own numbering), where it is the right answer.
    let token = session_of(&app, &headers).map(|session| session.token);
    let stored = local_json(&headers, &format!("/api/v1/statuses/{id}"), &host).await;
    if let Some(response) = from_origin(Read::Post, token.as_deref(), stored, &host) {
        return response;
    }
    let (target, id, token) = status_target(&app, &headers, &host, &id).await;
    relay(
        format!("https://{target}/api/v1/statuses/{id}"),
        token.as_deref(),
    )
    .await
}

pub async fn context(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path((host, id)): Path<(String, String)>,
) -> Response {
    if !valid_target(&host, &id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let (target, id, token) = status_target(&app, &headers, &host, &id).await;
    if token.is_none()
        && let Some(body) =
            local_json(&headers, &format!("/api/v1/statuses/{id}/context"), &host).await
    {
        return Json(body).into_response();
    }
    relay(
        format!("https://{target}/api/v1/statuses/{id}/context"),
        token.as_deref(),
    )
    .await
}

pub async fn account(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path((host, id)): Path<(String, String)>,
) -> Response {
    if !valid_target(&host, &id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    // The address may carry the account's name; the read endpoint wants its id.
    let Some(resolved) = resolve_account_ref(&host, &id, None).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (target, id, token) = account_target(&app, &headers, &host, &resolved).await;
    let stored = local_json(&headers, &format!("/api/v1/accounts/{id}"), &host).await;
    if let Some(response) = from_origin(Read::Profile, token.as_deref(), stored, &host) {
        return response;
    }
    relay(
        format!("https://{target}/api/v1/accounts/{id}"),
        token.as_deref(),
    )
    .await
}

#[derive(Debug, Deserialize)]
pub struct Before {
    max_id: Option<String>,
}

pub async fn account_statuses(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path((host, id)): Path<(String, String)>,
    Query(before): Query<Before>,
) -> Response {
    if !valid_target(&host, &id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if before.max_id.as_deref().is_some_and(|max| !id_valid(max)) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let (target, id, token) = account_target(&app, &headers, &host, &id).await;
    let suffix = before
        .max_id
        .as_ref()
        .map(|max| format!("?max_id={max}"))
        .unwrap_or_default();
    if token.is_none()
        && let Some(body) = local_json(
            &headers,
            &format!(
                "/api/v1/accounts/{id}/statuses?limit=20{}",
                suffix.replace('?', "&")
            ),
            &host,
        )
        .await
    {
        return Json(body).into_response();
    }
    if let Some(value) = json(
        &format!(
            "https://{target}/api/v1/accounts/{id}/statuses?limit=20{}",
            suffix.replace('?', "&")
        ),
        token.as_deref(),
    )
    .await
        && value.as_array().is_some_and(|items| !items.is_empty())
    {
        return Json(value).into_response();
    }
    let account = hydrate(&format!("/v1/account/{target}/{id}")).await;
    let posts = hydrate(&format!("/v1/account/{target}/{id}/statuses{suffix}")).await;
    if let (Some(account), Some(posts)) = (account, posts) {
        let items: Vec<Value> = posts
            .get("items")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|post| hydrated_status(post, &account))
                    .collect()
            })
            .unwrap_or_default();
        return Json(Value::Array(items)).into_response();
    }
    (StatusCode::BAD_GATEWAY, "account statuses unavailable").into_response()
}

pub async fn followers(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path((host, id)): Path<(String, String)>,
) -> Response {
    if !valid_target(&host, &id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let (target, id, token) = account_target(&app, &headers, &host, &id).await;
    relay(
        format!("https://{target}/api/v1/accounts/{id}/followers?limit=40"),
        token.as_deref(),
    )
    .await
}

pub async fn following(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Path((host, id)): Path<(String, String)>,
) -> Response {
    if !valid_target(&host, &id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let (target, id, token) = account_target(&app, &headers, &host, &id).await;
    relay(
        format!("https://{target}/api/v1/accounts/{id}/following?limit=40"),
        token.as_deref(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::{Read, answers_for, from_origin, host_of, host_valid};
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use serde_json::json;

    /// A post this origin copied from `host`: its own links name the instance that holds it.
    fn copied_post(host: &str) -> serde_json::Value {
        json!({
            "id": "01m7yzm11115gn85zsr68vhcx4",
            "uri": format!("https://{host}/@ada/01m7yzm11115gn85zsr68vhcx4"),
            "url": format!("https://{host}/@ada/01m7yzm11115gn85zsr68vhcx4"),
            "content": "<p>hello</p>"
        })
    }

    #[tokio::test]
    async fn a_matching_host_row_is_answered_from_this_origin() {
        let response = from_origin(
            Read::Post,
            None,
            Some(copied_post("mastodon.test")),
            "mastodon.test",
        )
        .expect("a row that names the requested host answers from here");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap_or_default();
        assert!(
            String::from_utf8_lossy(&body).contains("mastodon.test"),
            "the row itself is handed back"
        );
    }

    #[tokio::test]
    async fn a_signed_in_reader_is_answered_a_stored_peer_post_from_this_origin() {
        let response = from_origin(
            Read::Post,
            Some("token"),
            Some(copied_post("mastodon.test")),
            "mastodon.test",
        )
        .expect("the session must not send the reader to a peer that never had the id");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap_or_default();
        assert!(String::from_utf8_lossy(&body).contains("hello"));
    }

    #[test]
    fn a_signed_in_read_of_an_id_this_origin_does_not_hold_still_relays() {
        // The peer's own numbering: nothing here answers, so the handler must fall
        // through to the relay and its honest 502/404.
        assert!(from_origin(Read::Post, Some("token"), None, "mastodon.test").is_none());
    }

    #[test]
    fn a_stored_row_that_names_another_instance_is_refused_for_a_signed_in_reader() {
        assert!(
            from_origin(
                Read::Post,
                Some("token"),
                Some(copied_post("nodus.test")),
                "mastodon.test"
            )
            .is_none()
        );
    }

    #[test]
    fn a_row_that_lives_here_is_refused_under_another_instance() {
        // The live case: ada's local post read through a permalink naming the peer.
        assert!(!answers_for(&copied_post("nodus.test"), "mastodon.test"));
        assert!(
            from_origin(
                Read::Post,
                None,
                Some(copied_post("nodus.test")),
                "mastodon.test"
            )
            .is_none()
        );
        // The port a deployment may carry is not part of the name.
        assert!(answers_for(&copied_post("nodus.test"), "nodus.test"));
        assert!(answers_for(
            &copied_post("mastodon.test:18443"),
            "mastodon.test"
        ));
        // A row that names no host proves nothing, so it is refused too.
        assert!(!answers_for(
            &json!({"id": "1", "content": "<p>hello</p>"}),
            "mastodon.test"
        ));
    }

    #[tokio::test]
    async fn an_anonymous_profile_read_prefers_this_origin() {
        let profile = json!({
            "id": "01m7yznagw96czhm7tzaqyk8w6",
            "acct": "ada@mastodon.test",
            "url": "https://mastodon.test/@ada",
            "display_name": "Ada"
        });
        let response = from_origin(Read::Profile, None, Some(profile.clone()), "mastodon.test")
            .expect("an anonymous reader is served the stored profile");
        assert_eq!(response.status(), StatusCode::OK);
        // A signed-in reader keeps the relay to their home instance.
        assert!(
            from_origin(Read::Profile, Some("token"), Some(profile), "mastodon.test").is_none()
        );
        assert!(from_origin(Read::Profile, None, None, "mastodon.test").is_none());
    }

    #[test]
    fn host_of_reads_a_name_through_a_scheme_path_or_port() {
        assert_eq!(host_of("https://nodus.test"), "nodus.test");
        assert_eq!(host_of("nodus.test:18443"), "nodus.test");
        assert_eq!(host_of("https://nodus.test/status/1"), "nodus.test");
        assert_eq!(host_of("nodus.test"), "nodus.test");
        // A colon that is not a port (an IPv6 literal) is left alone.
        assert_eq!(host_of("[::1]"), "[::1]");
    }

    #[test]
    fn host_valid_accepts_public_domains_and_ports() {
        assert!(host_valid("mastodon.social"));
        assert!(host_valid("fosstodon.org:8443"));
        assert!(host_valid("sub.domain.example"));
    }

    #[test]
    fn host_valid_rejects_ssrf_and_malformed_hosts() {
        for bad in [
            "",
            "..",
            "...",
            ".example.com",
            "example.com.",
            "a..b",
            "localhost",
            "foo.localhost",
            "metadata.google.internal",
            "printer.local",
            "127.0.0.1",
            "127.0.0.1:8080",
            "127.1",
            "0x7f.0.0.1",
            "0177.0.0.1",
            "0.0.0.0",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "224.0.0.1",
            "::1",
            "[::1]",
            "::ffff:127.0.0.1",
            "example.com:0",
            "example.com:99999",
            "example.com:80:80",
            "https://mastodon.social",
        ] {
            assert!(!host_valid(bad), "expected host_valid({bad:?}) to be false");
        }
    }
}
