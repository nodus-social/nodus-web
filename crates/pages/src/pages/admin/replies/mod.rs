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
    /// The id of the preset being edited, when one is.
    #[prop(into)]
    #[prop(optional)]
    editing: Signal<Option<String>>,
    /// The title being edited.
    #[prop(into)]
    #[prop(optional)]
    edit_title: Signal<String>,
    /// The text being edited.
    #[prop(into)]
    #[prop(optional)]
    edit_text: Signal<String>,
    /// Opens one preset for editing.
    #[prop(optional)]
    on_edit: Option<Callback<WarningPreset>>,
    /// Records the title being edited.
    #[prop(optional)]
    on_edit_title: Option<Callback<String>>,
    /// Records the text being edited.
    #[prop(optional)]
    on_edit_text: Option<Callback<String>>,
    /// Saves the edit, keeping the preset's identity.
    #[prop(optional)]
    on_save_edit: Option<Callback<()>>,
    /// Abandons the edit.
    #[prop(optional)]
    on_cancel_edit: Option<Callback<()>>,
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
                            let here = preset.id.clone();
                            let open = Signal::derive(move || editing.get().as_deref() == Some(here.as_str()));
                            let stored = StoredValue::new(preset.clone());
                            view! {
                                <li class="rounded-box bg-base-200 p-2">
                                    <Show
                                        when=move || open.get()
                                        fallback=move || view! {
                                            <div class="flex items-start gap-2">
                                                <div class="min-w-0">
                                                    <p class="truncate text-sm font-medium">{preset.title.clone()}</p>
                                                    <p class="text-xs whitespace-pre-line opacity-70">{preset.text.clone()}</p>
                                                </div>
                                                <div class="ml-auto flex shrink-0 gap-1">
                                                    <button
                                                        class="btn btn-ghost btn-xs"
                                                        disabled=move || on_edit.is_none() || !write.get()
                                                        title=move || if !write.get() { crate::props::NO_WRITE } else { Default::default() }
                                                        on:click=move |_| {
                                                            if let Some(on_edit) = on_edit {
                                                                on_edit.run(stored.get_value());
                                                            }
                                                        }
                                                    >
                                                        "Edit"
                                                    </button>
                                                    <button
                                                        class="btn btn-ghost btn-xs text-error"
                                                        disabled=move || on_delete.is_none() || !write.get()
                                                        title=move || if !write.get() { crate::props::NO_WRITE } else { Default::default() }
                                                        on:click=move |_| {
                                                            if let Some(on_delete) = on_delete {
                                                                on_delete.run(id.get_value());
                                                            }
                                                        }
                                                    >
                                                        "Forget"
                                                    </button>
                                                </div>
                                            </div>
                                        }
                                    >
                                        <div class="flex flex-col gap-2">
                                            <input
                                                class="input input-bordered input-sm"
                                                aria-label="Edited preset title"
                                                prop:value=move || edit_title.get()
                                                disabled=on_edit_title.is_none()
                                                on:input=move |event| {
                                                    if let Some(on_edit_title) = on_edit_title {
                                                        on_edit_title.run(event_target_value(&event));
                                                    }
                                                }
                                            />
                                            <textarea
                                                class="textarea textarea-bordered textarea-sm h-24"
                                                aria-label="Edited preset text"
                                                prop:value=move || edit_text.get()
                                                disabled=on_edit_text.is_none()
                                                on:input=move |event| {
                                                    if let Some(on_edit_text) = on_edit_text {
                                                        on_edit_text.run(event_target_value(&event));
                                                    }
                                                }
                                            ></textarea>
                                            <div class="flex gap-2">
                                                <button
                                                    class="btn btn-primary btn-xs"
                                                    disabled=move || busy.get() || on_save_edit.is_none() || !write.get()
                                                    on:click=move |_| {
                                                        if let Some(on_save_edit) = on_save_edit {
                                                            on_save_edit.run(());
                                                        }
                                                    }
                                                >
                                                    "Save changes"
                                                </button>
                                                <button
                                                    class="btn btn-ghost btn-xs"
                                                    disabled=move || on_cancel_edit.is_none()
                                                    on:click=move |_| {
                                                        if let Some(on_cancel_edit) = on_cancel_edit {
                                                            on_cancel_edit.run(());
                                                        }
                                                    }
                                                >
                                                    "Cancel"
                                                </button>
                                            </div>
                                        </div>
                                    </Show>
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
