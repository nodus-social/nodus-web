//! Account standing: the list a moderator filters, the account they pick, and what it carries.
//!
//! The filter's state and each decision's request belong to the caller; the row, the search
//! row, the detail panel and the empty line are markup and live here. A decision that has no
//! callback — a paint with no token behind it — keeps its button and loses only the press.

use leptos::prelude::*;
use nodus_client_core::model::{
    AccountDetail, AccountOrigin, AccountStatus, AdminAccount, AdminAccountQuery, ModerationAction,
    WarningPreset,
};

use std::collections::HashMap;

use crate::components::panel::Panel;

use super::{AccountBatch, AccountBulkAction, BulkOutcome};

/// The states the filter offers, in the order it offers them.
const STATUSES: [(&str, Option<AccountStatus>); 6] = [
    ("Any standing", None),
    ("Active", Some(AccountStatus::Active)),
    ("Pending", Some(AccountStatus::Pending)),
    ("Silenced", Some(AccountStatus::Silenced)),
    ("Suspended", Some(AccountStatus::Suspended)),
    ("Disabled", Some(AccountStatus::Disabled)),
];

/// The rung an account would meet next, as the account payload carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextStep {
    /// The live strike count that triggers it.
    pub strikes: i64,
    /// `warn`, `flag`, `silence` or `suspend`.
    pub action: String,
}

/// What an account's live strikes mean right now, when the payload said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrikeHint {
    /// Live strikes inside the policy window.
    pub count: i64,
    /// The rung the next strike would meet, when the ladder still has one.
    pub next: Option<NextStep>,
}

/// The strikes on one account, in one line: "3 strikes, next: suspend at 4".
///
/// A moderator deciding a case should not have to count a policy against an audit log to
/// know what the next report would cost the account, so the instance's own two numbers are
/// said here instead of being left to be derived.
#[must_use]
pub fn strike_line(hint: &StrikeHint) -> String {
    let count = if hint.count == 1 {
        "1 strike".to_owned()
    } else {
        format!("{} strikes", hint.count)
    };
    match &hint.next {
        Some(next) => format!("{count}, next: {} at {}", next.action, next.strikes),
        None => format!("{count}, next: nothing — the ladder ends here"),
    }
}

/// What a row knows beyond the account payload: its staff role, and its live strikes when
/// the payload carried them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowFacts {
    /// `none`, `moderator` or `admin`, as the instance spelled it.
    pub role: String,
    /// Live strikes and the rung after them, when the account payload said.
    pub strikes: Option<StrikeHint>,
}

/// One strike in an account's history, as the instance keeps it.
///
/// There is no moderator field: the instance records the action, the reason and the rule,
/// not the person, so the panel says that rather than inventing an author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrikeRecord {
    pub id: String,
    /// `warn`, `flag`, `silence` or `suspend`.
    pub action: String,
    pub reason: String,
    /// The rule the strike was charged under, when the instance still names one.
    pub rule: Option<String>,
    pub created_at: String,
    pub expires_at: String,
    /// Whether the strike still counts against the account.
    pub live: bool,
}

/// One account's strike history, from a read the moderator asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrikeHistory {
    pub account: String,
    pub rows: Vec<StrikeRecord>,
}

/// The staff roles a role control offers, with the words it shows for them.
pub const ROLES: [(&str, &str); 3] = [
    ("none", "No staff role"),
    ("moderator", "Moderator"),
    ("admin", "Administrator"),
];

/// A role's name as people read it. A spelling this console does not know is shown as the
/// instance sent it rather than flattened to "none", which would be a claim about an
/// answer we did not understand.
#[must_use]
pub fn role_name(wire: &str) -> &str {
    ROLES
        .iter()
        .find(|(id, _)| *id == wire)
        .map_or(wire, |(_, name)| *name)
}

/// What a moderator typed into the warning form.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Warning {
    /// The preset whose words are sent when the text is empty.
    pub preset_id: Option<String>,
    pub text: String,
}

impl Warning {
    /// Whether there is anything to send: the instance refuses a warning with neither.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.preset_id.is_none()
    }
}

/// The sentence a warning came back with, and the row whose numbers it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarningNotice {
    pub account: String,
    /// "Warned ada: 1 warning in force", from the answer's own strike count.
    pub sentence: String,
}

/// Account list, its search row, and the detail of the account a moderator picked.
#[component]
pub fn AccountsPage(
    /// The accounts, as the caller last read them.
    #[prop(into)]
    accounts: Signal<Vec<AdminAccount>>,
    /// The search in force.
    #[prop(into)]
    query: Signal<AdminAccountQuery>,
    /// What each row knows beyond its payload: its role, and its strikes when they came.
    #[prop(into)]
    facts: Signal<HashMap<String, RowFacts>>,
    /// The account a moderator picked, once its detail has been read.
    #[prop(into)]
    detail: Signal<Option<AccountDetail>>,
    /// Whether a read is already on its way: an empty list while it is is not "nothing here".
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's refusal, when it had one.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Whether another page may exist behind this one.
    #[prop(into)]
    has_more: Signal<bool>,
    /// Re-reads the list under a new search.
    #[prop(optional)]
    on_query: Option<Callback<AdminAccountQuery>>,
    /// Reads the next page.
    #[prop(optional)]
    on_more: Option<Callback<()>>,
    /// Reads one account's detail.
    #[prop(optional)]
    on_open: Option<Callback<String>>,
    /// Closes the detail.
    #[prop(optional)]
    on_close: Option<Callback<()>>,
    /// Moderates one account, with the sentence that goes in the log.
    #[prop(optional)]
    on_action: Option<Callback<(String, ModerationAction, String)>>,
    /// Gives or takes the administrator flag.
    #[prop(optional)]
    on_staff: Option<Callback<(String, bool)>>,
    /// Sets what the account keeps of its media.
    #[prop(optional)]
    on_lossless: Option<Callback<(String, bool, bool)>>,
    /// Deletes one account for good.
    #[prop(optional)]
    on_purge: Option<Callback<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
    /// Whether the account's staff role is administrator. Absent means "assume yes".
    #[prop(optional_no_strip)]
    administrator: Option<Signal<bool>>,
    /// Whether the instance serves account deletion. Absent means "assume yes".
    #[prop(optional_no_strip)]
    deletable: Option<Signal<bool>>,
    /// Writes a note.
    #[prop(optional)]
    on_note: Option<Callback<(String, String)>>,
    /// Forgets a note.
    #[prop(optional)]
    on_note_delete: Option<Callback<(String, String)>>,
    /// What the last batch answered: its summary and the rows it refused.
    #[prop(into)]
    bulk: Signal<BulkOutcome>,
    /// Applies one action to every ticked account.
    #[prop(optional)]
    on_bulk: Option<Callback<AccountBatch>>,
    /// The warning presets the form offers, as the caller last read them.
    #[prop(into)]
    presets: Signal<Vec<WarningPreset>>,
    /// What the last warning answered, when it was this page that sent it.
    #[prop(into)]
    notice: Signal<Option<WarningNotice>>,
    /// The strike history a moderator opened, once it has been read.
    #[prop(into)]
    history: Signal<Option<StrikeHistory>>,
    /// The signed-in account's own id, so its row can say who changes its role.
    #[prop(into)]
    own: Signal<Option<String>>,
    /// Sets one account's staff role.
    #[prop(optional)]
    on_role: Option<Callback<(String, String)>>,
    /// Warns one account, with a preset or the moderator's own words.
    #[prop(optional)]
    on_warn: Option<Callback<(String, Warning)>>,
    /// Reads one account's strike history.
    #[prop(optional)]
    on_strikes: Option<Callback<String>>,
    /// Closes the strike history.
    #[prop(optional)]
    on_strikes_close: Option<Callback<()>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    let administrator = crate::props::administrator(administrator);
    let deletable = crate::props::capable(deletable);
    // The selection is the page's own: which rows are ticked is a state of this list, not
    // something the caller asked for or the server knows about.
    let selected = RwSignal::new(Vec::<String>::new());
    let chosen = StoredValue::new(AccountBulkAction::Approve);
    let reason = StoredValue::new(String::new());
    // A `Callback`, not a closure: the tick sits inside a `<Show>` in the row, and a
    // render-time child has to be shareable between threads.
    let toggle = Callback::new(move |(id, on): (String, bool)| {
        selected.update(|held| {
            if on {
                if !held.contains(&id) {
                    held.push(id);
                }
            } else {
                held.retain(|ticked| ticked != &id);
            }
        });
    });
    // A warning is typed in one row at a time: which row the form is open on, the preset it
    // picked, and the words it holds. Opening the form starts empty — words meant for one
    // account should not be waiting in another's form.
    let warning_for = RwSignal::new(None::<String>);
    let warning_text = RwSignal::new(String::new());
    let warning_preset = RwSignal::new(None::<String>);
    let warning_ready = Signal::derive(move || {
        !warning_text.get().trim().is_empty() || warning_preset.get().is_some()
    });
    view! {
        <Show when=move || error.get().is_some()>
            <p class="mb-2 text-sm text-error" role="alert">{move || error.get().unwrap_or_default()}</p>
        </Show>

        <Show when=move || !write.get()>
            <crate::components::chrome::Note class="mb-2 text-sm text-warning">
                {crate::props::NO_WRITE}
            </crate::components::chrome::Note>
        </Show>

        <Show when=move || !administrator.get()>
            <crate::components::chrome::Note class="mb-2 text-sm opacity-70">
                {crate::props::NO_ADMIN_ROLE}
            </crate::components::chrome::Note>
        </Show>

        <Panel
            title="Accounts"
            description="Every account, narrowed by name, address, domain or standing. A moderator sees the standing and the numbers; nothing here is shown to the account."
        >
            <div class="grid gap-2 sm:grid-cols-2">
                <input
                    class="input input-bordered input-sm"
                    placeholder="User name"
                    prop:value=move || query.get().username.unwrap_or_default()
                    on:input=move |event| {
                        if let Some(on_query) = on_query {
                            let mut next = query.get_untracked();
                            let value = event_target_value(&event);
                            next.username = Some(value.clone()).filter(|text| !text.is_empty());
                            next.offset = None;
                            on_query.run(next);
                        }
                    }
                />
                <input
                    class="input input-bordered input-sm"
                    placeholder="Mail address"
                    prop:value=move || query.get().email.unwrap_or_default()
                    on:input=move |event| {
                        if let Some(on_query) = on_query {
                            let mut next = query.get_untracked();
                            let value = event_target_value(&event);
                            next.email = Some(value.clone()).filter(|text| !text.is_empty());
                            next.offset = None;
                            on_query.run(next);
                        }
                    }
                />
                <input
                    class="input input-bordered input-sm"
                    placeholder="Domain (remote accounts)"
                    prop:value=move || query.get().by_domain.unwrap_or_default()
                    on:input=move |event| {
                        if let Some(on_query) = on_query {
                            let mut next = query.get_untracked();
                            let value = event_target_value(&event);
                            next.by_domain = Some(value.clone()).filter(|text| !text.is_empty());
                            // A narrower filter is a new first page, not a deeper one: the
                            // offset counted rows the filter has just removed.
                            next.offset = None;
                            on_query.run(next);
                        }
                    }
                />
                <select
                    class="select select-bordered select-sm"
                    on:change=move |event| {
                        if let Some(on_query) = on_query {
                            let picked = event_target_value(&event);
                            let mut next = query.get_untracked();
                            next.status = STATUSES
                                .iter()
                                .find(|(name, _)| *name == picked)
                                .and_then(|(_, status)| *status);
                            next.offset = None;
                            on_query.run(next);
                        }
                    }
                >
                    {STATUSES
                        .into_iter()
                        .map(|(name, _)| view! { <option value=name>{name}</option> })
                        .collect_view()}
                </select>
                <select
                    class="select select-bordered select-sm"
                    on:change=move |event| {
                        if let Some(on_query) = on_query {
                            let picked = event_target_value(&event);
                            let mut next = query.get_untracked();
                            next.origin = match picked.as_str() {
                                "local" => Some(AccountOrigin::Local),
                                "remote" => Some(AccountOrigin::Remote),
                                _ => None,
                            };
                            next.offset = None;
                            on_query.run(next);
                        }
                    }
                >
                    <option value="all">"Every origin"</option>
                    <option value="local">"On this instance"</option>
                    <option value="remote">"Seen over federation"</option>
                </select>
                <label class="label cursor-pointer justify-start gap-2">
                    <input
                        type="checkbox"
                        class="toggle toggle-sm"
                        prop:checked=move || query.get().staff
                        on:change=move |event| {
                            if let Some(on_query) = on_query {
                                let mut next = query.get_untracked();
                                next.staff = event_target_checked(&event);
                                next.offset = None;
                                on_query.run(next);
                            }
                        }
                    />
                    <span class="label-text">"Only administrators"</span>
                </label>
            </div>
            <button
                class="btn btn-ghost btn-xs mt-2 self-start"
                disabled=on_query.is_none()
                on:click=move |_| {
                    if let Some(on_query) = on_query {
                        // Every filter cleared, and the page size kept: this is the door
                        // back to the whole directory, not a search with an empty box.
                        let mut next = AdminAccountQuery::default();
                        next.limit = query.get_untracked().limit;
                        on_query.run(next);
                    }
                }
            >
                "Browse all accounts"
            </button>
        </Panel>

        <Show when=move || write.get() && !selected.get().is_empty()>
            <div
                class="mb-2 flex flex-wrap items-end gap-2 rounded-box border border-base-300 bg-base-200 px-3 py-2"
                role="group"
                aria-label="Bulk account action"
            >
                <p class="text-sm font-medium">
                    {move || format!("{} selected", selected.get().len())}
                </p>
                <select
                    class="select select-bordered select-sm"
                    aria-label="Bulk action"
                    on:change=move |event| {
                        let picked = event_target_value(&event);
                        if let Some(found) = AccountBulkAction::ALL
                            .into_iter()
                            .find(|candidate| candidate.wire() == picked)
                        {
                            chosen.set_value(found);
                        }
                    }
                >
                    {AccountBulkAction::ALL
                        .into_iter()
                        .map(|candidate| {
                            view! { <option value=candidate.wire()>{candidate.label()}</option> }
                        })
                        .collect_view()}
                </select>
                <input
                    class="input input-bordered input-sm w-full sm:w-64"
                    placeholder="Reason (recorded in the log)"
                    on:input=move |event| reason.set_value(event_target_value(&event))
                />
                <button
                    class="btn btn-primary btn-sm"
                    disabled=move || on_bulk.is_none()
                    on:click=move |_| {
                        if let Some(on_bulk) = on_bulk {
                            let ids = selected.get_untracked();
                            if ids.is_empty() {
                                return;
                            }
                            on_bulk
                                .run(AccountBatch {
                                    ids,
                                    action: chosen.get_value(),
                                    reason: reason.get_value(),
                                });
                            // The batch has gone out; what comes back is per row, so the tick
                            // marks have done their work. Clearing them also makes the next
                            // selection deliberate rather than a leftover.
                            selected.set(Vec::new());
                        }
                    }
                >
                    "Apply to selection"
                </button>
                <button class="btn btn-ghost btn-sm" on:click=move |_| selected.set(Vec::new())>
                    "Clear"
                </button>
            </div>
        </Show>

        <Show when=move || bulk.get() != BulkOutcome::default()>
            <p class="mb-2 text-sm" role="status">
                {move || bulk.get().summary()}
            </p>
        </Show>

        <Show when=move || detail.get().is_some()>
            {move || match detail.get() {
                Some(open) => detail_panel(
                    open,
                    on_close,
                    on_action,
                    on_staff,
                    on_lossless,
                    on_purge,
                    on_note,
                    on_note_delete,
                    write,
                    administrator,
                    deletable,
                ),
                None => ().into_any(),
            }}
        </Show>

        <ul class="flex flex-col gap-2">
            <For each=move || accounts.get() key=|account| account.id.clone() let:account>
                {
                    let id = StoredValue::new(account.id.clone());
                    let name = account
                        .account
                        .as_ref()
                        .map_or_else(|| account.username.clone(), |public| format!("@{}", public.acct));
                    let standing = account.standing().to_owned();
                    let email = account.email.clone().unwrap_or_default();
                    let reports = account.open_reports;
                    let notes = account.notes_count;
                    let label = format!("Select {name}");
                    // A row is "mine" when the instance says the signed-in account is this
                    // one: the role control is absent there, because the server refuses a
                    // self-demotion and a select that always fails is not a control.
                    let mine = Signal::derive(move || {
                        own.get().is_some_and(|own| own == id.get_value())
                    });
                    let role_label = format!("Role for {name}");
                    let warn_label = format!("Warn {name}");
                    let strikes_label = format!("Strikes for {name}");
                    let warn_name = name.clone();
                    let close_warning = Callback::new(move |()| warning_for.set(None));
                    view! {
                        <li class="flex items-start gap-2">
                            <Show when=move || write.get()>
                                <input
                                    type="checkbox"
                                    class="checkbox checkbox-sm mt-3"
                                    aria-label=label.clone()
                                    prop:checked=move || {
                                        selected.get().iter().any(|ticked| ticked == &id.get_value())
                                    }
                                    on:change=move |event| {
                                        toggle.run((id.get_value(), event_target_checked(&event)));
                                    }
                                />
                            </Show>
                            <div class="min-w-0 flex-1">
                            <button
                                class="flex w-full items-center justify-between gap-2 rounded-box bg-base-100 px-3 py-2 text-left shadow-sm hover:bg-base-200"
                                disabled=on_open.is_none()
                                on:click=move |_| {
                                    if let Some(on_open) = on_open {
                                        on_open.run(id.get_value());
                                    }
                                }
                            >
                                <div class="min-w-0">
                                    <p class="truncate text-sm font-medium">{name}</p>
                                    <p class="truncate text-xs opacity-60">
                                        {standing}
                                        {if email.is_empty() {
                                            String::new()
                                        } else {
                                            format!(" · {email}")
                                        }}
                                    </p>
                                </div>
                                <div class="shrink-0 text-right text-xs opacity-70">
                                    <p>{format!("{reports} open report(s)")}</p>
                                    <p>{format!("{notes} note(s)")}</p>
                                </div>
                            </button>
                            <Show when=move || {
                                bulk.get().refusal_for(&id.get_value()).is_some()
                            }>
                                <p class="mt-1 text-xs text-error" role="alert">
                                    {move || bulk.get().refusal_for(&id.get_value()).unwrap_or_default()}
                                </p>
                            </Show>
                            <div class="mt-1 flex flex-wrap items-center gap-2">
                                {role_cell(id, facts, mine, write, administrator, on_role, role_label)}
                                <Show when=move || write.get() && !mine.get() && on_warn.is_some()>
                                    <button
                                        class="btn btn-ghost btn-xs"
                                        aria-label=warn_label.clone()
                                        on:click=move |_| {
                                            warning_text.set(String::new());
                                            warning_preset.set(None);
                                            warning_for.set(Some(id.get_value()));
                                        }
                                    >
                                        "Warn"
                                    </button>
                                </Show>
                                <Show when=move || {
                                    facts
                                        .get()
                                        .get(&id.get_value())
                                        .is_some_and(|row| row.strikes.is_some())
                                }>
                                    <button
                                        class="btn btn-ghost btn-xs text-warning"
                                        data-testid="account-strikes"
                                        aria-label=strikes_label.clone()
                                        on:click=move |_| {
                                            let open = history
                                                .get()
                                                .is_some_and(|open| open.account == id.get_value());
                                            if open {
                                                if let Some(on_strikes_close) = on_strikes_close {
                                                    on_strikes_close.run(());
                                                }
                                            } else if let Some(on_strikes) = on_strikes {
                                                on_strikes.run(id.get_value());
                                            }
                                        }
                                    >
                                        {move || {
                                            facts
                                                .get()
                                                .get(&id.get_value())
                                                .and_then(|row| row.strikes.as_ref())
                                                .map(strike_line)
                                                .unwrap_or_default()
                                        }}
                                    </button>
                                </Show>
                            </div>
                            <Show when=move || {
                                notice.get().is_some_and(|said| said.account == id.get_value())
                            }>
                                <p class="mt-1 text-xs" role="status" data-testid="warning-notice">
                                    {move || notice.get().map(|said| said.sentence).unwrap_or_default()}
                                </p>
                            </Show>
                            <Show when=move || {
                                warning_for.get().is_some_and(|open| open == id.get_value())
                            }>
                                {warning_form(
                                    id,
                                    warn_name.clone(),
                                    presets,
                                    warning_text,
                                    warning_preset,
                                    warning_ready,
                                    on_warn,
                                    close_warning,
                                )}
                            </Show>
                            <Show when=move || {
                                history.get().is_some_and(|open| open.account == id.get_value())
                            }>
                                {move || match history.get() {
                                    Some(open) => strike_history(open, on_strikes_close),
                                    None => ().into_any(),
                                }}
                            </Show>
                            </div>
                        </li>
                    }
                }
            </For>
        </ul>

        <Show when=move || has_more.get()>
            <button
                class="btn btn-ghost btn-sm mt-2"
                disabled=on_more.is_none()
                on:click=move |_| {
                    if let Some(on_more) = on_more {
                        on_more.run(());
                    }
                }
            >
                "Load more accounts"
            </button>
        </Show>

        <Show when=move || accounts.get().is_empty() && !busy.get()>
            <p class="text-sm opacity-70">"No account matches that search."</p>
        </Show>
    }
}

/// The picked account: its numbers, its notes, and every decision.
fn detail_panel(
    open: AccountDetail,
    on_close: Option<Callback<()>>,
    on_action: Option<Callback<(String, ModerationAction, String)>>,
    on_staff: Option<Callback<(String, bool)>>,
    on_lossless: Option<Callback<(String, bool, bool)>>,
    on_purge: Option<Callback<String>>,
    on_note: Option<Callback<(String, String)>>,
    on_note_delete: Option<Callback<(String, String)>>,
    write: Signal<bool>,
    administrator: Signal<bool>,
    deletable: Signal<bool>,
) -> AnyView {
    let id = StoredValue::new(open.account.id.clone());
    let note = StoredValue::new(String::new());
    let name = open.account.account.as_ref().map_or_else(
        || open.account.username.clone(),
        |public| format!("@{}", public.acct),
    );
    let standing = open.account.standing().to_owned();
    let staff = open.account.admin;
    let images = open.account.lossless_images;
    let videos = open.account.lossless_videos;
    let reports = open.reports_against;
    let notes = StoredValue::new(open.notes.clone());
    let flags = StoredValue::new(open.flags.clone());
    let has_flags = !open.flags.is_empty();
    view! {
        <Panel
            title=name
            description=format!("{standing} · {reports} report(s) name this account")
        >
            <div class="flex flex-wrap gap-1">
                {action("Approve", ModerationAction::Approve, id, note, on_action)}
                {action("Reject", ModerationAction::Reject, id, note, on_action)}
                {action("Silence", ModerationAction::Silence, id, note, on_action)}
                {action("Suspend", ModerationAction::Suspend, id, note, on_action)}
                {action("Give back", ModerationAction::Enable, id, note, on_action)}
            </div>

            <input
                class="input input-bordered input-sm mt-2 w-full"
                placeholder="Why — this sentence goes in the audit log"
                on:input=move |event| note.set_value(event_target_value(&event))
            />

            <label class="label mt-2 cursor-pointer justify-start gap-2">
                <input
                    type="checkbox"
                    class="toggle toggle-sm"
                    prop:checked=staff
                    disabled=move || on_staff.is_none() || !write.get() || !administrator.get()
                    title=move || {
                        if !administrator.get() {
                            crate::props::NO_ADMIN_ROLE
                        } else if !write.get() {
                            crate::props::NO_WRITE
                        } else {
                            ""
                        }
                    }
                    on:change=move |event| {
                        if let Some(on_staff) = on_staff {
                            on_staff.run((id.get_value(), event_target_checked(&event)));
                        }
                    }
                />
                <span class="label-text">"Administrator"</span>
            </label>
            <label class="label cursor-pointer justify-start gap-2">
                <input
                    type="checkbox"
                    class="toggle toggle-sm"
                    prop:checked=images
                    disabled=move || on_lossless.is_none() || !administrator.get()
                    title=move || (!administrator.get()).then_some(crate::props::NO_ADMIN_ROLE).unwrap_or_default()
                    on:change=move |event| {
                        if let Some(on_lossless) = on_lossless {
                            let keep = event_target_checked(&event);
                            on_lossless.run((id.get_value(), keep, videos));
                        }
                    }
                />
                <span class="label-text">"Keep images at full size"</span>
            </label>
            <label class="label cursor-pointer justify-start gap-2">
                <input
                    type="checkbox"
                    class="toggle toggle-sm"
                    prop:checked=videos
                    disabled=move || on_lossless.is_none() || !administrator.get()
                    title=move || (!administrator.get()).then_some(crate::props::NO_ADMIN_ROLE).unwrap_or_default()
                    on:change=move |event| {
                        if let Some(on_lossless) = on_lossless {
                            let keep = event_target_checked(&event);
                            on_lossless.run((id.get_value(), images, keep));
                        }
                    }
                />
                <span class="label-text">"Keep video at full size"</span>
            </label>

            <div class="mt-3">
                <p class="text-sm font-medium">"Notes"</p>
                <ul class="flex flex-col gap-1">
                    <For each=move || notes.get_value() key=|entry| entry.id.clone() let:entry>
                        {
                            let note_id = StoredValue::new(entry.id.clone());
                            let who = if entry.author_username.is_empty() {
                                "someone".to_owned()
                            } else {
                                format!("@{}", entry.author_username)
                            };
                            view! {
                                <li class="flex items-start gap-2 rounded-box bg-base-200 p-2">
                                    <div class="min-w-0">
                                        <p class="text-xs opacity-60">{who}</p>
                                        <p class="text-sm whitespace-pre-line">{entry.body.clone()}</p>
                                    </div>
                                    <button
                                        class="btn btn-ghost btn-xs ml-auto shrink-0 text-error"
                                        disabled=on_note_delete.is_none()
                                        on:click=move |_| {
                                            if let Some(on_note_delete) = on_note_delete {
                                                on_note_delete.run((id.get_value(), note_id.get_value()));
                                            }
                                        }
                                    >
                                        "Forget"
                                    </button>
                                </li>
                            }
                        }
                    </For>
                </ul>
                <div class="mt-2 flex gap-2">
                    <input
                        class="input input-bordered input-sm w-full"
                        placeholder="What should the next moderator know?"
                        on:input=move |event| note.set_value(event_target_value(&event))
                    />
                    <button
                        class="btn btn-sm"
                        disabled=on_note.is_none()
                        on:click=move |_| {
                            if let Some(on_note) = on_note {
                                let body = note.get_value().trim().to_owned();
                                if body.is_empty() {
                                    return;
                                }
                                on_note.run((id.get_value(), body));
                                note.set_value(String::new());
                            }
                        }
                    >
                        "Write"
                    </button>
                </div>
            </div>

            <Show when=move || has_flags>
                <div class="mt-3">
                    <p class="text-sm font-medium">"What the scanners said"</p>
                    <ul class="flex flex-col gap-1 text-xs">
                        <For each=move || flags.get_value() key=|flag| flag.id.clone() let:flag>
                            <li class="rounded-box bg-base-200 p-2">
                                {format!("{} · {}", flag.reason, flag.severity.as_str())}
                            </li>
                        </For>
                    </ul>
                </div>
            </Show>

            <div class="mt-3 flex items-center gap-2">
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=on_close.is_none()
                    on:click=move |_| {
                        if let Some(on_close) = on_close {
                            on_close.run(());
                        }
                    }
                >
                    "Close"
                </button>
                <button
                    class="btn btn-error btn-sm ml-auto"
                    disabled=move || on_purge.is_none() || !write.get() || !deletable.get()
                    title=move || {
                        if !deletable.get() {
                            "This instance does not serve account deletion."
                        } else if !write.get() {
                            crate::props::NO_WRITE
                        } else {
                            ""
                        }
                    }
                    on:click=move |_| {
                        if let Some(on_purge) = on_purge {
                            on_purge.run(id.get_value());
                        }
                    }
                >
                    "Delete for good"
                </button>
            </div>
        </Panel>
    }
    .into_any()
}

/// One decision, as a button that keeps its box when there is nothing to send.
fn action(
    label: &'static str,
    action: ModerationAction,
    target: StoredValue<String>,
    note: StoredValue<String>,
    on_action: Option<Callback<(String, ModerationAction, String)>>,
) -> AnyView {
    view! {
        <button
            class="btn btn-ghost btn-xs"
            disabled=on_action.is_none()
            on:click=move |_| {
                if let Some(on_action) = on_action {
                    on_action.run((target.get_value(), action, note.get_value()));
                }
            }
        >
            {label}
        </button>
    }
    .into_any()
}

/// A row's staff role, and — for an administrator, on somebody else's row — the select that
/// changes it.
///
/// A moderator sees no select at all: the route answers to `role == admin`, so a control
/// they could see would only be a press the instance refuses. The signed-in account's own
/// row says who can change its role instead of offering a select that could leave an
/// instance with no administrator.
fn role_cell(
    id: StoredValue<String>,
    facts: Signal<HashMap<String, RowFacts>>,
    mine: Signal<bool>,
    write: Signal<bool>,
    administrator: Signal<bool>,
    on_role: Option<Callback<(String, String)>>,
    label: String,
) -> AnyView {
    let current = Signal::derive(move || {
        facts
            .get()
            .get(&id.get_value())
            .map(|row| row.role.clone())
            .unwrap_or_else(|| "none".to_owned())
    });
    view! {
        <span class="text-xs opacity-70" data-testid="account-role">
            {move || format!("Role: {}", role_name(&current.get()))}
        </span>
        <Show when=move || administrator.get() && !mine.get()>
            <select
                class="select select-bordered select-xs"
                aria-label=label.clone()
                disabled=move || !write.get() || on_role.is_none()
                on:change=move |event| {
                    if let Some(on_role) = on_role {
                        on_role.run((id.get_value(), event_target_value(&event)));
                    }
                }
            >
                {ROLES
                    .into_iter()
                    .map(|(wire, name)| {
                        let chosen = Signal::derive(move || current.get() == wire);
                        view! {
                            <option value=wire selected=move || chosen.get()>
                                {name}
                            </option>
                        }
                    })
                    .collect_view()}
            </select>
        </Show>
        <Show when=move || administrator.get() && mine.get()>
            <span class="text-xs opacity-60" data-testid="own-role-note">
                "Your role — another administrator has to change it."
            </span>
        </Show>
    }
    .into_any()
}

/// The warning form for one row: a preset or the moderator's own words, and a press that
/// hands both to the caller.
///
/// There is no "send quietly" switch because the instance has none: `warn` writes the
/// notification and the strike in one call, so the form says that instead of pretending the
/// two could be separated.
fn warning_form(
    id: StoredValue<String>,
    name: String,
    presets: Signal<Vec<WarningPreset>>,
    text: RwSignal<String>,
    preset: RwSignal<Option<String>>,
    ready: Signal<bool>,
    on_warn: Option<Callback<(String, Warning)>>,
    on_close: Callback<()>,
) -> AnyView {
    let send_label = format!("Send warning to {name}");
    view! {
        <div class="mt-1 flex flex-col gap-2 rounded-box border border-base-300 bg-base-200 p-2">
            <select
                class="select select-bordered select-xs"
                aria-label="Warning preset"
                on:change=move |event| {
                    let value = event_target_value(&event);
                    preset.set(Some(value).filter(|value| !value.is_empty()));
                }
            >
                <option value="">"Write my own words"</option>
                {move || {
                    presets
                        .get()
                        .into_iter()
                        .map(|preset| {
                            view! {
                                <option value=preset.id.clone()>{preset.title.clone()}</option>
                            }
                        })
                        .collect_view()
                }}
            </select>
            <textarea
                class="textarea textarea-bordered textarea-sm w-full"
                aria-label="Warning text"
                rows="2"
                placeholder="The words they will read in their notifications"
                prop:value=move || text.get()
                on:input=move |event| text.set(event_target_value(&event))
            ></textarea>
            <p class="text-xs opacity-60">
                "The instance sends this in their notifications and counts it as a strike; it cannot be sent quietly."
            </p>
            <div class="flex flex-wrap gap-1">
                <button
                    class="btn btn-warning btn-xs"
                    aria-label=send_label
                    disabled=move || !ready.get() || on_warn.is_none()
                    on:click=move |_| {
                        if let Some(on_warn) = on_warn {
                            on_warn
                                .run((
                                    id.get_value(),
                                    Warning {
                                        preset_id: preset.get_untracked(),
                                        text: text.get_untracked().trim().to_owned(),
                                    },
                                ));
                        }
                        on_close.run(());
                    }
                >
                    "Warn"
                </button>
                <button class="btn btn-ghost btn-xs" on:click=move |_| on_close.run(())>
                    "Cancel"
                </button>
            </div>
        </div>
    }
    .into_any()
}

/// One account's strikes, as the instance keeps them: what it was charged with, why, under
/// which rule, when, and whether the charge still counts.
fn strike_history(open: StrikeHistory, on_close: Option<Callback<()>>) -> AnyView {
    let empty = open.rows.is_empty();
    let rows = open.rows;
    view! {
        <div class="mt-1 rounded-box border border-base-300 bg-base-200 p-2" data-testid="strike-history">
            <div class="flex items-center gap-2">
                <p class="text-xs font-medium">"Strike history"</p>
                <button
                    class="btn btn-ghost btn-xs ml-auto"
                    on:click=move |_| {
                        if let Some(on_close) = on_close {
                            on_close.run(());
                        }
                    }
                >
                    "Close"
                </button>
            </div>
            <Show when=move || empty>
                <p class="text-xs opacity-70">"No strike on this account."</p>
            </Show>
            <ul class="flex flex-col gap-1">
                <For each=move || rows.clone() key=|row| row.id.clone() let:row>
                    {
                        let when = day_of(&row.created_at);
                        let kept = if row.live {
                            format!("counts until {}", day_of(&row.expires_at))
                        } else {
                            format!("no longer counted (expired {})", day_of(&row.expires_at))
                        };
                        let rule = row
                            .rule
                            .clone()
                            .map_or_else(|| "no rule cited".to_owned(), |rule| format!("rule: {rule}"));
                        let action = row.action.clone();
                        let reason = row.reason.clone();
                        view! {
                            <li class="text-xs" data-testid="strike-row">
                                <p class="font-medium">{format!("{action} · {when}")}</p>
                                <p class="opacity-80">{reason}</p>
                                <p class="opacity-60">{format!("{rule} · {kept}")}</p>
                            </li>
                        }
                    }
                </For>
            </ul>
            <p class="mt-1 text-xs opacity-60">
                "The instance records the action and its reason, not which moderator gave it; the audit log holds the name."
            </p>
        </div>
    }
    .into_any()
}

/// The date part of an RFC 3339 stamp, as the history reads it.
fn day_of(stamp: &str) -> String {
    stamp
        .split_once('T')
        .map_or(stamp, |(day, _)| day)
        .to_owned()
}
