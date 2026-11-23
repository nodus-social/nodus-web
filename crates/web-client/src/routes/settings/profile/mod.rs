//! What the account says about itself: display name, bio, profile picture and banner.

mod card;

pub(crate) use card::ProfileCard;

use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::pages::settings::form::{ProfileImage, ProfileImagePicker, ProfileImages};
use wasm_bindgen::JsCast as _;

use crate::state::alive::Alive;
use crate::state::bridge::{actions, profile_media, revision, with_sdk};

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
    let (avatar_shape, set_avatar_shape) = signal(None::<(u32, u32)>);
    let (header_shape, set_header_shape) = signal(None::<(u32, u32)>);
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
            set_avatar_shape.set(current.avatar_shape());
            set_header_shape.set(current.header_shape());
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
                        set_avatar_shape.set(current.avatar_shape());
                        set_header_shape.set(current.header_shape());
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

        spawn_local(async move {
            let progress = move |value: u8| alive.if_alive(|| set_percent.set(Some(value)));
            let answer = profile_media::upload(kind, file, progress)
                .await
                .map(|account| {
                    match kind {
                        ProfileImage::Avatar => {
                            set_avatar_shape.set(account.avatar_shape());
                            set_avatar.set(account.avatar);
                        }
                        ProfileImage::Header => {
                            set_header_shape.set(account.header_shape());
                            set_header.set(account.header);
                        }
                    }
                    format!("{} updated.", kind.label())
                });

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
                avatar_shape=Signal::derive(move || avatar_shape.get())
                header_shape=Signal::derive(move || header_shape.get())
                busy=Signal::derive(move || uploading.get().is_some() || fetching.get())
                progress=Signal::derive(move || percent.get())
                message=Signal::derive(move || message.get())
                picker=Some(picker)
            />
        </nodus_ui::components::panel::Panel>
    }
}
