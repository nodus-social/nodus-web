//! The sessions settings tab: who is signed in, read from the instance.
//!
//! The card is shared markup; this module is the clock, the request and the row mapping.
//! Timestamps become ages here because the server-side paint has no clock of its own,
//! and a revoke drops the row locally: the instance already answered 204, so asking for
//! the list again would only make the reader wait to see what they just did.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::AuthSession;
use nodus_pages::pages::settings::sessions::{
    SessionItem, SessionsCard as SharedSessionsCard, describe_agent,
};

use crate::routes::format::relative_time;
use crate::state::alive::Alive;
use crate::state::bridge::actions;

/// An age in the words the rest of the client uses, or the fact that the stamp is unreadable.
fn age(stamp: &str) -> String {
    let age = relative_time(stamp);
    if age.is_empty() {
        "at an unknown time".to_owned()
    } else {
        format!("{age} ago")
    }
}

fn item(session: AuthSession) -> SessionItem {
    SessionItem {
        id: session.id,
        client: describe_agent(&session.user_agent),
        created: age(&session.created_at),
        last_used: session
            .last_used_at
            .as_deref()
            .map_or_else(|| "never".to_owned(), age),
        network: session
            .ip
            .unwrap_or_else(|| "Network not recorded".to_owned()),
        current: session.current,
    }
}

/// Every session signed in to the account, with revoke controls.
#[component]
pub fn SessionsCard() -> impl IntoView {
    let alive = Alive::new();
    let (sessions, set_sessions) = signal(Vec::<SessionItem>::new());
    let (busy, set_busy) = signal(false);
    let (loaded, set_loaded) = signal(false);
    let (error, set_error) = signal(None::<String>);

    let read = Callback::new(move |()| {
        if busy.get_untracked() {
            return;
        }
        set_busy.set(true);
        let alive = alive;
        spawn_local(async move {
            let outcome = actions::sessions().await;
            alive.if_alive(|| {
                set_busy.set(false);
                set_loaded.set(true);
                match outcome {
                    Ok(list) => {
                        set_sessions.set(list.into_iter().map(item).collect());
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
            });
        });
    });

    // One read when the tab is opened. A remount is a new card, not a new list, so the
    // effect only runs while the first answer is still missing.
    Effect::new(move |_| {
        if !loaded.get_untracked() {
            read.run(());
        }
    });

    let on_revoke = Callback::new(move |id: String| {
        set_busy.set(true);
        let alive = alive;
        spawn_local(async move {
            let outcome = actions::revoke_session(&id).await;
            alive.if_alive(|| {
                set_busy.set(false);
                match outcome {
                    Ok(()) => {
                        set_error.set(None);
                        set_sessions.update(|list| list.retain(|session| session.id != id));
                    }
                    Err(message) => set_error.set(Some(message)),
                }
            });
        });
    });

    let on_revoke_others = Callback::new(move |()| {
        set_busy.set(true);
        let alive = alive;
        spawn_local(async move {
            let outcome = actions::revoke_other_sessions().await;
            alive.if_alive(|| {
                set_busy.set(false);
                match outcome {
                    Ok(()) => {
                        set_error.set(None);
                        set_sessions.update(|list| list.retain(|session| session.current));
                    }
                    Err(message) => set_error.set(Some(message)),
                }
            });
        });
    });

    view! {
        <SharedSessionsCard
            sessions=sessions
            busy=busy
            error=error
            on_revoke=Some(on_revoke)
            on_revoke_others=Some(on_revoke_others)
        />
    }
}
