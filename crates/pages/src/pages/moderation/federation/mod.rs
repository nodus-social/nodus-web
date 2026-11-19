//! Domains this instance refuses.
//!
//! The outbound half of federation lives under Administration (`relays` and allowed
//! domains), because allowing a domain is a decision about the network, while blocking one is
//! a decision about what the people here have to see.
//!
//! The two fields of the add form are the caller's state and arrive as signals: a page may not
//! hold a draft. What it does own is the form's own layout, so the input, the severity toggle
//! and the press cannot drift apart from the list above them.

use leptos::prelude::*;
use nodus_client_core::model::{DomainBlock, DomainSeverity};

use crate::components::panel::Panel;

/// Blocked domains, with an add form.
#[component]
pub fn FederationPage(
    /// The blocks this instance holds, as the caller last read them.
    #[prop(into)]
    blocks: Signal<Vec<DomainBlock>>,
    /// The domain being typed.
    #[prop(into)]
    domain: Signal<String>,
    /// Whether a new block refuses the domain entirely rather than silencing it.
    #[prop(into)]
    suspend: Signal<bool>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal, when it had one.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records the typed domain. Absent where there is nowhere to keep it.
    #[prop(optional)]
    on_domain: Option<Callback<String>>,
    /// Records the severity. Absent where nothing can be chosen.
    #[prop(optional)]
    on_suspend: Option<Callback<bool>>,
    /// Blocks the typed domain. Absent where nothing can be sent.
    #[prop(optional)]
    on_block: Option<Callback<()>>,
    /// Lifts one block. Absent where nothing can be sent.
    #[prop(optional)]
    on_remove: Option<Callback<String>>,
    /// Changes how much of one domain is refused. Absent where nothing can be sent.
    #[prop(optional)]
    on_severity: Option<Callback<(String, DomainSeverity)>>,
    /// Records the list being pasted in.
    #[prop(optional)]
    on_import: Option<Callback<String>>,
    /// Imports the pasted list in one request.
    #[prop(optional)]
    on_import_now: Option<Callback<()>>,
    /// What the last import did.
    #[prop(into)]
    #[prop(optional)]
    imported: Signal<Option<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    view! {
        <Show when=move || error.get().is_some()>
            <p class="text-sm text-error">{move || error.get().unwrap_or_default()}</p>
        </Show>

        <ul class="flex flex-col gap-2">
            <For each=move || blocks.get() key=|block| block.id.clone() let:block>
                {
                    let target = StoredValue::new(block.id.clone());
                    let domain = block.domain.clone();
                    let severity = block.severity;
                    let comment = block.public_comment.clone();
                    let has_comment = !comment.is_empty();
                    let comment_text = comment.clone();
                    let select_label = format!("Severity for {domain}");
                    view! {
                        <li class="flex flex-wrap items-center justify-between gap-2 rounded-box bg-base-100 px-3 py-2 shadow-sm">
                            <div class="min-w-0">
                                <p class="truncate text-sm">{domain}</p>
                                <Show when=move || has_comment>
                                    <p class="truncate text-xs opacity-60">{comment_text.clone()}</p>
                                </Show>
                            </div>
                            <div class="flex shrink-0 items-center gap-2">
                                <select
                                    class="select select-bordered select-xs"
                                    aria-label=select_label
                                    disabled=move || on_severity.is_none() || !write.get()
                                    title=move || (!write.get()).then_some(crate::props::NO_WRITE).unwrap_or_default()
                                    on:change=move |event| {
                                        if let Some(on_severity) = on_severity {
                                            let next = match event_target_value(&event).as_str() {
                                                "silence" => DomainSeverity::Silence,
                                                "noop" => DomainSeverity::Noop,
                                                _ => DomainSeverity::Suspend,
                                            };
                                            if next != severity {
                                                on_severity.run((target.get_value(), next));
                                            }
                                        }
                                    }
                                >
                                    <option value="suspend" selected=severity == DomainSeverity::Suspend>
                                        "Suspend"
                                    </option>
                                    <option value="silence" selected=severity == DomainSeverity::Silence>
                                        "Silence"
                                    </option>
                                    <option value="noop" selected=severity == DomainSeverity::Noop>
                                        "No-op (keep on record)"
                                    </option>
                                </select>
                                <button
                                    class="btn btn-ghost btn-xs"
                                    disabled=on_remove.is_none()
                                    on:click=move |_| {
                                        // A block that could not be removed is still in force;
                                        // the list is the record of that.
                                        if let Some(on_remove) = on_remove {
                                            on_remove.run(target.get_value());
                                        }
                                    }
                                >
                                    "Remove"
                                </button>
                            </div>
                        </li>
                    }
                }
            </For>
        </ul>

        <div class="mt-4 flex flex-col gap-2">
            <input
                class="input input-bordered input-sm"
                placeholder="example.social"
                prop:value=move || domain.get()
                disabled=on_domain.is_none()
                on:input=move |event| {
                    if let Some(on_domain) = on_domain {
                        on_domain.run(event_target_value(&event));
                    }
                }
            />
            <label class="label cursor-pointer justify-start gap-2">
                <input
                    type="checkbox"
                    class="toggle toggle-sm"
                    prop:checked=move || suspend.get()
                    disabled=on_suspend.is_none()
                    on:change=move |event| {
                        if let Some(on_suspend) = on_suspend {
                            on_suspend.run(event_target_checked(&event));
                        }
                    }
                />
                <span class="label-text">"Suspend entirely (off: silence from timelines)"</span>
            </label>
            <button
                class="btn btn-error btn-sm self-start"
                disabled=move || busy.get() || on_block.is_none()
                on:click=move |_| {
                    if let Some(on_block) = on_block {
                        on_block.run(());
                    }
                }
            >
                "Block domain"
            </button>
        </div>

        <div class="mt-4 flex flex-col gap-2">
            <p class="text-sm font-medium">"Import a list"</p>
            <p class="text-xs opacity-60">
                "One domain per line, as a block list exports them. Domains already blocked are skipped."
            </p>
            <textarea
                class="textarea textarea-bordered textarea-sm h-24 font-mono text-xs"
                placeholder="one.domain"
                disabled=on_import.is_none()
                on:input=move |event| {
                    if let Some(on_import) = on_import {
                        on_import.run(event_target_value(&event));
                    }
                }
            ></textarea>
            <div class="flex items-center gap-2">
                <button
                    class="btn btn-sm"
                    disabled=move || busy.get() || on_import_now.is_none()
                    on:click=move |_| {
                        if let Some(on_import_now) = on_import_now {
                            on_import_now.run(());
                        }
                    }
                >
                    "Block these"
                </button>
                <Show when=move || imported.get().is_some()>
                    <span class="text-xs opacity-70">{move || imported.get().unwrap_or_default()}</span>
                </Show>
            </div>
        </div>
    }
}

/// Which action a moderated remote account is under.
///
/// The instance holds two: a silence keeps the account's posts off timelines, and a suspend
/// refuses it altogether. The wire spelling is the instance's word, so it is translated here
/// once rather than in every arm of the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteAction {
    /// The account's posts are kept off timelines; its follows are left alone.
    Silence,
    /// The account is refused altogether.
    Suspend,
}

impl RemoteAction {
    /// Reads the wire spelling. A name this build does not know reads as the narrower
    /// action, which is the honest reading of a word from a newer instance.
    #[must_use]
    pub fn of(name: &str) -> Self {
        match name {
            "suspend" => Self::Suspend,
            _ => Self::Silence,
        }
    }

    /// The wire spelling.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Silence => "silence",
            Self::Suspend => "suspend",
        }
    }

    /// The word the select and the row's status sentence show.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Silence => "Silence",
            Self::Suspend => "Suspend",
        }
    }

    /// The word for the middle of a sentence.
    #[must_use]
    pub const fn gerund(self) -> &'static str {
        match self {
            Self::Silence => "silence",
            Self::Suspend => "suspension",
        }
    }
}

/// One account on another instance this one has silenced or suspended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteAccountRow {
    /// The account's absolute address, which is how it is changed and lifted.
    pub actor_uri: String,
    /// The account's local name on its own instance, as the instance names it.
    pub handle: String,
    /// The instance the account lives on, when the address had one.
    pub domain: Option<String>,
    /// What is being done to it.
    pub action: RemoteAction,
    /// Why, as the administrator who applied it wrote it down.
    pub reason: Option<String>,
    /// When it lapses, if it does. RFC 3339 as the instance sends it.
    pub expires_at: Option<String>,
    /// Whether that moment has already passed, by the instance's clock.
    ///
    /// A lapsed action is doing nothing: the row reads as expired rather than as a silence or
    /// a suspension in force, because an administrator looking at a list of eight rows needs
    /// to see which ones are still biting without reading eight timestamps against today.
    pub expired: bool,
    /// When it was applied.
    pub created_at: String,
    /// Who applied it, as a name this instance knows, when it still knows one.
    pub created_by_handle: Option<String>,
}

impl RemoteAccountRow {
    /// The account as people write it: `someone@instance.example`.
    #[must_use]
    pub fn name(&self) -> String {
        match &self.domain {
            Some(domain) => format!("{}@{domain}", self.handle),
            None => self.handle.clone(),
        }
    }

    /// Whether the action is doing anything right now.
    #[must_use]
    pub const fn in_force(&self) -> bool {
        !self.expired
    }

    /// The row's standing, in one sentence: what is being done and until when.
    ///
    /// An expired row says so in words: "expired on 2026-11-01" is a record of something that
    /// happened, and a reader who sees only "Suspend" would take it for something happening.
    #[must_use]
    pub fn standing(&self) -> String {
        let lapses = self.expires_at.as_deref().map(day_of);
        match (self.expired, lapses) {
            (false, Some(day)) => format!("{} in force until {day}", self.action.label()),
            (false, None) => format!("{} in force, with no expiry", self.action.label()),
            (true, Some(day)) => format!(
                "Expired on {day}: this {} is no longer in force",
                self.action.gerund()
            ),
            (true, None) => format!(
                "Expired: this {} is no longer in force",
                self.action.gerund()
            ),
        }
    }

    /// Who applied it and when, for the row's small print.
    #[must_use]
    pub fn attribution(&self) -> String {
        match self.created_by_handle.as_deref() {
            Some(who) => format!("Applied by {who} on {}", day_of(&self.created_at)),
            None => format!(
                "Applied by an account since deleted on {}",
                day_of(&self.created_at)
            ),
        }
    }

    /// The expiry as the date input takes it, empty when it does not lapse.
    #[must_use]
    pub fn expiry_day(&self) -> String {
        self.expires_at.as_deref().map(day_of).unwrap_or_default()
    }
}

/// What one row's controls would send, held until the press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteAccountDraft {
    /// The action the select holds.
    pub action: RemoteAction,
    /// The reason as typed. Empty means no reason recorded.
    pub reason: String,
    /// The expiry as the date input holds it, `YYYY-MM-DD`, or empty for none.
    pub expires_at: String,
}

/// Moderated accounts on other instances, each row applying or lifting its own action.
///
/// The list sits beside the domain blocks because both are decisions about what the people
/// here have to see. A row carries its own controls rather than a shared form above the list:
/// the action, the reason and the expiry all belong to the account in that row, and a shared
/// form would need a selection first and a way to tell which row it applied to after.
#[component]
pub fn RemoteAccountsPage(
    /// The rows as the caller last read them.
    #[prop(into)]
    accounts: Signal<Vec<RemoteAccountRow>>,
    /// The row whose lift is waiting on a confirmation, by actor address.
    #[prop(into)]
    confirming: Signal<Option<String>>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal, when it had one.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Applies a row's action. Absent where nothing can be sent.
    #[prop(optional)]
    on_apply: Option<Callback<(String, RemoteAccountDraft)>>,
    /// Asks to lift one row, so the confirmation can be drawn. Absent where nothing can be sent.
    #[prop(optional)]
    on_ask: Option<Callback<String>>,
    /// Takes back the question.
    #[prop(optional)]
    on_cancel: Option<Callback<()>>,
    /// Lifts one row, once confirmed. Absent where nothing can be sent.
    #[prop(optional)]
    on_lift: Option<Callback<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    view! {
        <Panel
            title="Remote accounts"
            description="Accounts on other instances this one silences or suspends. A lapsed action stays on record, and its row says so: an expiry that has passed is not doing anything."
        >
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>

            <Show when=move || accounts.get().is_empty()>
                <p class="text-sm opacity-60">"No remote account is moderated."</p>
            </Show>

            <ul class="flex flex-col gap-2">
                <For each=move || accounts.get() key=|account| account.actor_uri.clone() let:account>
                    {
                        let target = StoredValue::new(account.actor_uri.clone());
                        let name = account.name();
                        let standing = account.standing();
                        let attribution = account.attribution();
                        let reason = account.reason.clone();
                        let has_reason = reason.is_some();
                        let reason_text = reason.unwrap_or_default();
                        let reason_shown = StoredValue::new(reason_text.clone());
                        let lapsed = account.expired;
                        let badge = if lapsed { "badge badge-ghost badge-sm" } else { "badge badge-warning badge-sm" };
                        let badge_word = if lapsed { "Expired" } else { "In force" };
                        let draft = StoredValue::new(RemoteAccountDraft {
                            action: account.action,
                            reason: reason_text.clone(),
                            expires_at: account.expiry_day(),
                        });
                        let select_label = format!("Action for {name}");
                        let reason_label = format!("Reason for {name}");
                        let expiry_label = format!("Expiry for {name}");
                        view! {
                            <li class="flex flex-col gap-2 rounded-box bg-base-100 px-3 py-2 shadow-sm">
                                <div class="flex flex-wrap items-center justify-between gap-2">
                                    <div class="min-w-0">
                                        <p class="truncate text-sm">
                                            <span class="font-medium">{name.clone()}</span>
                                            <span class=badge>{badge_word}</span>
                                        </p>
                                        <p class="truncate text-xs opacity-60">{target.get_value()}</p>
                                    </div>
                                    <p class="text-xs">{standing.clone()}</p>
                                </div>
                                <Show when=move || has_reason>
                                    <p class="truncate text-xs opacity-60">{move || reason_shown.get_value()}</p>
                                </Show>
                                <p class="text-xs opacity-60">{attribution.clone()}</p>
                                <div class="flex flex-wrap items-center gap-2">
                                    <select
                                        class="select select-bordered select-xs"
                                        aria-label=select_label
                                        disabled=move || on_apply.is_none() || !write.get()
                                        title=move || (!write.get()).then_some(crate::props::NO_WRITE).unwrap_or_default()
                                        on:change=move |event| {
                                            let action = RemoteAction::of(&event_target_value(&event));
                                            draft.update_value(|draft| draft.action = action);
                                        }
                                    >
                                        <option value="silence" selected=account.action == RemoteAction::Silence>
                                            "Silence"
                                        </option>
                                        <option value="suspend" selected=account.action == RemoteAction::Suspend>
                                            "Suspend"
                                        </option>
                                    </select>
                                    <input
                                        class="input input-bordered input-xs w-56"
                                        placeholder="Reason"
                                        aria-label=reason_label
                                        prop:value=reason_text.clone()
                                        disabled=move || on_apply.is_none() || !write.get()
                                        on:input=move |event| {
                                            let typed = event_target_value(&event);
                                            draft.update_value(|draft| draft.reason = typed);
                                        }
                                    />
                                    <input
                                        type="date"
                                        class="input input-bordered input-xs"
                                        aria-label=expiry_label
                                        prop:value=account.expiry_day()
                                        disabled=move || on_apply.is_none() || !write.get()
                                        on:input=move |event| {
                                            let day = event_target_value(&event);
                                            draft.update_value(|draft| draft.expires_at = day);
                                        }
                                    />
                                    <button
                                        class="btn btn-warning btn-xs"
                                        disabled=move || busy.get() || on_apply.is_none() || !write.get()
                                        title=move || (!write.get()).then_some(crate::props::NO_WRITE).unwrap_or_default()
                                        on:click=move |_| {
                                            if let Some(on_apply) = on_apply {
                                                on_apply.run((target.get_value(), draft.get_value()));
                                            }
                                        }
                                    >
                                        "Apply"
                                    </button>
                                    <Show when=move || {
                                        confirming.get().as_deref() == Some(target.get_value().as_str())
                                    }>
                                        <div class="flex items-center gap-1">
                                            <span class="text-xs opacity-70">"Lift this action?"</span>
                                            <button
                                                class="btn btn-ghost btn-xs"
                                                on:click=move |_| {
                                                    if let Some(on_cancel) = on_cancel {
                                                        on_cancel.run(());
                                                    }
                                                }
                                            >
                                                "Keep"
                                            </button>
                                            <button
                                                class="btn btn-error btn-xs"
                                                disabled=move || busy.get() || on_lift.is_none() || !write.get()
                                                on:click=move |_| {
                                                    if let Some(on_lift) = on_lift {
                                                        on_lift.run(target.get_value());
                                                    }
                                                }
                                            >
                                                "Lift action"
                                            </button>
                                        </div>
                                    </Show>
                                    <Show when=move || {
                                        confirming.get().as_deref() != Some(target.get_value().as_str())
                                    }>
                                        <button
                                            class="btn btn-ghost btn-xs"
                                            disabled=move || busy.get() || on_ask.is_none() || !write.get()
                                            title=move || (!write.get()).then_some(crate::props::NO_WRITE).unwrap_or_default()
                                            on:click=move |_| {
                                                if let Some(on_ask) = on_ask {
                                                    on_ask.run(target.get_value());
                                                }
                                            }
                                        >
                                            "Lift"
                                        </button>
                                    </Show>
                                </div>
                            </li>
                        }
                    }
                </For>
            </ul>
        </Panel>
    }
}

/// The date part of an RFC 3339 stamp, as the row reads it.
fn day_of(stamp: &str) -> String {
    stamp
        .split_once('T')
        .map_or(stamp, |(day, _)| day)
        .to_owned()
}
