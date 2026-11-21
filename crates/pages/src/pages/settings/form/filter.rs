//! The rows of the settings forms that are not the profile: filters and featured tags.

use leptos::prelude::*;

/// Name, phrases and what a match does: the rows of the "add a filter" form.
#[component]
pub fn FilterFields(
    /// The filter's name.
    #[prop(into)]
    name: Signal<String>,
    /// The phrases, as one comma-separated line.
    #[prop(into)]
    phrases: Signal<String>,
    /// Whether a match is hidden instead of warned about.
    #[prop(into)]
    hide: Signal<bool>,
    /// Whether the create is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// Reports the name as it changes.
    #[prop(optional_no_strip)]
    on_name: Option<Callback<String>>,
    /// Reports the phrase line as it changes.
    #[prop(optional_no_strip)]
    on_phrases: Option<Callback<String>>,
    /// Reports the hide choice as it changes.
    #[prop(optional_no_strip)]
    on_hide: Option<Callback<bool>>,
    /// Creates the filter from the current draft.
    #[prop(optional_no_strip)]
    on_add: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-2">
            <input
                class="input input-bordered input-sm"
                placeholder="Name, for example \"crypto\""
                prop:value=move || name.get()
                disabled=on_name.is_none()
                on:input=move |event| {
                    if let Some(on_name) = on_name {
                        on_name.run(event_target_value(&event));
                    }
                }
            />
            <input
                class="input input-bordered input-sm"
                placeholder="Phrases, separated by commas"
                prop:value=move || phrases.get()
                disabled=on_phrases.is_none()
                on:input=move |event| {
                    if let Some(on_phrases) = on_phrases {
                        on_phrases.run(event_target_value(&event));
                    }
                }
            />
            <label class="label cursor-pointer justify-start gap-2">
                <input
                    type="checkbox"
                    class="toggle toggle-sm"
                    prop:checked=move || hide.get()
                    disabled=on_hide.is_none()
                    on:change=move |event| {
                        if let Some(on_hide) = on_hide {
                            on_hide.run(event_target_checked(&event));
                        }
                    }
                />
                <span class="label-text">"Hide these posts instead of warning"</span>
            </label>
            <button
                class="btn btn-primary btn-sm self-start"
                disabled=move || busy.get() || on_add.is_none()
                on:click=move |_| {
                    if let Some(on_add) = on_add {
                        on_add.run(());
                    }
                }
            >
                {move || if busy.get() { "Working…" } else { "Add filter" }}
            </button>
        </div>
    }
}

/// The row that turns a typed tag into a featured one.
#[component]
pub fn FeaturedTagField(
    /// The tag as it is being typed.
    #[prop(into)]
    name: Signal<String>,
    /// Whether the feature is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// Reports the tag as it changes.
    #[prop(optional_no_strip)]
    on_name: Option<Callback<String>>,
    /// Features the typed tag.
    #[prop(optional_no_strip)]
    on_add: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="flex gap-2">
            <input
                type="text"
                class="input input-bordered input-sm grow"
                placeholder="rust"
                prop:value=move || name.get()
                disabled=on_name.is_none()
                on:input=move |event| {
                    if let Some(on_name) = on_name {
                        on_name.run(event_target_value(&event));
                    }
                }
            />
            <button
                class="btn btn-primary btn-sm"
                disabled=move || busy.get() || on_add.is_none()
                on:click=move |_| {
                    if let Some(on_add) = on_add {
                        on_add.run(());
                    }
                }
            >
                "Feature"
            </button>
        </div>
    }
}
