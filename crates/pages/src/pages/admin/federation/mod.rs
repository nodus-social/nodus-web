//! Federation delivery: what this instance still owes its peers, and why a delivery stopped.
//!
//! A queued delivery is invisible from anywhere but `curl` otherwise. The list answers the
//! first question — is anything stuck? — and the states carry their whole-queue counts, so
//! "three gave up" is visible before a single row is read.
//!
//! The reason a `gave_up` row stopped is the point of the screen, and the two cases read
//! differently on purpose. A peer that answered 4xx answered us: a 401 or 403 on a signed
//! request is the peer refusing our signature, and retrying it alone cannot fix that — the
//! peer's copy of our key has to agree first. A peer that answered 5xx, or never answered,
//! was down; the queue retried with backoff and only then gave up. A moderator who cannot
//! tell those apart will retry a refused signature forever.

use leptos::prelude::*;

use crate::components::panel::Panel;
use crate::props::{writable, NO_WRITE};
use crate::Note;

pub mod forwarded;
pub mod pauses;

/// How many rows one page asks for.
pub const PAGE: u16 = 20;

/// One delivery, as `GET /api/v1/admin/federation/deliveries` sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    /// The row's id, which the retry names.
    pub id: String,
    /// The inbox the activity was posted to.
    pub inbox_url: String,
    /// The peer, as the queue keys it.
    pub host: String,
    /// What kind of activity it is, when the queue recorded one.
    pub activity_kind: Option<String>,
    /// `pending`, `delivered`, `failed` or `gave_up`.
    pub state: String,
    /// How many times it has been tried.
    pub attempts: i32,
    /// What the peer answered last, when it answered at all.
    pub last_status_code: Option<i32>,
    /// What went wrong last, as the transport worded it.
    pub last_error: Option<String>,
    /// When the queue will try it again.
    pub next_attempt_at: String,
    /// When it was queued.
    pub created_at: String,
    /// When it got through, for a delivered row.
    pub delivered_at: Option<String>,
}

/// The whole queue's count per state, so a filter can show its own size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Waiting to be sent.
    pub pending: i64,
    /// Tried, failed, and waiting for its next attempt.
    pub failed: i64,
    /// Out of attempts.
    pub gave_up: i64,
    /// Reached the peer.
    pub delivered: i64,
}

impl Counts {
    /// The whole queue, whichever state a row is in.
    #[must_use]
    pub const fn all(self) -> i64 {
        self.pending + self.failed + self.gave_up + self.delivered
    }

    /// The count beside one filter, `None` being the unfiltered one.
    #[must_use]
    pub fn of(self, state: Option<&str>) -> i64 {
        match state {
            None => self.all(),
            Some("pending") => self.pending,
            Some("failed") => self.failed,
            Some("gave_up") => self.gave_up,
            Some("delivered") => self.delivered,
            Some(_) => 0,
        }
    }
}

/// The filter in force: which state, and which peer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// The state asked for; `None` is the whole queue.
    pub state: Option<String>,
    /// The peer asked for, as typed.
    pub host: String,
}

/// The filters, in the order they are offered, with the word on each.
pub const FILTERS: [Option<&str>; 5] = [
    None,
    Some("pending"),
    Some("failed"),
    Some("gave_up"),
    Some("delivered"),
];

/// The word on a filter.
#[must_use]
pub fn state_label(state: Option<&str>) -> &'static str {
    match state {
        None => "All",
        Some("pending") => "Pending",
        Some("failed") => "Failed",
        Some("gave_up") => "Gave up",
        Some("delivered") => "Delivered",
        Some(_) => "Other",
    }
}

/// "1 attempt" or "3 attempts", so a count reads as a sentence.
#[must_use]
pub fn attempts_line(attempts: i32) -> String {
    if attempts == 1 {
        "1 attempt".to_owned()
    } else {
        format!("{attempts} attempts")
    }
}

/// Why a row stopped, told apart by what the peer answered.
///
/// Only a `gave_up` row is read this way: a `failed` row has a next attempt and has not
/// stopped. The 4xx/5xx split is the one that decides whether a retry can help.
#[must_use]
pub fn stop_line(row: &Delivery) -> String {
    let host = &row.host;
    match row.last_status_code {
        Some(code) if (400..500).contains(&code) => format!(
            "{host} answered {code}: the peer refused the delivery after {}. A 401 or 403 on a signed request means it will not accept our signature, and retrying alone will not fix that — the peer's copy of our key has to agree first.",
            attempts_line(row.attempts)
        ),
        Some(code) if (500..600).contains(&code) => format!(
            "{host} was down: it answered {code} after {}, so the queue retried with backoff and then gave up. This is an outage, not a refusal; retrying once the peer is back should deliver.",
            attempts_line(row.attempts)
        ),
        Some(code) => format!(
            "{host} answered {code} after {}, and the queue gave up.",
            attempts_line(row.attempts)
        ),
        None => format!(
            "{host} never answered ({} after {}): the peer was unreachable, so the queue retried with backoff and then gave up.",
            row.last_error.as_deref().unwrap_or("no detail was recorded"),
            attempts_line(row.attempts)
        ),
    }
}

/// The delivery queue, its filters, and the two ways to retry.
#[component]
pub fn DeliveriesPage(
    /// The rows on the page, newest first.
    #[prop(into)]
    rows: Signal<Vec<Delivery>>,
    /// The whole queue's counts, not this page's.
    #[prop(into)]
    counts: Signal<Counts>,
    /// The filter in force.
    #[prop(into)]
    query: Signal<Query>,
    /// Whether a read or a write is already on its way: an empty list while it is is not
    /// "nothing is waiting".
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// What the last retry did, in the server's own numbers.
    #[prop(optional_no_strip)]
    notice: Option<Signal<Option<String>>>,
    /// Whether another page may exist behind this one.
    #[prop(into)]
    has_more: Signal<bool>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
    /// Re-reads the queue under a new filter.
    #[prop(optional)]
    on_query: Option<Callback<Query>>,
    /// Reads the next page.
    #[prop(optional)]
    on_more: Option<Callback<()>>,
    /// Sends one delivery again, after the row's own confirmation.
    #[prop(optional)]
    on_retry: Option<Callback<String>>,
    /// Sends every failed and given-up delivery again.
    #[prop(optional)]
    on_retry_failed: Option<Callback<()>>,
) -> impl IntoView {
    let write = writable(write);
    // Which row is waiting for its second press. The confirmation is the row's own, so the
    // button a reader presses twice is the one whose delivery is about to be sent.
    let (confirming, set_confirming) = signal(None::<String>);

    let filters = move || {
        FILTERS
            .into_iter()
            .map(|state| {
                let active = query.get().state.as_deref() == state;
                let testid = format!("delivery-filter-{}", state.unwrap_or("all"));
                let text = format!("{} ({})", state_label(state), counts.get().of(state));
                view! {
                    <button
                        type="button"
                        class=if active { "btn btn-xs btn-primary" } else { "btn btn-xs" }
                        data-testid=testid
                        disabled=on_query.is_none()
                        on:click=move |_| {
                            if let Some(on_query) = on_query {
                                on_query.run(Query {
                                    state: state.map(str::to_owned),
                                    host: query.get_untracked().host,
                                });
                            }
                        }
                    >
                        {text}
                    </button>
                }
            })
            .collect_view()
    };

    view! {
        <Panel
            title="Federation delivery"
            description="What this instance still has to deliver to its peers, and why a delivery stopped."
        >
            <Show when=move || error.get().is_some()>
                <p class="alert alert-error alert-soft py-1 text-sm" role="alert" data-testid="delivery-error">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || notice.and_then(|notice| notice.get()).is_some()>
                <p class="alert alert-success alert-soft py-1 text-sm" role="status" data-testid="delivery-notice">
                    {move || notice.and_then(|notice| notice.get()).unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || !write.get()>
                <Note class="text-sm text-warning">{NO_WRITE}</Note>
            </Show>

            <div class="mb-3 flex flex-wrap items-center gap-2" role="group" aria-label="Delivery state">
                {filters}
            </div>

            <div class="mb-3 flex flex-wrap items-end gap-2">
                <label class="flex flex-col gap-1 text-xs" for="delivery-host">
                    <span>"Peer host"</span>
                    <input
                        id="delivery-host"
                        type="text"
                        class="input input-bordered input-sm w-56"
                        placeholder="peer.example"
                        data-testid="delivery-host"
                        prop:value=move || query.get().host
                        on:change=move |event| {
                            if let Some(on_query) = on_query {
                                on_query.run(Query {
                                    state: query.get_untracked().state,
                                    host: event_target_value(&event),
                                });
                            }
                        }
                    />
                </label>
                <button
                    type="button"
                    class="btn btn-sm btn-outline"
                    data-testid="delivery-retry-failed"
                    disabled=move || on_retry_failed.is_none() || busy.get() || !write.get()
                    title=move || (!write.get()).then_some(NO_WRITE).unwrap_or_default()
                    on:click=move |_| {
                        if let Some(on_retry_failed) = on_retry_failed {
                            on_retry_failed.run(());
                        }
                    }
                >
                    "Retry everything failed"
                </button>
            </div>

            <ul class="divide-y divide-base-300">
                <For each=move || rows.get() key=|row| row.id.clone() let:row>
                    {
                        let id = StoredValue::new(row.id.clone());
                        let host = row.host.clone();
                        let kind = row.activity_kind.clone().unwrap_or_else(|| "activity".to_owned());
                        let state = row.state.clone();
                        let state_text = state_label(Some(row.state.as_str())).to_owned();
                        let attempts = attempts_line(row.attempts);
                        let code = row.last_status_code.map(|code| format!("HTTP {code}"));
                        let last_error = row.last_error.clone();
                        let error_shown = row.last_error.clone();
                        let when = if row.state == "delivered" {
                            row.delivered_at.clone().unwrap_or_else(|| row.created_at.clone())
                        } else {
                            row.next_attempt_at.clone()
                        };
                        let when_label = if row.state == "delivered" { "delivered" } else { "next attempt" };
                        let when = when.get(..19).unwrap_or(&when).replace('T', " ");
                        let gave_up = row.state == "gave_up";
                        let failed = row.state == "failed";
                        let reason = gave_up.then(|| stop_line(&row));
                        let retryable = failed || gave_up;
                        let weak = state == "failed" || state == "gave_up";
                        view! {
                            <li class="flex flex-col gap-1 py-2 text-sm" data-testid="delivery-row">
                                <div class="flex flex-wrap items-baseline justify-between gap-2">
                                    <span class="font-medium" data-testid="delivery-row-host">{host.clone()}</span>
                                    <span
                                        class=if weak { "badge badge-sm badge-error" } else { "badge badge-sm badge-ghost" }
                                        data-testid="delivery-state"
                                    >
                                        {state_text}
                                    </span>
                                </div>
                                <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs opacity-70">
                                    <span data-testid="delivery-kind">{kind}</span>
                                    <span data-testid="delivery-attempts">{attempts}</span>
                                    {code.map(|code| view! { <span data-testid="delivery-code">{code}</span> })}
                                    <span data-testid="delivery-when">{format!("{when_label} {when}")}</span>
                                </div>
                                <Show when=move || error_shown.is_some() && !gave_up>
                                    <p class="text-xs opacity-60">{last_error.clone().unwrap_or_default()}</p>
                                </Show>
                                <Show when=move || gave_up>
                                    <p class="text-xs text-error" data-testid="delivery-reason">
                                        {reason.clone().unwrap_or_default()}
                                    </p>
                                </Show>
                                <div class="flex flex-wrap items-center gap-2">
                                    <Show when=move || retryable>
                                        <Show
                                            when=move || {
                                                confirming.get().as_deref() == Some(id.get_value().as_str())
                                            }
                                            fallback=move || {

                                                view! {
                                                    <button
                                                        type="button"
                                                        class="btn btn-ghost btn-xs"
                                                        data-testid="delivery-retry"
                                                        disabled=move || on_retry.is_none() || busy.get() || !write.get()
                                                        title=move || (!write.get()).then_some(NO_WRITE).unwrap_or_default()
                                                        on:click=move |_| set_confirming.set(Some(id.get_value()))
                                                    >
                                                        "Retry"
                                                    </button>
                                                }
                                            }
                                        >
                                            <span class="text-xs opacity-70">"Send it again now?"</span>
                                            <button
                                                type="button"
                                                class="btn btn-warning btn-xs"
                                                data-testid="delivery-retry-confirm"
                                                disabled=move || on_retry.is_none() || busy.get() || !write.get()
                                                on:click=move |_| {
                                                    if let Some(on_retry) = on_retry {
                                                        on_retry.run(id.get_value());
                                                    }
                                                    set_confirming.set(None);
                                                }
                                            >
                                                "Retry now"
                                            </button>
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs"
                                                data-testid="delivery-retry-cancel"
                                                on:click=move |_| set_confirming.set(None)
                                            >
                                                "Cancel"
                                            </button>
                                        </Show>
                                    </Show>
                                </div>
                            </li>
                        }
                    }
                </For>
            </ul>

            <Show when=move || rows.get().is_empty() && !busy.get()>
                <p class="text-sm opacity-60" data-testid="delivery-empty">
                    "Nothing is waiting to be delivered."
                </p>
            </Show>
            <Show when=move || busy.get()>
                <p class="text-sm opacity-60">"Reading the queue…"</p>
            </Show>
            <Show when=move || has_more.get()>
                <button
                    type="button"
                    class="btn btn-ghost btn-sm mt-2"
                    data-testid="delivery-more"
                    disabled=on_more.is_none()
                    on:click=move |_| {
                        if let Some(on_more) = on_more {
                            on_more.run(());
                        }
                    }
                >
                    "Load more"
                </button>
            </Show>
        </Panel>
    }
}
