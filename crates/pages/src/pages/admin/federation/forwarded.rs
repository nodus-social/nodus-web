//! Report forwarding: the reports this instance sent on to a peer, and what became of them.
//!
//! A report about a remote account is never seen by that account's own instance unless this
//! one forwards it, and a forward is a delivery like any other: it can wait, fail, or give
//! up. "Forwarded" therefore still leaves a moderator with the question that matters — was
//! the peer actually told? — so the list is grouped by the state each forward reached.
//!
//! `gave_up` is the one state in which the peer was never told: the account was never
//! actioned on its own instance, so a report left there has to be dealt with another way.

use leptos::prelude::*;

use super::attempts_line;
use crate::components::panel::Panel;

/// The states a forward passes through, in the order a moderator reads them: the one still
/// owed to a peer, the one that arrived, then the two that did not.
pub const GROUPS: [&str; 4] = ["pending", "delivered", "failed", "gave_up"];

/// One forward, as `GET /api/v1/admin/reports/{id}/forward` and
/// `GET /api/v1/admin/federation/forwarded` send it.
///
/// The same row appears on both screens: the queue uses it to say what a single report's
/// forward reached, and this list holds every one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forward {
    /// The forward's own id.
    pub id: String,
    /// The report it carries.
    pub report_id: String,
    /// The peer it was addressed to.
    pub target_domain: String,
    /// The activity a peer would receive, as it was signed.
    pub activity_uri: String,
    /// `pending`, `delivered`, `failed` or `gave_up`.
    pub state: String,
    /// How many times delivery has been tried.
    pub attempts: i32,
    /// What the peer or the queue last said went wrong.
    pub last_error: Option<String>,
    /// When the forward was queued.
    pub created_at: String,
    /// When it last changed.
    pub updated_at: String,
}

impl Forward {
    /// What this state means for the peer, in the sentence a row shows.
    ///
    /// `delivered` and `gave_up` are the two answers to "was the peer told?", and they must
    /// not read alike: one report reached the peer's moderators and the other never left.
    #[must_use]
    pub fn state_line(&self) -> &'static str {
        match self.state.as_str() {
            "pending" => "waiting in the delivery queue, so the peer has not been told yet",
            "delivered" => "the peer's instance accepted it",
            "failed" => "the last attempt failed, and the queue will try again",
            "gave_up" => "the queue gave up, so the peer was never told",
            _ => "the last state this instance recorded",
        }
    }
}

/// The forward already recorded for one report, when there is one.
///
/// A report with a forward shows its state instead of the action: the endpoint is
/// idempotent and would only answer the row that already exists, so offering the press
/// again would invite a second press that cannot do anything.
#[must_use]
pub fn forward_of<'rows>(rows: &'rows [Forward], report_id: &str) -> Option<&'rows Forward> {
    rows.iter().find(|row| row.report_id == report_id)
}

/// The heading one state's group carries.
#[must_use]
pub fn group_label(state: &str) -> &'static str {
    match state {
        "pending" => "Pending",
        "delivered" => "Delivered",
        "failed" => "Failed",
        "gave_up" => "Given up",
        _ => "Other",
    }
}

/// The reports this instance forwarded, grouped by what became of each.
#[component]
pub fn ForwardedPage(
    /// What has been forwarded, newest first.
    #[prop(into)]
    rows: Signal<Vec<Forward>>,
    /// Whether a read is on its way: an empty list while it is is not "nothing was sent".
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
) -> impl IntoView {
    let groups = move || {
        let held = rows.get();
        GROUPS
            .into_iter()
            .map(|state| {
                let group = held
                    .iter()
                    .filter(|row| row.state == state)
                    .cloned()
                    .collect::<Vec<_>>();
                (state, group)
            })
            .filter(|(_, group)| !group.is_empty())
            .map(|(state, group)| {
                let count = group.len();
                let label = format!("{} ({count})", group_label(state));
                view! {
                    <div class="mt-3" data-testid=format!("forwarded-group-{state}")>
                        <p class="text-xs font-semibold uppercase tracking-wide opacity-60">{label}</p>
                        <ul class="divide-y divide-base-300">
                            {group.into_iter().map(sent_row).collect_view()}
                        </ul>
                    </div>
                }
            })
            .collect_view()
    };

    view! {
        <Panel
            title="Report forwarding"
            description="The reports this instance sent to a peer's moderators, grouped by what became of the delivery."
        >
            <Show when=move || error.get().is_some()>
                <p class="alert alert-error alert-soft py-1 text-sm" role="alert" data-testid="forwarded-error">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>

            {groups}

            <Show when=move || busy.get() && rows.get().is_empty()>
                <p class="text-sm opacity-60">"Reading what was forwarded…"</p>
            </Show>
            <Show when=move || !busy.get() && rows.get().is_empty()>
                <p class="text-sm opacity-60" data-testid="forwarded-empty">
                    "Nothing has been forwarded."
                </p>
            </Show>
        </Panel>
    }
}

/// One forwarded report, on one line: which peer, which report, and what went wrong.
fn sent_row(row: Forward) -> impl IntoView {
    let day = row
        .created_at
        .get(..10)
        .map_or_else(|| row.created_at.clone(), str::to_owned);
    let failure = row.last_error.clone();
    view! {
        <li class="flex flex-wrap items-baseline gap-x-3 gap-y-0.5 py-1" data-testid="forwarded-row">
            <span class="text-sm font-medium">{row.target_domain.clone()}</span>
            <span class="text-xs opacity-60">{format!("report {}", row.report_id)}</span>
            <span class="text-xs opacity-60">{format!("queued {day}")}</span>
            <span class="text-xs opacity-60">{attempts_line(row.attempts)}</span>
            {failure.map(|error| view! { <span class="w-full text-xs text-error">{error}</span> })}
        </li>
    }
}
