//! The account's own pictures: profile picture and header image, uploaded and applied.
//!
//! This is the same path the settings editor uses, lifted out of that card so the profile can
//! offer it in place. A side banner is not here: it belongs to the session style, which has
//! its own editor and its own upload path. Keeping the two apart is what makes the four
//! controls on a profile mean four different things.

use std::cell::RefCell;

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::pages::settings::form::ProfileImage;
use nodus_pages::profile::ProfileEditor;
use nodus_ui::session_style::BannerRegionKind;
use wasm_bindgen::JsCast as _;

use nodus_client_core::model::Account;

use crate::state::bridge::{actions, style};

/// How many times to look again while the instance is still making the small copy.
const PROCESSING_POLLS: u8 = 30;

thread_local! {
    static BUSY: RefCell<Option<RwSignal<Option<ProfileImage>>>> = const { RefCell::new(None) };
    static MESSAGE: RefCell<Option<RwSignal<Option<Result<String, String>>>>> =
        const { RefCell::new(None) };
}

/// Creates the two app-lifetime signals while no owner exists.
///
/// Must run before the first mount, like every other signal in [`super`]: they are written
/// by an upload that outlives the profile screen which opened it, so a signal owned by that
/// screen would be disposed mid-flight and the next click would panic before the file dialog.
pub(crate) fn install() {
    let _ = busy();
    let _ = message();
}

fn busy() -> RwSignal<Option<ProfileImage>> {
    BUSY.with(|cell| *cell.borrow_mut().get_or_insert_with(|| RwSignal::new(None)))
}

fn message() -> RwSignal<Option<Result<String, String>>> {
    MESSAGE.with(|cell| *cell.borrow_mut().get_or_insert_with(|| RwSignal::new(None)))
}

/// The profile's four owner-only controls, reading the live session style for the banners.
#[must_use]
pub(crate) fn editor() -> ProfileEditor {
    let active_style = style::active_style_signal();
    ProfileEditor {
        pick_picture: Callback::new(|(kind, done): (ProfileImage, Callback<Account>)| {
            pick_picture(kind, done);
        }),
        edit_banner: Callback::new(|region: BannerRegionKind| {
            // The style editor already owns placement for every region. The region travels
            // ahead of the route so the editor opens on the banner the button named,
            // instead of at the top of the page where that banner is not.
            crate::state::bridge::style::focus_region(region);
            crate::router::navigate_href(nodus_web::shared::router::STYLE_EDITOR_PATH);
        }),
        busy: Signal::derive(move || busy().get()),
        message: Signal::derive(move || message().get()),
        left_banner: Signal::derive(move || !active_style.get().left.is_empty()),
        right_banner: Signal::derive(move || !active_style.get().right.is_empty()),
        on_saved: Callback::new(|_account: Account| {}),
    }
}

/// Opens the browser's file dialog and starts the upload for `kind` once a file is chosen.
fn pick_picture(kind: ProfileImage, done: Callback<Account>) {
    if busy().get_untracked().is_some() {
        return;
    }
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        message().set(Some(Err(String::from(
            "This browser has no file dialog to offer.",
        ))));
        return;
    };
    let Ok(element) = document.create_element("input") else {
        return;
    };
    let Ok(input) = element.dyn_into::<web_sys::HtmlInputElement>() else {
        return;
    };
    input.set_type("file");
    input.set_accept("image/*");
    let picker = input.clone();
    let on_change = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        let file = picker.files().and_then(|list| list.get(0));
        picker.set_value("");
        if let Some(file) = file {
            start_upload(kind, file, done);
        }
    });
    input
        .add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref())
        .ok();
    // The dialog is driven by the click, so the closure has to outlive this call.
    on_change.forget();
    input.click();
}

fn start_upload(kind: ProfileImage, file: web_sys::File, done: Callback<Account>) {
    if busy().get_untracked().is_some() {
        return;
    }
    busy().set(Some(kind));
    message().set(None);
    spawn_local(async move {
        let answer = upload(kind, file, |_| {}).await;
        if let Ok(account) = &answer {
            done.run(account.clone());
        }
        busy().set(None);
        message().set(Some(match answer {
            Ok(_) => Ok(format!("{} updated.", noun(kind))),
            Err(error) => Err(error),
        }));
    });
}

/// Reads the chosen file, uploads it, waits for a renderable copy, then applies it.
///
/// Shared with the settings card so both surfaces take the same path. A deferred upload
/// answers `202` with an id and no address; this keeps looking until the instance finishes,
/// and applies the attachment id either way: the write is the same, and the picture appears
/// as soon as the instance can render it.
pub(crate) async fn upload(
    kind: ProfileImage,
    file: web_sys::File,
    progress: impl Fn(u8) + 'static,
) -> Result<Account, String> {
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| format!("Could not read the file: {error:?}"))?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    // A browser may leave the type empty for an extension it does not know; the instance
    // sniffs the bytes anyway, so a PNG is the safe guess.
    let mime = if file.type_().is_empty() {
        String::from("image/png")
    } else {
        file.type_()
    };
    let mut media =
        actions::upload_media_with_progress(bytes, &mime, Some(&file.name()), progress).await?;
    // The instance answers before the preview copy exists, so a picture with no url yet is
    // still being processed: look again rather than applying an attachment nothing can render.
    for _ in 0..PROCESSING_POLLS {
        if media.url.is_some() || media.processing_state.as_deref() == Some("failed") {
            break;
        }
        gloo_timers::future::TimeoutFuture::new(400).await;
        let Ok(current) = actions::media_status(&media.id).await else {
            break;
        };
        media = current;
    }
    if media.processing_state.as_deref() == Some("failed") {
        return Err("The instance could not process that image.".to_owned());
    }
    match kind {
        ProfileImage::Avatar => actions::update_profile_images(Some(media.id), None).await,
        ProfileImage::Header => actions::update_profile_images(None, Some(media.id)).await,
    }
}

fn noun(kind: ProfileImage) -> &'static str {
    match kind {
        ProfileImage::Avatar => "Profile picture",
        ProfileImage::Header => "Header image",
    }
}
