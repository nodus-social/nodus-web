//! The report queue: the list a moderator filters, and the decision one report carries.
//!
//! The filter, the selection and every request belong to the caller; the page draws what it
//! was handed and asks for the verb. A control the caller cannot send keeps its box and
//! loses only the press, so a paint with no token still shows what the screen is.

use leptos::prelude::*;
use nodus_client_core::model::{Report, ReportState};

use super::{BulkOutcome, ReportBatch, ReportBulkAction};
use crate::pages::admin::federation::forwarded::{Forward, forward_of};

/// What forwarding has answered about this queue's rows so far.
///
/// The queue keeps this rather than a per-row flag because a refusal belongs to the row
/// that caused it, and two presses on two rows answer independently. It is the page's own
/// state: the caller records the answers, the page reads them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Forwarding {
    /// Reports whose forward is on its way, by id.
    pub in_flight: Vec<String>,
    /// The last refusal per report id, in the instance's own words.
    pub refusals: Vec<(String, String)>,
}

impl Forwarding {
    /// Whether this report's forward has not been answered yet.
    #[must_use]
    pub fn sending(&self, report_id: &str) -> bool {
        self.in_flight.iter().any(|held| held == report_id)
    }

    /// The instance's refusal of this report's forward, when it refused one.
    #[must_use]
    pub fn refusal_for(&self, report_id: &str) -> Option<&str> {
        self.refusals
            .iter()
            .find(|(held, _)| held == report_id)
            .map(|(_, reason)| reason.as_str())
    }
}

/// What the list is filtered by. Empty strings mean "no restriction".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportQuery {
    /// `open`, `resolved`, or empty for both.
    pub state: String,
    /// A category the server knows, verbatim.
    pub category: String,
    /// The account id reports are assigned to, verbatim.
    pub assignee: String,
}

impl Default for ReportQuery {
    /// Opens on what still needs a decision: a queue that started on every report ever
    /// filed would bury the open ones under the closed ones.
    fn default() -> Self {
        Self {
            state: "open".to_owned(),
            category: String::new(),
            assignee: String::new(),
        }
    }
}

impl ReportQuery {
    /// The state the server is asked for, when the query names one.
    #[must_use]
    pub fn state_value(&self) -> Option<ReportState> {
        match self.state.as_str() {
            "open" => Some(ReportState::Open),
            "resolved" => Some(ReportState::Resolved),
            _ => None,
        }
    }
}

/// The report queue.
#[component]
pub fn ReportsPage(
    /// The reports, in the server's order.
    #[prop(into)]
    reports: Signal<Vec<Report>>,
    /// The filter the caller last applied.
    #[prop(into)]
    query: Signal<ReportQuery>,
    /// Whether a read is on its way.
    #[prop(into)]
    loading: Signal<bool>,
    /// The last read's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Whether the server has more reports after these.
    #[prop(into)]
    has_more: Signal<bool>,
    /// The report whose detail panel is open.
    #[prop(into)]
    selected: Signal<Option<Report>>,
    /// The reports already forwarded, so a row shows what became of its forward.
    #[prop(into)]
    forwarded: Signal<Vec<Forward>>,
    /// Which rows are in flight, and what forwarding refused.
    #[prop(into)]
    forwarding: Signal<Forwarding>,
    /// How long ago a report was filed.
    #[prop(optional)]
    age_of: Option<Callback<String, String>>,
    /// Applies a filter.
    #[prop(optional)]
    on_query: Option<Callback<ReportQuery>>,
    /// Asks for the next page.
    #[prop(optional)]
    on_more: Option<Callback<()>>,
    /// Opens one report's detail.
    #[prop(optional)]
    on_open: Option<Callback<String>>,
    /// Closes the detail.
    #[prop(optional)]
    on_close: Option<Callback<()>>,
    /// Resolves a report with a note, which may be empty.
    #[prop(optional)]
    on_resolve: Option<Callback<(String, String)>>,
    /// Reopens a report.
    #[prop(optional)]
    on_reopen: Option<Callback<String>>,
    /// Assigns a report to an account id, or to nobody.
    #[prop(optional)]
    on_assign: Option<Callback<(String, Option<String>)>>,
    /// What the last batch answered: its summary and the rows it refused.
    #[prop(into)]
    bulk: Signal<BulkOutcome>,
    /// Applies one action to every ticked report.
    #[prop(optional)]
    on_bulk: Option<Callback<ReportBatch>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
    /// Whether the instance serves the forwarding endpoints (`report_forwarding`). Absent
    /// means "assume yes".
    #[prop(optional_no_strip)]
    forwardable: Option<Signal<bool>>,
    /// Forwards one report to its peer, with a note that may be empty.
    #[prop(optional)]
    on_forward: Option<Callback<(String, String)>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    let forwardable = crate::props::capable(forwardable);
    let category = StoredValue::new(String::new());
    let assignee = StoredValue::new(String::new());
    let note = StoredValue::new(String::new());
    // Which rows are ticked is this list's own state; the batch bar reads it. Not called
    // `selected`: that name is the report whose detail is open.
    let ticked = RwSignal::new(Vec::<String>::new());
    let chosen = StoredValue::new(ReportBulkAction::Resolve);
    let bulk_note = StoredValue::new(String::new());
    let bulk_assignee = StoredValue::new(String::new());
    // A `Callback`, not a closure: the tick sits inside a `<Show>` in the row, and a
    // render-time child has to be shareable between threads.
    let toggle = Callback::new(move |(id, on): (String, bool)| {
        ticked.update(|held| {
            if on {
                if !held.contains(&id) {
                    held.push(id);
                }
            } else {
                held.retain(|ticked| ticked != &id);
            }
        });
    });
    let applied = StoredValue::new(query.get_untracked());
    let submit = move || ReportQuery {
        state: applied.get_value().state,
        category: category.get_value().trim().to_owned(),
        assignee: assignee.get_value().trim().to_owned(),
    };

    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">{move || error.get().unwrap_or_default()}</p>
            </Show>

            <div class="flex flex-col gap-2 rounded-box border border-base-300 bg-base-100 p-3">
                <div class="flex flex-wrap items-center gap-2">
                    {state_button("Open", "open", applied, on_query, submit)}
                    {state_button("Resolved", "resolved", applied, on_query, submit)}
                    {state_button("All", "", applied, on_query, submit)}
                </div>
                <div class="flex flex-wrap gap-2">
                    <input
                        type="text"
                        class="input input-sm w-full sm:w-48"
                        placeholder="Category"
                        on:input=move |event| category.set_value(event_target_value(&event))
                    />
                    <input
                        type="text"
                        class="input input-sm w-full sm:w-56"
                        placeholder="Assigned account id"
                        on:input=move |event| assignee.set_value(event_target_value(&event))
                    />
                    <button
                        class="btn btn-sm"
                        disabled=on_query.is_none()
                        on:click=move |_| {
                            if let Some(on_query) = on_query {
                                on_query.run(submit());
                            }
                        }
                    >
                        "Apply"
                    </button>
                </div>
            </div>

            <Show when=move || write.get() && !ticked.get().is_empty()>
                <div
                    class="flex flex-wrap items-end gap-2 rounded-box border border-base-300 bg-base-200 px-3 py-2"
                    role="group"
                    aria-label="Bulk report action"
                >
                    <p class="text-sm font-medium">
                        {move || format!("{} selected", ticked.get().len())}
                    </p>
                    <select
                        class="select select-bordered select-sm"
                        aria-label="Bulk action"
                        on:change=move |event| {
                            let picked = event_target_value(&event);
                            if let Some(found) = ReportBulkAction::ALL
                                .into_iter()
                                .find(|candidate| candidate.wire() == picked)
                            {
                                chosen.set_value(found);
                            }
                        }
                    >
                        {ReportBulkAction::ALL
                            .into_iter()
                            .map(|candidate| {
                                view! { <option value=candidate.wire()>{candidate.label()}</option> }
                            })
                            .collect_view()}
                    </select>
                    <input
                        class="input input-bordered input-sm w-full sm:w-56"
                        placeholder="Assignee account id (Assign)"
                        on:input=move |event| bulk_assignee.set_value(event_target_value(&event))
                    />
                    <input
                        class="input input-bordered input-sm w-full sm:w-56"
                        placeholder="Note (Resolve)"
                        on:input=move |event| bulk_note.set_value(event_target_value(&event))
                    />
                    <button
                        class="btn btn-primary btn-sm"
                        disabled=move || on_bulk.is_none()
                        on:click=move |_| {
                            if let Some(on_bulk) = on_bulk {
                                let ids = ticked.get_untracked();
                                if ids.is_empty() {
                                    return;
                                }
                                on_bulk
                                    .run(ReportBatch {
                                        ids,
                                        action: chosen.get_value(),
                                        assignee: Some(bulk_assignee.get_value().trim().to_owned())
                                            .filter(|id| !id.is_empty()),
                                        note: bulk_note.get_value(),
                                    });
                                // The batch has gone out; the answer comes back per row, so
                                // the ticks have done their work and are cleared.
                                ticked.set(Vec::new());
                            }
                        }
                    >
                        "Apply to selection"
                    </button>
                    <button class="btn btn-ghost btn-sm" on:click=move |_| ticked.set(Vec::new())>
                        "Clear"
                    </button>
                </div>
            </Show>

            <Show when=move || bulk.get() != BulkOutcome::default()>
                <p class="text-sm" role="status">
                    {move || bulk.get().summary()}
                </p>
            </Show>

            <Show when=move || selected.get().is_some()>
                {move || {
                    selected
                        .get()
                        .map(|report| {
                            view! {
                                <div class="rounded-box border border-base-300 bg-base-100 p-3">
                                    {detail(report.clone(), age_of, on_close, on_resolve, on_reopen, on_assign, note)}
                                </div>
                            }
                        })
                }}
            </Show>

            <Show when=move || loading.get() && reports.get().is_empty()>
                <p class="text-sm opacity-60">"Reading reports…"</p>
            </Show>

            <ul class="flex flex-col gap-2">
                <For each=move || reports.get() key=|report| report.id.clone() let:report>
                    {row(
                        report.clone(),
                        age_of,
                        on_open,
                        ticked,
                        bulk,
                        write,
                        toggle,
                        forwarded,
                        forwarding,
                        forwardable,
                        on_forward,
                    )}
                </For>
            </ul>

            <Show when=move || !loading.get() && reports.get().is_empty()>
                <p class="text-sm opacity-70">"Nothing here with this filter."</p>
            </Show>

            <Show when=move || has_more.get()>
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=move || loading.get() || on_more.is_none()
                    on:click=move |_| {
                        if let Some(on_more) = on_more {
                            on_more.run(());
                        }
                    }
                >
                    {move || if loading.get() { "Loading…" } else { "Load more" }}
                </button>
            </Show>
        </div>
    }
}

/// One state choice; the applied state, not the caller's later reading, says which is pressed.
fn state_button(
    label: &'static str,
    value: &'static str,
    applied: StoredValue<ReportQuery>,
    on_query: Option<Callback<ReportQuery>>,
    submit: impl Fn() -> ReportQuery + Copy + 'static,
) -> impl IntoView {
    let active = move || applied.get_value().state == value;
    view! {
        <button
            class=move || if active() { "btn btn-xs btn-primary" } else { "btn btn-xs btn-ghost" }
            disabled=on_query.is_none()
            on:click=move |_| {
                let mut next = submit();
                next.state = value.to_owned();
                applied.set_value(next.clone());
                if let Some(on_query) = on_query {
                    on_query.run(next);
                }
            }
        >
            {label}
        </button>
    }
}

/// One row of the queue.
fn row(
    report: Report,
    age_of: Option<Callback<String, String>>,
    on_open: Option<Callback<String>>,
    ticked: RwSignal<Vec<String>>,
    bulk: Signal<BulkOutcome>,
    write: Signal<bool>,
    toggle: Callback<(String, bool)>,
    forwarded: Signal<Vec<Forward>>,
    forwarding: Signal<Forwarding>,
    forwardable: Signal<bool>,
    on_forward: Option<Callback<(String, String)>>,
) -> impl IntoView {
    let id = report.id.clone();
    let target = report.target_account.as_ref().map_or_else(
        || "unknown account".to_owned(),
        |account| format!("@{}", account.acct),
    );
    let reporter = report.reporter.as_ref().map_or_else(
        || "someone".to_owned(),
        |account| format!("@{}", account.acct),
    );
    let when = age_of.map_or_else(
        || report.created_at.clone(),
        |age_of| age_of.run(report.created_at.clone()),
    );
    let state = report.state.clone();
    let assigned = report.assigned_account.as_ref().map_or_else(
        || "unassigned".to_owned(),
        |account| format!("@{}", account.acct),
    );
    let comment = report.comment.clone();
    let comment_gate = comment.clone();
    let category = if report.category.is_empty() {
        "uncategorised".to_owned()
    } else {
        report.category.clone()
    };
    let label = format!("Select the report on {target}");
    // Three handles on one id: a `move` closure can own a value once, and the row has three
    // of them — the tick, the press, and the refusal line.
    let refuse_id = StoredValue::new(report.id.clone());
    let check_id = StoredValue::new(report.id.clone());
    let change_id = StoredValue::new(report.id.clone());
    // Where a remote target is: a report about `@someone@peer.example` is the only kind that
    // can leave this instance, so the host half of the handle is what the forward addresses.
    let peer = report.target_account.as_ref().and_then(|account| {
        account
            .acct
            .split_once('@')
            .map(|(_, host)| host.to_owned())
    });
    let peer = StoredValue::new(peer);
    let forward_id = StoredValue::new(report.id.clone());
    let forward_note = StoredValue::new(String::new());
    view! {
        <li class="flex items-start gap-2">
            <Show when=move || write.get()>
                <input
                    type="checkbox"
                    class="checkbox checkbox-sm mt-3"
                    aria-label=label.clone()
                    prop:checked=move || ticked.get().iter().any(|held| held == &check_id.get_value())
                    on:change=move |event| {
                        toggle.run((change_id.get_value(), event_target_checked(&event)));
                    }
                />
            </Show>
            <div class="min-w-0 flex-1">
            <button
                class="flex w-full flex-col gap-1 text-left"
                disabled=on_open.is_none()
                on:click=move |_| {
                    if let Some(on_open) = on_open {
                        on_open.run(id.clone());
                    }
                }
            >
                <span class="flex flex-wrap items-baseline gap-2 text-sm">
                    <span class="font-medium">{target}</span>
                    <span class="opacity-60">{format!("reported by {reporter}")}</span>
                    <span class="badge badge-sm">{state}</span>
                </span>
                <span class="text-xs opacity-60">{format!("{category} · {when} · {assigned}")}</span>
                <Show when=move || !comment_gate.is_empty()>
                    <span class="line-clamp-2 text-sm opacity-80">{comment.clone()}</span>
                </Show>
            </button>
            <Show when=move || bulk.get().refusal_for(&refuse_id.get_value()).is_some()>
                <p class="mt-1 text-xs text-error" role="alert">
                    {move || bulk.get().refusal_for(&refuse_id.get_value()).unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || peer.get_value().is_some() && forward_of(&forwarded.get(), &forward_id.get_value()).is_some()>
                <div class="mt-1 flex flex-wrap items-baseline gap-2 text-xs" data-testid="forward-state">
                    {move || {
                        forward_of(&forwarded.get(), &forward_id.get_value())
                            .map(|held| {
                                let domain = held.target_domain.clone();
                                let state = held.state.clone();
                                let line = held.state_line();
                                let failure = held.last_error.clone();
                                view! {
                                    <span class="badge badge-sm" data-testid="forward-badge">{state}</span>
                                    <span class="opacity-70">
                                        {format!("Forwarded to {domain}: {line}.")}
                                    </span>
                                    {failure
                                        .map(|error| {
                                            view! { <span class="text-error">{error}</span> }
                                        })}
                                }
                            })
                    }}
                </div>
            </Show>
            <Show when=move || {
                peer.get_value().is_some() && write.get() && forwardable.get() && on_forward.is_some()
                    && forward_of(&forwarded.get(), &forward_id.get_value()).is_none()
            }>
                <div class="mt-1 flex flex-wrap items-center gap-2">
                    <input
                        type="text"
                        class="input input-bordered input-xs w-full sm:w-56"
                        placeholder="Note to the peer (optional)"
                        aria-label={
                            move || {
                                format!(
                                    "Note to {}, attached to the forwarded report",
                                    peer.get_value().unwrap_or_default(),
                                )
                            }
                        }
                        on:input=move |event| forward_note.set_value(event_target_value(&event))
                    />
                    <button
                        type="button"
                        class="btn btn-xs"
                        data-testid="forward-report"
                        title={
                            move || {
                                format!(
                                    "Send this report to the moderators on {}",
                                    peer.get_value().unwrap_or_default(),
                                )
                            }
                        }
                        disabled=move || forwarding.get().sending(&forward_id.get_value())
                        on:click=move |_| {
                            if let Some(on_forward) = on_forward {
                                let id = forward_id.get_value();
                                if forwarding.get_untracked().sending(&id) {
                                    return;
                                }
                                on_forward.run((id, forward_note.get_value().trim().to_owned()));
                            }
                        }
                    >
                        {move || {
                            if forwarding.get().sending(&forward_id.get_value()) {
                                "Forwarding…"
                            } else {
                                "Forward to the peer"
                            }
                        }}
                    </button>
                </div>
            </Show>
            <Show when=move || forwarding.get().refusal_for(&forward_id.get_value()).is_some()>
                <p class="mt-1 text-xs text-error" role="alert" data-testid="forward-refusal">
                    {move || {
                        forwarding
                            .get()
                            .refusal_for(&forward_id.get_value())
                            .map(str::to_owned)
                            .unwrap_or_default()
                    }}
                </p>
            </Show>
            </div>
        </li>
    }
}

/// The detail of one report, with the decisions the server supports for its state.
fn detail(
    report: Report,
    age_of: Option<Callback<String, String>>,
    on_close: Option<Callback<()>>,
    on_resolve: Option<Callback<(String, String)>>,
    on_reopen: Option<Callback<String>>,
    on_assign: Option<Callback<(String, Option<String>)>>,
    note: StoredValue<String>,
) -> impl IntoView {
    let id = StoredValue::new(report.id.clone());
    let target = report.target_account.as_ref().map_or_else(
        || "unknown account".to_owned(),
        |account| format!("@{}", account.acct),
    );
    let reporter = report.reporter.as_ref().map_or_else(
        || "someone".to_owned(),
        |account| format!("@{}", account.acct),
    );
    let when = age_of.map_or_else(
        || report.created_at.clone(),
        |age_of| age_of.run(report.created_at.clone()),
    );
    let assigned = report.assigned_account.as_ref().map_or_else(
        || "nobody".to_owned(),
        |account| format!("@{}", account.acct),
    );
    let comment = report.comment.clone();
    let comment_gate = comment.clone();
    let action = if report.action_taken {
        "Action taken"
    } else {
        "No action taken yet"
    };
    let action_note = report.action_taken_note.clone();
    let action_note_gate = action_note.clone();
    let action_by = report
        .action_taken_by
        .as_ref()
        .map(|account| format!(" by @{}", account.acct))
        .unwrap_or_default();
    let statuses = report
        .statuses
        .iter()
        .map(|status| status.content.clone())
        .collect::<Vec<_>>()
        .join("\n\n");
    let statuses_gate = statuses.clone();
    let open = report.state == "open";
    let has_assignee = report.assigned_account_id.is_some();
    let category = if report.category.is_empty() {
        "uncategorised".to_owned()
    } else {
        report.category.clone()
    };
    let close = move |_| {
        if let Some(on_close) = on_close {
            on_close.run(());
        }
    };
    let resolve_id = id;
    let reopen_id = id;
    let assign_id = id;
    let unassign_id = id;

    view! {
        <div class="flex flex-col gap-2">
            <div class="flex items-start justify-between gap-2">
                <div class="min-w-0">
                    <p class="text-sm font-semibold">{format!("{target} · {}", report.state.clone())}</p>
                    <p class="text-xs opacity-60">{format!("reported by {reporter} · {when}")}</p>
                </div>
                <button class="btn btn-ghost btn-xs" on:click=close>"Close"</button>
            </div>

            <p class="text-xs opacity-60">{format!("{category} · assigned to {assigned}")}</p>
            <Show when=move || !comment_gate.is_empty()>
                <p class="whitespace-pre-wrap text-sm">{comment.clone()}</p>
            </Show>
            <Show when=move || !statuses_gate.is_empty()>
                <div class="max-h-48 overflow-y-auto rounded-box bg-base-200/60 p-2 text-sm whitespace-pre-wrap">
                    {statuses.clone()}
                </div>
            </Show>
            <p class="text-xs opacity-70">{format!("{action}{action_by}")}</p>
            <Show when=move || !action_note_gate.is_empty()>
                <p class="text-xs opacity-70">{format!("Note: {action_note}")}</p>
            </Show>

            <Show when=move || open>
                <textarea
                    class="textarea textarea-sm w-full"
                    rows="2"
                    placeholder="What was done, for the record (optional)"
                    on:input=move |event| note.set_value(event_target_value(&event))
                ></textarea>
                <div class="flex flex-wrap gap-1">
                    <button
                        class="btn btn-primary btn-xs"
                        disabled=on_resolve.is_none()
                        on:click=move |_| {
                            if let Some(on_resolve) = on_resolve {
                                on_resolve
                                    .run((resolve_id.get_value(), note.get_value().trim().to_owned()));
                            }
                        }
                    >
                        "Resolve"
                    </button>
                    <button
                        class="btn btn-ghost btn-xs"
                        disabled=on_assign.is_none()
                        on:click=move |_| {
                            if let Some(on_assign) = on_assign {
                                on_assign.run((assign_id.get_value(), None));
                            }
                        }
                    >
                        "Assign to me"
                    </button>
                    <Show when=move || has_assignee>
                        <button
                            class="btn btn-ghost btn-xs"
                            disabled=on_assign.is_none()
                            on:click=move |_| {
                                if let Some(on_assign) = on_assign {
                                    on_assign.run((unassign_id.get_value(), None));
                                }
                            }
                        >
                            "Unassign"
                        </button>
                    </Show>
                </div>
            </Show>

            <Show when=move || !open>
                <button
                    class="btn btn-ghost btn-xs self-start"
                    disabled=on_reopen.is_none()
                    on:click=move |_| {
                        if let Some(on_reopen) = on_reopen {
                            on_reopen.run(reopen_id.get_value());
                        }
                    }
                >
                    "Reopen"
                </button>
            </Show>
        </div>
    }
}
