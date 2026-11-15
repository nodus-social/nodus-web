//! Client-side session styling route: handles image drag-and-drop / file picker uploads
//! via Mastodon media API, and persists styling/navigation updates via `bridge::style`.

use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::pages::settings::styling::{StylingPage, UploadSlotRenderer};
use nodus_ui::session_style::{BannerRegionKind, ImageSlot, MAX_SLOTS_PER_REGION, SessionStyle};
use wasm_bindgen::JsCast as _;
use wasm_bindgen_futures::JsFuture;

use crate::state::alive::Alive;
use crate::state::bridge::{actions, style as bridge_style};

async fn read_file_bytes(file: web_sys::File) -> Result<(Vec<u8>, String), String> {
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| format!("Could not read the file: {error:?}"))?;
    Ok((js_sys::Uint8Array::new(&buffer).to_vec(), file.type_()))
}

fn upload_files_into_region(
    files: Vec<web_sys::File>,
    kind: BannerRegionKind,
    draft: RwSignal<SessionStyle>,
    set_uploading: WriteSignal<bool>,
    set_percent: WriteSignal<Option<u8>>,
    set_error: WriteSignal<Option<String>>,
) {
    if files.is_empty() {
        return;
    }
    let alive = Alive::new();
    set_uploading.set(true);
    set_error.set(None);
    spawn_local(async move {
        for file in files {
            let current_len = draft.with_untracked(|s| s.region(kind).slots.len());
            if current_len >= MAX_SLOTS_PER_REGION {
                alive.if_alive(|| {
                    set_error.set(Some(format!(
                        "At most {MAX_SLOTS_PER_REGION} slots fit in {}.",
                        kind.label()
                    )));
                });
                break;
            }
            let name = file.name();
            match read_file_bytes(file).await {
                Ok((bytes, mime)) => {
                    let mime = if mime.is_empty() {
                        "image/png".to_owned()
                    } else {
                        mime
                    };
                    alive.if_alive(|| set_percent.set(Some(0)));
                    let progress = move |percent: u8| {
                        alive.if_alive(|| set_percent.set(Some(percent)));
                    };
                    match actions::upload_media_with_progress(bytes, &mime, Some(&name), progress)
                        .await
                    {
                        Ok(mut media) => {
                            if media.url.is_none() {
                                for _ in 0..30 {
                                    gloo_timers::future::TimeoutFuture::new(400).await;
                                    let Ok(current) = actions::media_status(&media.id).await else {
                                        break;
                                    };
                                    media = current;
                                    if media.url.is_some()
                                        || media.processing_state.as_deref() == Some("failed")
                                    {
                                        break;
                                    }
                                }
                            }
                            let resolved_url = media.url.or(media.preview_url);
                            match resolved_url
                                .and_then(|url| ImageSlot::new(&url, Some(&name), Some(&media.id)))
                            {
                                Some(slot) => {
                                    alive.if_alive(|| {
                                        draft.update(|s| {
                                            let _ = s.region_mut(kind).add_slot(slot);
                                        });
                                    });
                                }
                                None => {
                                    alive.if_alive(|| {
                                        set_error.set(Some(
                                            "Upload finished, but no usable image URL was returned."
                                                .to_owned(),
                                        ));
                                    });
                                }
                            }
                        }
                        Err(message) => {
                            alive.if_alive(|| set_error.set(Some(message)));
                        }
                    }
                }
                Err(message) => {
                    alive.if_alive(|| set_error.set(Some(message)));
                }
            }
        }
        alive.if_alive(|| {
            set_uploading.set(false);
            set_percent.set(None);
        });
    });
}

#[component]
fn RegionUploadZone(kind: BannerRegionKind, draft: RwSignal<SessionStyle>) -> impl IntoView {
    let (uploading, set_uploading) = signal(false);
    let (percent, set_percent) = signal(None::<u8>);
    let (drag_over, set_drag_over) = signal(false);
    let (upload_error, set_upload_error) = signal(None::<String>);
    let input_id = format!("nodus-style-upload-{}", kind.metadata_key());
    let input_id_for_label = input_id.clone();

    view! {
        <div
            class="flex flex-col gap-2 rounded-box border-2 border-dashed p-3 transition-colors"
            class:border-primary=move || drag_over.get()
            class:bg-base-200=move || drag_over.get()
            class:border-base-300=move || !drag_over.get()
            on:dragover=move |ev: web_sys::DragEvent| {
                ev.prevent_default();
                set_drag_over.set(true);
            }
            on:dragleave=move |_| set_drag_over.set(false)
            on:drop=move |ev: web_sys::DragEvent| {
                ev.prevent_default();
                set_drag_over.set(false);
                let Some(files) = ev.data_transfer().and_then(|dt| dt.files()) else {
                    return;
                };
                let picked: Vec<web_sys::File> = (0..files.length())
                    .filter_map(|idx| files.get(idx))
                    .collect();
                upload_files_into_region(
                    picked,
                    kind,
                    draft,
                    set_uploading,
                    set_percent,
                    set_upload_error,
                );
            }
        >
            <div class="flex flex-wrap items-center justify-between gap-2">
                <span class="text-xs opacity-75">
                    "Drag and drop images here or choose a file to upload into this slot."
                </span>
                <label class="btn btn-outline btn-xs" for=input_id_for_label>
                    {move || if uploading.get() { "Uploading..." } else { "Upload image" }}
                </label>
                <input
                    id=input_id
                    type="file"
                    accept="image/*"
                    multiple=true
                    class="hidden"
                    disabled=move || uploading.get()
                    on:change=move |ev| {
                        let Some(input) = ev
                            .target()
                            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
                        else {
                            return;
                        };
                        let Some(files) = input.files() else {
                            return;
                        };
                        let picked: Vec<web_sys::File> = (0..files.length())
                            .filter_map(|idx| files.get(idx))
                            .collect();
                        input.set_value("");
                        upload_files_into_region(
                            picked,
                            kind,
                            draft,
                            set_uploading,
                            set_percent,
                            set_upload_error,
                        );
                    }
                />
            </div>
            <Show when=move || percent.get().is_some()>
                <progress
                    class="progress progress-primary h-1.5 w-full"
                    value=move || f64::from(percent.get().unwrap_or(0))
                    max="100"
                />
            </Show>
            <Show when=move || upload_error.get().is_some()>
                <p class="text-xs text-error">{move || upload_error.get().unwrap_or_default()}</p>
            </Show>
        </div>
    }
}

/// Interactive session styling & navigation layout editor route body.
#[component]
pub(crate) fn StylingEditor() -> impl IntoView {
    let active_style = bridge_style::active_style_signal();
    let draft = RwSignal::new(active_style.get_untracked());
    let (saving, set_saving) = signal(false);
    let (message, set_message) = signal(None::<String>);
    let (error, set_error) = signal(None::<String>);

    Effect::new(move |prev_key: Option<Option<String>>| {
        let key = bridge_style::current_account_key();
        let current = active_style.get();
        if prev_key.as_ref() != Some(&key) {
            draft.set(current);
        }
        key
    });

    let upload_zone: UploadSlotRenderer =
        Arc::new(move |kind| view! { <RegionUploadZone kind=kind draft=draft/> }.into_any());

    let on_save = Callback::new(move |next: SessionStyle| {
        let alive = Alive::new();
        set_saving.set(true);
        set_message.set(None);
        set_error.set(None);
        spawn_local(async move {
            match bridge_style::save_style(next).await {
                Ok(saved) => {
                    alive.if_alive(|| {
                        draft.set(saved);
                        set_message.set(Some(String::from("Session styling saved.")));
                        set_saving.set(false);
                    });
                }
                Err(err) => {
                    alive.if_alive(|| {
                        set_error.set(Some(err));
                        set_saving.set(false);
                    });
                }
            }
        });
    });

    let on_reset = Callback::new(move |()| {
        let alive = Alive::new();
        set_saving.set(true);
        set_message.set(None);
        set_error.set(None);
        spawn_local(async move {
            match bridge_style::reset_style().await {
                Ok(()) => {
                    alive.if_alive(|| {
                        draft.set(SessionStyle::default());
                        set_message.set(Some(String::from("Reset to default styling.")));
                        set_saving.set(false);
                    });
                }
                Err(err) => {
                    alive.if_alive(|| {
                        set_error.set(Some(err));
                        set_saving.set(false);
                    });
                }
            }
        });
    });

    view! {
        <StylingPage
            style=draft
            saving=Signal::derive(move || saving.get())
            message=Signal::derive(move || message.get())
            error=Signal::derive(move || error.get())
            on_save=Some(on_save)
            on_reset=Some(on_reset)
            upload_zone=upload_zone
        />
    }
}
