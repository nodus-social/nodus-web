//! The capability answer a server render can know before it paints.
//!
//! The browser negotiates with `/api/v1/meta` and keeps the result in a signal. The server
//! has the same facts at a different moment: the dialect was proved at sign-in
//! (`Session::protocol`), and a Nodus instance publishes the same list the browser reads.
//! This module turns those two facts into the one [`Capabilities`] value the shared frame
//! gates on, so a server-painted rail and the rail the browser draws after hydration are
//! decided by one rule rather than two.
//!
//! ## Cost
//!
//! Mastodon and Misskey are answered from the SDK's baseline and cost nothing. A Nodus
//! answer is one public read of `/api/v1/meta`, cached for the life of the process and keyed
//! by host: the list changes when the instance is upgraded, not per request. A failed read
//! is cached for [`RETRY_AFTER`] so a broken endpoint is retried occasionally instead of on
//! every paint (and logged once), and until it answers the page is painted permissively.
//!
//! ## What "no answer" means here
//!
//! [`answer_for`] returns `None` when nobody is signed in and also when the one read failed.
//! The first is the public page that never had an account to ask about; the second is a
//! signed-in page whose answer is unknown for this render. Both paint the doors they always
//! drew, which is why the shared gate treats `None` as unknown rather than absent. A known
//! answer that omits a feature is `Some(Capabilities)` without that bit, and hides the door.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use nodus_ui::capabilities::Capabilities;

use crate::protocol::Protocol;
use crate::session::Session;

/// How long a failed meta read is remembered before another attempt is made.
///
/// Short enough that a restart of the instance is noticed without restarting this process,
/// long enough that an unreachable host does not pay a timeout on every request.
const RETRY_AFTER: Duration = Duration::from_secs(30);

/// The deadline for the one meta read; it is a page render, not a background job.
const META_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone)]
struct Cached {
    /// `None` records a failed read. The timestamp is when that answer was checked.
    at: Option<Capabilities>,
    checked: Instant,
}

#[derive(Debug, Default)]
struct Answers {
    by_host: HashMap<String, Cached>,
    /// Hosts already warned about, so an endpoint that stays down logs once rather than
    /// once per reader.
    warned: HashSet<String>,
}

fn answers() -> MutexGuard<'static, Answers> {
    static ANSWERS: LazyLock<Mutex<Answers>> = LazyLock::new(|| Mutex::new(Answers::default()));
    // A panic while holding this map leaves no invariant broken: the entries are plain
    // answers, and refusing every later render would be worse than reusing them.
    ANSWERS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn key(host: &str) -> String {
    host.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// The answer for a signed-in session, reading `/api/v1/meta` once for a Nodus host.
///
/// `None` is "no answer this render" — either the host cannot be reached or its document
/// could not be read. The caller keeps painting; it does not turn the failure into a page.
pub(crate) async fn resolve(session: &Session) -> Option<Capabilities> {
    let host = key(&session.host);
    if let Some(cached) = cached(&host) {
        return cached;
    }
    let at = match session.protocol {
        Protocol::Mastodon => Some(Capabilities::mastodon()),
        Protocol::Misskey => Some(Capabilities::misskey()),
        Protocol::Nodus => read_meta(session).await,
    };
    if let Some(at) = at.as_ref() {
        remember(&host, at.clone());
        remember_account(&session.acct, at.clone());
    }
    at
}

/// The answer a painted page should gate on, looked up by the reader's host.
///
/// This is a synchronous read because the document layer paints inside a component owner,
/// not inside an async function. The value was put here by [`resolve`] before the paint.
pub(crate) fn answer_for(host: &str) -> Option<Capabilities> {
    cached(&key(host)).flatten()
}

fn cached(host: &str) -> Option<Option<Capabilities>> {
    let mut answers = answers();
    let cached = answers.by_host.get(host)?;
    if cached.at.is_some() || cached.checked.elapsed() < RETRY_AFTER {
        return Some(cached.at.clone());
    }
    answers.by_host.remove(host);
    None
}

fn remember(host: &str, at: Capabilities) {
    answers().by_host.insert(
        host.to_owned(),
        Cached {
            at: Some(at),
            checked: Instant::now(),
        },
    );
}

/// Cache the same answer under the account's own host too.
///
/// The document layer builds a [`crate::document::Reader`] from the account handle for some
/// pages, so its host is the handle's instance rather than the session's normalized host.
/// For a local account those are the same; for a remote one they are not, and both spellings
/// should find the answer the session already has.
fn remember_account(acct: &str, at: Capabilities) {
    if let Some((_, host)) = acct.rsplit_once('@') {
        let host = key(host);
        if !host.is_empty() {
            remember(&host, at);
        }
    }
}

fn failed(host: &str, reason: &str) {
    let first = answers().warned.insert(host.to_owned());
    if first {
        tracing::warn!(
            %host,
            %reason,
            "could not read /api/v1/meta; painting capability doors permissively"
        );
    }
    answers().by_host.insert(
        host.to_owned(),
        Cached {
            at: None,
            checked: Instant::now(),
        },
    );
}

async fn read_meta(session: &Session) -> Option<Capabilities> {
    let host = key(&session.host);
    let client = match crate::mastodon::Client::new(&session.host, "") {
        Ok(client) => client,
        Err(reason) => {
            failed(&host, &reason);
            return None;
        }
    };
    let http = match crate::validate::shared_public_http_client() {
        Ok(http) => http,
        Err(error) => {
            failed(&host, &error.to_string());
            return None;
        }
    };
    let response = http
        .get(format!("{}/api/v1/meta", client.base()))
        .timeout(META_TIMEOUT)
        .send()
        .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            failed(&host, &error.to_string());
            return None;
        }
    };
    if !response.status().is_success() {
        failed(&host, &format!("the host answered {}", response.status()));
        return None;
    }
    let document = match response.json::<serde_json::Value>().await {
        Ok(document) => document,
        Err(error) => {
            failed(&host, &error.to_string());
            return None;
        }
    };
    match Capabilities::from_meta(&document) {
        Ok(at) => Some(at),
        Err(error) => {
            failed(&host, &error.to_string());
            None
        }
    }
}

#[cfg(test)]
pub(crate) fn remember_for_test(host: &str, at: Capabilities) {
    remember(&key(host), at);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cached_success_is_returned_without_asking_the_host() {
        let host = "cache-success.test";
        let at = Capabilities::from_parts("0.1.0", &["admin".to_owned()]);
        remember_for_test(host, at.clone());
        assert_eq!(answer_for(host), Some(at));
    }

    #[test]
    fn a_failed_answer_expires_and_is_not_a_permanent_verdict() {
        let host = "cache-failure.test";
        failed(host, "test failure");
        assert_eq!(answer_for(host), None);
        {
            let mut answers = answers();
            if let Some(cached) = answers.by_host.get_mut(host) {
                cached.checked = Instant::now() - RETRY_AFTER;
            }
        }
        assert!(
            cached(host).is_none(),
            "an expired failure should be retried"
        );
    }
}
