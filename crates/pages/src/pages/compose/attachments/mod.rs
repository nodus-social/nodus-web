//! Attachments on a compose form.
//!
//! The list and the tiles are markup; the file is not. Reading a `File` and posting its
//! bytes is transport, and the hidden `<input type=file>` is the browser's own control, so
//! it arrives as a slot. The colour of the row — "nothing chosen yet" versus "two files
//! here" — is the caller's to decide by what it puts in `media`.

use leptos::prelude::*;
use nodus_client_core::model::{MediaAttachment, MediaKind};

/// The uploaded files, and the picker that adds more.
#[component]
pub fn Attachments(
    /// The attachments the instance has already accepted.
    #[prop(into)]
    media: Signal<Vec<MediaAttachment>>,
    /// Whether a transfer is running.
    #[prop(into)]
    uploading: Signal<bool>,
    /// How much of the file has been sent, when the browser can say.
    #[prop(into)]
    percent: Signal<Option<u8>>,
    /// Drops one attachment by id.
    #[prop(optional)]
    on_remove: Option<Callback<String>>,
    /// Saves one attachment's description.
    #[prop(optional)]
    on_alt: Option<Callback<(String, String)>>,
    /// Marks one attachment sensitive.
    #[prop(optional)]
    on_sensitive: Option<Callback<(String, bool)>>,
    /// Attachments that came from the account's drive, by id and drive filename.
    ///
    /// A tile whose id is named here shows the filename it had in the drive and a
    /// marker, so a reader can tell it apart from a file uploaded through this form.
    #[prop(default = Signal::derive(Vec::new))]
    from_drive: Signal<Vec<(String, String)>>,
    /// The hidden file input, which owns the browser's file handling.
    #[prop(optional_no_strip)]
    picker: Option<ChildrenFn>,
) -> impl IntoView {
    view! {
        <div class="flex flex-wrap items-center gap-2">
            {picker.map(|picker| picker())}
            <Show when=move || uploading.get()>
                {
                    move || match percent.get() {
                        Some(value) => view! {
                            <span class="flex items-center gap-1" title="How much of the file has been sent">
                                <progress class="progress progress-primary w-16" value=value max="100"></progress>
                                <span class="text-xs tabular-nums opacity-70">{format!("{value}%")}</span>
                            </span>
                        }.into_any(),
                        None => view! {
                            <span class="loading loading-spinner loading-xs" aria-label="Uploading"></span>
                        }.into_any(),
                    }
                }
            </Show>
            <ul class="flex flex-wrap gap-2">
                <For
                    each=move || media.get()
                    key=|attachment| {
                        (
                            attachment.id.clone(),
                            attachment.processing_state.clone(),
                            attachment.processing_progress,
                            attachment.sensitive,
                        )
                    }
                    let:attachment
                >
                    <AttachmentTile
                        attachment=attachment
                        on_remove=on_remove
                        on_alt=on_alt
                        on_sensitive=on_sensitive
                        from_drive=from_drive
                    />
                </For>
            </ul>
        </div>
    }
}

/// One attachment: what it is, and the alt text that makes it readable to someone who
/// cannot see it.
///
/// Alt text is edited after the upload because that is when the writer knows what the
/// picture is going to say in the post; asking for it before the transfer would be asking
/// about a picture nobody can see yet.
#[component]
fn AttachmentTile(
    attachment: MediaAttachment,
    on_remove: Option<Callback<String>>,
    on_alt: Option<Callback<(String, String)>>,
    on_sensitive: Option<Callback<(String, bool)>>,
    from_drive: Signal<Vec<(String, String)>>,
) -> impl IntoView {
    let remove_id = attachment.id.clone();
    let save_id = attachment.id.clone();
    let sensitive_id = attachment.id.clone();
    let processing = attachment
        .processing_state
        .as_deref()
        .is_some_and(|state| state != "ready" && state != "failed");
    let progress = attachment.processing_progress.unwrap_or(0);
    // A signal, not a `String`: the fallback closure must be `Fn`, and a captured
    // `String` moved into it once would make the whole component `FnOnce`.
    let (label, _) = signal(format!("{:?}", attachment.kind));
    let has_label = attachment
        .description
        .as_deref()
        .is_some_and(|description| !description.trim().is_empty());
    let is_image = attachment.kind == MediaKind::Image;
    // The drive filename, when this tile came from the drive: read from the signal
    // inside the tile so a marker that arrives after the tile still paints.
    let drive_id = attachment.id.clone();
    let drive_name = Signal::derive(move || {
        from_drive
            .get()
            .into_iter()
            .find(|(id, _)| id == &drive_id)
            .map(|(_, name)| name)
    });
    let (editing, set_editing) = signal(false);
    let (alt, set_alt) = signal(attachment.description.clone().unwrap_or_default());
    let (label_set, set_label_set) = signal(has_label);

    // `Callback` is `Copy`, so the field and the blur can both ask for the same save.
    let save = Callback::new(move |()| {
        let text = alt.get_untracked();
        if let Some(on_alt) = on_alt {
            on_alt.run((save_id.clone(), text.clone()));
        }
        set_label_set.set(!text.trim().is_empty());
        set_editing.set(false);
    });

    view! {
        <li class="relative flex flex-col gap-1">
            <Show
                when=move || is_image
                fallback=move || {
                    view! { <span class="badge badge-outline badge-sm">{label.get()}</span> }
                }
            >
                <div class="relative h-20 w-20">
                    <img
                        class="h-20 w-20 rounded-box object-cover"
                        src=attachment.preview_url.clone().unwrap_or_default()
                        alt=attachment.description.clone().unwrap_or_default()
                    />
                    <Show when=move || processing>
                        <span class="absolute inset-0 flex items-center justify-center rounded-box bg-base-300/80">
                            <span class="loading loading-spinner loading-sm" aria-label="Processing media"></span>
                            <span class="ml-1 text-xs tabular-nums">{progress}"%"</span>
                        </span>
                    </Show>
                </div>
            </Show>

            <button
                type="button"
                class="btn btn-circle btn-xs absolute -right-1 -top-1"
                title="Remove this attachment"
                on:click=move |_| {
                    if let Some(on_remove) = on_remove {
                        on_remove.run(remove_id.clone());
                    }
                }
            >
                "×"
            </button>

            <Show when=move || drive_name.get().is_some()>
                <span
                    class="badge badge-primary badge-xs"
                    title="This file stayed in your drive; nothing was uploaded again"
                >
                    "from your drive"
                </span>
                <span class="max-w-20 truncate text-xs opacity-70">
                    {move || drive_name.get().unwrap_or_default()}
                </span>
            </Show>

            <div class="flex items-center gap-1">
                <button
                    type="button"
                    class="btn btn-ghost px-1 btn-xs"
                    title="Describe this attachment for someone who cannot see it"
                    class:text-warning=move || !label_set.get()
                    on:click=move |_| set_editing.update(|open| *open = !*open)
                >
                    {move || if label_set.get() { "Alt ✓" } else { "Alt" }}
                </button>
                <label class="flex cursor-pointer items-center gap-1 text-xs" title="Hide this attachment behind a warning">
                    <input
                        type="checkbox"
                        class="checkbox checkbox-xs"
                        prop:checked=attachment.sensitive
                        on:change=move |event| {
                            if let Some(on_sensitive) = on_sensitive {
                                on_sensitive.run((sensitive_id.clone(), event_target_checked(&event)));
                            }
                        }
                    />
                    "Sensitive"
                </label>
            </div>

            <Show when=move || editing.get()>
                <input
                    class="input input-bordered input-xs w-40"
                    placeholder="What does it show?"
                    aria-label="Attachment description"
                    prop:value=alt
                    on:input=move |event| set_alt.set(event_target_value(&event))
                    on:keydown=move |event| {
                        #[cfg(target_arch = "wasm32")]
                        if event.key() == "Enter" {
                            event.prevent_default();
                            save.run(());
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        let _ = &event;
                    }
                    on:blur=move |_| save.run(())
                />
            </Show>
        </li>
    }
}
