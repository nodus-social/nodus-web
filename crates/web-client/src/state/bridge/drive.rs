//! The cloud drive: the caller's own folders and files.
//!
//! Every call answers for the signed-in account and no one else — the server keys both
//! tables by `(account_id, id)` — so there is no "whose drive" question here. What the
//! screen needs is the summary, the folder tree, the files of the open folder, and the
//! five writes: a folder, a rename/move, a delete, the visibility, and an upload.
//!
//! Uploads travel as `multipart/form-data` through `XMLHttpRequest` rather than `fetch`:
//! the upload's `progress` events are the only per-file signal a browser gives, and the
//! screen reports how much of the file has left this machine.

use std::rc::Rc;

use serde::Deserialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen_futures::JsFuture;
use web_sys::{File, FormData, HtmlAnchorElement, ProgressEvent, RequestInit, Response};

/// The scope a read of the drive needs.
pub const READ: &str = "read:drive";
/// The scope a write to the drive needs.
pub const WRITE: &str = "write:drive";

/// What the drive holds, as `GET /api/v1/drive` answers it.
///
/// There is no quota in this document, so the screen shows the used figure and invents
/// no denominator: a meter whose limit the server never named would be a lie in pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct Summary {
    /// Deduplicated bytes: two rows naming one digest occupy the quota once.
    pub used_bytes: i64,
    /// How many files the drive holds.
    pub objects: i64,
    /// How many folders, the root not included.
    pub folders: i64,
}

/// One folder in the tree, as the API nests it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Folder {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub children: Vec<Folder>,
}

/// The whole tree, roots first.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tree {
    #[serde(default)]
    pub folders: Vec<Folder>,
}

/// One file, as `GET /api/v1/drive/files` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DriveFile {
    pub id: String,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub byte_size: i64,
    #[serde(default)]
    pub visibility: String,
    #[serde(default)]
    pub processing_state: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// A folder flattened for the sidebar and the move picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    /// The folder's id, which is what a write names.
    pub id: String,
    /// The name the reader chose.
    pub name: String,
    /// Its parent, `None` at the root.
    pub parent_id: Option<String>,
    /// How far below the root it sits, so the sidebar can indent it.
    pub depth: usize,
}

/// Walks a tree into rows, parents before children.
pub fn flatten(folders: &[Folder], depth: usize, out: &mut Vec<FolderRow>) {
    for folder in folders {
        out.push(FolderRow {
            id: folder.id.clone(),
            name: folder.name.clone(),
            parent_id: folder.parent_id.clone(),
            depth,
        });
        flatten(&folder.children, depth + 1, out);
    }
}

/// The scopes this browser's session carries.
fn scopes() -> Vec<String> {
    super::with_sdk(|sdk| {
        sdk.state()
            .session
            .get()
            .map(|session| session.scopes)
            .unwrap_or_default()
    })
}

/// Whether the session's grant names the drive's read scope, or a token broad enough to
/// carry it.
///
/// The client's stored session assumes `read write follow` (see `session::scopes`), so a
/// strict `read:drive` test would hide the door from every session this client can
/// create. `read` is the honest wider grant those tokens carry: the server still answers
/// `403` for a token that lacks the drive scope, and the screen says so.
#[must_use]
pub fn may_read() -> bool {
    scopes()
        .iter()
        .any(|scope| scope == READ || scope == "read" || scope == "*")
}

/// Whether the same grant may write.
#[must_use]
pub fn may_write() -> bool {
    scopes()
        .iter()
        .any(|scope| scope == WRITE || scope == "write" || scope == "*")
}

/// The drive transport, or the sentence a signed-out reader gets.
fn transport() -> Result<nodus_mastodon::BrowserInstance, String> {
    super::instance_transport().ok_or_else(|| "Sign in to use the drive.".to_owned())
}

fn drive_url(path: &[&str]) -> Result<url::Url, String> {
    let mut segments = vec!["api", "v1", "drive"];
    segments.extend_from_slice(path);
    transport()?
        .url_segments(&segments)
        .map_err(|error| error.to_string())
}

fn json_request(
    method: &str,
    url: url::Url,
    body: &serde_json::Value,
) -> Result<nodus_mastodon::rest::HttpRequest, String> {
    let transport = transport()?;
    let bytes = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    Ok(match method {
        "PATCH" => transport.patch_body(url, "application/json", bytes),
        _ => transport.post_body(url, "application/json", bytes),
    })
}

/// The summary, the files of one folder, and the whole tree.
pub async fn load(folder: Option<&str>) -> Result<(Summary, Vec<DriveFile>, Vec<Folder>), String> {
    let transport = transport()?;
    let summary: Summary = transport
        .fetch(transport.get(drive_url(&[])?))
        .await
        .map_err(|error| error.to_string())?;
    let tree: Tree = transport
        .fetch(transport.get(drive_url(&["folders"])?))
        .await
        .map_err(|error| error.to_string())?;
    let mut files_url = drive_url(&["files"])?;
    if let Some(folder) = folder {
        files_url.set_query(Some(&format!("folder_id={folder}")));
    }
    let files: Vec<DriveFile> = transport
        .fetch(transport.get(files_url))
        .await
        .map_err(|error| error.to_string())?;
    Ok((summary, files, tree.folders))
}

/// Every file the drive holds, from every folder.
///
/// `GET /api/v1/drive/files` answers one folder at a time, and a picker cannot ask the
/// writer to open the folders first: the tree is walked and each folder's files are read,
/// in the order the sidebar lists them.
pub async fn all_files() -> Result<Vec<DriveFile>, String> {
    let (_, mut files, folders) = load(None).await?;
    let mut rows = Vec::new();
    flatten(&folders, 0, &mut rows);
    for row in rows {
        let transport = transport()?;
        let mut url = drive_url(&["files"])?;
        url.set_query(Some(&format!("folder_id={}", row.id)));
        let more: Vec<DriveFile> = transport
            .fetch(transport.get(url))
            .await
            .map_err(|error| error.to_string())?;
        files.extend(more);
    }
    Ok(files)
}

/// Adds a folder under `parent` (`None` at the root).
pub async fn create_folder(name: &str, parent: Option<&str>) -> Result<(), String> {
    let transport = transport()?;
    let body = serde_json::json!({ "name": name.trim(), "parent_id": parent });
    transport
        .fetch::<serde_json::Value>(json_request(
            "POST",
            drive_url(&["folders"])?,
            &body,
        )?)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Renames one folder, leaving its place in the tree alone.
pub async fn rename_folder(id: &str, name: &str) -> Result<(), String> {
    let transport = transport()?;
    let body = serde_json::json!({ "name": name.trim() });
    transport
        .fetch::<serde_json::Value>(json_request(
            "PATCH",
            drive_url(&["folders", id])?,
            &body,
        )?)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Deletes a folder and the subtree under it, which is what the confirmation says.
pub async fn delete_folder(id: &str) -> Result<(), String> {
    let transport = transport()?;
    let url = drive_url(&["folders", id])?;
    transport
        .send_empty(transport.delete(url))
        .await
        .map_err(|error| error.to_string())
}

/// Renames one file.
pub async fn rename_file(id: &str, filename: &str) -> Result<(), String> {
    let transport = transport()?;
    let body = serde_json::json!({ "filename": filename.trim() });
    transport
        .fetch::<serde_json::Value>(json_request(
            "PATCH",
            drive_url(&["files", id])?,
            &body,
        )?)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Moves one file to a folder, or to the root when `folder` is `None`.
pub async fn move_file(id: &str, folder: Option<&str>) -> Result<(), String> {
    let transport = transport()?;
    let body = serde_json::json!({ "folder_id": folder });
    transport
        .fetch::<serde_json::Value>(json_request(
            "PATCH",
            drive_url(&["files", id])?,
            &body,
        )?)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Changes one file's visibility.
pub async fn set_visibility(id: &str, visibility: &str) -> Result<(), String> {
    let transport = transport()?;
    let body = serde_json::json!({ "visibility": visibility });
    transport
        .fetch::<serde_json::Value>(json_request(
            "PATCH",
            drive_url(&["files", id])?,
            &body,
        )?)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Deletes one file and, when nothing else needs them, its bytes.
pub async fn delete_file(id: &str) -> Result<(), String> {
    let transport = transport()?;
    let url = drive_url(&["files", id])?;
    transport
        .send_empty(transport.delete(url))
        .await
        .map_err(|error| error.to_string())
}

/// Uploads one file, reporting the share of the body that has left the browser.
///
/// `multipart/form-data` with `file`, `folder_id` (when a folder is open) and
/// `visibility`. The browser writes the boundary, so no content type is set by hand.
pub async fn upload(
    file: &File,
    folder: Option<&str>,
    visibility: &str,
    on_progress: Rc<dyn Fn(u8)>,
) -> Result<DriveFile, String> {
    let transport = transport()?;
    let url = drive_url(&["files"])?;
    let form = FormData::new().map_err(|_| "This browser refused the upload.".to_owned())?;
    let name = file.name();
    form.append_with_blob_and_filename("file", file, &name)
        .map_err(|_| "This browser refused the upload.".to_owned())?;
    if let Some(folder) = folder {
        form.append_with_str("folder_id", folder)
            .map_err(|_| "This browser refused the upload.".to_owned())?;
    }
    form.append_with_str("visibility", visibility)
        .map_err(|_| "This browser refused the upload.".to_owned())?;

    let xhr = web_sys::XmlHttpRequest::new().map_err(|_| "This browser refused the upload.".to_owned())?;
    xhr.open_with_async("POST", url.as_str(), true)
        .map_err(|_| "This browser refused the upload.".to_owned())?;
    let token = transport.token().expose();
    if !token.is_empty() {
        xhr.set_request_header("Authorization", &format!("Bearer {token}"))
            .map_err(|_| "This browser refused the upload.".to_owned())?;
    }
    xhr.set_response_type(web_sys::XmlHttpRequestResponseType::Text);
    let upload = xhr.upload().map_err(|_| "This browser refused the upload.".to_owned())?;
    let ticker = Rc::clone(&on_progress);
    let upload_ticker = Closure::<dyn FnMut(ProgressEvent)>::new(move |event: ProgressEvent| {
        if event.length_computable() && event.total() > 0.0 {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a share of the body is a whole percentage"
            )]
            let percent = ((event.loaded() / event.total()) * 100.0) as u8;
            ticker(percent);
        }
    });
    upload.set_onprogress(Some(upload_ticker.as_ref().unchecked_ref()));

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let loaded = {
            let resolve = resolve.clone();
            let xhr = xhr.clone();
            Closure::<dyn FnMut()>::new(move || {
                resolve.call1(&JsValue::NULL, &xhr).ok();
            })
        };
        let failed = {
            let reject = reject.clone();
            Closure::<dyn FnMut()>::new(move || {
                reject.call0(&JsValue::NULL).ok();
            })
        };
        xhr.set_onload(Some(loaded.as_ref().unchecked_ref()));
        xhr.set_onerror(Some(failed.as_ref().unchecked_ref()));
        loaded.forget();
        failed.forget();
    });
    xhr.send_with_opt_form_data(Some(&form))
        .map_err(|_| "This browser refused the upload.".to_owned())?;
    upload_ticker.forget();
    JsFuture::from(promise)
        .await
        .map_err(|_| "The upload was interrupted.".to_owned())?;

    let status = xhr.status().map_err(|_| "The upload was interrupted.".to_owned())?;
    let text = xhr
        .response_text()
        .map_err(|_| "The upload was interrupted.".to_owned())?
        .unwrap_or_default();
    if status >= 400 {
        return Err(format!("The instance refused the upload: {text}"));
    }
    serde_json::from_str::<DriveFile>(&text).map_err(|error| error.to_string())
}

/// Fetches a file's bytes and hands them to the browser as a download.
///
/// A link cannot carry the `Authorization` header, so the bytes are fetched here and
/// clicked into a blob URL; the object URL is revoked once the click is dispatched.
pub async fn download(file: &DriveFile) -> Result<(), String> {
    let transport = transport()?;
    let url = drive_url(&["files", &file.id, "content"])?;
    let response: Response = JsFuture::from(
        web_sys::window()
            .ok_or_else(|| "This browser has no window.".to_owned())?
            .fetch_with_request(&{
                let init = RequestInit::new();
                init.set_method("GET");
                let headers = web_sys::Headers::new().map_err(|_| "This browser refused the download.".to_owned())?;
                headers
                    .set("Authorization", &format!("Bearer {}", transport.token().expose()))
                    .map_err(|_| "This browser refused the download.".to_owned())?;
                init.set_headers_headers(&headers);
                web_sys::Request::new_with_str_and_init(url.as_str(), &init)
                    .map_err(|_| "This browser refused the download.".to_owned())?
            }),
    )
    .await
    .map_err(|_| "The file could not be read.".to_owned())?
    .dyn_into::<Response>()
    .map_err(|_| "The answer was not a response.".to_owned())?;
    if !response.ok() {
        return Err(format!("The instance answered {} for the download.", response.status()));
    }
    let blob = JsFuture::from(response.blob().map_err(|_| "The file could not be read.".to_owned())?)
        .await
        .map_err(|_| "The file could not be read.".to_owned())?;
    let blob: web_sys::Blob = blob
        .dyn_into()
        .map_err(|_| "The answer was not a file.".to_owned())?;
    let window = web_sys::window().ok_or_else(|| "This browser has no window.".to_owned())?;
    let object = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| "This browser refused the download.".to_owned())?;
    let document = window
        .document()
        .ok_or_else(|| "This browser has no document.".to_owned())?;
    let anchor = document
        .create_element("a")
        .map_err(|_| "This browser refused the download.".to_owned())?
        .dyn_into::<HtmlAnchorElement>()
        .map_err(|_| "This browser refused the download.".to_owned())?;
    anchor.set_href(&object);
    anchor.set_download(&file.filename);
    anchor.click();
    web_sys::Url::revoke_object_url(&object).ok();
    Ok(())
}

/// Bytes as a reader reads them: `12.4 MB`.
#[must_use]
pub fn size(bytes: i64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    #[expect(
        clippy::cast_precision_loss,
        reason = "a byte count is displayed, not computed with"
    )]
    let mut value = bytes.max(0) as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes.max(0), UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
