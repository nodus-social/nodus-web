//! Domains this instance refuses, and accounts on other instances it silences or suspends.
//!
//! The outbound half of federation lives under Administration (`relays` and allowed
//! domains), because allowing a domain is a decision about the network, while blocking
//! one is a decision about what the people here have to see. Moderating a single account on
//! a peer is the same decision at a smaller grain, so it sits beside the domain list.

use leptos::prelude::*;

use crate::state::bridge::feature;

mod blocked;
mod remote;

use blocked::BlockedList;
use remote::RemoteAccountList;

#[component]
pub fn Federation() -> impl IntoView {
    // The account panel is a Nodus extension like the inbound-block panel; an instance that
    // advertises either `remote_blocks` or `moderation` serves it. Both names already exist in
    // the contract, and a new name would be a door no instance has been taught to open.
    let remote_blocks = feature("remote_blocks");
    let moderation = feature("moderation");
    view! {
        <BlockedList/>
        <Show when=move || remote_blocks.get() || moderation.get()>
            <RemoteAccountList/>
        </Show>
    }
}
