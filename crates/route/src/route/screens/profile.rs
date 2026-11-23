//! A profile addressed by its permanent address: `/accounts/{host}/{name}`.
//!
//! The address names the instance that holds the account and the name that instance knows
//! it by, which is what makes it portable: an id is private numbering, a name is a handle.
//! A signed-in reader resolves the handle through their own instance (so follows, blocks
//! and mutes all apply), and the account that comes back is the local row for it — the
//! same row every other screen links to.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_client_core::model::Account;

use crate::components::CardRenderer;

use super::super::Runtime;
use super::public_account::PublicAccountScreen;

/// Whether `wanted` is the account the instance answered with.
///
/// Compared on the `acct`, because that is what both sides agree on: our instance may
/// print a remote account with its own id, but never with a different handle.
fn matches_handle(acct: &str, wanted: &str) -> bool {
    let left = acct.trim().trim_start_matches('@').to_lowercase();
    let right = wanted.trim().trim_start_matches('@').to_lowercase();
    left == right
}

#[component]
pub fn ProfileScreen(
    runtime: Arc<dyn Runtime>,
    host: String,
    name: String,
    card: CardRenderer,
) -> impl IntoView {
    let runtime_stored = StoredValue::new(runtime);
    let found = RwSignal::new(None::<Account>);
    let failed = RwSignal::new(false);
    let alive = crate::Alive::new();

    let wanted = format!("@{}@{}", name.trim_start_matches('@'), host);
    let search_term = wanted.clone();
    Effect::new(move |_| {
        if found.get_untracked().is_some() || failed.get_untracked() {
            return;
        }
        let wanted = search_term.clone();
        let match_against = wanted.clone();
        let done = Callback::new(move |result: Result<Vec<Account>, String>| {
            alive.if_alive(|| match result {
                Ok(accounts) => match accounts
                    .iter()
                    .find(|account| matches_handle(&account.acct, &match_against))
                {
                    Some(account) => found.set(Some(account.clone())),
                    // A name that resolves to nothing is not an error to shout about: the
                    // account may simply be unknown here, so the visitor path takes over.
                    None => failed.set(true),
                },
                Err(_) => failed.set(true),
            });
        });
        runtime_stored
            .get_value()
            .search_accounts(wanted.clone(), done);
    });

    // A visitor path, and a safety net for a name this instance has not heard of: the
    // account's own instance is asked directly instead of being guessed at.
    let fallback_card = card.clone();
    let fallback_host = host.clone();
    let fallback_name = name.clone();
    let fallback = move || {
        view! {
            <PublicAccountScreen
                runtime=runtime_stored.get_value()
                host=fallback_host.clone()
                id=fallback_name.clone()
                card=fallback_card.clone()
            />
        }
        .into_any()
    };

    view! {
        {move || match found.get() {
            Some(account) => {
                view! {
                    <super::account::AccountScreen
                        runtime=runtime_stored.get_value()
                        id=account.id.clone()
                        card=card.clone()
                    />
                }
                    .into_any()
            }
            None if failed.get() => fallback().into_any(),
            None => {
                view! {
                    <div class="flex flex-col gap-3">
                        <p class="text-sm opacity-70">"Looking this account up on your instance…"</p>
                    </div>
                }
                    .into_any()
            }
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::matches_handle;

    #[test]
    fn a_handle_is_compared_without_case_or_an_at_sign() {
        assert!(matches_handle(
            "gargron@mastodon.social",
            "@Gargron@mastodon.social"
        ));
        assert!(matches_handle("@luxuride", "luxuride"));
        assert!(!matches_handle("luxuride", "someone-else"));
        assert!(!matches_handle(
            "luxuride@nodus.luxuride.eu",
            "luxuride@mastodon.social"
        ));
    }
}
