//! Account standing: search, the detail of one account, and every decision it carries.

use std::collections::HashMap;

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_client_core::model::{
    AccountDetail, AdminAccount, AdminAccountQuery, ModerationAction, WarningPreset,
};
use nodus_pages::pages::moderation::accounts::{
    AccountsPage, NextStep, RowFacts, StrikeHint, StrikeHistory, StrikeRecord, Warning,
    WarningNotice,
};
use nodus_pages::pages::moderation::{AccountBatch, BulkOutcome};

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// How many accounts one page holds.
///
/// The same number the request asks the instance for, so "another page exists" is the
/// answer to "did this page come back full?" rather than a guess about the server's own
/// default.
const PAGE: u16 = 20;

/// Splits a page of rows into the accounts the list draws and what each row knows beyond
/// its payload.
///
/// An instance that predates the escalation policy sends neither strike field, and one that
/// predates roles sends no role: a row without either simply has no hint, and the role falls
/// back to the administrator flag the directory has always carried.
fn split(rows: Vec<admin::AccountRow>) -> (Vec<AdminAccount>, HashMap<String, RowFacts>) {
    let mut list = Vec::with_capacity(rows.len());
    let mut facts = HashMap::new();
    for row in rows {
        let role = row.role.clone().unwrap_or_else(|| {
            if row.account.admin {
                "admin".to_owned()
            } else {
                "none".to_owned()
            }
        });
        let strikes = row.strike_count.map(|count| StrikeHint {
            count,
            next: row.next_escalation.map(|next| NextStep {
                strikes: next.strikes,
                action: next.action,
            }),
        });
        facts.insert(row.account.id.clone(), RowFacts { role, strikes });
        list.push(row.account);
    }
    (list, facts)
}

/// Rewrites one row's facts from what the instance answered.
///
/// The answer, not the request, is what the row shows: a warning's strike count includes the
/// strikes the account already carried, and the role is the one the instance now holds.
fn apply(facts: &mut HashMap<String, RowFacts>, id: &str, answer: &admin::AccountAfter) {
    let Some(row) = facts.get_mut(id) else { return };
    if !answer.role.is_empty() {
        row.role = answer.role.clone();
    }
    if let Some(count) = answer.strike_count {
        row.strikes = Some(StrikeHint {
            count,
            next: answer.next_escalation.as_ref().map(|next| NextStep {
                strikes: next.strikes,
                action: next.action.clone(),
            }),
        });
    }
}

/// One strike as the history panel draws it.
fn strike_of(row: admin::Strike) -> StrikeRecord {
    StrikeRecord {
        id: row.id,
        action: row.action,
        reason: row.reason,
        // The name is what a moderator recognises; the id is all that is left once the rule
        // is deleted, and it is still better than saying nothing.
        rule: row.rule_name.or(row.rule_id),
        created_at: row.created_at,
        expires_at: row.expires_at,
        live: row.live,
    }
}

/// One page of accounts, the search that narrows it, and the account a moderator picked.
#[component]
pub fn AccountList() -> impl IntoView {
    let alive = Alive::new();
    let (accounts, set_accounts) = signal(Vec::<AdminAccount>::new());
    let (query, set_query) = signal(AdminAccountQuery {
        limit: Some(PAGE),
        ..AdminAccountQuery::default()
    });
    let (facts, set_facts) = signal(HashMap::<String, RowFacts>::new());
    let (presets, set_presets) = signal(Vec::<WarningPreset>::new());
    let (notice, set_notice) = signal(None::<WarningNotice>);
    let (history, set_history) = signal(None::<StrikeHistory>);
    // The signed-in account's own id: its row is the one that cannot change its own role.
    let own = Signal::derive(move || crate::state::bridge::admin_scope().map(|(_, id)| id));
    let (detail, set_detail) = signal(None::<AccountDetail>);
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (bulk, set_bulk) = signal(BulkOutcome::default());

    let reload = Callback::new(move |only_staff: bool| {
        spawn_local(async move {
            let mut search = query.get_untracked();
            search.staff = only_staff;
            search.offset = None;
            set_busy.set(true);
            let outcome = admin::search_accounts(&search).await;
            alive.if_alive(|| {
                match outcome {
                    Ok(rows) => {
                        let (list, hints) = split(rows);
                        set_accounts.set(list);
                        set_facts.set(hints);
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    // The presets are a read of their own: the list is useful without them, so a refusal
    // here leaves the form with the moderator's own words rather than an empty page.
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(list) = admin::warning_presets().await {
                alive.if_alive(|| set_presets.set(list));
            }
        });
    });

    // The search is a re-read: the instance decides which accounts match it.
    Effect::new(move |_| {
        let search = query.get();
        set_busy.set(true);
        spawn_local(async move {
            let outcome = admin::search_accounts(&search).await;
            alive.if_alive(|| {
                match outcome {
                    Ok(rows) => {
                        let (list, hints) = split(rows);
                        set_accounts.set(list);
                        set_facts.set(hints);
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
                Ok(rows) => alive.if_alive(|| {
                    let (list, hints) = split(rows);
                    set_accounts.update(|held| held.extend(list));
                    set_facts.update(|held| held.extend(hints));
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

    let act = Callback::new(
        move |(id, action, note): (String, ModerationAction, String)| {
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
        },
    );

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

    // One action over every ticked account. The instance answers per id, so the refusals
    // land on the rows they name and the list is re-read for the ones that went through.
    let batch = Callback::new(move |request: AccountBatch| {
        spawn_local(async move {
            let outcome =
                admin::bulk_moderate(&request.ids, request.action.wire(), &request.reason).await;
            match outcome {
                Ok(answer) => alive.if_alive(|| {
                    set_bulk.set(BulkOutcome {
                        applied: answer.applied,
                        refusals: answer.refusals,
                    });
                    set_error.set(None);
                }),
                Err(problem) => alive.if_alive(|| set_error.set(Some(problem))),
            }
            reload.run(query.get_untracked().staff);
            refresh_detail();
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

    // A role change touches one row, and the answer is that row as the instance now holds
    // it: the list is not re-read, so the reader keeps their place in it.
    let role = Callback::new(move |(id, role): (String, String)| {
        spawn_local(async move {
            match admin::set_role(&id, &role).await {
                Ok(answer) => alive.if_alive(|| {
                    set_facts.update(|held| apply(held, &id, &answer));
                    set_error.set(None);
                    set_notice.set(None);
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    // A warning is written and counted by the instance in one call, so the notice says what
    // the account now carries rather than what was sent.
    let warn = Callback::new(move |(id, warning): (String, Warning)| {
        spawn_local(async move {
            let outcome =
                admin::send_warning(&id, warning.preset_id.as_deref(), &warning.text).await;
            match outcome {
                Ok(answer) => alive.if_alive(|| {
                    set_facts.update(|held| apply(held, &id, &answer));
                    let name = if answer.username.is_empty() {
                        id.clone()
                    } else {
                        answer.username.clone()
                    };
                    let sentence = match answer.strike_count.unwrap_or(0) {
                        1 => format!("Warned {name}: 1 warning in force"),
                        count => format!("Warned {name}: {count} warnings in force"),
                    };
                    set_notice.set(Some(WarningNotice {
                        account: id.clone(),
                        sentence,
                    }));
                    set_error.set(None);
                    // An open history was read before this strike existed.
                    set_history.set(None);
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let strikes = Callback::new(move |id: String| {
        spawn_local(async move {
            match admin::account_strikes(&id).await {
                Ok(rows) => alive.if_alive(|| {
                    set_history.set(Some(StrikeHistory {
                        account: id,
                        rows: rows.into_iter().map(strike_of).collect(),
                    }));
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    view! {
        <AccountsPage
            accounts=accounts
            query=query
            facts=facts
            presets=presets
            notice=notice
            history=history
            own=own
            on_role=role
            on_warn=warn
            on_strikes=strikes
            on_strikes_close=Callback::new(move |()| set_history.set(None))
            detail=detail
            busy=busy
            error=error
            has_more=Signal::derive(move || accounts.get().len() >= usize::from(PAGE))
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
            bulk=bulk
            on_bulk=batch
            write=Some(crate::state::bridge::admin::may_write())
            administrator=Some(crate::state::bridge::admin::may_administer())
            deletable=Some(crate::state::bridge::feature("account_deletion"))
        />
    }
}
