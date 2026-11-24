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
///
/// A local account is the one place the two sides spell the same person differently: our
/// instance answers with the bare name (`admin`), while the address that named it carries
/// the instance too (`@admin@nodus.test`). So a bare `acct` also matches that name at
/// `local_host`, and only there — a remote account still has to carry its own host.
fn matches_handle(acct: &str, wanted: &str, local_host: Option<&str>) -> bool {
    let left = normalize_handle(acct);
    let right = normalize_handle(wanted);
    if left == right {
        return true;
    }
    let Some((name, host)) = right.split_once('@') else {
        return false;
    };
    // An `acct` that carries a host is a remote one: a bare name is not its identity, and
    // the address only names the same person when it names that same host.
    !left.contains('@')
        && left == name
        && local_host.is_some_and(|mine| normalize_handle(mine) == host)
}

/// A handle with the leading `@` and the case taken off, which are not part of an identity.
fn normalize_handle(handle: &str) -> String {
    handle.trim().trim_start_matches('@').to_lowercase()
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
    // The instance that may answer a local account with a bare name: the reader's own.
    let own_host = runtime_stored.get_value().session_host();
    Effect::new(move |_| {
        if found.get_untracked().is_some() || failed.get_untracked() {
            return;
        }
        let wanted = search_term.clone();
        let match_against = wanted.clone();
        let own_host = own_host.clone();
        let done = Callback::new(move |result: Result<Vec<Account>, String>| {
            alive.if_alive(|| match result {
                Ok(accounts) => match accounts.iter().find(|account| {
                    matches_handle(&account.acct, &match_against, own_host.as_deref())
                }) {
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
            "@Gargron@mastodon.social",
            None
        ));
        assert!(matches_handle("@luxuride", "luxuride", None));
        assert!(!matches_handle("luxuride", "someone-else", None));
        assert!(!matches_handle(
            "luxuride@nodus.luxuride.eu",
            "luxuride@mastodon.social",
            None
        ));
    }

    /// The instance answers a local account with its bare name, while the address that
    /// named it carries this instance's host. Those are the same person.
    #[test]
    fn a_bare_acct_matches_its_own_host() {
        assert!(matches_handle(
            "admin",
            "@admin@nodus.test",
            Some("nodus.test")
        ));
        // Case and a leading `@` are not part of an identity, on either side.
        assert!(matches_handle(
            "@Admin",
            "@admin@NODUS.test",
            Some("nodus.test")
        ));
    }

    /// A bare name only means a local account. The same name at another instance is a
    /// different person, and the instance that holds them answers with a qualified `acct`.
    #[test]
    fn a_bare_acct_does_not_match_another_host() {
        assert!(!matches_handle(
            "admin",
            "@admin@mastodon.social",
            Some("nodus.test")
        ));
        // Without a session there is no instance to be local to, so nothing is local.
        assert!(!matches_handle("admin", "@admin@nodus.test", None));
        assert!(!matches_handle(
            "someone-else",
            "@admin@nodus.test",
            Some("nodus.test")
        ));
    }

    /// A remote account is identified by its whole handle, so the host is required.
    #[test]
    fn a_remote_acct_matches_only_with_its_host() {
        assert!(matches_handle(
            "gargron@mastodon.social",
            "@gargron@mastodon.social",
            Some("nodus.test")
        ));
        assert!(!matches_handle(
            "gargron@mastodon.social",
            "@gargron@nodus.test",
            Some("nodus.test")
        ));
        assert!(!matches_handle(
            "gargron@mastodon.social",
            "@gargron",
            Some("nodus.test")
        ));
    }
}
