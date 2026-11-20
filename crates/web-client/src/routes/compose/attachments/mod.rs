//! Attachments on a compose form.
//!
//! The file is read and uploaded as soon as it is picked, so the write on submit only has
//! to reference ids the instance already accepted. That is also why a failed upload never
//! blocks the text: the attachment list is what says whether the post is ready.
//!
//! The row of thumbnails is the view's; the file input is not, and neither is the transfer
//! behind it — both are browser objects (`File`, `XmlHttpRequest` progress), so the input
//! arrives at the row as a slot and the tile's two verbs arrive as callbacks.

use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::MediaAttachment;
use nodus_pages::pages::compose::attachments::{
    Attachments as AttachmentList, AttachmentsProps as AttachmentListProps,
};
use nodus_ui::icons::Icon;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::state::alive::Alive;
use crate::state::bridge::actions;
use crate::state::bridge::drive::{self, DriveFile};

/// The id of the hidden file input, so the control row can open the picker.
const ATTACH_INPUT: &str = "nodus-attach-input";

/// The hidden button the control row's drive control clicks, so the picker's state stays
/// in this crate's component while the button stays in the view's row.
const DRIVE_OPEN: &str = "nodus-drive-open";

/// Opens the drive picker from the control row.
pub fn request_drive() {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(DRIVE_OPEN))
    {
        if let Some(button) = element.dyn_into::<web_sys::HtmlElement>().ok() {
            button.click();
        }
    }
}

/// Reads a picked file into bytes and its declared MIME type.
async fn read(file: web_sys::File) -> Result<(Vec<u8>, String), String> {
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| format!("Could not read the file: {error:?}"))?;
    Ok((js_sys::Uint8Array::new(&buffer).to_vec(), file.type_()))
}

/// Opens the hidden picker. The button lives in the control row, so the row is the only
/// place that has to know how a file gets chosen.
pub fn request_pick() {
    if let Some(input) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(ATTACH_INPUT))
        .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
    {
        input.click();
    }
}

/// The change handler of the hidden input: uploads every picked file, one at a time.
///
/// A free function rather than a closure: the input lives inside a slot, and the slot is
/// called more than once, so the handler cannot be a `FnOnce` that consumes the upload
/// signals.
fn pick(
    event: leptos::ev::Event,
    set_uploading: WriteSignal<bool>,
    set_percent: WriteSignal<Option<u8>>,
    media: RwSignal<Vec<MediaAttachment>>,
    set_error: WriteSignal<Option<String>>,
) {
    let Some(input) = event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
    else {
        return;
    };
    let Some(files) = input.files() else {
        return;
    };
    let picked: Vec<web_sys::File> = (0..files.length())
        .filter_map(|index| files.get(index))
        .collect();
    // Clearing the input is what lets the same file be picked twice in a row.
    input.set_value("");
    if picked.is_empty() {
        return;
    }
    let alive = Alive::new();
    set_uploading.set(true);
    set_error.set(None);
    spawn_local(async move {
        for file in picked {
            let name = file.name();
            match read(file).await {
                Ok((bytes, mime)) => {
                    let mime = if mime.is_empty() {
                        "application/octet-stream".to_owned()
                    } else {
                        mime
                    };
                    // The bar has to move while the bytes leave the browser, so the
                    // upload reports each chunk it has sent.
                    alive.if_alive(|| set_percent.set(Some(0)));
                    let progress = {
                        let alive = alive;
                        move |percent: u8| {
                            alive.if_alive(|| {
                                set_percent.set(Some(percent));
                            });
                        }
                    };
                    match actions::upload_media_with_progress(bytes, &mime, Some(&name), progress)
                        .await
                    {
                        Ok(attachment) => {
                            let id = attachment.id.clone();
                            alive.if_alive(|| media.update(|list| list.push(attachment)));
                            spawn_local(async move {
                                for _ in 0..120 {
                                    gloo_timers::future::TimeoutFuture::new(500).await;
                                    let Ok(current) = actions::media_status(&id).await else {
                                        break;
                                    };
                                    let done = current
                                        .processing_state
                                        .as_deref()
                                        .is_none_or(|state| state == "ready" || state == "failed");
                                    media.update(|list| {
                                        if let Some(slot) =
                                            list.iter_mut().find(|item| item.id == current.id)
                                        {
                                            *slot = current.clone();
                                        }
                                    });
                                    if done {
                                        break;
                                    }
                                }
                            });
                        }
                        Err(message) => alive.if_alive(|| set_error.set(Some(message))),
                    }
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        }
        alive.if_alive(|| {
            set_uploading.set(false);
            set_percent.set(None);
        });
    });
}

/// The hidden file input and the thumbnails of what has been uploaded.
#[component]
pub fn Attachments(
    media: RwSignal<Vec<MediaAttachment>>,
    set_error: WriteSignal<Option<String>>,
    /// Whether this row draws its own media button; a caller with one in its bar passes false.
    #[prop(default = true)]
    show_button: bool,
) -> impl IntoView {
    let (uploading, set_uploading) = signal(false);
    let (percent, set_percent) = signal(None::<u8>);
    // The drive picker: open, what the drive answered, the search, and whether a read or
    // an attach is in flight. One flag for both, because the row shows the same spinner.
    let (open, set_open) = signal(false);
    let (files, set_files) = signal(Vec::<DriveFile>::new());
    let (query, set_query) = signal(String::new());
    let (loading, set_loading) = signal(false);
    // Which attachments came from the drive, and the filename each had there. Kept beside
    // `media` because the wire attachment carries only the minted id, not the drive name.
    let from_drive = RwSignal::new(Vec::<(String, String)>::new());

    let open_drive = Callback::new(move |()| {
        set_open.set(true);
        set_query.set(String::new());
        set_loading.set(true);
        let alive = Alive::new();
        spawn_local(async move {
            match drive::all_files().await {
                Ok(rows) => alive.if_alive(|| set_files.set(rows)),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
            alive.if_alive(|| set_loading.set(false));
        });
    });

    // Attaching a drive file mints an attachment over bytes the drive already holds: no
    // transfer, so the row has no progress to show, only the wait for the instance.
    let choose = Callback::new(move |file: DriveFile| {
        if file.processing_state != "ready" {
            return;
        }
        set_loading.set(true);
        let alive = Alive::new();
        spawn_local(async move {
            let name = file.filename.clone();
            match actions::attach_from_drive(&file.id).await {
                Ok(attachment) => {
                    let id = attachment.id.clone();
                    alive.if_alive(|| {
                        media.update(|list| list.push(attachment));
                        from_drive.update(|list| list.push((id, name)));
                        set_open.set(false);
                    });
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
            alive.if_alive(|| set_loading.set(false));
        });
    });

    let matches = Memo::new(move |_| {
        let needle = query.get().trim().to_lowercase();
        files
            .get()
            .into_iter()
            .filter(|file| needle.is_empty() || file.filename.to_lowercase().contains(&needle))
            .collect::<Vec<_>>()
    });

    // The input is a slot because it is the browser's, not the view's: `Files`, the accept
    // filter and the click that opens the dialog all belong to this crate.
    let input = Arc::new(move || {
        view! {
            {show_button.then(|| view! {
                <button
                    type="button"
                    class="btn btn-outline btn-sm gap-1 border-dashed max-sm:btn-square"
                    title="Add media"
                    aria-label="Add media"
                    disabled=move || uploading.get()
                    on:click=move |_| request_pick()
                >
                    <Icon icon=Icon::Image/>
                    <span class="max-sm:hidden">"Media"</span>
                </button>
                <button
                    type="button"
                    class="btn btn-outline btn-sm gap-1 border-dashed max-sm:btn-square"
                    title="Choose from your drive"
                    aria-label="Choose from your drive"
                    disabled=move || uploading.get()
                    on:click=move |_| open_drive.run(())
                >
                    <Icon icon=Icon::List/>
                    <span class="max-sm:hidden">"Drive"</span>
                </button>
            })}
            <input
                id=ATTACH_INPUT
                class="hidden"
                type="file"
                accept="image/*,video/*,audio/*"
                multiple
                disabled=move || uploading.get()
                on:change=move |event| pick(event, set_uploading, set_percent, media, set_error)
            />
            <button
                id=DRIVE_OPEN
                class="hidden"
                type="button"
                on:click=move |_| open_drive.run(())
            ></button>

            <Show when=move || open.get()>
                <div
                    class="fixed inset-0 z-50 flex items-center justify-center bg-base-300/60 p-4"
                    role="dialog"
                    aria-modal="true"
                    aria-label="Choose from your drive"
                >
                    <div class="card w-full max-w-md gap-2 bg-base-100 p-4 shadow-xl">
                        <h2 class="text-sm font-semibold">"Choose from your drive"</h2>
                        <input
                            class="input input-bordered input-sm w-full"
                            type="search"
                            placeholder="Search your drive"
                            aria-label="Search your drive"
                            prop:value=query
                            on:input=move |event| set_query.set(event_target_value(&event))
                        />
                        <Show when=move || loading.get()>
                            <span class="loading loading-spinner loading-sm" aria-label="Reading your drive"></span>
                        </Show>
                        <Show when=move || !loading.get() && matches.get().is_empty()>
                            <p class="text-sm opacity-70">"Nothing in your drive matches."</p>
                        </Show>
                        <ul class="flex max-h-72 flex-col gap-1 overflow-auto">
                            <For
                                each=move || matches.get()
                                key=|file| (file.id.clone(), file.processing_state.clone())
                                let:file
                            >
                                {
                                    let ready = file.processing_state == "ready";
                                    let chosen = file.clone();
                                    let name = file.filename.clone();
                                    let label = name.clone();
                                    view! {
                                        <li>
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-sm w-full justify-start gap-2"
                                                disabled=!ready
                                                on:click=move |_| choose.run(chosen.clone())
                                            >
                                                <span class="truncate">{name}</span>
                                                {(!ready).then(|| view! {
                                                    <span class="text-xs opacity-60">
                                                        {format!("{label}: the preview is still being made")}
                                                    </span>
                                                })}
                                            </button>
                                        </li>
                                    }
                                }
                            </For>
                        </ul>
                        <div class="flex justify-end">
                            <button
                                type="button"
                                class="btn btn-ghost btn-sm"
                                on:click=move |_| set_open.set(false)
                            >
                                "Cancel"
                            </button>
                        </div>
                    </div>
                </div>
            </Show>
        }
        .into_any()
    }) as ChildrenFn;

    let remove = Callback::new(move |id: String| {
        media.update(|list| list.retain(|item| item.id != id));
        from_drive.update(|list| list.retain(|(entry, _)| entry != &id));
    });

    // Alt text is edited after the upload because that is when the writer knows what the
    // picture is going to say in the post; asking for it before the transfer would be asking
    // about a picture nobody can see yet.
    let sensitive = Callback::new(move |(id, flag): (String, bool)| {
        let alive = Alive::new();
        spawn_local(async move {
            match actions::set_media_sensitive(&id, flag).await {
                Ok(updated) => alive.if_alive(|| {
                    media.update(|list| {
                        if let Some(slot) = list.iter_mut().find(|item| item.id == updated.id) {
                            *slot = updated;
                        }
                    });
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let describe = Callback::new(move |(id, text): (String, String)| {
        let alive = Alive::new();
        spawn_local(async move {
            match actions::update_media(&id, &text).await {
                Ok(updated) => {
                    media.update(|list| {
                        if let Some(slot) = list.iter_mut().find(|item| item.id == updated.id) {
                            *slot = updated;
                        }
                    });
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    AttachmentList(AttachmentListProps {
        media: media.into(),
        uploading: Signal::derive(move || uploading.get()),
        percent: Signal::derive(move || percent.get()),
        on_remove: Some(remove),
        on_alt: Some(describe),
        on_sensitive: Some(sensitive),
        from_drive: from_drive.into(),
        picker: Some(input),
    })
}
