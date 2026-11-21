//! What the account says about itself: display name, bio, metadata rows and the save row.

use leptos::prelude::*;

use super::fields::ProfileField;
use super::metadata::MetadataFields;

/// The account's own fields, and the save row.
#[component]
pub fn ProfileFields(
    /// The display name as it is being typed.
    #[prop(into)]
    display_name: Signal<String>,
    /// The bio as it is being typed.
    #[prop(into)]
    note: Signal<String>,
    /// The bio limit the instance enforces.
    max_note: usize,
    /// Whether the save is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last save's answer, when there was one.
    #[prop(into)]
    message: Signal<Option<String>>,
    /// Reports the display name as it changes.
    #[prop(optional_no_strip)]
    on_display_name: Option<Callback<String>>,
    /// Reports the bio as it changes.
    #[prop(optional_no_strip)]
    on_note: Option<Callback<String>>,
    /// Saves every field.
    #[prop(optional_no_strip)]
    on_save: Option<Callback<()>>,
    /// The metadata rows; absent when the caller does not offer them, and then no editor
    /// is drawn at all rather than an editor with nowhere to write.
    #[prop(optional_no_strip)]
    fields: Option<Signal<Vec<ProfileField>>>,
    /// One complaint per row, by position; `None` for a row the instance has no objection to.
    #[prop(optional_no_strip)]
    field_errors: Option<Signal<Vec<Option<String>>>>,
    /// Reports one row's label as it changes.
    #[prop(optional_no_strip)]
    on_field_name: Option<Callback<(usize, String)>>,
    /// Reports one row's value as it changes.
    #[prop(optional_no_strip)]
    on_field_value: Option<Callback<(usize, String)>>,
    /// Adds an empty row.
    #[prop(optional_no_strip)]
    on_add_field: Option<Callback<()>>,
    /// Drops the row at this position.
    #[prop(optional_no_strip)]
    on_remove_field: Option<Callback<usize>>,
) -> impl IntoView {
    let counter = move || {
        let length = note.get().chars().count();
        let class = if length > max_note {
            "text-xs text-error"
        } else {
            "text-xs opacity-60"
        };
        view! { <span class=class>{format!("{length} / {max_note}")}</span> }
    };

    view! {
        <p class="text-sm opacity-70">"How this account appears to the people who find it."</p>

        <label class="form-control w-full">
            <span class="label-text">"Display name"</span>
            <input
                type="text"
                class="input input-bordered input-sm w-full"
                prop:value=move || display_name.get()
                disabled=on_display_name.is_none()
                on:input=move |event| {
                    if let Some(on_display_name) = on_display_name {
                        on_display_name.run(event_target_value(&event));
                    }
                }
            />
        </label>

        <label class="form-control w-full">
            <span class="label-text">"Bio"</span>
            <textarea
                class="textarea textarea-bordered w-full"
                rows="4"
                prop:value=move || note.get()
                disabled=on_note.is_none()
                on:input=move |event| {
                    if let Some(on_note) = on_note {
                        on_note.run(event_target_value(&event));
                    }
                }
            ></textarea>
            <div class="flex justify-end pt-1">{counter}</div>
        </label>

        <MetadataFields
            fields=fields
            field_errors=field_errors
            on_field_name=on_field_name
            on_field_value=on_field_value
            on_add_field=on_add_field
            on_remove_field=on_remove_field
        />

        <div class="flex items-center gap-3">
            <button
                class="btn btn-primary btn-sm"
                disabled=move || busy.get() || on_save.is_none()
                on:click=move |_| {
                    if let Some(on_save) = on_save {
                        on_save.run(());
                    }
                }
            >
                {move || if busy.get() { "Saving…" } else { "Save profile" }}
            </button>
            <span class="text-sm">{move || message.get().unwrap_or_default()}</span>
        </div>
    }
}
