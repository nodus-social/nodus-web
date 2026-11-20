//! Instance announcements, and the form that publishes them.

use leptos::prelude::*;
use nodus_client_core::model::Announcement;

use crate::components::panel::Panel;

/// Every announcement, with publishing controls.
#[component]
pub fn AnnouncementsPage(
    /// The announcements, as the caller last read them.
    #[prop(into)]
    items: Signal<Vec<Announcement>>,
    /// The message being written.
    #[prop(into)]
    draft: Signal<String>,
    /// Whether the new announcement is published immediately.
    #[prop(into)]
    published: Signal<bool>,
    /// Whether a read or write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records the typed message.
    #[prop(optional_no_strip)]
    on_draft: Option<Callback<String>>,
    /// Records the publish toggle.
    #[prop(optional_no_strip)]
    on_published: Option<Callback<bool>>,
    /// Publishes the draft.
    #[prop(optional_no_strip)]
    on_publish: Option<Callback<()>>,
    /// Withdraws one announcement.
    #[prop(optional_no_strip)]
    on_delete: Option<Callback<String>>,
    /// The announcement being edited, when one is.
    #[prop(into)]
    #[prop(optional)]
    editing: Signal<Option<Announcement>>,
    /// The text being edited.
    #[prop(into)]
    #[prop(optional)]
    edit_content: Signal<String>,
    /// Opens one announcement for editing.
    #[prop(optional_no_strip)]
    on_edit: Option<Callback<Announcement>>,
    /// Records the text being edited.
    #[prop(optional_no_strip)]
    on_edit_content: Option<Callback<String>>,
    /// Saves the edit, keeping the announcement's id and schedule.
    #[prop(optional_no_strip)]
    on_save_edit: Option<Callback<()>>,
    /// Abandons the edit.
    #[prop(optional_no_strip)]
    on_cancel_edit: Option<Callback<()>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
    /// Whether the account's staff role is administrator. Absent means "assume yes".
    #[prop(optional_no_strip)]
    administrator: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    let administrator = crate::props::administrator(administrator);
    // A moderator can read the announcements and can change none of them: every write route
    // answers `role == admin`.
    let may = Signal::derive(move || write.get() && administrator.get());
    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || !administrator.get()>
                <crate::components::chrome::Note class="text-sm text-warning">
                    {crate::props::ADMIN_ONLY}
                </crate::components::chrome::Note>
            </Show>
            <Panel title="Announcements" description="Messages this instance shows to its members.">
                <Show when=move || error.get().is_some()>
                    <p class="alert alert-error alert-soft py-1 text-sm">
                        {move || error.get().unwrap_or_default()}
                    </p>
                </Show>
                <ul class="divide-y divide-base-300">
                    <For each=move || items.get() key=|announcement| announcement.id.clone() let:announcement>
                        {
                            let id = announcement.id.clone();
                            let target = StoredValue::new(id.clone());
                            let here = id.clone();
                            let open = Signal::derive(move || editing.get().is_some_and(|held| held.id == here));
                            let stored = StoredValue::new(announcement.clone());
                            let status = if announcement.published { "published" } else { "draft" };
                            view! {
                                <li class="py-2">
                                    <Show
                                        when=move || open.get()
                                        fallback=move || view! {
                                            <div class="flex items-start justify-between gap-3">
                                                <div class="min-w-0">
                                                    <p class="whitespace-pre-wrap text-sm">{announcement.content.clone()}</p>
                                                    <p class="mt-1 text-xs opacity-60">
                                                        {format!("{status} · {}", announcement.published_at.clone().unwrap_or_default())}
                                                    </p>
                                                </div>
                                                <div class="flex shrink-0 gap-1">
                                                    <button
                                                        class="btn btn-ghost btn-xs"
                                                        disabled=move || busy.get() || on_edit.is_none() || !may.get()
                                                        title=move || crate::props::refusal(write, administrator)
                                                        on:click=move |_| {
                                                            if let Some(on_edit) = on_edit {
                                                                on_edit.run(stored.get_value());
                                                            }
                                                        }
                                                    >
                                                        "Edit"
                                                    </button>
                                                    <button
                                                        class="btn btn-ghost btn-xs"
                                                        disabled=move || busy.get() || on_delete.is_none() || !may.get()
                                                        title=move || crate::props::refusal(write, administrator)
                                                        on:click=move |_| {
                                                            if let Some(on_delete) = on_delete {
                                                                on_delete.run(target.get_value());
                                                            }
                                                        }
                                                    >
                                                        "Delete"
                                                    </button>
                                                </div>
                                            </div>
                                        }
                                    >
                                        <div class="flex flex-col gap-2">
                                            <textarea
                                                class="textarea textarea-bordered min-h-24"
                                                aria-label="Edited announcement"
                                                prop:value=move || edit_content.get()
                                                disabled=on_edit_content.is_none()
                                                on:input=move |event| {
                                                    if let Some(on_edit_content) = on_edit_content {
                                                        on_edit_content.run(event_target_value(&event));
                                                    }
                                                }
                                            ></textarea>
                                            <div class="flex gap-2">
                                                <button
                                                    class="btn btn-primary btn-xs"
                                                    disabled=move || busy.get() || on_save_edit.is_none() || !may.get()
                                                    title=move || crate::props::refusal(write, administrator)
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
                <Show when=move || items.get().is_empty()>
                    <p class="text-sm opacity-60">"No announcements yet."</p>
                </Show>
            </Panel>

            <Panel title="New announcement" description="Text is shown as written.">
                <form
                    class="flex flex-col gap-2"
                    on:submit=move |event| {
                        stop_submit(&event);
                        if let Some(on_publish) = on_publish {
                            on_publish.run(());
                        }
                    }
                >
                    <textarea
                        class="textarea textarea-bordered min-h-24"
                        placeholder="What should members know?"
                        prop:value=move || draft.get()
                        disabled=move || on_draft.is_none() || !may.get()
                        on:input=move |event| {
                            if let Some(on_draft) = on_draft {
                                on_draft.run(event_target_value(&event));
                            }
                        }
                    ></textarea>
                    <label class="label cursor-pointer justify-start gap-2">
                        <input
                            type="checkbox"
                            class="toggle toggle-sm"
                            prop:checked=move || published.get()
                            disabled=move || on_published.is_none() || !may.get()
                            on:change=move |event| {
                                if let Some(on_published) = on_published {
                                    on_published.run(event_target_checked(&event));
                                }
                            }
                        />
                        <span class="label-text">"Publish immediately"</span>
                    </label>
                    <button
                        class="btn btn-primary btn-sm self-start"
                        disabled=move || busy.get() || on_publish.is_none() || !may.get()
                        title=move || crate::props::refusal(write, administrator)
                    >
                        "Publish"
                    </button>
                </form>
            </Panel>
        </div>
    }
}

/// Keeps a submit from reloading the page on wasm, and keeps the native build's event used.
fn stop_submit(event: &leptos::ev::SubmitEvent) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = event;
    #[cfg(target_arch = "wasm32")]
    event.prevent_default();
}
