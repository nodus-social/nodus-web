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

use leptos::prelude::*;
use nodus_client_sdk::protocol::Protocol;
use nodus_ui::capabilities::{Capabilities, Surface};

use super::provider;

/// What this session's server serves, as the screens read it.
///
/// Derived from the store's own revision and the negotiated protocol, so it refreshes when
/// the session changes (a switch rebuilds the SDK and its store) and when the probe lands,
/// without a second read of `/api/v1/meta` anywhere.
#[must_use]
pub fn serves() -> Signal<Capabilities> {
    let revision = super::revision();
    let extensions = super::extensions_revision();
    let protocol = provider::protocol();
    Signal::derive(move || {
        revision.get();
        extensions.get();
        let negotiated = super::with_sdk(|sdk| sdk.state().capabilities.get());
        // A Nodus answer is authoritative whatever the dialect probe said: it published a
        // list, and only our API publishes one.
        if negotiated.knows_extensions() {
            return negotiated;
        }
        match protocol.get() {
            Protocol::Misskey => Capabilities::misskey(),
            Protocol::Mastodon | Protocol::Nodus => negotiated,
        }
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
