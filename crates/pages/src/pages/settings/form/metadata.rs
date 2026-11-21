//! The metadata rows: a label and a value each, at most as many as an instance keeps.

use leptos::prelude::*;

use super::fields::{MAX_FIELDS, ProfileField, is_link};

/// The account's metadata rows, and the control that adds one.
///
/// Absent when the caller offers no rows: then nothing is drawn at all rather than an
/// editor with nowhere to write.
#[component]
pub fn MetadataFields(
    /// The rows as they are being typed; absent where the caller keeps none.
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
    let rows = move || {
        let Some(fields) = fields else {
            return ().into_any();
        };
        fields
            .get()
            .into_iter()
            .enumerate()
            .map(|(index, field)| {
                view! {
                    <FieldRow
                        index=index
                        field=field
                        error=Signal::derive(move || {
                            field_errors
                                .and_then(|errors| errors.get().get(index).cloned().flatten())
                                .unwrap_or_default()
                        })
                        on_field_name=on_field_name
                        on_field_value=on_field_value
                        on_remove=on_remove_field
                    />
                }
            })
            .collect_view()
            .into_any()
    };

    let add_disabled = move || {
        fields.is_none_or(|fields| fields.get().len() >= MAX_FIELDS || on_add_field.is_none())
    };

    view! {
        <Show when=move || fields.is_some()>
            <div class="flex flex-col gap-2">
                <span class="label-text">"Metadata"</span>
                <span class="text-xs opacity-60">
                    "A label and a value, up to six rows: a site, a handle, a project."
                </span>
                {rows}
                <button
                    class="btn btn-outline btn-xs self-start"
                    disabled=add_disabled
                    title="An instance accepts at most six metadata rows."
                    on:click=move |_| {
                        if let Some(on_add_field) = on_add_field {
                            on_add_field.run(());
                        }
                    }
                >
                    "Add a row"
                </button>
                <span class="text-xs opacity-60">
                    "A link is marked verified when this instance has checked that it points back to your profile; a mark never comes from typing one in."
                </span>
            </div>
        </Show>
    }
}

/// One row: its label, its value, the mark an instance put on the value, and its complaint.
#[component]
fn FieldRow(
    index: usize,
    field: ProfileField,
    /// The complaint about this row, empty when there is none.
    #[prop(into)]
    error: Signal<String>,
    on_field_name: Option<Callback<(usize, String)>>,
    on_field_value: Option<Callback<(usize, String)>>,
    on_remove: Option<Callback<usize>>,
) -> impl IntoView {
    let verified = field.verified_at.is_some() && is_link(&field.value);
    view! {
        <div class="flex flex-col gap-1">
            <div class="flex items-end gap-2">
                <label class="form-control grow">
                    <span class="label-text">"Label"</span>
                    <input
                        type="text"
                        class="input input-bordered input-sm w-full"
                        prop:value=field.name.clone()
                        disabled=on_field_name.is_none()
                        on:input=move |event| {
                            if let Some(on_field_name) = on_field_name {
                                on_field_name.run((index, event_target_value(&event)));
                            }
                        }
                    />
                </label>
                <label class="form-control grow">
                    <span class="label-text">"Value"</span>
                    <input
                        type="text"
                        class="input input-bordered input-sm w-full"
                        prop:value=field.value.clone()
                        disabled=on_field_value.is_none()
                        on:input=move |event| {
                            if let Some(on_field_value) = on_field_value {
                                on_field_value.run((index, event_target_value(&event)));
                            }
                        }
                    />
                </label>
                <Show when=move || verified>
                    <span class="badge badge-success badge-sm mb-1" role="img" aria-label="Verified link">
                        "✔ verified"
                    </span>
                </Show>
                <button
                    class="btn btn-ghost btn-xs mb-1"
                    disabled=on_remove.is_none()
                    on:click=move |_| {
                        if let Some(on_remove) = on_remove {
                            on_remove.run(index);
                        }
                    }
                >
                    "Remove"
                </button>
            </div>
            <span class="text-xs text-error" role="alert">
                {move || error.get()}
            </span>
        </div>
    }
}
