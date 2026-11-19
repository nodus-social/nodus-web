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
    // The account panel is its own capability: `remote_accounts` names it, so an instance
    // that advertises that name — and only that one — serves it.
    let remote_accounts = feature("remote_accounts");
    view! {
        <BlockedList/>
        <Show when=move || remote_accounts.get()>
            <RemoteAccountList/>
        </Show>
    }
}
