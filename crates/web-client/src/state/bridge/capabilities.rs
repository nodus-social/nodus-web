//! The one capability answer the shared screens gate on.
//!
//! `/api/v1/meta` is the only capability list any server publishes; the SDK probes it once
//! per instance and answers the other two protocols from its probed baseline. This module
//! turns that into the value the views ask, so the gate itself lives in `nodus-ui` and reads
//! the same answer on both paints.
//!
//! One correction is made here and only here: `load_capabilities` falls back to the Mastodon
//! baseline when no meta document answers, which is the compatibility floor, but a Misskey
//! session is not that floor — reading it as Mastodon would hide the drive, the account
//! deletion and the reactions that instance really serves.
//!
//! ## The server already answered
//!
//! A page the server painted carries the same answer in `#nodus-page`, under
//! `capabilities`. Reading it before the probe means the first client render gates on the
//! answer the first paint already used, and the one read of `/api/v1/meta` is skipped
//! rather than repeated. A page without the island (a static deploy, a client-side route,
//! an answer that failed server-side) still negotiates, which is the fallback.

use leptos::prelude::*;
use nodus_client_sdk::protocol::Protocol;
use nodus_ui::capabilities::{Capabilities, Surface};

use super::provider;

thread_local! {
    /// The island's answer, read once. `Some(None)` records that this document had none,
    /// so a static page does not re-parse the DOM on every read.
    static SEEDED: std::cell::RefCell<Option<Option<Capabilities>>> =
        const { std::cell::RefCell::new(None) };
}

/// The capability answer the server painted into this document, if it painted one.
///
/// Public to the bridge so `build_live` can skip the negotiation the island already
/// answered; the read is lazy because the island is read before the mount replaces the
/// node that holds it.
pub(crate) fn seeded() -> Option<Capabilities> {
    SEEDED.with(|slot| slot.borrow_mut().get_or_insert_with(read_island).clone())
}

/// Reads `#nodus-page`'s `capabilities` value into the typed answer.
fn read_island() -> Option<Capabilities> {
    let document = web_sys::window()?.document()?;
    let text = document.get_element_by_id("nodus-page")?.text_content()?;
    seed_from(&serde_json::from_str(&text).ok()?)
}

/// The answer inside a parsed island, if it carries one.
///
/// The value is the server's own serialized [`Capabilities`], so `provider` and `contract`
/// survive beside the list; a page from an older server has no such key and seeds nothing.
fn seed_from(island: &serde_json::Value) -> Option<Capabilities> {
    serde_json::from_value(island.get("capabilities")?.clone()).ok()
}

/// The island's answer when the page carried one, the negotiated one otherwise.
fn prefer(seed: Option<Capabilities>, negotiated: Capabilities) -> Capabilities {
    seed.unwrap_or(negotiated)
}

/// What this session's server serves, as the screens read it.
///
/// Derived from the store's own revision and the negotiated protocol, so it refreshes when
/// the session changes (a switch rebuilds the SDK and its store) and when the probe lands,
/// without a second read of `/api/v1/meta` anywhere. A capability answer painted into the
/// document is preferred over both: the first paint used it, and the first client render
/// must agree with it.
#[must_use]
pub fn serves() -> Signal<Capabilities> {
    let revision = super::revision();
    let extensions = super::extensions_revision();
    let protocol = provider::protocol();
    let seeded = seeded();
    Signal::derive(move || {
        revision.get();
        extensions.get();
        let negotiated = super::with_sdk(|sdk| sdk.state().capabilities.get());
        // A Nodus answer is authoritative whatever the dialect probe said: it published a
        // list, and only our API publishes one.
        let answered = if negotiated.knows_extensions() {
            negotiated
        } else {
            match protocol.get() {
                Protocol::Misskey => Capabilities::misskey(),
                Protocol::Mastodon | Protocol::Nodus => negotiated,
            }
        };
        prefer(seeded.clone(), answered)
    })
}

/// Whether this session's server serves one surface, alone, so a control can be drawn from
/// it without reading the whole answer.
#[must_use]
pub fn served(surface: Surface) -> Signal<bool> {
    let at = serves();
    Signal::derive(move || surface.served(&at.get()))
}

/// The one sentence a failed command should show instead of the transport's own text.
#[must_use]
pub fn explained(error: &str) -> String {
    nodus_ui::capabilities::explain(Some(error.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_islands_answer_wins_over_a_probe() {
        let island = serde_json::json!({
            "capabilities": {
                "contract": "0.1.0",
                "provider": "nodus",
                "features": ["admin", "moderation", "drive", "feed_accounts"],
                "unknown": []
            }
        });
        let seed = seed_from(&island).expect("the island names the server's features");
        let probed = Capabilities::mastodon();
        assert!(!probed.supports(nodus_ui::capabilities::Feature::Admin));
        assert!(prefer(Some(seed), probed).supports(nodus_ui::capabilities::Feature::Admin));
    }

    #[test]
    fn a_page_without_an_answer_keeps_the_probe() {
        let island = serde_json::json!({ "which": "home", "statuses": [] });
        assert!(seed_from(&island).is_none());
    }
}
