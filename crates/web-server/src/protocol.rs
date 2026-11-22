//! Which dialect the reader's server speaks, and how a token is proven in it.
//!
//! The server used to assume Mastodon and prove it through `/api/v1/accounts/verify_credentials`.
//! A Misskey instance answers `404` there and `404` for `/api/v1/instance`, so a Misskey reader
//! could not hold a session at all — the browser was the only place they existed. Detection is
//! the SDK's (`Probe::detect`), not a second guess written here, so the server and the browser
//! agree on what a server is; this module only supplies the transport and the one call each
//! dialect needs to turn a token into a fact.
//!
//! The transport is passed in rather than built here: the server already keeps one shared
//! `reqwest::Client` for public reads (`validate::shared_public_http_client`), and a test can
//! hand in a client pointed at a stub instead of the network.

use std::time::Duration;

use nodus_client_sdk::protocol::{Probe, Protocol as SdkProtocol};
use serde::{Deserialize, Serialize};

/// How long any one probe document may take.
///
/// Detection runs inside sign-in, which the reader is waiting on. A host that accepts the
/// connection and then says nothing must not hold the form open, so each document gets its
/// own deadline and silence counts as absence.
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);

/// The dialect a session was proven in, as the session store persists it.
///
/// Deliberately not the SDK's `Protocol`: this type is serialised into `NODUS_SESSIONS` and
/// must keep parsing when the SDK adds a dialect, so a value this build has never heard of
/// reads as the compatibility floor rather than failing the whole file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// Nodus: Mastodon's endpoints plus our own under `/api/v1/nodus/*`.
    Nodus,
    /// Mastodon and anything that speaks its client API.
    #[default]
    Mastodon,
    /// Misskey: `/api/*`, token in the body.
    Misskey,
}

impl Protocol {
    /// The name used in logs, state and the session view.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nodus => "nodus",
            Self::Mastodon => "mastodon",
            Self::Misskey => "misskey",
        }
    }

    /// Whether the server reads through Mastodon's client API (`/api/v1/*`, bearer token).
    ///
    /// This is what the SSR reads are built on, so it is also the honest answer to "can this
    /// process paint this reader's timeline?". Nodus answers the same shapes; Misskey does
    /// not, and a page that pretended otherwise would paint an error as content.
    #[must_use]
    pub const fn reads_mastodon_api(self) -> bool {
        !matches!(self, Self::Misskey)
    }
}

impl From<SdkProtocol> for Protocol {
    fn from(protocol: SdkProtocol) -> Self {
        match protocol {
            SdkProtocol::Nodus => Self::Nodus,
            SdkProtocol::Mastodon => Self::Mastodon,
            SdkProtocol::Misskey => Self::Misskey,
        }
    }
}

/// What a proven token turned out to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The dialect the proof was made in.
    pub protocol: Protocol,
    /// The account's id on its own server.
    pub account_id: String,
    /// `user@host`, as the account names itself in a handle.
    pub acct: String,
    /// What it calls itself; empty when it never set one.
    pub display_name: String,
    /// Avatar URL, kept so a painted rail can draw the reader before any wasm runs.
    pub avatar: String,
}

/// The dialect `base` speaks, or `None` when no document answered.
///
/// `None` is not an error: it means "nothing recognisable", and the caller keeps the
/// compatibility floor it already relied on.
pub async fn detect_with(http: &reqwest::Client, base: &str) -> Option<Protocol> {
    let base = base.trim_end_matches('/');
    let nodus = document(http, &format!("{base}/api/v1/nodus/capabilities")).await;
    let misskey = document(http, &format!("{base}/api/meta")).await;
    let mastodon = document(http, &format!("{base}/api/v2/instance")).await;
    Probe {
        nodus: nodus.as_ref(),
        mastodon: mastodon.as_ref(),
        misskey: misskey.as_ref(),
    }
    .detect()
    .map(Protocol::from)
}

/// Prove `token` in `protocol` and answer what it names.
///
/// A refusal is the server's own answer, kept as text: the sign-in form shows it, and
/// "unauthorised" without the instance's reason is the message a reader cannot act on.
pub async fn verify_with(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    protocol: Protocol,
) -> Result<Verified, String> {
    match protocol {
        Protocol::Misskey => verify_misskey(http, base, token).await,
        Protocol::Mastodon | Protocol::Nodus => verify_mastodon(http, base, token, protocol).await,
    }
}

/// Misskey: `POST /api/i` with the token as the body's `i`, answering the account.
async fn verify_misskey(
    http: &reqwest::Client,
    base: &str,
    token: &str,
) -> Result<Verified, String> {
    #[derive(Deserialize)]
    struct Me {
        id: String,
        username: String,
        #[serde(default)]
        name: Option<String>,
        /// Set when the account is remote; empty for a local one.
        #[serde(default)]
        host: Option<String>,
        /// Misskey spells this `avatarUrl`; the field name here stays snake_case.
        #[serde(default, rename = "avatarUrl")]
        avatar_url: Option<String>,
    }

    let url = format!("{}/api/i", base.trim_end_matches('/'));
    let response = http
        .post(&url)
        .timeout(PROBE_TIMEOUT)
        .json(&serde_json::json!({ "i": token }))
        .send()
        .await
        .map_err(|error| format!("Could not reach {base}: {error}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "{base} refused this token ({} for /api/i)",
            status.as_u16()
        ));
    }
    let me: Me = serde_json::from_str(&body)
        .map_err(|error| format!("{base} answered /api/i with something unreadable: {error}"))?;
    let host = me
        .host
        .filter(|host| !host.trim().is_empty())
        .or_else(|| host_of(base))
        .unwrap_or_default();
    Ok(Verified {
        protocol: Protocol::Misskey,
        account_id: me.id,
        acct: if host.is_empty() {
            me.username
        } else {
            format!("{}@{host}", me.username)
        },
        display_name: me.name.unwrap_or_default(),
        avatar: me.avatar_url.unwrap_or_default(),
    })
}

/// Mastodon and Nodus: `GET /api/v1/accounts/verify_credentials`, bearer token.
async fn verify_mastodon(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    protocol: Protocol,
) -> Result<Verified, String> {
    #[derive(Deserialize)]
    struct Me {
        id: String,
        acct: String,
        #[serde(default)]
        display_name: String,
        #[serde(default)]
        avatar: String,
    }

    let url = format!(
        "{}/api/v1/accounts/verify_credentials",
        base.trim_end_matches('/')
    );
    let response = http
        .get(&url)
        .timeout(PROBE_TIMEOUT)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| format!("Could not reach {base}: {error}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "{base} refused this token ({} for /api/v1/accounts/verify_credentials)",
            status.as_u16()
        ));
    }
    let me: Me = serde_json::from_str(&body).map_err(|error| {
        format!("{base} answered verify_credentials with something unreadable: {error}")
    })?;
    Ok(Verified {
        protocol,
        account_id: me.id,
        acct: me.acct,
        display_name: me.display_name,
        avatar: me.avatar,
    })
}

/// One probe document, or `None` when it did not answer successfully.
async fn document(http: &reqwest::Client, url: &str) -> Option<serde_json::Value> {
    let response = http.get(url).timeout(PROBE_TIMEOUT).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

/// The host a base URL names, for a Misskey account that did not report its own.
///
/// The port is kept when there is one: an instance on `:3000` names its accounts
/// `user@host:3000`, and dropping it would print a handle that does not resolve.
fn host_of(base: &str) -> Option<String> {
    let url = url::Url::parse(base).ok()?;
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

// The tests live beside the crate rather than inside it: they stand up stub instances, and
// that is more lines than the module they prove.
#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
