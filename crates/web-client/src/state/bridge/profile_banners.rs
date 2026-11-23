//! The banners an account publishes for other readers: the one request that reads them and
//! the one that writes them.
//!
//! Both exist because the published document is not the owner's style: it is a narrow copy of
//! the three banner regions that lives on the account, so a visitor's browser can draw what
//! the account chose without seeing its navigation or rail. Everything here degrades to
//! "nothing to draw" rather than to an error a reader has to dismiss.

use leptos::prelude::*;
use nodus_ui::session_style::{PublishedStyle, SessionStyle};

/// The reader a profile page calls, one account at a time.
pub(crate) fn reader() -> nodus_pages::profile::ProfileBanners {
    nodus_pages::profile::ProfileBanners {
        load: Callback::new(
            |(account_id, done): (String, Callback<Option<PublishedStyle>>)| {
                leptos::task::spawn_local(async move {
                    done.run(read(&account_id).await);
                });
            },
        ),
    }
}

/// One account's published style, or `None` when there is nothing honest to draw.
///
/// A missing capability, a never-published account (404), a transport failure and a body
/// this build cannot read all end here: a profile without banners is a normal profile, so
/// none of them deserves a request of its own or a sentence on screen.
async fn read(account_id: &str) -> Option<PublishedStyle> {
    if !super::feature("style_public").get_untracked() {
        return None;
    }
    let transport = super::instance_transport()?;
    let url = transport
        .url_segments(&["api", "v1", "accounts", account_id, "style_public"])
        .ok()?;
    transport
        .fetch::<PublishedStyle>(transport.get(url))
        .await
        .ok()
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
        .map_err(|error| format!("the instance refused the published document ({error})"))
}
