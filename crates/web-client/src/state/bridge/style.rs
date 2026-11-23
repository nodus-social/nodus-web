//! Per-account custom session styling and navigation layout state (`nodus-backend` sync,
//! SSR `#nodus-session-style` island adoption, multi-account `(host, account_id)` cache,
//! and local fallback persistence).

use std::cell::{Cell, RefCell};

use leptos::prelude::*;
use nodus_client_core::rest::{HttpRequest, Method};
use nodus_client_sdk::instance_api::Instance;
use nodus_mastodon::BrowserInstance;
use nodus_mastodon::rest::Transport as _;
use nodus_ui::session_style::{
    ImageSlot, LibraryPicture, PictureTarget, STYLE_KEYS, SessionStyle, StyleEditor, UploadRequest,
};
use nodus_web::shared::account_state::{AccountCache, AccountKey};
use serde::Deserialize;
use url::Url;
use wasm_bindgen::JsCast as _;

use super::{accounts, active_cache_key, instance_transport, revision};

const STYLE_STORAGE_PREFIX: &str = "nodus.session_style.";

thread_local! {
    static STYLE_CACHE: RefCell<AccountCache<SessionStyle>> = RefCell::new(AccountCache::new());
    static STYLE_SIGNAL: RefCell<Option<RwSignal<SessionStyle>>> = const { RefCell::new(None) };
    static EDITING: RefCell<Option<RwSignal<bool>>> = const { RefCell::new(None) };
    static DRAFT: RefCell<Option<RwSignal<SessionStyle>>> = const { RefCell::new(None) };
    static NOTICE: RefCell<Option<RwSignal<Option<String>>>> = const { RefCell::new(None) };
    static EDITOR: RefCell<Option<StyleEditor>> = const { RefCell::new(None) };
    static INSTALLED: RefCell<bool> = const { RefCell::new(false) };
    /// The account's own pictures, as the editor's tray shows them.
    static LIBRARY: RefCell<Option<RwSignal<Vec<LibraryPicture>>>> = const { RefCell::new(None) };
    static LIBRARY_STATUS: RefCell<Option<RwSignal<Option<String>>>> = const { RefCell::new(None) };
    /// Object URLs this page minted for the tray; revoked when the tray is read again.
    static LIBRARY_URLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Bumped by every read of the collection, so a slow read that lost the race appends
    /// nothing to the tray the reader is now looking at.
    static LIBRARY_EPOCH: Cell<u64> = const { Cell::new(0) };
    /// Bumped whenever an editing session starts or ends. A picture upload that spans either
    /// boundary belongs to the editor that is gone, so its result is dropped.
    static PICK_EPOCH: Cell<u64> = const { Cell::new(0) };
}

#[derive(Deserialize)]
struct MetadataEnvelope {
    #[serde(default)]
    metadata: std::collections::BTreeMap<String, serde_json::Value>,
}

/// App-lifetime reactive signal holding the active account's `SessionStyle`.
pub fn active_style_signal() -> Signal<SessionStyle> {
    let signal = style_rw();
    Signal::derive(move || signal.get())
}

/// Whether the reader is editing the site on the page it is showing.
///
/// Editing is a mode of the shell, not a separate screen: the styling page is where pictures
/// are added, but the arrangement is changed where it is seen.
pub fn editing() -> RwSignal<bool> {
    EDITING.with_borrow_mut(|slot| *slot.get_or_insert_with(|| RwSignal::new(false)))
}

/// The style the editor is working on; the site paints this while editing is on.
pub fn draft() -> RwSignal<SessionStyle> {
    DRAFT
        .with_borrow_mut(|slot| *slot.get_or_insert_with(|| RwSignal::new(SessionStyle::default())))
}

/// The handle the painted shell uses to change its own modules.
///
/// One handle for the life of the document, not one per render: the handle owns the signal
/// that remembers what is being dragged, and a handle rebuilt on every paint leaves the rail
/// children holding a drag signal whose owner is already gone — reading it then panics with
/// "Tried to access a reactive value that has already been disposed".
pub fn editor() -> StyleEditor {
    EDITOR.with_borrow_mut(|slot| *slot.get_or_insert_with(|| StyleEditor::new(draft(), editing())))
}

/// The one line of feedback the editor toolbar shows (a picture that would not upload, for
/// instance). The editor has no room for a message elsewhere: it is the whole page.
pub fn notice() -> RwSignal<Option<String>> {
    NOTICE.with_borrow_mut(|slot| *slot.get_or_insert_with(|| RwSignal::new(None)))
}

/// The account's own pictures, the shelf the editor drags from.
pub fn library() -> RwSignal<Vec<LibraryPicture>> {
    LIBRARY.with_borrow_mut(|slot| *slot.get_or_insert_with(|| RwSignal::new(Vec::new())))
}

/// What the tray says while the collection is being read, or why it could not be.
pub fn library_status() -> RwSignal<Option<String>> {
    LIBRARY_STATUS.with_borrow_mut(|slot| *slot.get_or_insert_with(|| RwSignal::new(None)))
}

/// Start editing from the style currently in force.
pub fn begin_edit() {
    PICK_EPOCH.set(PICK_EPOCH.get() + 1);
    draft().set(active_style_signal().get_untracked());
    notice().set(None);
    editing().set(true);
    reload_library();
}

/// Read the account's media into the tray: one listing, then one drawable thumbnail each.
///
/// The listing is [`super::drive::all_files`] — three requests plus one per folder, however
/// deep the drive is. A drive file's contents are behind the account's own credential, so
/// each thumbnail is fetched through the instance transport and handed to the page as an
/// object URL; that address lives as long as this editing session and is never stored.
pub fn reload_library() {
    let pictures = library();
    let status = library_status();
    let epoch = LIBRARY_EPOCH.get() + 1;
    LIBRARY_EPOCH.set(epoch);
    for url in LIBRARY_URLS.with_borrow_mut(std::mem::take) {
        let _ = web_sys::Url::revoke_object_url(&url);
    }
    pictures.set(Vec::new());
    status.set(Some(String::from("Reading your drive…")));
    leptos::task::spawn_local(async move {
        let listed = match super::drive::all_files().await {
            Ok(files) => files,
            Err(error) => {
                if LIBRARY_EPOCH.get() == epoch {
                    status.set(Some(format!("Could not read your drive: {error}")));
                }
                return;
            }
        };
        let images: Vec<_> = listed
            .into_iter()
            .filter(|file| {
                file.mime_type.starts_with("image/")
                    && (file.processing_state.is_empty() || file.processing_state == "ready")
            })
            .take(60)
            .collect();
        if images.is_empty() {
            if LIBRARY_EPOCH.get() == epoch {
                status.set(Some(String::from(
                    "Your drive holds no pictures yet. Upload one below.",
                )));
            }
            return;
        }
        if LIBRARY_EPOCH.get() == epoch {
            status.set(None);
        }
        for file in images {
            let Some(bytes) = drive_picture_bytes(&file.id).await else {
                continue;
            };
            let Some(thumbnail) = object_url(&bytes) else {
                continue;
            };
            if LIBRARY_EPOCH.get() != epoch {
                let _ = web_sys::Url::revoke_object_url(&thumbnail);
                return;
            }
            LIBRARY_URLS.with_borrow_mut(|urls| urls.push(thumbnail.clone()));
            pictures.update(|tray| {
                tray.push(LibraryPicture {
                    id: file.id.clone(),
                    name: file.filename.clone(),
                    thumbnail,
                });
            });
        }
    });
}

/// The bytes of one drive picture, through the signed-in transport.
async fn drive_picture_bytes(id: &str) -> Option<Vec<u8>> {
    let transport = super::instance_transport()?;
    let url = transport
        .url_segments(&["api", "v1", "drive", "files", id, "content"])
        .ok()?;
    transport.fetch_bytes(transport.get(url)).await.ok()
}

/// A browser-only address for these bytes, good until the tray is read again.
fn object_url(bytes: &[u8]) -> Option<String> {
    let part = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&part.buffer());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).ok()?;
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}

/// What one upload answer gives the editor: a picture to place, and whether the instance is
/// still preparing the layer it points at.
#[derive(Debug)]
struct PlacedUpload {
    slot: ImageSlot,
    processing: bool,
}

/// Why an upload answer left nothing to place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadRefusal {
    /// The answer named no id, so no layer address could be derived.
    Unnamed,
    /// The answer named an address the shell will not draw.
    Unsupported,
}

impl UploadRefusal {
    fn message(self) -> &'static str {
        match self {
            Self::Unnamed => "The instance did not name the picture, so it could not be placed.",
            Self::Unsupported => "That picture came back with an address this site will not draw.",
        }
    }
}

/// Decides where an upload's answer can be drawn.
///
/// A `202` from a deferred upload carries the attachment's id but no URL yet. The layer's
/// canonical address is `/media/{id}/compressed` on the instance that answered, and the
/// media service renders it on first request, so the picture can be placed now instead of
/// being dropped until the reader uploads it again.
fn uploaded_picture(
    id: &str,
    url: Option<&str>,
    alt: Option<&str>,
    base: &Url,
) -> Result<PlacedUpload, UploadRefusal> {
    let id = id.trim();
    let url = url.map(str::trim).filter(|url| !url.is_empty());
    let (address, processing) = match url {
        Some(url) => (url.to_owned(), false),
        // A deferred upload answers `202` with the attachment's id and no address at all.
        // The layer's canonical address is the one the attach route mints on this instance,
        // and the media service renders it on first request, so deriving it now places the
        // picture immediately instead of dropping it until the reader uploads it again.
        None if !id.is_empty() => {
            let mut address = base.clone();
            address.set_query(None);
            address.set_fragment(None);
            {
                let mut segments = address
                    .path_segments_mut()
                    .map_err(|()| UploadRefusal::Unsupported)?;
                segments.clear().push("media").push(id).push("compressed");
            }
            (address.to_string(), true)
        }
        None => return Err(UploadRefusal::Unnamed),
    };
    let Some(slot) = ImageSlot::new(&address, alt, Some(id)) else {
        return Err(UploadRefusal::Unsupported);
    };
    Ok(PlacedUpload { slot, processing })
}

/// Mint the permanent address a drive picture needs before it can be stored in the style.
///
/// The style stores addresses, not files: attaching gives the picture one on this instance
/// (`/media/{id}/compressed`) without copying its bytes anywhere.
async fn attach_drive_picture(
    transport: &BrowserInstance,
    id: &str,
) -> Result<(String, String), String> {
    #[derive(Deserialize)]
    struct Attached {
        attachment_id: String,
        #[serde(default)]
        url: String,
    }
    let url = transport
        .url_segments(&["api", "v1", "drive", "files", id, "attach"])
        .map_err(|error| error.to_string())?;
    let attached: Attached = transport
        .fetch(transport.post(url))
        .await
        .map_err(|error| error.to_string())?;
    Ok((attached.attachment_id, attached.url))
}

/// The browser's half of a collection drop: resolve the file, then let the editor file it.
fn place_library_picture(picture: LibraryPicture, target: PictureTarget) {
    leptos::task::spawn_local(async move {
        let Some(transport) = super::instance_transport() else {
            notice().set(Some(String::from("Sign in to place a picture.")));
            return;
        };
        let (attachment_id, url) = match attach_drive_picture(&transport, &picture.id).await {
            Ok(attached) => attached,
            Err(error) => {
                notice().set(Some(format!("Could not place that picture: {error}")));
                return;
            }
        };
        let uploaded = match uploaded_picture(
            &attachment_id,
            Some(url.as_str()).filter(|url| !url.trim().is_empty()),
            None,
            transport.base(),
        ) {
            Ok(uploaded) => uploaded,
            Err(refusal) => {
                notice().set(Some(refusal.message().to_owned()));
                return;
            }
        };
        editor().place_library_slot(target, uploaded.slot);
        notice().set(uploaded.processing.then(|| {
            String::from(
                "That picture is still being processed; it will appear when the instance finishes.",
            )
        }));
    });
}

/// Put a dropped file into the collection, so it can be dragged from there like the rest.
pub fn upload_to_library(request: UploadRequest) {
    leptos::task::spawn_local(async move {
        let Some(thumbnail) = object_url(&request.bytes) else {
            notice().set(Some(String::from("Could not read that picture.")));
            return;
        };
        let parts = js_sys::Array::new();
        parts.push(&js_sys::Uint8Array::from(request.bytes.as_slice()));
        let options = web_sys::FilePropertyBag::new();
        options.set_type(&request.mime);
        let Ok(file) =
            web_sys::File::new_with_u8_array_sequence_and_options(&parts, &request.name, &options)
        else {
            notice().set(Some(String::from("This browser refused that file.")));
            return;
        };
        match super::drive::upload(&file, None, "private", std::rc::Rc::new(|_| {})).await {
            Ok(stored) => {
                LIBRARY_URLS.with_borrow_mut(|urls| urls.push(thumbnail.clone()));
                library().update(|tray| {
                    tray.push(LibraryPicture {
                        id: stored.id,
                        name: request.name,
                        thumbnail,
                    });
                });
                notice().set(None);
            }
            Err(error) => notice().set(Some(format!("Could not upload that picture: {error}"))),
        }
    });
}

/// Asks the browser for a picture and files it where `target` says of the draft.
///
/// The dialog belongs to the browser, but the upload does not: the picture is sent through
/// the media endpoint the composer already uses, and only its URL and id reach the draft, so
/// nothing is uploaded twice and Save stays the single step that publishes anything.
pub fn pick_picture_for(target: PictureTarget) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        notice().set(Some(String::from(
            "This browser has no file dialog to offer.",
        )));
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
        let files = picker
            .files()
            .map(|list| {
                (0..list.length())
                    .filter_map(|index| list.get(index))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !files.is_empty() {
            // The target and the epoch ride with the closure: the dialog can outlive the
            // click that opened it, and the answer has to say which slot it is for and
            // whether that editor is still the one on screen.
            upload_pictures_into(target, files, PICK_EPOCH.get());
        }
    });
    input
        .add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref())
        .ok();
    // The dialog is driven by the click, so the closure has to outlive this call.
    on_change.forget();
    input.click();
}

/// Uploads pictures one at a time and files each into the draft as it lands.
///
/// `epoch` is the editing session that asked; a file that arrives after the reader left that
/// session is uploaded and then dropped rather than landing in a draft nobody is looking at.
fn upload_pictures_into(target: PictureTarget, files: Vec<web_sys::File>, epoch: u64) {
    leptos::task::spawn_local(async move {
        let Some(base) = super::instance_transport().map(|instance| instance.base().clone()) else {
            notice().set(Some(String::from("Sign in to place a picture.")));
            return;
        };
        for file in files {
            let name = file.name();
            let buffer = match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                Ok(buffer) => buffer,
                Err(error) => {
                    notice().set(Some(format!("Could not read that file: {error:?}")));
                    continue;
                }
            };
            let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
            let mime = if file.type_().is_empty() {
                String::from("image/png")
            } else {
                file.type_()
            };
            match super::actions::upload_media_with_progress(bytes, &mime, Some(&name), |_| {})
                .await
            {
                Ok(attachment) => {
                    let uploaded = match uploaded_picture(
                        &attachment.id,
                        attachment.url.as_deref(),
                        attachment.description.as_deref(),
                        &base,
                    ) {
                        Ok(uploaded) => uploaded,
                        Err(refusal) => {
                            notice().set(Some(refusal.message().to_owned()));
                            continue;
                        }
                    };
                    let PlacedUpload { slot, processing } = uploaded;
                    if PICK_EPOCH.get() != epoch {
                        return;
                    }
                    let mut landed = false;
                    let done = match target {
                        PictureTarget::Region(kind) => {
                            draft().update(|style| {
                                landed = style.region_mut(kind).add_slot(slot);
                            });
                            if landed {
                                None
                            } else {
                                Some(format!(
                                    "{} already holds as many pictures as it can.",
                                    kind.label()
                                ))
                            }
                        }
                        PictureTarget::Slot(kind, index) => {
                            draft().update(|style| {
                                landed = style.region_mut(kind).replace_slot(index, slot);
                            });
                            if landed {
                                None
                            } else {
                                Some(String::from(
                                    "That picture is no longer on the page; nothing was replaced.",
                                ))
                            }
                        }
                    };
                    notice().set(done.or_else(|| {
                        processing.then(|| {
                            String::from(
                                "That picture is still being processed; it will appear when the instance finishes.",
                            )
                        })
                    }));
                }
                Err(error) => notice().set(Some(format!("Could not upload that picture: {error}"))),
            }
        }
    });
}

/// Leave the editor and drop the edits.
pub fn cancel_edit() {
    PICK_EPOCH.set(PICK_EPOCH.get() + 1);
    draft().set(active_style_signal().get_untracked());
    editing().set(false);
}

/// Save the draft, leave the editor, and publish the saved style.
///
/// The editor closes only after the save succeeds, so a failed request leaves the reader's
/// arrangement on screen to retry instead of throwing it away.
pub async fn save_edit() -> Result<SessionStyle, String> {
    // A layout that hides the way back to settings is not a layout, it is a trap: refuse it
    // while the reader is still in the editor with the draft on screen.
    if let Some(problem) = draft().with_untracked(|style| style.unplaced_required()) {
        notice().set(Some(problem.clone()));
        return Err(problem);
    }
    let saved = save_style(draft().get_untracked()).await?;
    editing().set(false);
    Ok(saved)
}

fn style_rw() -> RwSignal<SessionStyle> {
    STYLE_SIGNAL
        .with_borrow_mut(|slot| *slot.get_or_insert_with(|| RwSignal::new(SessionStyle::default())))
}

/// Ensures the app-lifetime signals exist and adopts SSR's `#nodus-session-style` island.
pub fn install() {
    let _ = style_rw();
    // Created here, with the style itself, so they belong to the application's owner rather
    // than to whichever component happened to read them first.
    let _ = editing();
    let _ = draft();
    let _ = library();
    let _ = library_status();
    editor().set_library_placer(Callback::new(|(picture, target)| {
        place_library_picture(picture, target);
    }));
    let _ = editor();
    let _ = notice();
    let already = INSTALLED.replace(true);
    if already {
        return;
    }
    if let Some(style) = read_ssr_style_island() {
        style_rw().set(style);
    }
}

/// Stores the SSR-painted style island under the restored/active `(host, account_id)` once known.
pub fn adopt_ssr_style_for_active() {
    let Some(style) = read_ssr_style_island() else {
        return;
    };
    let clean = style.sanitized();
    if let Some(key) = active_cache_key() {
        cache_put(&key, &clean);
    }
    style_rw().set(clean);
}

fn read_ssr_style_island() -> Option<SessionStyle> {
    let document = web_sys::window()?.document()?;
    let node = document.get_element_by_id("nodus-session-style")?;
    let raw = node.text_content()?;
    let parsed = serde_json::from_str::<SessionStyle>(&raw).ok()?;
    Some(parsed.sanitized())
}

fn storage_key(host: &str, account_id: &str) -> String {
    format!("{STYLE_STORAGE_PREFIX}{host}::{account_id}")
}

fn load_local(host: &str, account_id: &str) -> Option<SessionStyle> {
    if host.is_empty() || account_id.is_empty() {
        return None;
    }
    let storage = web_sys::window()?.local_storage().ok().flatten()?;
    let raw = storage
        .get_item(&storage_key(host, account_id))
        .ok()
        .flatten()?;
    let parsed = serde_json::from_str::<SessionStyle>(&raw).ok()?;
    Some(parsed.sanitized())
}

fn save_local(host: &str, account_id: &str, style: &SessionStyle) {
    if host.is_empty() || account_id.is_empty() {
        return;
    }
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    let key = storage_key(host, account_id);
    if style.is_default() {
        let _ = storage.remove_item(&key);
    } else if let Ok(json) = serde_json::to_string(style) {
        let _ = storage.set_item(&key, &json);
    }
}

fn cache_get(key: &AccountKey) -> Option<SessionStyle> {
    if let Some(found) = STYLE_CACHE.with_borrow(|cache| cache.get(key).cloned()) {
        return Some(found);
    }
    let loaded = load_local(&key.0, &key.1)?;
    STYLE_CACHE.with_borrow_mut(|cache| cache.insert(key.clone(), loaded.clone()));
    Some(loaded)
}

fn cache_put(key: &AccountKey, style: &SessionStyle) {
    let clean = style.clone().sanitized();
    STYLE_CACHE.with_borrow_mut(|cache| cache.insert(key.clone(), clean.clone()));
    save_local(&key.0, &key.1, &clean);
}

/// Thumbnail image URL for an account in the switcher, if customized.
pub fn cached_thumbnail(host: &str, account_id: &str) -> Option<String> {
    let key = (host.to_owned(), account_id.to_owned());
    cache_get(&key).and_then(|style| style.thumbnail_url().map(str::to_owned))
}

/// Updates the reactive shell style immediately from the per-account cache when an account
/// becomes active, then refreshes from `nodus-backend` in the background.
pub fn on_account_activated(host: &str, account_id: &str) {
    let key = (host.to_owned(), account_id.to_owned());
    let immediate = cache_get(&key).unwrap_or_default();
    style_rw().set(immediate);

    if host.is_empty() || account_id.is_empty() {
        return;
    }
    wasm_bindgen_futures::spawn_local(async move {
        let _ = refresh_active().await;
    });
}

/// Clears the displayed session style on sign-out when no active account remains.
pub fn on_signed_out() {
    style_rw().set(SessionStyle::default());
}

/// Fetch the active account's custom style metadata from `nodus-backend`.
pub async fn refresh_active() -> Result<SessionStyle, String> {
    if !keeps_styling_async().await {
        return Err(String::from("This instance does not keep session styling."));
    }
    let key = active_cache_key().ok_or_else(|| String::from("No active account."))?;
    let Some(transport) = instance_transport() else {
        return Err(String::from("No instance transport."));
    };
    let fetched = fetch_style_with(&transport).await?;
    cache_put(&key, &fetched);
    if active_cache_key().as_ref() == Some(&key) {
        style_rw().set(fetched.clone());
        sync_ssr_style(&key, &fetched).await;
    }
    Ok(fetched)
}

/// Background-fetch thumbnails/styles for remembered accounts in the account switcher.
pub async fn prefetch_account_thumbnails() -> bool {
    if !keeps_styling_async().await {
        return false;
    }
    let mut updated = false;
    for account in accounts::load() {
        let key = (account.host.clone(), account.id.clone());
        if cache_get(&key).is_some() {
            continue;
        }
        let Ok(instance) = Instance::parse(&account.host) else {
            continue;
        };
        let transport = BrowserInstance::new(&instance, &account.token);
        if let Ok(fetched) = fetch_style_with(&transport).await {
            cache_put(&key, &fetched);
            if active_cache_key().as_ref() == Some(&key) {
                style_rw().set(fetched);
            }
            updated = true;
        }
    }
    updated
}

/// Persist updated session styling to `nodus-backend`, update caches & shell immediately,
/// and notify the SSR host so subsequent document loads paint without a flash.
pub async fn save_style(style: SessionStyle) -> Result<SessionStyle, String> {
    if !keeps_styling_async().await {
        return Err(String::from("This instance does not keep session styling."));
    }
    let key = active_cache_key().ok_or_else(|| String::from("Sign in to save session style."))?;
    let Some(transport) = instance_transport() else {
        return Err(String::from("No active instance connection."));
    };

    let clean = style.sanitized();
    let patch_body = serde_json::json!({ "metadata": clean.to_patch_metadata() });
    let bytes = serde_json::to_vec(&patch_body).map_err(|error| error.to_string())?;
    let url = transport
        .url("/api/v1/nodus/account/metadata")
        .map_err(|error| error.to_string())?;
    let req = HttpRequest {
        method: Method::Patch,
        url,
        headers: vec![
            (
                String::from("Authorization"),
                format!("Bearer {}", transport.token().expose()),
            ),
            (
                String::from("Content-Type"),
                String::from("application/json"),
            ),
        ],
        body: Some(bytes),
    };
    let response = transport
        .transport()
        .send(req)
        .await
        .map_err(|error| error.to_string())?;
    if !(200..300).contains(&response.status) {
        let detail = String::from_utf8_lossy(&response.body);
        return Err(format!(
            "Instance rejected style update ({}): {}",
            response.status, detail
        ));
    }

    cache_put(&key, &clean);
    if active_cache_key().as_ref() == Some(&key) {
        style_rw().set(clean.clone());
        sync_ssr_style(&key, &clean).await;
    }
    Ok(clean)
}

/// Reset session styling by clearing all Nodus styling keys on `nodus-backend` and locally.
#[expect(
    dead_code,
    reason = "kept for the account page's reset: the fullscreen editor saves a draft instead"
)]
pub async fn reset_style() -> Result<(), String> {
    let key = active_cache_key().ok_or_else(|| String::from("Sign in first."))?;
    let Some(transport) = instance_transport() else {
        return Err(String::from("No active instance connection."));
    };
    let mut url = transport
        .url("/api/v1/nodus/account/metadata")
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("keys", &STYLE_KEYS.join(","));
    let req = transport.delete(url);
    transport
        .send_empty(req)
        .await
        .map_err(|error| error.to_string())?;

    let default = SessionStyle::default();
    cache_put(&key, &default);
    if active_cache_key().as_ref() == Some(&key) {
        style_rw().set(default.clone());
        sync_ssr_style(&key, &default).await;
    }
    Ok(())
}

/// Whether the account's instance keeps session styling.
///
/// Session styling is one part of the extension surface, so the question is the same one
/// the rest of the interface asks: does the instance this account lives on serve it?
async fn keeps_styling_async() -> bool {
    super::probe_extensions().await
}

async fn fetch_style_with(transport: &BrowserInstance) -> Result<SessionStyle, String> {
    let mut url = transport
        .url("/api/v1/nodus/account/metadata")
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("keys", &STYLE_KEYS.join(","));
    let req = transport.get(url);
    let envelope: MetadataEnvelope = transport
        .fetch(req)
        .await
        .map_err(|error| error.to_string())?;
    Ok(SessionStyle::from_metadata_map(&envelope.metadata))
}

fn normalized_host(host: &str) -> String {
    Instance::parse(host).map_or_else(
        |_| host.trim().to_ascii_lowercase(),
        |instance| {
            let domain = instance.base.host_str().unwrap_or_default();
            match instance.base.port() {
                Some(port) => format!("{domain}:{port}"),
                None => domain.to_owned(),
            }
        },
    )
}

async fn sync_ssr_style(key: &AccountKey, style: &SessionStyle) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(body) = serde_json::to_string(style) else {
        return;
    };
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_credentials(web_sys::RequestCredentials::SameOrigin);
    init.set_body(&wasm_bindgen::JsValue::from_str(&body));
    let Ok(request) = web_sys::Request::new_with_str_and_init("/api/session/style", &init) else {
        return;
    };
    let headers = request.headers();
    let _ = headers.set("content-type", "application/json");
    let _ = headers.set("x-nodus-host", &normalized_host(&key.0));
    let _ = headers.set("x-nodus-account-id", &key.1);
    if let Ok(response) =
        wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await
    {
        let _ = response.dyn_into::<web_sys::Response>();
    }
}

/// Active account key helper exposed for UI editors that reset draft state on account switch.
#[expect(
    dead_code,
    reason = "the styling page keyed uploads by it; the editor adds slots to the draft"
)]
pub fn current_account_key() -> Option<String> {
    let _ = revision().get();
    active_cache_key().map(|(host, account_id)| format!("{}::{account_id}", normalized_host(&host)))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::future::Future;
    use std::rc::Rc;
    use std::task::Poll;

    use nodus_client_core::Instance;
    use nodus_client_core::actions::MediaActions;
    use nodus_client_core::rest::{HttpRequest, HttpResponse, Transport};
    use nodus_mastodon::rest::RestInstance;
    use serde_json::json;
    use url::Url;

    use super::{UploadRefusal, uploaded_picture};

    /// An instance that answers from a queue, in the style of the bridge's other fakes.
    #[derive(Clone, Default)]
    struct FakeTransport {
        answers: Rc<RefCell<VecDeque<HttpResponse>>>,
    }

    impl FakeTransport {
        fn with(answers: Vec<HttpResponse>) -> Self {
            Self {
                answers: Rc::new(RefCell::new(answers.into())),
            }
        }
    }

    impl Transport for FakeTransport {
        fn send(
            &self,
            _request: HttpRequest,
        ) -> impl Future<Output = nodus_client_core::Result<HttpResponse>> {
            let answer = self
                .answers
                .borrow_mut()
                .pop_front()
                .expect("the test queued one answer per request");
            core::future::ready(Ok(answer))
        }
    }

    fn answer_response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            retry_after_s: None,
            body: body.as_bytes().to_vec(),
        }
    }

    /// Runs a future the fake transport never pends on to completion.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = core::pin::pin!(future);
        let waker = core::task::Waker::noop();
        let mut context = core::task::Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("the fake transport never pends"),
        }
    }

    fn instance(answers: Vec<HttpResponse>) -> (RestInstance<FakeTransport>, FakeTransport) {
        let transport = FakeTransport::with(answers);
        let base = Instance::parse("https://stub.instance").expect("a valid instance");
        (
            RestInstance::with_transport(&base, "token", transport.clone()),
            transport,
        )
    }

    #[test]
    fn a_pending_upload_is_placed_at_its_compressed_layer() {
        let answer = json!({
            "id": "42",
            "type": "image",
            "url": null,
            "preview_url": null,
            "description": null,
            "blurhash": null,
            "meta": {},
            "processing": "pending"
        });
        let (instance, _transport) = instance(vec![answer_response(202, &answer.to_string())]);
        let attachment = block_on(instance.upload_media(b"png", "image/png", None))
            .expect("a 202 is an answer, not a refusal");

        let placed = uploaded_picture(
            &attachment.id,
            attachment.url.as_deref(),
            attachment.description.as_deref(),
            instance.base(),
        )
        .expect("a pending upload names an id, so its layer can be derived");

        assert!(
            placed.processing,
            "a deferred layer is still being prepared by the instance"
        );
        assert_eq!(
            placed.slot.url, "https://stub.instance/media/42/compressed",
            "the address is the one the attach route mints on this instance"
        );
        assert_eq!(placed.slot.media_id.as_deref(), Some("42"));
    }

    #[test]
    fn an_address_the_shell_will_not_draw_is_still_refused() {
        let base = Url::parse("https://stub.instance").expect("a valid instance");
        let refusal = uploaded_picture("9", Some("javascript:alert(1)"), None, &base)
            .expect_err("the shell does not draw script addresses");

        assert_eq!(refusal, UploadRefusal::Unsupported);
        assert_eq!(
            refusal.message(),
            "That picture came back with an address this site will not draw."
        );
    }

    #[test]
    fn a_deferred_answer_without_an_id_says_the_picture_was_not_named() {
        let base = Url::parse("https://stub.instance").expect("a valid instance");
        let refusal = uploaded_picture("", None, None, &base)
            .expect_err("without an id there is no layer address to derive");

        assert_eq!(refusal, UploadRefusal::Unnamed);
        assert_eq!(
            refusal.message(),
            "The instance did not name the picture, so it could not be placed."
        );
    }
}
