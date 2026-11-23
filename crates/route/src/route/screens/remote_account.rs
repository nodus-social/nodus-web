//! A profile link that came from another instance, resolved to this reader's own copy.
//!
//! A link a visitor follows — `/accounts/{host}/{id}` — carries the id the *other* instance
//! printed, and no instance agrees with another about what an account is called. A visitor
//! gets the read-only page they came for. A reader who is signed in should get the profile
//! they can act on: same person, Follow and Mute and Block included. So the handle is looked
//! up in this instance first (`search` resolves and remembers it), and the screen is then
//! built from the local id. When the instance cannot reach or does not know the account, the
//! visitor's page is what the reader gets, which is still the right answer for a link nobody
//! here has met before.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_client_core::model::Account;

use crate::components::CardRenderer;

use super::super::Runtime;
use super::account::AccountScreen;
use super::public_account::PublicAccountScreen;

/// Whether a stored account is the handle we are looking for.
///
/// The handle in the search answer carries no leading `@`, and case is not part of an
/// identity, so the comparison is by lowercase text after trimming one.
fn matches_handle(acct: &str, wanted: &str) -> bool {
    acct.trim_start_matches('@').to_lowercase() == wanted.trim_start_matches('@').to_lowercase()
}

/// The handle an account carries, qualified with the instance that printed it.
///
/// An instance names its own people without a host (`lux`), which is not a handle anybody
/// else can look up — and worse, it is a name this instance may well have given someone
/// else. The host comes from the link, so the two together name the same person.
fn qualified_handle(acct: &str, host: &str) -> String {
    let trimmed = acct.trim().trim_start_matches('@');
    if trimmed.is_empty() || trimmed.contains('@') {
        return trimmed.to_owned();
    }
    format!("{trimmed}@{host}")
}

/// A profile reached by a link from another instance.
#[component]
pub fn RemoteAccountScreen(
    runtime: Arc<dyn Runtime>,
    /// Instance that printed the id in the link.
    host: String,
    /// The id as that instance printed it.
    id: String,
    card: CardRenderer,
) -> impl IntoView {
    let runtime = StoredValue::new(runtime);
    let card = StoredValue::new(card);
    // The id this instance knows the account by, once it has been resolved.
    let local = RwSignal::new(None::<String>);
    // Set when the account cannot be had locally, which sends the reader to the public page.
    let fallback = RwSignal::new(false);
    let notice = RwSignal::new(None::<String>);
    let alive = crate::Alive::new();

    let load_host = host.clone();
    let load_id = id.clone();
    Effect::new(move |_| {
        if local.get_untracked().is_some() || fallback.get_untracked() {
            return;
        }
        let handle_host = load_host.clone();
        let done = Callback::new(move |result: Result<Account, String>| {
            let Ok(account) = result else {
                // The origin instance is unreachable or the id is gone: there is nothing to
                // resolve, and the visitor's page will explain itself.
                fallback.set(true);
                return;
            };
            let handle = qualified_handle(&account.acct, &handle_host);
            let wanted = handle.clone();
            let done = Callback::new(move |result: Result<Vec<Account>, String>| {
                alive.if_alive(|| match result {
                    Ok(found) => match found
                        .into_iter()
                        .find(|held| matches_handle(&held.acct, &wanted))
                    {
                        Some(held) => local.set(Some(held.id)),
                        None => fallback.set(true),
                    },
                    Err(message) => {
                        notice.set(Some(message));
                        fallback.set(true);
                    }
                });
            });
            runtime.get_value().search_accounts(handle, done);
        });
        runtime
            .get_value()
            .load_public_account(load_host.clone(), load_id.clone(), done);
    });

    let fallback_host = host.clone();
    let fallback_id = id.clone();
    view! {
        {move || match (local.get(), fallback.get()) {
            (Some(id), _) => view! {
                <AccountScreen runtime=runtime.get_value() id card=card.get_value()/>
            }
            .into_any(),
            (None, true) => view! {
                <PublicAccountScreen
                    runtime=runtime.get_value()
                    host=fallback_host.clone()
                    id=fallback_id.clone()
                    card=card.get_value()
                />
            }
            .into_any(),
            (None, false) => view! {
                <crate::Screen>
                    <crate::Note>
                        {move || {
                            notice
                                .get()
                                .unwrap_or_else(|| {
                                    "Looking this account up on your instance…".to_owned()
                                })
                        }}
                    </crate::Note>
                </crate::Screen>
            }
            .into_any(),
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::{matches_handle, qualified_handle};

    #[test]
    fn a_local_name_from_another_instance_becomes_a_handle_anyone_can_look_up() {
        assert_eq!(
            qualified_handle("lux", "mastodon.luxuride.eu"),
            "lux@mastodon.luxuride.eu"
        );
        assert_eq!(
            qualified_handle("@lux", "mastodon.luxuride.eu"),
            "lux@mastodon.luxuride.eu"
        );
        // Already qualified: the host in the link is not the one to append.
        assert_eq!(
            qualified_handle("tom@baraag.net", "baraag.net"),
            "tom@baraag.net"
        );
        assert_eq!(qualified_handle("", "baraag.net"), "");
    }

    #[test]
    fn a_handle_is_compared_without_case_or_an_at_sign() {
        assert!(matches_handle(
            "Tomostrawberry@baraag.net",
            "@tomostrawberry@baraag.net"
        ));
        assert!(matches_handle(
            "tomostrawberry@baraag.net",
            "Tomostrawberry@baraag.net"
        ));
        assert!(matches_handle("@luxuride", "luxuride"));
        assert!(!matches_handle("luxuride@nodus.luxuride.eu", "luxuride"));
        assert!(!matches_handle(
            "Tomostrawberry@baraag.net",
            "Tomostrawberry@mastodon.social"
        ));
    }
}
