//! Everything this server asks an instance: who a token belongs to, and the first page.
//!
//! Both calls go through the SDK's native client, so the endpoints, the pagination header
//! (`Link: rel="next"`) and the payload shapes are the ones the browser app already uses.
//! A second hand-written HTTP layer would be a second place to be wrong about Mastodon.

use std::collections::BTreeMap;

use nodus_client_core::LocalId;
use nodus_client_core::actions::{
    Browse, RegistrationActions, ScheduleActions, Search as SearchApi,
};
use nodus_client_core::model::{
    Account, Context, Conversation, List, Notification, PageParams, ScheduledStatus, SearchResults,
    Status,
};
use nodus_client_core::{Instance, InstanceApi};
use nodus_client_sdk::reads::{Headless, Reads};
use nodus_mastodon::HttpInstance;
use nodus_ui::session_style::{STYLE_KEYS, SessionStyle};
use serde::Deserialize;

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

    /// Whether this token may moderate. One request, at sign-in.
    ///
    /// The pending queue is the cheapest endpoint that is staff-only: a moderator gets a
    /// list, anyone else a `403` with a JSON body (Mastodon does not answer `404` here).
    pub async fn is_staff(&self) -> bool {
        self.instance.pending_accounts(1).await.is_ok()
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
