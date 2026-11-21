//! Appeals: a person disagreed with a decision, and a moderator answers.
//!
//! An appeal is not a report. A report says something about a post; an appeal says the
//! decision about a *person* was wrong, in the person's own words — so the row is built to be
//! read in that order: who they are, what was done to them, what they said, and how long they
//! have waited. The answer is a choice and a note, and the note is what they will be told.
//!
//! The choice is asked for explicitly rather than defaulted: accepting lifts a restriction and
//! rejecting leaves it in force, and there is no reading of "press the button" that picks one
//! of those for the moderator.
//!
//! The queue opens on what still needs an answer, and a pending appeal sorts before a decided
//! one even when the filter shows both: the work is the point of the screen, and a decided
//! appeal is there for the record.

use leptos::prelude::*;

use crate::Note;
use crate::props::{NO_WRITE, writable};

/// Which appeals the queue is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppealFilter {
    /// Nobody has answered them yet: the work list.
    Pending,
    /// The decision was lifted, because the appeal was right.
    Accepted,
    /// The decision stood.
    Rejected,
    /// Every appeal, whenever it was decided.
    All,
}

impl AppealFilter {
    /// Every filter, in the order the row shows them.
    pub const ALL: [Self; 4] = [Self::Pending, Self::Accepted, Self::Rejected, Self::All];

    /// The word on the button.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Pending => "Waiting",
            Self::Accepted => "Accepted",
            Self::Rejected => "Rejected",
            Self::All => "All",
        }
    }

    /// The `status` the server is asked for. `all` is the server's own spelling of "no
    /// restriction", so the client never has to send an empty parameter and hope.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::All => "all",
        }
    }

    /// What an empty queue under this filter means.
    ///
    /// Each of the four is a different sentence because each is a different message: an empty
    /// work list is good news, and an empty "accepted" list on an instance whose appeals have
    /// all been refused is a fact about the decisions rather than about the screen.
    #[must_use]
    pub const fn empty(self) -> &'static str {
        match self {
            Self::Pending => "No appeal is waiting.",
            Self::Accepted => "No appeal has been accepted yet.",
            Self::Rejected => "No appeal has been refused yet.",
            Self::All => "Nobody has appealed a decision on this instance yet.",
        }
    }
}

/// One appeal, as the queue draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appeal {
    /// The instance's id for this appeal.
    pub id: String,
    /// `@user`, or `@user@host` for a remote account.
    pub handle: String,
    /// The action appealed against: `suspend`, `silence` or `reject`.
    pub action: String,
    /// Why the person wrote, in their own words.
    pub reason: String,
    /// `pending`, `accepted` or `rejected`.
    pub status: String,
    /// The restriction in force *now*, which is absent once an appeal is accepted.
    pub restriction: Option<String>,
    /// The note the decision came with; the person was sent it.
    pub decision_note: Option<String>,
    /// The account that decided it.
    pub decided_by: Option<String>,
    /// How long it has waited, already in words ("waited 3d").
    pub waited: String,
    /// When it was decided, already as a date.
    pub decided: Option<String>,
}

/// What was done, in a sentence a person who was not at the keyboard understands.
///
/// The store's words are `suspend`, `silence` and `reject`; none of them is a thing that
/// happened to a person, so the row says what happened instead of naming the verb.
#[must_use]
pub fn action_sentence(action: &str) -> &'static str {
    match action {
        "suspend" => "The account was suspended.",
        "silence" => "The account was silenced.",
        "reject" => "The registration was rejected.",
        _ => "A decision was taken about this account.",
    }
}

/// What accepting the appeal did, in the same kind of sentence.
#[must_use]
pub fn lifted_sentence(action: &str) -> &'static str {
    match action {
        "suspend" => "Accepted: the suspension was lifted.",
        "silence" => "Accepted: the silence was lifted.",
        "reject" => "Accepted: the registration was approved.",
        _ => "Accepted: the restriction was lifted.",
    }
}

/// The restriction that is still in force, named for a person.
#[must_use]
pub fn force_sentence(restriction: &str) -> &'static str {
    match restriction {
        "suspend" => "The suspension is still in force.",
        "silence" => "The silence is still in force.",
        "reject" => "The registration is still refused.",
        _ => "The restriction is still in force.",
    }
}

/// The state of one appeal, in one word.
#[must_use]
pub fn status_word(status: &str) -> &'static str {
    match status {
        "pending" => "Waiting",
        "accepted" => "Accepted",
        "rejected" => "Rejected",
        _ => "Unknown",
    }
}

/// A moderator's answer to one appeal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppealDecision {
    /// The appeal being decided.
    pub id: String,
    /// Accepting lifts the restriction the appeal names.
    pub accept: bool,
    /// What the person will be told, possibly empty.
    pub note: String,
}

/// The appeal queue.
#[component]
pub fn AppealsPage(
    /// The appeals the server sent, in the order it sent them.
    #[prop(into)]
    appeals: Signal<Vec<Appeal>>,
    /// Which appeals the queue is showing.
    #[prop(into)]
    filter: Signal<AppealFilter>,
    /// How many appeals are waiting, as far as this read can tell.
    #[prop(into)]
    waiting: Signal<usize>,
    /// Whether the waiting count stopped at one page, so it is a floor rather than a total.
    #[prop(into)]
    waiting_more: Signal<bool>,
    /// Whether a read is on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// What the last decision did, said above the queue.
    #[prop(optional_no_strip)]
    notice: Option<Signal<Option<String>>>,
    /// Whether the instance holds more appeals after these.
    #[prop(optional_no_strip)]
    has_more: Option<Signal<bool>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
    /// Refilters the queue. Absent where nothing can be sent.
    #[prop(optional)]
    on_filter: Option<Callback<AppealFilter>>,
    /// Reads the next page. Absent where nothing can be sent.
    #[prop(optional)]
    on_more: Option<Callback<()>>,
    /// Answers one appeal. Absent where nothing can be sent.
    #[prop(optional)]
    on_decide: Option<Callback<AppealDecision>>,
) -> impl IntoView {
    let write = writable(write);
    let chosen = move |value: AppealFilter| {
        let active = Signal::derive(move || filter.get() == value);
        view! {
            <button
                class="btn btn-xs"
                class:btn-active=move || active.get()
                disabled=on_filter.is_none()
                on:click=move |_| {
                    if let Some(on_filter) = on_filter {
                        on_filter.run(value);
                    }
                }
            >
                {value.word()}
            </button>
        }
    };
    // Pending first, and stable within each half so the server's newest-first order survives:
    // the queue a moderator opens is the work, not the archive.
    let ordered = Signal::derive(move || {
        let mut rows = appeals.get();
        rows.sort_by_key(|appeal| u8::from(appeal.status != "pending"));
        rows
    });
    // The floor the reader is told about. A page that ended full may have more behind it, and
    // saying "20" then would be a claim the read cannot back.
    let waiting_line = move || {
        let count = waiting.get();
        if count == 0 {
            return String::new();
        }
        let more = waiting_more.get();
        let noun = if count == 1 && !more {
            "appeal is"
        } else {
            "appeals are"
        };
        let exact = if more {
            format!("{count} or more")
        } else {
            count.to_string()
        };
        format!("{exact} {noun} waiting for a decision.")
    };

    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">{move || error.get().unwrap_or_default()}</p>
            </Show>
            <Show when=move || notice.and_then(|notice| notice.get()).is_some()>
                <p class="text-sm opacity-70" role="status">
                    {move || notice.and_then(|notice| notice.get()).unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || !write.get()>
                <Note class="text-sm text-warning">{NO_WRITE}</Note>
            </Show>

            <div class="flex flex-wrap items-center gap-2">
                {AppealFilter::ALL.into_iter().map(chosen).collect_view()}
            </div>

            <p class="text-sm font-medium">{waiting_line}</p>

            <Show when=move || busy.get() && appeals.get().is_empty()>
                <p class="text-sm opacity-60">"Reading the queue…"</p>
            </Show>

            <ul class="flex flex-col gap-2">
                // The key is the id *and* what the row draws: a keyed diff keeps the DOM of
                // a row whose key did not change, so an appeal somebody else decided would go
                // on reading as waiting. Folding the verdict into the key makes a changed row
                // a new row, which is what the moderator has to be able to see.
                <For
                    each=move || ordered.get()
                    key=|appeal| {
                        (
                            appeal.id.clone(),
                            appeal.status.clone(),
                            appeal.restriction.clone(),
                            appeal.decision_note.clone(),
                            appeal.decided_by.clone(),
                        )
                    }
                    let:appeal
                >
                    {
                        let Appeal {
                            id,
                            handle,
                            action,
                            reason,
                            status,
                            restriction,
                            decision_note,
                            decided_by,
                            waited,
                            decided,
                        } = appeal;
                        let pending = status == "pending";
                        let badge = status_word(&status);
                        let happened = action_sentence(&action);
                        let lifted = (status == "accepted").then(|| lifted_sentence(&action));
                        let still = restriction.as_deref().map(force_sentence);
                        let decided_line = decided_by.map(|who| match decided {
                            Some(date) => format!("Decided by {who} on {date}."),
                            None => format!("Decided by {who}."),
                        });
                        let group = format!("appeal-{id}");
                        let note_label = format!("Note for {handle}");
                        let accept_label = format!("Accept the appeal from {handle}");
                        let reject_label = format!("Reject the appeal from {handle}");
                        let account = handle.clone();
                        let (choice, set_choice) = signal(None::<bool>);
                        let (note, set_note) = signal(String::new());
                        // A press holds the button until the answer lands: a second press can
                        // only be a second decision, and the instance refuses it. The read
                        // that follows every answer settles `busy`, which gives the button
                        // back if the row is still waiting.
                        let (sent, set_sent) = signal(false);
                        Effect::new(move |was: Option<bool>| {
                            let now = busy.get();
                            if was == Some(true) && !now {
                                set_sent.set(false);
                            }
                            now
                        });
                        // What each optional part says, and whether it is drawn at all, kept
                        // as plain values: a `Show` body is rebuilt on every render, so its
                        // `when` may only capture things that can be copied.
                        let has_decided = !pending && decided_line.is_some();
                        let decided_text = decided_line.unwrap_or_default();
                        let has_note = decision_note.is_some();
                        // A `StoredValue` rather than a plain string: the `Show` body is
                        // rebuilt on every render, and a copied handle can be read from
                        // there while a moved `String` cannot.
                        let shown_note = StoredValue::new(decision_note.unwrap_or_default());
                        let has_lifted = lifted.is_some();
                        let lift_text = lifted.unwrap_or_default().to_owned();
                        let has_force = still.is_some();
                        let force_text = still.unwrap_or_default();
                        let decide_id = StoredValue::new(id.clone());
                        view! {
                            <li
                                class="flex flex-col gap-2 rounded-box border border-base-300 bg-base-100 p-3"
                                data-appeal=id.clone()
                            >
                                <div class="flex flex-wrap items-center gap-2">
                                    <span class="font-medium">{account.clone()}</span>
                                    <span class="badge badge-sm">{badge}</span>
                                    <span class="text-xs opacity-70">{happened}</span>
                                    <span class="text-xs opacity-60">{waited.clone()}</span>
                                </div>
                                <blockquote class="border-l-2 border-base-300 pl-2 text-sm opacity-80">
                                    {reason.clone()}
                                </blockquote>
                                <Show when=move || has_lifted>
                                    <p class="text-xs opacity-70">{lift_text.clone()}</p>
                                </Show>
                                <Show when=move || has_force>
                                    <p class="text-xs opacity-70">{force_text}</p>
                                </Show>

                                <Show when=move || has_decided>
                                    <div class="flex flex-col gap-1 rounded-box bg-base-200 p-2">
                                        <p class="text-xs opacity-80">{decided_text.clone()}</p>
                                        <Show when=move || has_note>
                                            <p class="text-xs opacity-70">{move || shown_note.get_value()}</p>
                                        </Show>
                                    </div>
                                </Show>

                                <Show when=move || pending && on_decide.is_some()>
                                    <div class="flex flex-col gap-2 rounded-box bg-base-200 p-2">
                                        <p class="text-xs opacity-70">
                                            "Accepting lifts the restriction it names and rejecting leaves it in force. The note is sent to the person."
                                        </p>
                                        <div class="flex flex-wrap items-center gap-4">
                                            <label class="flex cursor-pointer items-center gap-1 text-sm">
                                                <input
                                                    class="radio radio-sm"
                                                    type="radio"
                                                    name=group.clone()
                                                    aria-label=accept_label.clone()
                                                    on:change=move |_| set_choice.set(Some(true))
                                                />
                                                "Accept the appeal"
                                            </label>
                                            <label class="flex cursor-pointer items-center gap-1 text-sm">
                                                <input
                                                    class="radio radio-sm"
                                                    type="radio"
                                                    name=group.clone()
                                                    aria-label=reject_label.clone()
                                                    on:change=move |_| set_choice.set(Some(false))
                                                />
                                                "Reject the appeal"
                                            </label>
                                        </div>
                                        <textarea
                                            class="textarea textarea-bordered w-full"
                                            rows="2"
                                            placeholder="Why, in the words the person will read."
                                            aria-label=note_label.clone()
                                            on:input=move |event| set_note.set(event_target_value(&event))
                                        ></textarea>
                                        <div class="flex justify-end">
                                            <button
                                                class="btn btn-sm btn-primary"
                                                disabled=move || {
                                                    choice.get().is_none()
                                                        || sent.get()
                                                        || busy.get()
                                                        || on_decide.is_none()
                                                        || !write.get()
                                                }
                                                on:click=move |_| {
                                                    let Some(accept) = choice.get() else { return };
                                                    let Some(on_decide) = on_decide else { return };
                                                    set_sent.set(true);
                                                    on_decide
                                                        .run(AppealDecision {
                                                            id: decide_id.get_value(),
                                                            accept,
                                                            note: note.get(),
                                                        });
                                                }
                                            >
                                                "Decide"
                                            </button>
                                        </div>
                                    </div>
                                </Show>
                            </li>
                        }
                    }
                </For>
            </ul>

            <Show when=move || has_more.is_some_and(|has_more| has_more.get())>
                <button
                    class="btn btn-sm btn-ghost"
                    disabled=move || busy.get() || on_more.is_none()
                    on:click=move |_| {
                        if let Some(on_more) = on_more {
                            on_more.run(());
                        }
                    }
                >
                    {move || if busy.get() { "Reading…" } else { "Older appeals" }}
                </button>
            </Show>

            <Show when=move || !busy.get() && appeals.get().is_empty()>
                <p class="text-sm opacity-70">{move || filter.get().empty()}</p>
            </Show>
        </div>
    }
}
