//! Domain pauses: a temporary, unannounced stop that is not a block.
//!
//! A block is permanent and announced — the peer is told it is blocked. A pause is
//! neither: it lapses on its own, and the peer is never told why its posts stopped
//! arriving or why this instance stopped delivering to it. The panel says that in words
//! twice — once above the list, once in the row's own standing — because a moderator who
//! reads a pause as a block will go looking for a block they never made, and one who
//! reads a block as a pause will wait for an expiry that does not exist.
//!
//! An expired pause stays on the list: it is the record of something that happened. Its
//! row reads as expired rather than as a pause in force. That mistake was made once
//! already on remote accounts, where a lapsed row still said "in force" and made a list
//! of five look like five live actions.

use leptos::prelude::*;

use crate::Note;
use crate::components::panel::Panel;
use crate::props::{NO_WRITE, writable};

/// The difference between a pause and a block, said once for the whole panel.
pub const PAUSE_VERSUS_BLOCK: &str = "A pause is temporary and unannounced: this instance stops delivering to the domain and keeps its posts out of timelines, and the peer is never told. It ends by itself at the expiry, or when it is lifted. A block is permanent and announced: the peer is told it is blocked. Pause to wait something out; block to refuse it and say so.";

/// One paused domain, as `GET /api/v1/admin/federation/domain-pauses` sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PausedDomain {
    /// The bare hostname the pause was set on, which is also its path segment.
    pub domain: String,
    /// Why, as the administrator who set it wrote it down.
    pub reason: String,
    /// When it lapses, in RFC 3339, when it does.
    pub expires_at: Option<String>,
    /// Whether that moment has already passed, by the instance's clock.
    ///
    /// The instance answers this rather than the console working it out, so a row and the
    /// instance it describes cannot disagree about which pauses are still biting.
    pub expired: bool,
    /// When it was set.
    pub created_at: String,
    /// Who set it, as a name this instance still knows.
    pub created_by_handle: Option<String>,
}

impl PausedDomain {
    /// Whether the pause is stopping anything right now.
    #[must_use]
    pub const fn in_force(&self) -> bool {
        !self.expired
    }

    /// The row's standing in one sentence: what is stopped, and until when.
    ///
    /// An expired row says so in words. "Paused until 2026-11-01" on a lapsed row would
    /// read as a pause still in force that happens to be old, and the reader would look
    /// for a lift that is not needed.
    #[must_use]
    pub fn standing(&self) -> String {
        let lapses = self.expires_at.as_deref().map(day_of);
        match (self.expired, lapses) {
            (false, Some(day)) => format!(
                "Paused until {day}: delivery to this domain and its timeline visibility resume on their own when that day is reached."
            ),
            (false, None) => "Paused with no expiry: it lasts until it is lifted, and it is never announced to the peer.".to_owned(),
            (true, Some(day)) => format!(
                "Expired on {day}: this pause is no longer in force, and delivery to this domain and its timeline visibility have resumed."
            ),
            (true, None) => "Expired: this pause is no longer in force, and delivery and timeline visibility have resumed.".to_owned(),
        }
    }

    /// Who set it and when, for the row's small print.
    #[must_use]
    pub fn attribution(&self) -> String {
        match self.created_by_handle.as_deref() {
            Some(who) => format!("Paused by {who} on {}", day_of(&self.created_at)),
            None => format!(
                "Paused by an account since deleted on {}",
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

/// What the pause form would send, held until the press.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DomainPauseDraft {
    /// The bare hostname, as typed.
    pub domain: String,
    /// The reason as typed. The endpoint takes an empty one; the form asks for it anyway,
    /// because a pause nobody wrote down is one nobody can judge later.
    pub reason: String,
    /// The expiry as the date input holds it, `YYYY-MM-DD`, or empty for none.
    pub expires_at: String,
}

/// Paused domains, the form that adds one, and the lift with its confirmation.
///
/// One panel rather than a form above a list: the form is above the list it adds to, and
/// the lift is on the row it lifts, because the domain is already in the row.
#[component]
pub fn DomainPausesPage(
    /// The rows as the caller last read them, expired ones included.
    #[prop(into)]
    rows: Signal<Vec<PausedDomain>>,
    /// Whether a read or a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// What the last write did, when it has something to say.
    #[prop(optional_no_strip)]
    notice: Option<Signal<Option<String>>>,
    /// The row whose lift is waiting on a confirmation, by domain.
    #[prop(optional_no_strip)]
    confirming: Option<Signal<Option<String>>>,
    /// Pauses a domain. Absent where nothing can be sent.
    #[prop(optional)]
    on_pause: Option<Callback<DomainPauseDraft>>,
    /// Asks to lift one row, so the confirmation can be drawn.
    #[prop(optional)]
    on_ask_lift: Option<Callback<String>>,
    /// Takes back the question.
    #[prop(optional)]
    on_cancel_lift: Option<Callback<()>>,
    /// Lifts one row, once confirmed. Absent where nothing can be sent.
    #[prop(optional)]
    on_lift: Option<Callback<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = writable(write);
    let (domain, set_domain) = signal(String::new());
    let (reason, set_reason) = signal(String::new());
    let (expiry, set_expiry) = signal(String::new());
    let confirming = confirming.unwrap_or_else(|| Signal::derive(|| None::<String>));

    let submit = {
        move || {
            let draft = DomainPauseDraft {
                domain: domain.get_untracked().trim().to_owned(),
                reason: reason.get_untracked().trim().to_owned(),
                expires_at: expiry.get_untracked().trim().to_owned(),
            };
            if draft.domain.is_empty() {
                return;
            }
            if let Some(on_pause) = on_pause {
                on_pause.run(draft);
            }
        }
    };

    view! {
        <Panel
            title="Paused domains"
            description="Domains this instance has stopped delivering to and hidden from timelines, for a while, without telling them."
        >
            <Note class="text-sm opacity-80" attr:data-testid="pause-versus-block">
                {PAUSE_VERSUS_BLOCK}
            </Note>

            <Show when=move || error.get().is_some()>
                <p class="alert alert-error alert-soft py-1 text-sm" role="alert" data-testid="pause-error">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || notice.and_then(|notice| notice.get()).is_some()>
                <p class="alert alert-success alert-soft py-1 text-sm" role="status" data-testid="pause-notice">
                    {move || notice.and_then(|notice| notice.get()).unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || !write.get()>
                <Note class="text-sm text-warning">{NO_WRITE}</Note>
            </Show>

            <form
                class="mb-4 flex flex-wrap items-end gap-2"
                on:submit=move |event: leptos::ev::SubmitEvent| {
                    event.prevent_default();
                    submit();
                }
            >
                <label class="flex flex-col gap-1 text-xs" for="pause-domain">
                    <span>"Domain"</span>
                    <input
                        id="pause-domain"
                        type="text"
                        class="input input-bordered input-sm w-56"
                        placeholder="peer.example"
                        data-testid="pause-domain"
                        prop:value=move || domain.get()
                        on:input=move |event| set_domain.set(event_target_value(&event))
                    />
                </label>
                <label class="flex flex-col gap-1 text-xs" for="pause-reason">
                    <span>"Reason"</span>
                    <input
                        id="pause-reason"
                        type="text"
                        class="input input-bordered input-sm w-72"
                        placeholder="why this domain is paused"
                        data-testid="pause-reason"
                        prop:value=move || reason.get()
                        on:input=move |event| set_reason.set(event_target_value(&event))
                    />
                </label>
                <label class="flex flex-col gap-1 text-xs" for="pause-expiry">
                    <span>"Until (optional)"</span>
                    <input
                        id="pause-expiry"
                        type="date"
                        class="input input-bordered input-sm"
                        data-testid="pause-expiry"
                        prop:value=move || expiry.get()
                        on:input=move |event| set_expiry.set(event_target_value(&event))
                    />
                </label>
                <button
                    type="submit"
                    class="btn btn-sm btn-warning"
                    data-testid="pause-submit"
                    disabled=move || on_pause.is_none() || busy.get() || !write.get()
                    title=move || if write.get() { Default::default() } else { NO_WRITE }
                >
                    "Pause domain"
                </button>
            </form>

            <Show when=move || rows.get().is_empty() && !busy.get()>
                <p class="text-sm opacity-60" data-testid="pause-empty">
                    "No domain is paused."
                </p>
            </Show>

            <ul class="divide-y divide-base-300">
                <For each=move || rows.get() key=|row| row.domain.clone() let:row>
                    {
                        let domain = StoredValue::new(row.domain.clone());
                        let name = row.domain.clone();
                        let standing = row.standing();
                        let attribution = row.attribution();
                        let reason = row.reason.clone();
                        let has_reason = !reason.trim().is_empty();
                        let lapsed = row.expired;
                        let badge = if lapsed { "badge badge-ghost badge-sm" } else { "badge badge-warning badge-sm" };
                        let badge_word = if lapsed { "Expired" } else { "Paused" };
                        view! {
                            <li class="flex flex-col gap-1 py-2 text-sm" data-testid="pause-row">
                                <div class="flex flex-wrap items-baseline justify-between gap-2">
                                    <span class="font-medium" data-testid="pause-row-domain">{name.clone()}</span>
                                    <span class=badge data-testid="pause-state">{badge_word}</span>
                                </div>
                                <p
                                    class=if lapsed { "text-xs opacity-60" } else { "text-xs" }
                                    data-testid="pause-standing"
                                >
                                    {standing.clone()}
                                </p>
                                <Show when=move || has_reason>
                                    <p class="text-xs opacity-70" data-testid="pause-reason-text">
                                        {reason.clone()}
                                    </p>
                                </Show>
                                <p class="text-xs opacity-60" data-testid="pause-attribution">{attribution.clone()}</p>
                                <Show when=move || !lapsed>
                                    <div class="flex flex-wrap items-center gap-2">
                                        <Show
                                            when=move || confirming.get().as_deref() == Some(domain.get_value().as_str())
                                            fallback=move || {
                                                let domain = domain;
                                                view! {
                                                    <button
                                                        type="button"
                                                        class="btn btn-ghost btn-xs"
                                                        data-testid="pause-lift"
                                                        disabled=move || on_ask_lift.is_none() || busy.get() || !write.get()
                                                        title=move || if write.get() { Default::default() } else { NO_WRITE }
                                                        on:click=move |_| {
                                                            if let Some(on_ask_lift) = on_ask_lift {
                                                                on_ask_lift.run(domain.get_value());
                                                            }
                                                        }
                                                    >
                                                        "Lift pause"
                                                    </button>
                                                }
                                            }
                                        >
                                            <span class="text-xs opacity-70">
                                                "Lift the pause on this domain now? Delivery and timeline visibility resume at once."
                                            </span>
                                            <button
                                                type="button"
                                                class="btn btn-warning btn-xs"
                                                data-testid="pause-lift-confirm"
                                                disabled=move || on_lift.is_none() || busy.get() || !write.get()
                                                on:click=move |_| {
                                                    if let Some(on_lift) = on_lift {
                                                        on_lift.run(domain.get_value());
                                                    }
                                                    if let Some(on_cancel_lift) = on_cancel_lift {
                                                        on_cancel_lift.run(());
                                                    }
                                                }
                                            >
                                                "Lift now"
                                            </button>
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs"
                                                data-testid="pause-lift-cancel"
                                                on:click=move |_| {
                                                    if let Some(on_cancel_lift) = on_cancel_lift {
                                                        on_cancel_lift.run(());
                                                    }
                                                }
                                            >
                                                "Cancel"
                                            </button>
                                        </Show>
                                    </div>
                                </Show>
                            </li>
                        }
                    }
                </For>
            </ul>

            <Show when=move || busy.get()>
                <p class="text-sm opacity-60">"Reading the paused domains…"</p>
            </Show>
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
