//! The report queue: the list a moderator filters, and the decision one report carries.
//!
//! The filter, the selection and every request belong to the caller; the page draws what it
//! was handed and asks for the verb. A control the caller cannot send keeps its box and
//! loses only the press, so a paint with no token still shows what the screen is.

use leptos::prelude::*;
use nodus_client_core::model::{Report, ReportState};

/// What the list is filtered by. Empty strings mean "no restriction".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReportQuery {
    /// `open`, `resolved`, or empty for both.
    pub state: String,
    /// A category the server knows, verbatim.
    pub category: String,
    /// The account id reports are assigned to, verbatim.
    pub assignee: String,
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
) -> impl IntoView {
    let category = StoredValue::new(String::new());
    let assignee = StoredValue::new(String::new());
    let note = StoredValue::new(String::new());
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
                    {row(report.clone(), age_of, on_open)}
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
    view! {
        <li class="rounded-box border border-base-300 bg-base-100 p-3">
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
