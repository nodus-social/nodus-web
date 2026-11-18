//! Per-account custom session styling and navigation layout state (`nodus-backend` sync,
//! SSR `#nodus-session-style` island adoption, multi-account `(host, account_id)` cache,
//! and local fallback persistence).

use std::cell::RefCell;

use leptos::prelude::*;
use nodus_client_core::rest::{HttpRequest, Method};
use nodus_client_sdk::instance_api::Instance;
use nodus_mastodon::BrowserInstance;
use nodus_mastodon::rest::Transport as _;
use nodus_ui::session_style::{BannerRegionKind, ImageSlot, STYLE_KEYS, SessionStyle, StyleEditor};
use nodus_web::shared::account_state::{AccountCache, AccountKey};
use serde::Deserialize;
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

/// Start editing from the style currently in force.
pub fn begin_edit() {
    draft().set(active_style_signal().get_untracked());
    notice().set(None);
    editing().set(true);
}

/// Asks the browser for a picture and files it under `kind` of the draft.
///
/// The dialog belongs to the browser, but the upload does not: the picture is sent through
/// the media endpoint the composer already uses, and only its URL and id reach the draft, so
/// nothing is uploaded twice and Save stays the single step that publishes anything.
pub fn pick_picture_for(kind: BannerRegionKind) {
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
            upload_pictures_into(kind, files);
        }
    });
    input
        .add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref())
        .ok();
    // The dialog is driven by the click, so the closure has to outlive this call.
    on_change.forget();
    input.click();
}

/// Uploads pictures one at a time and adds each to the draft as it lands.
fn upload_pictures_into(kind: BannerRegionKind, files: Vec<web_sys::File>) {
    leptos::task::spawn_local(async move {
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
                    // An upload that is still transcoding has no URL yet; the picture is
                    // left out of the draft rather than stored as a slot nothing can draw.
                    let url = attachment.url.as_deref().unwrap_or_default();
                    let Some(slot) = ImageSlot::new(
                        url,
                        attachment.description.as_deref(),
                        Some(&attachment.id),
                    ) else {
                        notice().set(Some(String::from(
                            "That picture came back with an address this site will not draw.",
                        )));
                        continue;
                    };
                    let mut landed = false;
                    draft().update(|style| {
                        landed = style.region_mut(kind).add_slot(slot);
                    });
                    notice().set(if landed {
                        None
                    } else {
                        Some(format!(
                            "{} already holds as many pictures as it can.",
                            kind.label()
                        ))
                    });
                }
                Err(error) => notice().set(Some(format!("Could not upload that picture: {error}"))),
            }
        }
    });
}

/// Leave the editor and drop the edits.
pub fn cancel_edit() {
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
    if !super::knows_extensions() {
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
    if !super::knows_extensions() {
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
