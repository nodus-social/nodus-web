//! The banners an account publishes for other readers: the one request that reads them and
//! the one that writes them.
//!
//! Both exist because the published document is not the owner's style: it is a narrow copy of
//! the three banner regions that lives on the account, so a visitor's browser can draw what
//! the account chose without seeing its navigation or rail. Everything here degrades to
//! "nothing to draw" rather than to an error a reader has to dismiss.

use std::cell::RefCell;
use std::collections::HashMap;

use leptos::prelude::*;
use nodus_ui::session_style::{PublishedStyle, SessionStyle};

thread_local! {
    /// The app's one banner reader. Created by [`install`], never inside a render.
    static READER: RefCell<Option<nodus_pages::profile::ProfileBanners>> = const { RefCell::new(None) };
    /// The reads in flight, keyed by the URL they fetch, each with the callbacks waiting.
    static IN_FLIGHT: RefCell<HashMap<String, Vec<Callback<Option<PublishedStyle>>>>> =
        RefCell::new(HashMap::new());
    /// The answer each URL already gave during this page load.
    static ANSWERS: RefCell<HashMap<String, Option<PublishedStyle>>> = RefCell::new(HashMap::new());
}

/// Creates the app-lifetime reader, before the first mount.
///
/// The reader must not be minted per render. Its callback carries `ProfilePage`'s "one read
/// per account" guard, so a fresh callback was a fresh empty guard, and every render of a
/// profile issued another `GET /api/v1/accounts/{id}/style_public` — measured 4, 3 and 4
/// across three consecutive loads, because the count tracked the number of renders. See
/// `docs/02-architecture.md` item 10: an app-lifetime value is created by an `install()`,
/// never lazily inside a render.
pub(crate) fn install() {
    READER.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = Some(nodus_pages::profile::ProfileBanners {
                load: Callback::new(load),
            });
        }
    });
}

/// The one reader every profile hands to `ProfilePage`.
///
/// # Panics
///
/// If [`install`] has not run. `boot.rs` calls it before the first mount.
pub(crate) fn reader() -> nodus_pages::profile::ProfileBanners {
    READER
        .with_borrow(|slot| *slot)
        .expect("profile_banners::install() must run before the first profile")
}

/// Starts one read of `account_id`'s published style, or joins the read already in flight.
///
/// A missing capability, a never-published account (404), a transport failure and a body
/// this build cannot read all end in `None`: a profile without banners is a normal profile,
/// so none of them deserves a request of its own or a sentence on screen. The capability
/// question is the caller's, not this read's: `ProfilePage` gates on the reactive answer and
/// only calls this when the instance advertises `style_public`.
///
/// The app shares one reader now, but a second caller can still ask for the same account
/// while the first read is out — a remount, or a second surface painting the same profile.
/// Without this map each of them issued its own identical request; the guard that only holds
/// while nobody calls twice is not a guard.
fn load((account_id, done): (String, Callback<Option<PublishedStyle>>)) {
    let Some((transport, url)) = address(&account_id) else {
        // No transport or no address: nothing to read, and nothing to report.
        done.try_run(None);
        return;
    };
    let key = url.to_string();
    // One read per account per page load, not one per mount. The route rebuilds the profile
    // as the SDK settles, and every rebuild starts a fresh `ProfilePage` with a fresh empty
    // "already asked" guard, so the reads arrive one after another rather than at once and
    // the in-flight map alone never sees them together: it was 4, 3 and 4 reads per load.
    // The answer is remembered here, and `publish` forgets it so an owner who just saved
    // reads back what they saved.
    if let Some(answer) = ANSWERS.with_borrow(|answers| answers.get(&key).cloned()) {
        done.try_run(answer);
        return;
    }
    let joined = IN_FLIGHT.with_borrow_mut(|flights| {
        if let Some(waiters) = flights.get_mut(&key) {
            waiters.push(done);
            true
        } else {
            flights.insert(key.clone(), vec![done]);
            false
        }
    });
    if joined {
        return;
    }
    leptos::task::spawn_local(async move {
        let answer = transport
            .fetch::<PublishedStyle>(transport.get(url))
            .await
            .ok();
        let waiters = IN_FLIGHT
            .with_borrow_mut(|flights| flights.remove(&key))
            .unwrap_or_default();
        ANSWERS.with_borrow_mut(|answers| answers.insert(key, answer.clone()));
        for waiter in waiters {
            // The page may be left while the read is in flight; its callback is disposed
            // with it, and calling a disposed callback panics.
            waiter.try_run(answer.clone());
        }
    });
}

/// One account's published style address, for the active instance.
fn address(account_id: &str) -> Option<(nodus_mastodon::BrowserInstance, url::Url)> {
    let transport = super::instance_transport()?;
    let url = transport
        .url_segments(&["api", "v1", "accounts", account_id, "style_public"])
        .ok()?;
    Some((transport, url))
}

/// Replace the account's published document with the narrow half of `style`.
///
/// Whole-document replacement is the endpoint's contract: a banner the owner removed must
/// leave the published copy too. An instance that does not advertise `style_public` is not
/// asked at all — the local save is the whole answer there, not a failure.
pub(crate) async fn publish(style: &SessionStyle) -> Result<(), String> {
    if !super::feature("style_public").get_untracked() {
        return Ok(());
    }
    let transport =
        super::instance_transport().ok_or_else(|| String::from("No active instance connection"))?;
    let url = transport
        .url("/api/v1/style/public")
        .map_err(|error| error.to_string())?;
    let body = serde_json::to_vec(&style.published()).map_err(|error| error.to_string())?;
    transport
        .send_empty(transport.put_body(url, body))
        .await
        .map_err(|error| format!("the instance refused the published document ({error}"))?;
    ANSWERS.with_borrow_mut(HashMap::clear);
    Ok(())
}
