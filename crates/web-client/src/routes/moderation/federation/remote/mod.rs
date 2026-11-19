//! Accounts on other instances this one silences or suspends.
//!
//! The list and the three verbs that move it live here; the row's layout, its status
//! sentence and its controls are markup. A row carries its own controls because the action,
//! the reason and the expiry all belong to the account in that row.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::pages::moderation::federation::{
    RemoteAccountDraft, RemoteAccountRow, RemoteAccountsPage, RemoteAction,
};

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// Reads one of the bridge's answers into the row the panel draws.
fn row_of(account: admin::RemoteAccount) -> RemoteAccountRow {
    RemoteAccountRow {
        action: RemoteAction::of(&account.action),
        actor_uri: account.actor_uri,
        handle: account.handle,
        domain: account.domain,
        reason: account.reason,
        expires_at: account.expires_at,
        expired: account.expired,
        created_at: account.created_at,
        created_by_handle: account.created_by_handle,
    }
}

/// Moderated remote accounts, each row applying or lifting its own action.
#[component]
pub(super) fn RemoteAccountList() -> impl IntoView {
    let alive = Alive::new();
    let (accounts, set_accounts) = signal(Vec::<RemoteAccountRow>::new());
    let (confirming, set_confirming) = signal(None::<String>);
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);

    let reload = Callback::new(move |()| {
        spawn_local(async move {
            let outcome = admin::remote_accounts().await;
            alive.if_alive(|| {
                match outcome {
                    Ok(list) => {
                        set_accounts.set(list.into_iter().map(row_of).collect());
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| reload.run(()));

    let apply = Callback::new(move |(actor, draft): (String, RemoteAccountDraft)| {
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            match admin::apply_remote_account(
                &actor,
                draft.action.wire(),
                &draft.reason,
                &draft.expires_at,
            )
            .await
            {
                Ok(account) => {
                    // The answer is the row as the instance now holds it, so the list is
                    // rewritten from it rather than re-read: a reader who just pressed
                    // Apply should not see the row blink while a second request runs.
                    let row = row_of(account);
                    alive.if_alive(move || {
                        set_accounts.update(move |held| {
                            match held.iter_mut().find(|held| held.actor_uri == row.actor_uri) {
                                Some(slot) => *slot = row,
                                None => held.push(row),
                            }
                        });
                        set_busy.set(false);
                    });
                }
                Err(message) => alive.if_alive(move || {
                    set_error.set(Some(message));
                    set_busy.set(false);
                }),
            }
        });
    });

    let ask = Callback::new(move |actor: String| set_confirming.set(Some(actor)));
    let cancel = Callback::new(move |()| set_confirming.set(None));

    let lift = Callback::new(move |actor: String| {
        set_busy.set(true);
        spawn_local(async move {
            let outcome = admin::lift_remote_account(&actor).await;
            alive.if_alive(move || {
                set_confirming.set(None);
                match outcome {
                    // A lifted action is gone from the list, because the list is what is
                    // being done, not what was ever done.
                    Ok(()) => {
                        set_accounts.update(|held| held.retain(|held| held.actor_uri != actor))
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    view! {
        <RemoteAccountsPage
            accounts=accounts
            confirming=confirming
            busy=busy
            error=error
            on_apply=apply
            on_ask=ask
            on_cancel=cancel
            on_lift=lift
            write=Some(admin::may_write())
        />
    }
}
