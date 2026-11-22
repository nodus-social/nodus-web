//! Everything this server asks an instance: who a token belongs to, and the first page.
//!
//! Both calls go through the SDK's native client, so the endpoints, the pagination header
//! (`Link: rel="next"`) and the payload shapes are the ones the browser app already uses.
//! A second hand-written HTTP layer would be a second place to be wrong about Mastodon.

use std::collections::BTreeMap;

use nodus_client_core::LocalId;
use nodus_client_core::actions::{Browse, ScheduleActions, Search as SearchApi};
use nodus_client_core::model::{
    Account, Context, Conversation, List, Notification, PageParams, ScheduledStatus, SearchResults,
    Status,
};
use nodus_client_core::{Instance, InstanceApi};
use nodus_client_sdk::reads::{Headless, Reads};
use nodus_mastodon::HttpInstance;
use nodus_ui::session_style::{STYLE_KEYS, SessionStyle};
use serde::Deserialize;

/// The staff-relevant fields of an account's own credentials document.
///
/// Read here rather than through the SDK's account model because that model names only the
/// portable Mastodon fields; `role` and Nodus's own `staff_role` are dropped by it. Nodus
/// sends `role` on every account read and `staff_role` only on the caller's own document;
/// Mastodon sends `role` on the own document of a staff account and nothing for a plain
/// one. Both spellings are honoured so the answer does not depend on the dialect.
#[derive(Debug, Default, Deserialize)]
struct Credentials {
    #[serde(default)]
    role: Option<Role>,
    #[serde(default)]
    staff_role: Option<String>,
}

/// The `role` entity Mastodon puts on an account (`{id, name, color, permissions, …}`).
#[derive(Debug, Deserialize)]
struct Role {
    #[serde(default)]
    name: String,
}

impl Credentials {
    /// Whether either spelling names a staff role.
    ///
    /// The Nodus spelling is checked by value because it is a closed vocabulary
    /// (`admin`/`moderator`). Mastodon's role name is the administrator's own label, which
    /// may be a custom word, so a role entity with a non-empty name is the signal there:
    /// Mastodon has no other reason to put `role` on an account.
    fn is_staff(&self) -> bool {
        if let Some(spelling) = self.staff_role.as_deref() {
            return matches!(spelling, "admin" | "moderator");
        }
        self.role
            .as_ref()
            .is_some_and(|role| !role.name.trim().is_empty())
    }
}

/// A signed-in instance, ready to ask.
#[derive(Debug, Clone)]
pub struct Client {
    instance: HttpInstance,
    host: String,
    /// The validated origin, without a trailing slash: probe and verify URLs are built on it.
    base: String,
}

impl Client {
    /// Build a client for `host` with `token`. The host is whatever the reader typed in
    /// the app's sign-in screen; `Instance::parse` supplies `https://` when they omitted it.
    pub fn new(host: &str, token: &str) -> Result<Self, String> {
        let instance = Instance::parse(host).map_err(|error| error.to_string())?;
        crate::validate::validate_instance_url(&instance.base)?;
        let normalized = crate::validate::normalized_host(&instance.base);
        let http =
            crate::validate::shared_public_http_client().map_err(|error| error.to_string())?;
        Ok(Self {
            instance: HttpInstance::with_client(&instance, token.to_owned(), http),
            host: normalized,
            base: instance.base.as_str().trim_end_matches('/').to_owned(),
        })
    }

    /// Normalized `host` or `host:port` for this client.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The validated origin this client talks to, for the calls `HttpInstance` does not make.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Whether this token belongs to a staff account. One request, at sign-in.
    ///
    /// The answer is read from the caller's **own** credentials document, which every
    /// account may read and which now publishes the account's role. The pending queue and
    /// the admin account list are staff-only endpoints, but they answer `403` to a token
    /// without `admin:read` — including the SPA's own `read write follow` grant — so asking
    /// them said "not staff" about the instance's administrators.
    ///
    /// A failure is not staff: an unreadable document leaves no role to draw a door from.
    /// Misskey answers `404` here and its own `/api/i` is not read for a role, so a Misskey
    /// session draws no staff doors today.
    pub async fn is_staff(&self) -> bool {
        let Ok(url) = self.instance.url("/api/v1/accounts/verify_credentials") else {
            return false;
        };
        match self
            .instance
            .fetch::<Credentials>(self.instance.get(url))
            .await
        {
            Ok(credentials) => credentials.is_staff(),
            Err(_) => false,
        }
    }

    /// The first page of the home timeline, newest first.
    ///
    /// Read through the SDK's `Reads` trait rather than by calling the instance directly:
    /// the browser's `Sdk` implements the same trait, so the page this server paints and
    /// the page the browser would have fetched come from one definition of "home timeline".
    pub async fn home(&self, limit: u16) -> Result<(Vec<Status>, Option<String>), String> {
        let headless = Headless::new(self.instance.clone());
        let page = headless
            .home(PageParams::first(limit))
            .await
            .map_err(|error| error.to_string())?;
        Ok((page.items, page.next_max_id))
    }

    /// The local or federated timeline. Both are public reads: the cookie supplies the
    /// token, but a stranger's timeline would answer the same way.
    pub async fn feed(
        &self,
        local: bool,
        limit: u16,
    ) -> Result<(Vec<Status>, Option<String>), String> {
        let kind = if local {
            nodus_client_core::StreamKind::Local
        } else {
            nodus_client_core::StreamKind::Federated
        };
        let page = self
            .instance
            .timeline(kind, PageParams::first(limit))
            .await
            .map_err(|error| error.to_string())?;
        Ok((page.items, page.next_max_id))
    }

    /// Notifications, in the instance's order, plus the pagination cursor if more follow.
    pub async fn notifications(
        &self,
        limit: u16,
    ) -> Result<(Vec<Notification>, Option<String>), String> {
        let page = self
            .instance
            .notifications(PageParams::first(limit))
            .await
            .map_err(|error| error.to_string())?;
        Ok((page.items, page.next_max_id))
    }

    /// Conversations, newest first, plus the pagination cursor if more follow.
    pub async fn conversations(
        &self,
        limit: u16,
    ) -> Result<(Vec<Conversation>, Option<String>), String> {
        let page = self
            .instance
            .conversations(PageParams::first(limit))
            .await
            .map_err(|error| error.to_string())?;
        Ok((page.items, page.next_max_id))
    }

    /// Favourites or bookmarks.
    pub async fn saved(
        &self,
        bookmarks: bool,
        limit: u16,
    ) -> Result<(Vec<Status>, Option<String>), String> {
        let params = PageParams::first(limit);
        let page = if bookmarks {
            self.instance.bookmarks(params).await
        } else {
            self.instance.favourites(params).await
        }
        .map_err(|error| error.to_string())?;
        Ok((page.items, page.next_max_id))
    }

    /// One account, by id.
    pub async fn account(&self, id: &str) -> Result<Account, String> {
        self.instance
            .account(&LocalId(id.to_owned()))
            .await
            .map_err(|error| error.to_string())
    }

    /// One account's posts.
    pub async fn account_statuses(
        &self,
        id: &str,
        limit: u16,
    ) -> Result<(Vec<Status>, Option<String>), String> {
        let page = self
            .instance
            .account_statuses(&LocalId(id.to_owned()), PageParams::first(limit))
            .await
            .map_err(|error| error.to_string())?;
        Ok((page.items, page.next_max_id))
    }

    /// One status, and the thread around it.
    pub async fn status(&self, id: &str) -> Result<Status, String> {
        self.instance
            .status(id)
            .await
            .map_err(|error| error.to_string())
    }

    /// The thread around one status.
    pub async fn context(&self, id: &str) -> Result<Context, String> {
        self.instance
            .status_context(id)
            .await
            .map_err(|error| error.to_string())
    }

    /// The reader's lists.
    pub async fn lists(&self) -> Result<Vec<List>, String> {
        self.instance
            .lists()
            .await
            .map_err(|error| error.to_string())
    }

    /// The queue of posts waiting to go out.
    pub async fn scheduled(&self) -> Result<Vec<ScheduledStatus>, String> {
        self.instance
            .scheduled()
            .await
            .map_err(|error| error.to_string())
    }

    /// The instance's directory: people it suggests.
    pub async fn directory(&self, local: bool) -> Result<Vec<Account>, String> {
        self.instance
            .directory(local, 0)
            .await
            .map_err(|error| error.to_string())
    }

    /// A search.
    pub async fn search(&self, query: &str) -> Result<SearchResults, String> {
        self.instance
            .search(query, false)
            .await
            .map_err(|error| error.to_string())
    }

    /// Fetch Nodus session styling & nav layout from `nodus-backend` fine-grained client metadata.
    ///
    /// A plain Mastodon instance has no such endpoint, and this runs on every painted page,
    /// so a host that has answered "no" once is not asked again in this process.
    pub async fn client_metadata_style(&self) -> Result<SessionStyle, String> {
        #[derive(Deserialize)]
        struct Envelope {
            #[serde(default)]
            metadata: BTreeMap<String, serde_json::Value>,
        }

        if !keeps_styling(self.host()) {
            return Err(String::from("This instance does not keep session styling."));
        }

        let mut url = self
            .instance
            .url("/api/v1/nodus/account/metadata")
            .map_err(|error| error.to_string())?;
        url.query_pairs_mut()
            .append_pair("keys", &STYLE_KEYS.join(","));
        let req = self.instance.get(url);
        let envelope: Envelope = match self.instance.fetch(req).await {
            Ok(envelope) => envelope,
            Err(error) => {
                forget_styling(self.host());
                return Err(error.to_string());
            }
        };
        remember_styling(self.host());
        Ok(SessionStyle::from_metadata_map(&envelope.metadata))
    }

    #[cfg(test)]
    pub(crate) fn for_test(base_url: &str, token: &str) -> Self {
        let instance = Instance::parse(base_url).unwrap();
        let host = crate::validate::normalized_host(&instance.base);
        Self {
            instance: HttpInstance::with_client(
                &instance,
                token.to_owned(),
                reqwest::Client::new(),
            ),
            base: instance.base.as_str().trim_end_matches('/').to_owned(),
            host,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, extract::Query, routing::get};
    use nodus_ui::session_style::{KEY_NAV_LAYOUT, KEY_TOP_BANNER, NavPlacement, SplitDirection};

    #[tokio::test]
    async fn client_metadata_style_requests_fine_grained_keys_and_sanitizes() {
        let app = Router::new().route(
            "/api/v1/nodus/account/metadata",
            get(|Query(params): Query<BTreeMap<String, String>>| async move {
                let keys = params.get("keys").cloned().unwrap_or_default();
                assert!(keys.contains(KEY_TOP_BANNER));
                assert!(keys.contains(KEY_NAV_LAYOUT));
                Json(serde_json::json!({
                    "metadata": {
                        "nodus-top-banner": {
                            "split": "vertical",
                            "slots": [
                                { "url": "https://cdn.example/banner.png", "alt": "Banner" },
                                { "url": "javascript:alert(1)" }
                            ]
                        },
                        "nodus-nav-layout": {
                            "items": [
                                { "id": "lists", "label": "Lists", "path": "/lists", "pinned": true, "placement": "right" }
                            ]
                        }
                    }
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = Client::for_test(&format!("http://{addr}"), "test-token");
        let style = client.client_metadata_style().await.unwrap();
        assert_eq!(style.top.split, SplitDirection::Vertical);
        assert_eq!(style.top.slots.len(), 1);
        assert_eq!(style.top.slots[0].url, "https://cdn.example/banner.png");
        assert_eq!(style.nav.items_in(NavPlacement::Right).len(), 1);
    }

    #[tokio::test]
    async fn staff_answer_is_read_from_the_own_document_not_the_admin_probe() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        use axum::http::StatusCode;

        // The probe this replaced answered staff-only endpoints, so its stub answers an
        // error and counts any call: a render that still asked it would either draw the
        // wrong door or fail this assertion.
        let probed = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&probed);
        let app = Router::new()
            .route(
                "/api/v1/accounts/verify_credentials",
                get(|| async {
                    Json(serde_json::json!({
                        "id": "1",
                        "acct": "admin",
                        "role": { "name": "Admin" },
                        "staff_role": "admin"
                    }))
                }),
            )
            .route(
                "/api/v2/admin/accounts",
                get(move || {
                    let seen = Arc::clone(&seen);
                    async move {
                        seen.fetch_add(1, Ordering::SeqCst);
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let client = Client::for_test(&format!("http://{addr}"), "test-token");
        assert!(client.is_staff().await);
        assert_eq!(probed.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_mastodon_role_on_the_own_document_is_staff_and_a_plain_one_is_not() {
        async fn answer(body: serde_json::Value) -> bool {
            let app = Router::new().route(
                "/api/v1/accounts/verify_credentials",
                get(move || {
                    let body = body.clone();
                    async move { Json(body) }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            Client::for_test(&format!("http://{addr}"), "test-token")
                .is_staff()
                .await
        }

        // Mastodon's own credentials document names a staff role with `role`; its name is
        // the administrator's label, so a custom one must still count.
        assert!(
            answer(serde_json::json!({
                "id": "1",
                "acct": "owner",
                "role": {
                    "id": "3",
                    "name": "Owner",
                    "color": "",
                    "permissions": "1",
                    "highlighted": true
                }
            }))
            .await
        );
        assert!(
            !answer(serde_json::json!({
                "id": "2",
                "acct": "user",
                "role": null,
                "staff_role": null
            }))
            .await
        );
    }

    #[tokio::test]
    async fn home_transport_error_preserves_root_cause_beyond_url() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let client = Client::for_test(&format!("http://{addr}"), "test-token");
        let err = client.home(20).await.unwrap_err();
        assert!(
            err.contains("error sending request for url"),
            "expected reqwest transport prefix, got: {err}"
        );
        assert!(
            err.contains("client error (Connect)")
                || err.contains("Connection refused")
                || err.contains("os error"),
            "expected root cause to be preserved in transport error, got: {err}"
        );
    }
}

/// Hosts this process has seen answer the styling endpoint.
fn keeps_styling(host: &str) -> bool {
    styling_table()
        .lock()
        .map(|known| known.get(host).copied().unwrap_or(true))
        .unwrap_or(true)
}

fn remember_styling(host: &str) {
    if let Ok(mut known) = styling_table().lock() {
        known.insert(host.to_owned(), true);
    }
}

fn forget_styling(host: &str) {
    if let Ok(mut known) = styling_table().lock() {
        known.insert(host.to_owned(), false);
    }
}

/// Whether each host keeps session styling. `true` until it says otherwise, because the
/// first page of a new host has to ask to find out.
fn styling_table() -> &'static std::sync::Mutex<std::collections::HashMap<String, bool>> {
    static TABLE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, bool>>> =
        std::sync::OnceLock::new();
    TABLE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}
