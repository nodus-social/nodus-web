//! Quarantined media: a content set matched an attachment, and a person decides.
//!
//! Ingest hides a matched attachment on its own, which stops a hostile picture being seen
//! before anyone looked; it cannot decide what should happen to it. The two outcomes are not
//! ends of one scale, they are opposites: releasing keeps the picture and puts the post back,
//! deleting destroys the file for good. Neither is a default, so the row asks which one and
//! then asks why, and the answer is what the audit log and the report record.
//!
//! The row is read in the order a moderator needs it: what was caught, the set that named it,
//! and the post it belonged to. A cleared attachment leaves this list either way — the audit
//! log is where it goes on record.

use leptos::prelude::*;

use crate::props::{writable, NO_WRITE};
use crate::Note;

/// One attachment a content set caught, as the queue draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quarantined {
    /// The instance's id for this attachment.
    pub id: String,
    /// The post it hung on, when it hung on one.
    pub status_id: Option<String>,
    /// The media type, e.g. `image/png`.
    pub content_type: String,
    /// Where the picture lives, on the instance that hosts it.
    pub url: Option<String>,
    /// The content set that matched, which is the name a moderator acts on.
    pub set: Option<String>,
    /// The bytes' digest, when ingest computed one.
    pub content_hash: Option<String>,
    /// The perceptual digest, when ingest computed one.
    pub perceptual_hash: Option<String>,
    /// `quarantined` or `released`.
    pub state: String,
    /// When it arrived, already as a date.
    pub created_at: String,
}

/// A moderator's answer about one attachment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDecision {
    /// The attachment being cleared.
    pub id: String,
    /// `true` keeps the picture and restores the post; `false` destroys the file.
    pub release: bool,
    /// Why. Required by the screen, recorded in the audit row and on the report.
    pub reason: String,
}

/// The state of one attachment, in one word.
#[must_use]
pub fn state_word(state: &str) -> &'static str {
    match state {
        "quarantined" => "Quarantined",
        "released" => "Released",
        _ => "Unknown",
    }
}

/// What releasing does, in one line beside the button.
#[must_use]
pub fn release_sentence() -> &'static str {
    "Release keeps the picture and puts the post back."
}

/// What deleting does, in one line beside the button.
#[must_use]
pub fn delete_sentence() -> &'static str {
    "Delete destroys the file, and there is no copy here to restore."
}

/// The question a release asks before it happens.
#[must_use]
pub fn release_question() -> &'static str {
    "Release this picture? It stays on the post, and the post goes back into the timelines."
}

/// The question a delete asks before it happens.
#[must_use]
pub fn delete_question() -> &'static str {
    "Delete this picture? The file is destroyed and cannot be recovered."
}

/// What the picture is, in words, so a row without a thumbnail still names it.
fn attachment_line(id: &str, content_type: &str) -> String {
    if content_type.is_empty() {
        format!("Attachment {id}")
    } else {
        format!("Attachment {id} ({content_type})")
    }
}

/// The quarantine work list.
#[component]
pub fn MediaPage(
    /// The attachments the instance says a set caught, in the order it sent them.
    #[prop(into)]
    media: Signal<Vec<Quarantined>>,
    /// Whether a read is on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// What the last decision did, said above the list.
    #[prop(optional_no_strip)]
    notice: Option<Signal<Option<String>>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
    /// Clears one attachment. Absent where nothing can be sent.
    #[prop(optional)]
    on_decide: Option<Callback<MediaDecision>>,
) -> impl IntoView {
    let write = writable(write);
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

            <p class="text-sm opacity-70">
                "A content set caught these and ingest hid them before anyone looked. Every decision asks why, and the answer goes into the audit log."
            </p>

            <Show when=move || busy.get() && media.get().is_empty()>
                <p class="text-sm opacity-60">"Reading the quarantine…"</p>
            </Show>

            <ul class="flex flex-col gap-2">
                // The key carries the state as well as the id: a row somebody else cleared is
                // a new row, not the old one relabelled, so it cannot go on offering buttons.
                <For
                    each=move || media.get()
                    key=|item| (item.id.clone(), item.state.clone())
                    let:item
                >
                    {
                        let Quarantined {
                            id,
                            status_id,
                            content_type,
                            url,
                            set,
                            content_hash,
                            perceptual_hash,
                            state,
                            created_at,
                        } = item;
                        let badge = state_word(&state);
                        let caught = attachment_line(&id, &content_type);
                        let named = set.map_or_else(
                            || "No set name was recorded with it.".to_owned(),
                            |set| format!("Matched the set {set}."),
                        );
                        // What the attachment is, drawn when the browser can show it: the
                        // picture is the thing being judged, so the row shows it rather than
                        // describing it.
                        let shown_url = url.clone();
                        let picture = url.clone().filter(|_| content_type.starts_with("image/"));
                        let digest = content_hash.map_or_else(String::new, |hash| format!("SHA-256 {hash}"));
                        let sees_digest = !digest.is_empty();
                        let perceptual =
                            perceptual_hash.map_or_else(String::new, |hash| format!("perceptual {hash}"));
                        let sees_perceptual = !perceptual.is_empty();
                        // The post is where the decision lands, so the row links it instead of
                        // printing an id nobody can follow.
                        let post = status_id.map_or_else(
                            || {
                                view! {
                                    <span class="text-xs opacity-70">"It was not attached to a post."</span>
                                }
                                .into_any()
                            },
                            |status| {
                                view! {
                                    <a class="link text-xs" href=format!("/s/{status}")>"Open the post it was on"</a>
                                }
                                .into_any()
                            },
                        );
                        let shown = created_at.chars().take(10).collect::<String>();
                        let decide_id = StoredValue::new(id.clone());
                        let label_id = StoredValue::new(id.clone());
                        // Which outcome the moderator picked, then the reason, then a press:
                        // the row asks twice on purpose, because one of the answers destroys
                        // something that cannot be put back.
                        let (intent, set_intent) = signal(None::<bool>);
                        let (note, set_note) = signal(String::new());
                        let (sent, set_sent) = signal(false);
                        // A press holds the button until the answer lands and the list is read
                        // again: a second press can only be a second decision.
                        Effect::new(move |was: Option<bool>| {
                            let now = busy.get();
                            if was == Some(true) && !now {
                                set_sent.set(false);
                            }
                            now
                        });
                        view! {
                            <li
                                class="flex flex-col gap-2 rounded-box border border-base-300 bg-base-100 p-3"
                                data-media=id.clone()
                            >
                                <div class="flex flex-wrap items-center gap-2">
                                    <span class="badge badge-sm">{badge}</span>
                                    <span class="text-sm font-medium">{caught.clone()}</span>
                                    <span class="text-xs opacity-60">{shown}</span>
                                </div>
                                <div class="flex flex-wrap items-start gap-3">
                                    {picture.map(|src| view! {
                                        <img
                                            class="h-20 w-20 rounded-box border border-base-300 object-cover"
                                            src=src
                                            alt="The quarantined attachment"
                                        />
                                    })}
                                    <div class="flex min-w-0 flex-col gap-1">
                                        <p class="text-sm">{named.clone()}</p>
                                        <p>{post}</p>
                                        {shown_url.map(|href| view! {
                                            <a class="link text-xs" href=href>"Open the attachment at its source"</a>
                                        })}
                                        <Show when=move || sees_digest>
                                            <p class="text-xs opacity-60">{digest.clone()}</p>
                                        </Show>
                                        <Show when=move || sees_perceptual>
                                            <p class="text-xs opacity-60">{perceptual.clone()}</p>
                                        </Show>
                                    </div>
                                </div>

                                <Show when=move || intent.get().is_none()>
                                    <div class="flex flex-col gap-2 rounded-box bg-base-200 p-2">
                                        <p class="text-xs opacity-70">
                                            "The two outcomes are not variations of one another: one keeps the picture, the other destroys it."
                                        </p>
                                        <div class="flex flex-wrap items-center gap-4">
                                            <button
                                                class="btn btn-sm btn-primary"
                                                disabled=move || busy.get() || on_decide.is_none() || !write.get()
                                                on:click=move |_| set_intent.set(Some(true))
                                            >
                                                "Release"
                                            </button>
                                            <span class="text-xs opacity-70">{release_sentence()}</span>
                                        </div>
                                        <div class="flex flex-wrap items-center gap-4">
                                            <button
                                                class="btn btn-sm btn-error"
                                                disabled=move || busy.get() || on_decide.is_none() || !write.get()
                                                on:click=move |_| set_intent.set(Some(false))
                                            >
                                                "Delete"
                                            </button>
                                            <span class="text-xs opacity-70">{delete_sentence()}</span>
                                        </div>
                                    </div>
                                </Show>

                                <Show when=move || intent.get().is_some()>
                                    <div class="flex flex-col gap-2 rounded-box border border-base-300 bg-base-200 p-2">
                                        <p class="text-sm font-medium">
                                            {move || if intent.get() == Some(true) {
                                                release_question()
                                            } else {
                                                delete_question()
                                            }}
                                        </p>
                                        <textarea
                                            class="textarea textarea-bordered w-full"
                                            rows="2"
                                            placeholder="Why. This is recorded with your name."
                                            aria-label=move || {
                                                format!("Reason for the attachment {}", label_id.get_value())
                                            }
                                            on:input=move |event| set_note.set(event_target_value(&event))
                                        ></textarea>
                                        <p class="text-xs opacity-60">
                                            "A reason is required. Nothing is cleared without one."
                                        </p>
                                        <div class="flex justify-end gap-2">
                                            <button
                                                class="btn btn-sm btn-ghost"
                                                on:click=move |_| {
                                                    set_intent.set(None);
                                                    set_note.set(String::new());
                                                }
                                            >
                                                "Cancel"
                                            </button>
                                            <button
                                                class=move || {
                                                    if intent.get() == Some(true) {
                                                        "btn btn-sm btn-primary"
                                                    } else {
                                                        "btn btn-sm btn-error"
                                                    }
                                                }
                                                disabled=move || {
                                                    sent.get()
                                                        || busy.get()
                                                        || note.get().trim().is_empty()
                                                        || on_decide.is_none()
                                                        || !write.get()
                                                }
                                                on:click=move |_| {
                                                    let Some(release) = intent.get() else { return };
                                                    let Some(on_decide) = on_decide else { return };
                                                    set_sent.set(true);
                                                    on_decide
                                                        .run(MediaDecision {
                                                            id: decide_id.get_value(),
                                                            release,
                                                            reason: note.get(),
                                                        });
                                                }
                                            >
                                                {move || if intent.get() == Some(true) {
                                                    "Release the picture"
                                                } else {
                                                    "Delete the picture"
                                                }}
                                            </button>
                                        </div>
                                    </div>
                                </Show>
                            </li>
                        }
                    }
                </For>
            </ul>

            <Show when=move || !busy.get() && media.get().is_empty()>
                <p class="text-sm opacity-70">"No media is quarantined."</p>
            </Show>
        </div>
    }
}
