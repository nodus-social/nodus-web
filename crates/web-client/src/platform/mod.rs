//! The browser's edges, behind four functions.
//!
//! Everything that reaches the platform — storage, the clock, navigation and the task queue
//! — goes through here, so a component can be rendered somewhere that has none of them. That
//! is the precondition for drawing the same tree on the server: a screen that calls
//! `local_storage()` directly cannot be rendered into a string, and one that calls
//! [`storage::get`] can.

#![allow(
    dead_code,
    reason = "a seam is filled in before its call sites move; `storage::get`/`set` are live, \
              `clock` and `location` land here so the next migration has one place to go"
)]

/// Persistence the browser owns: a key/value store that survives a reload.
pub mod storage {
    /// The value stored under `key`, or `None` when storage is unavailable.
    ///
    /// Unavailable is not an error: private windows and blocked third-party storage both
    /// make `local_storage()` fail, and a preference that cannot be remembered must not stop
    /// the interface from rendering.
    #[must_use]
    pub fn get(key: &str) -> Option<String> {
        let storage = web_sys::window()?.local_storage().ok()??;
        storage.get_item(key).ok()?
    }

    /// Stores `value` under `key`, ignoring a refusal.
    pub fn set(key: &str, value: &str) {
        if let Some(Ok(Some(storage))) = web_sys::window().map(|window| window.local_storage()) {
            let _ = storage.set_item(key, value);
        }
    }

    /// Removes `key`, ignoring a refusal.
    pub fn remove(key: &str) {
        if let Some(Ok(Some(storage))) = web_sys::window().map(|window| window.local_storage()) {
            let _ = storage.remove_item(key);
        }
    }
}

/// The current time, in milliseconds since the Unix epoch.
pub mod clock {
    /// Milliseconds since the epoch, as the browser's clock reports them.
    ///
    /// This is wall-clock time, not a monotonic reading: it exists for cache timestamps,
    /// which must agree with the instance's own notion of "now", not for measuring intervals.
    // A `Date` holds a double; milliseconds since 1970 fit an `i64` for the next few
    // hundred thousand years, so the narrowing is the point rather than an oversight.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "epoch milliseconds are far inside i64"
    )]
    pub fn now_ms() -> i64 {
        js_sys::Date::now() as i64
    }
}

/// Where the reader is, and where they may go.
pub mod location {
    /// The current path, without the query string.
    #[must_use]
    pub fn path() -> String {
        web_sys::window()
            .and_then(|window| window.location().pathname().ok())
            .unwrap_or_default()
    }
}

/// Handing bytes to the reader as a file they can keep.
pub mod download {
    use wasm_bindgen::JsCast;

    /// How long a download's object URL outlives its click, so the browser has started
    /// reading it before the address is released.
    const RELEASE_AFTER_MS: i32 = 50;

    /// Hands `bytes` to the browser as a file named `name`.
    ///
    /// Some answers are files rather than JSON this console could decode — a scanner's
    /// content set, the audit log's own CSV. Those arrive as bytes and leave as an object
    /// URL over those same bytes. The browser only starts reading that URL after the click
    /// returns, so it is released on a later turn rather than in this one.
    pub fn save_bytes(bytes: &[u8], name: &str) -> Result<(), String> {
        let array = js_sys::Uint8Array::from(bytes);
        let parts = js_sys::Array::new();
        parts.push(&array.buffer());
        let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
            .map_err(|_| "Could not build the file.".to_owned())?;
        let url = web_sys::Url::create_object_url_with_blob(&blob)
            .map_err(|_| "Could not build the file's address.".to_owned())?;
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| "There is no page to download into.".to_owned())?;
        let anchor = document
            .create_element("a")
            .map_err(|_| "Could not build the link.".to_owned())?
            .dyn_into::<web_sys::HtmlAnchorElement>()
            .map_err(|_| "Could not build the link.".to_owned())?;
        anchor.set_href(&url);
        anchor.set_download(name);
        anchor.click();
        let release = wasm_bindgen::closure::Closure::once_into_js(move || {
            let _ = web_sys::Url::revoke_object_url(&url);
        });
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                release.unchecked_ref(),
                RELEASE_AFTER_MS,
            );
        }
        Ok(())
    }
}
