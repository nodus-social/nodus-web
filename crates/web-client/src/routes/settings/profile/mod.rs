//! What the account says about itself: display name, bio, profile picture and banner.

use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::pages::settings::form::{
    ProfileFields, ProfileImage, ProfileImagePicker, ProfileImages,
};
use wasm_bindgen::JsCast as _;
use wasm_bindgen_futures::JsFuture;

use crate::state::alive::Alive;
use crate::state::bridge::{actions, revision, with_sdk};

/// The bio limit the instance enforces. Shown as a counter rather than enforced on every
/// keystroke: the instance owns the rule, and a paste longer than it should be visibly
/// over budget instead of silently truncated here.
const NOTE_MAX_CHARS: usize = 500;

/// How many times to look again while the instance is still making the small copy.
const PROCESSING_POLLS: u8 = 30;

/// The bytes and media type of a chosen file.
async fn read_file_bytes(file: web_sys::File) -> Result<(Vec<u8>, String), String> {
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| format!("Could not read the file: {error:?}"))?;
    Ok((js_sys::Uint8Array::new(&buffer).to_vec(), file.type_()))
}

/// The account's current pictures, uploaded the moment one is chosen.
///
/// The upload is the slow half and the write is instant, so the picture is applied as soon
/// as it lands rather than waiting for a Save: a reader who picked a banner has already
/// decided, and a second button to confirm it is a step that only exists to be clicked past.
#[component]
pub(crate) fn ProfileImagesCard() -> impl IntoView {
    let revision = revision();
    let alive = Alive::new();

    let account = move || {
        revision.get();
        with_sdk(|sdk| {
            sdk.state()
                .session
                .get()
                .and_then(|session| sdk.read_account(&session.account_id))
        })
    };

    let (avatar, set_avatar) = signal(String::new());
    let (header, set_header) = signal(String::new());
    let (uploading, set_uploading) = signal(None::<ProfileImage>);
    let (percent, set_percent) = signal(None::<u8>);
    let (message, set_message) = signal(None::<String>);
    let (seeded, set_seeded) = signal(false);
    let (fetching, set_fetching) = signal(false);

    Effect::new(move |_| {
        if seeded.get() {
            return;
        }
        if let Some(current) = account() {
            set_avatar.set(current.avatar);
            set_header.set(current.header);
            set_seeded.set(true);
            return;
        }
        if fetching.get_untracked() {
            return;
        }
        let Some(id) = with_sdk(|sdk| sdk.state().session.get().map(|session| session.account_id))
        else {
            return;
        };
        set_fetching.set(true);
        spawn_local(async move {
            let answer = actions::account(&id).await;
            alive.if_alive(|| {
                set_fetching.set(false);
                match answer {
                    Ok(current) => {
                        set_avatar.set(current.avatar);
                        set_header.set(current.header);
                        set_seeded.set(true);
                    }
                    Err(message) => set_message.set(Some(message)),
                }
            });
        });
    });

    let start_upload = move |kind: ProfileImage, file: web_sys::File| {
        if uploading.get_untracked().is_some() {
            return;
        }
        set_uploading.set(Some(kind));
        set_percent.set(Some(0));
        set_message.set(None);

        let name = file.name();
        spawn_local(async move {
            let read = read_file_bytes(file).await;
            let answer = match read {
                Ok((bytes, mime)) => {
                    // A browser may leave the type empty for an extension it does not know;
                    // the instance sniffs the bytes anyway, so a PNG is the safe guess.
                    let mime = if mime.is_empty() {
                        "image/png".to_owned()
                    } else {
                        mime
                    };
                    let progress = move |value: u8| alive.if_alive(|| set_percent.set(Some(value)));
                    match actions::upload_media_with_progress(bytes, &mime, Some(&name), progress)
                        .await
                    {
                        Ok(mut media) => {
                            // The instance answers before the preview copy exists, so a
                            // picture with no url yet is still being processed: look again
                            // rather than applying an attachment nothing can render.
                            for _ in 0..PROCESSING_POLLS {
                                if media.url.is_some()
                                    || media.processing_state.as_deref() == Some("failed")
                                {
                                    break;
                                }
                                gloo_timers::future::TimeoutFuture::new(400).await;
                                let Ok(current) = actions::media_status(&media.id).await else {
                                    break;
                                };
                                media = current;
                            }
                            if media.processing_state.as_deref() == Some("failed") {
                                Err("The instance could not process that image.".to_owned())
                            } else {
                                let written = match kind {
                                    ProfileImage::Avatar => {
                                        actions::update_profile_images(Some(media.id), None)
                                            .await
                                            .map(|account| set_avatar.set(account.avatar))
                                    }
                                    ProfileImage::Header => {
                                        actions::update_profile_images(None, Some(media.id))
                                            .await
                                            .map(|account| set_header.set(account.header))
                                    }
                                };
                                written.map(|()| format!("{} updated.", kind.label()))
                            }
                        }
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            };

            alive.if_alive(|| {
                set_uploading.set(None);
                set_percent.set(None);
                set_message.set(Some(match answer {
                    Ok(message) => message,
                    Err(message) => message,
                }));
            });
        });
    };

    let picker: ProfileImagePicker = Arc::new(move |kind: ProfileImage| {
        let input_id = format!("nodus-profile-{}", kind.key());
        let label = kind.label();
        let label_for = input_id.clone();
        view! {
            <span class="flex flex-col gap-1">
                <label class="btn btn-outline btn-xs" for=label_for>
                    {move || if uploading.get() == Some(kind) { "Uploading…" } else { label }}
                </label>
                <span class="text-xs opacity-60">{kind.hint()}</span>
                <input
                    id=input_id
                    type="file"
                    accept="image/*"
                    class="hidden"
                    disabled=move || uploading.get().is_some()
                    on:change=move |event| {
                        let Some(input) = event
                            .target()
                            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
                        else {
                            return;
                        };
                        let Some(files) = input.files() else {
                            return;
                        };
                        let Some(file) = files.get(0) else {
                            return;
                        };
                        input.set_value("");
                        start_upload(kind, file);
                    }
                />
            </span>
        }
        .into_any()
    });

    view! {
        <nodus_ui::components::panel::Panel title="Pictures">
            <ProfileImages
                avatar=Signal::derive(move || avatar.get())
                header=Signal::derive(move || header.get())
                busy=Signal::derive(move || uploading.get().is_some() || fetching.get())
                progress=Signal::derive(move || percent.get())
                message=Signal::derive(move || message.get())
                picker=Some(picker)
            />
        </nodus_ui::components::panel::Panel>
    }
}

/// The profile card.
///
/// The fields are seeded once and then owned by the user until Save: writing them on every
/// store change would overwrite what is being typed as soon as any other screen refreshed
/// the account.
#[component]
pub(crate) fn ProfileCard() -> impl IntoView {
    let revision = revision();
    let alive = Alive::new();

    let account = move || {
        revision.get();
        with_sdk(|sdk| {
            sdk.state()
                .session
                .get()
                .and_then(|session| sdk.read_account(&session.account_id))
        })
    };

    let (display_name, set_display_name) = signal(String::new());
    let (note, set_note) = signal(String::new());
    let (busy, set_busy) = signal(false);
    let (message, set_message) = signal(None::<String>);
    let (seeded, set_seeded) = signal(false);
    let (fetching, set_fetching) = signal(false);

    Effect::new(move |_| {
        if seeded.get() {
            return;
        }
        if let Some(current) = account() {
            set_display_name.set(current.display_name);
            set_note.set(current.note);
            set_seeded.set(true);
            return;
        }
        if fetching.get_untracked() {
            return;
        }
        let Some(id) = with_sdk(|sdk| sdk.state().session.get().map(|session| session.account_id))
        else {
            return;
        };
        set_fetching.set(true);
        spawn_local(async move {
            let answer = actions::account(&id).await;
            alive.if_alive(|| {
                set_fetching.set(false);
                match answer {
                    Ok(current) => {
                        set_display_name.set(current.display_name);
                        set_note.set(current.note);
                        set_seeded.set(true);
                    }
                    Err(message) => set_message.set(Some(message)),
                }
            });
        });
    });

    let save = move || {
        if busy.get() {
            return;
        }
        set_busy.set(true);
        set_message.set(None);
        let display_name = display_name.get_untracked();
        let note = note.get_untracked();
        spawn_local(async move {
            let answer = actions::update_profile(display_name, note).await;
            alive.if_alive(|| {
                set_busy.set(false);
                set_message.set(Some(match answer {
                    Ok(_) => "Saved.".to_owned(),
                    Err(message) => message,
                }));
            });
        });
    };

    view! {
        <div class="flex flex-col gap-4">
            <nodus_ui::components::panel::Panel title="Profile">
                <ProfileFields
                    display_name=Signal::derive(move || display_name.get())
                    note=Signal::derive(move || note.get())
                    max_note=NOTE_MAX_CHARS
                    busy=Signal::derive(move || busy.get() || fetching.get())
                    message=Signal::derive(move || message.get())
                    on_display_name=Some(Callback::new(move |value: String| set_display_name.set(value)))
                    on_note=Some(Callback::new(move |value: String| set_note.set(value)))
                    on_save=Some(Callback::new(move |()| save()))
                />
            </nodus_ui::components::panel::Panel>
            <ProfileImagesCard/>
        </div>
    }
}
