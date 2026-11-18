//! Canned replies: the sentences a moderator sends over and over.
//!
//! Mastodon calls them warning presets. They are administration, not moderation, because they
//! are the instance's wording rather than one moderator's judgement — and a moderator who is
//! not an administrator should not be able to put words in the instance's mouth.

use leptos::prelude::*;
use nodus_client_core::model::WarningPreset;

use crate::components::panel::Panel;

/// The presets, and the one being written.
#[component]
pub fn RepliesPage(
    /// The presets this instance holds.
    #[prop(into)]
    presets: Signal<Vec<WarningPreset>>,
    /// The title being typed.
    #[prop(into)]
    title: Signal<String>,
    /// The text being typed.
    #[prop(into)]
    text: Signal<String>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records the title.
    #[prop(optional)]
    on_title: Option<Callback<String>>,
    /// Records the text.
    #[prop(optional)]
    on_text: Option<Callback<String>>,
    /// Saves the draft.
    #[prop(optional)]
    on_create: Option<Callback<()>>,
    /// Forgets one preset.
    #[prop(optional)]
    on_delete: Option<Callback<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">{move || error.get().unwrap_or_default()}</p>
            </Show>
            <Show when=move || !write.get()>
                <crate::components::chrome::Note class="text-sm text-warning">
                    {crate::props::NO_WRITE}
                </crate::components::chrome::Note>
            </Show>

            <Panel
                title="Canned replies"
                description="Sentences a moderator can attach to a decision instead of writing them again."
            >
                <ul class="flex flex-col gap-2">
                    <For each=move || presets.get() key=|preset| preset.id.clone() let:preset>
                        {
                            let id = StoredValue::new(preset.id.clone());
                            view! {
                                <li class="flex items-start gap-2 rounded-box bg-base-200 p-2">
                                    <div class="min-w-0">
                                        <p class="truncate text-sm font-medium">{preset.title.clone()}</p>
                                        <p class="text-xs whitespace-pre-line opacity-70">{preset.text.clone()}</p>
                                    </div>
                                    <button
                                        class="btn btn-ghost btn-xs ml-auto shrink-0 text-error"
                                        disabled=move || on_delete.is_none() || !write.get()
                                        title=move || (!write.get()).then_some(crate::props::NO_WRITE).unwrap_or_default()
                                        on:click=move |_| {
                                            if let Some(on_delete) = on_delete {
                                                on_delete.run(id.get_value());
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
                <Show when=move || presets.get().is_empty()>
                    <p class="text-sm opacity-70">"No canned reply yet."</p>
                </Show>
            </Panel>

            <Panel title="Write one" description="A short title is what the dropdown shows; the text is what the reported person reads.">
                <div class="flex flex-col gap-2">
                    <input
                        class="input input-bordered input-sm"
                        placeholder="Title"
                        prop:value=move || title.get()
                        on:input=move |event| {
                            if let Some(on_title) = on_title {
                                on_title.run(event_target_value(&event));
                            }
                        }
                    />
                    <textarea
                        class="textarea textarea-bordered textarea-sm h-24"
                        placeholder="What the report is about, and what happens next."
                        prop:value=move || text.get()
                        on:input=move |event| {
                            if let Some(on_text) = on_text {
                                on_text.run(event_target_value(&event));
                            }
                        }
                    ></textarea>
                    <button
                        class="btn btn-sm btn-primary self-start"
                        disabled=move || busy.get() || on_create.is_none()
                        on:click=move |_| {
                            if let Some(on_create) = on_create {
                                on_create.run(());
                            }
                        }
                    >
                        "Save"
                    </button>
                </div>
            </Panel>
        </div>
    }
}
