//! Cookie sessions, and what they are allowed to hold.
//!
//! The cookie is an opaque id. It never carries the OAuth token, and no response body ever
//! contains one: the token lives in this process (and in the file named by
//! `NODUS_SESSIONS`) mapped from the id. A cookie that leaked would let its holder read
//! pages *as* the account, which is already what a browser session means; a token that
//! leaked would also let its holder write to the instance from anywhere, forever.
//!
//! Storage is one JSON file rewritten on every change. That is deliberately the smallest
//! thing that survives a restart: at M2 this becomes the `oauth_token` table described in
//! `nodus-backend/docs/06-ssr.md`, and the endpoint shapes do not change.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::protocol::Protocol;
use nodus_ui::session_style::SessionStyle;
use nodus_web::shared::account_state::{AccountCache, AccountKey};
use serde::{Deserialize, Serialize};

fn cache_key(host: &str, account_id: &str) -> AccountKey {
    (host.to_owned(), account_id.to_owned())
}

/// The cookie name. `HttpOnly` because no script needs it, `SameSite=Lax` because a
/// cross-site `POST` must not be able to act as the reader, `Secure` because the SSR host
/// is only ever served over TLS.
pub const COOKIE: &str = "nodus_session";

/// One signed-in visitor: which instance, which account, and the token that proves it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub host: String,
    pub token: String,
    pub account_id: String,
    pub acct: String,
    /// The dialect the token was proven in, decided once at sign-in.
    ///
    /// Persisted so a later render knows which verbs this server has: a Misskey session can
    /// be signed in and read in the browser, but this process reads through Mastodon's
    /// client API, and painting an error as content would be the lie that hides it.
    #[serde(default)]
    pub protocol: Protocol,
    /// What the account calls itself and what it looks like, read once at sign-in.
    ///
    /// Cached for the same reason as [`Session::style`]: the rail is drawn before any wasm
    /// runs, and a `verify_credentials` call on every page would be paid by every reader to
    /// learn something that changes about as often as a display name does.
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar: String,
    /// Whether this account answered the moderation queue when it signed in.
    ///
    /// Probed once, at sign-in, rather than on every render: the rail has to know before
    /// the first paint whether to draw the moderation doors, and an instance call per page
    /// would be paid by every reader. A role granted later shows up on the next sign-in.
    #[serde(default)]
    pub admin: Option<bool>,
    /// Per-account session styling and navigation layout, cached at sign-in or via sync.
    #[serde(default)]
    pub style: Option<SessionStyle>,
}

/// What the browser is told about its own session. Note the absence of the token.
#[derive(Debug, Clone, Serialize)]
pub struct SessionView<'a> {
    pub host: &'a str,
    pub account_id: &'a str,
    pub acct: &'a str,
    /// Which dialect this session was proven in, so the browser can leave out verbs the
    /// server does not have instead of offering them and failing.
    pub protocol: &'a str,
}

impl Session {
    #[must_use]
    pub fn view(&self) -> SessionView<'_> {
        SessionView {
            host: &self.host,
            account_id: &self.account_id,
            acct: &self.acct,
            protocol: self.protocol.as_str(),
        }
    }
}

/// The session store: an id map, plus the file it is mirrored to.
#[derive(Debug)]
pub struct Sessions {
    path: Option<PathBuf>,
    inner: Mutex<HashMap<String, Session>>,
    styles: Mutex<AccountCache<SessionStyle>>,
}

impl Sessions {
    /// Load the file if it exists. A corrupt file is reported and then ignored, because a
    /// server that refuses to start over a stale JSON blob is worse than one that asks
    /// everyone to sign in again.
    #[must_use]
    pub fn load(path: Option<PathBuf>) -> Self {
        let inner = path
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(
                |bytes| match serde_json::from_slice::<HashMap<String, Session>>(&bytes) {
                    Ok(map) => Some(map),
                    Err(error) => {
                        tracing::warn!(%error, "ignoring an unreadable session file");
                        None
                    }
                },
            )
            .unwrap_or_default();
        let mut styles = AccountCache::new();
        for session in inner.values() {
            if let Some(style) = session.style.as_ref() {
                styles.insert(
                    cache_key(&session.host, &session.account_id),
                    style.clone().sanitized(),
                );
            }
        }
        Self {
            path,
            inner: Mutex::new(inner),
            styles: Mutex::new(styles),
        }
    }

    fn guard(&self) -> MutexGuard<'_, HashMap<String, Session>> {
        // A poisoned map is still readable data: the panic that poisoned it already
        // happened, and refusing every later request helps nobody.
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn style_guard(&self) -> MutexGuard<'_, AccountCache<SessionStyle>> {
        self.styles.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Stores a proven token and answers the id to put in the cookie.
    pub fn insert(&self, mut session: Session) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let key = cache_key(&session.host, &session.account_id);
        if let Some(style) = session.style.as_ref() {
            let clean = style.clone().sanitized();
            self.style_guard().insert(key, clean.clone());
            session.style = Some(clean);
        } else if let Some(cached) = self.style_guard().get(&key).cloned() {
            session.style = Some(cached);
        }
        self.guard().insert(id.clone(), session);
        self.persist();
        id
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<Session> {
        self.guard().get(id).cloned()
    }

    pub fn remove(&self, id: &str) {
        self.guard().remove(id);
        self.persist();
    }

    /// Cached per-account session styling keyed by `(host, account_id)`.
    #[must_use]
    pub fn cached_style(&self, host: &str, account_id: &str) -> Option<SessionStyle> {
        self.style_guard()
            .get(&cache_key(host, account_id))
            .cloned()
    }

    /// Records sanitized session styling for `(host, account_id)` across the account cache
    /// and all matching active sessions.
    pub fn set_style(&self, host: &str, account_id: &str, style: SessionStyle) {
        let clean = style.sanitized();
        self.style_guard()
            .insert(cache_key(host, account_id), clean.clone());
        for session in self.guard().values_mut() {
            if session.host == host && session.account_id == account_id {
                session.style = Some(clean.clone());
            }
        }
        self.persist();
    }

    /// Records what the moderation probe answered for one account.
    ///
    /// Sessions made before this field existed carry `None`, and the first request after an
    /// upgrade asks the instance once and writes the answer down here, so the rail is
    /// complete on the page the reader is actually looking at.
    pub fn set_admin(&self, host: &str, acct: &str, admin: bool) {
        for session in self.guard().values_mut() {
            if session.host == host && session.acct == acct {
                session.admin = Some(admin);
            }
        }
        self.persist();
    }

    fn persist(&self) {
        let Some(path) = self.path.as_ref() else {
            return;
        };
        let snapshot = serde_json::to_vec_pretty(&*self.guard());
        match snapshot {
            Ok(bytes) => {
                if let Err(error) = std::fs::write(path, bytes) {
                    tracing::error!(%error, path = %path.display(), "could not write sessions");
                }
            }
            Err(error) => tracing::error!(%error, "could not serialise sessions"),
        }
    }
}
