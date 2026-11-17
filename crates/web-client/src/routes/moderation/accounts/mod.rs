//! Account standing: search, the detail of one account, and every decision it carries.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_client_core::model::{
    AccountDetail, AdminAccount, AdminAccountQuery, ModerationAction,
};
use nodus_pages::pages::moderation::accounts::AccountsPage;

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// One page of accounts, the search that narrows it, and the account a moderator picked.
#[component]
pub fn AccountList() -> impl IntoView {
    let alive = Alive::new();
    let (accounts, set_accounts) = signal(Vec::<AdminAccount>::new());
    let (query, set_query) = signal(AdminAccountQuery::default());
    let (detail, set_detail) = signal(None::<AccountDetail>);
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);

    let reload = Callback::new(move |only_staff: bool| {
        spawn_local(async move {
            let mut search = query.get_untracked();
            search.staff = only_staff;
            let outcome = admin::search_accounts(&search).await;
            alive.if_alive(|| {
                set_busy.set(true);
                match outcome {
                    Ok(list) => {
                        set_accounts.set(list);
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    // The search is a re-read: the instance decides which accounts match it.
    Effect::new(move |_| {
        let search = query.get();
        spawn_local(async move {
            let outcome = admin::search_accounts(&search).await;
            alive.if_alive(|| {
                set_busy.set(true);
                match outcome {
                    Ok(list) => {
                        set_accounts.set(list);
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    let more = Callback::new(move |()| {
        spawn_local(async move {
            let mut search = query.get_untracked();
            search.offset = Some(u32::try_from(accounts.get_untracked().len()).unwrap_or(0));
            match admin::search_accounts(&search).await {
                Ok(list) => alive.if_alive(|| {
                    set_accounts.update(|held| held.extend(list));
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let open = Callback::new(move |id: String| {
        spawn_local(async move {
            match admin::account_detail(&id).await {
                Ok(account) => alive.if_alive(|| set_detail.set(Some(account))),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    // Every decision changes the detail the panel is showing, so the panel is re-read from
    // the account it is about rather than patched in place.
    let refresh_detail = move || {
        let current = detail.get_untracked().map(|open| open.account.id.clone());
        spawn_local(async move {
            let Some(id) = current else { return };
            if let Ok(account) = admin::account_detail(&id).await {
                alive.if_alive(|| set_detail.set(Some(account)));
            }
        });
    };

    let act = Callback::new(move |(id, action, note): (String, ModerationAction, String)| {
        spawn_local(async move {
            let outcome = if note.trim().is_empty() {
                admin::moderate(&id, action).await
            } else {
                admin::moderate_with_note(&id, action, &note, None).await
            };
            match outcome {
                Ok(_) => {
                    reload.run(query.get_untracked().staff);
                    refresh_detail();
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let staff = Callback::new(move |(id, on): (String, bool)| {
        spawn_local(async move {
            match admin::set_staff(&id, on).await {
                Ok(_) => {
                    reload.run(query.get_untracked().staff);
                    refresh_detail();
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let lossless = Callback::new(move |(id, images, videos): (String, bool, bool)| {
        spawn_local(async move {
            match admin::set_lossless(&id, images, videos).await {
                Ok(()) => refresh_detail(),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let purge = Callback::new(move |id: String| {
        spawn_local(async move {
            match admin::purge_account(&id).await {
                Ok(()) => {
                    alive.if_alive(|| set_detail.set(None));
                    reload.run(query.get_untracked().staff);
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let write_note = Callback::new(move |(id, body): (String, String)| {
        spawn_local(async move {
            match admin::add_account_note(&id, &body).await {
                Ok(_) => refresh_detail(),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let drop_note = Callback::new(move |(id, note_id): (String, String)| {
        spawn_local(async move {
            match admin::delete_account_note(&id, &note_id).await {
                Ok(()) => refresh_detail(),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    view! {
        <AccountsPage
            accounts=accounts
            query=query
            detail=detail
            busy=busy
            error=error
            has_more=Signal::derive(move || accounts.get().len() >= 20)
            on_query=Callback::new(move |next: AdminAccountQuery| set_query.set(next))
            on_more=more
            on_open=open
            on_close=Callback::new(move |()| set_detail.set(None))
            on_action=act
            on_staff=staff
            on_lossless=lossless
            on_purge=purge
            on_note=write_note
            on_note_delete=drop_note
        />
    }
}
