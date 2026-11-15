//! Enabling background sync, which is not a request to us.
//!
//! The service holds a token of its own (D37), so "on" means the user granted *that*
//! application access on their instance. The browser can only open the consent screen:
//! the answer arrives at the service's callback, and the card re-reads `/me` when the
//! user comes back.

use leptos::prelude::*;

use crate::state::alive::Alive;
use crate::state::bridge::authorize_sync;

fn safe_popup_url(raw: &str) -> Option<String> {
    let parsed = url::Url::parse(raw.trim()).ok()?;
    if matches!(parsed.scheme(), "https" | "http") && parsed.has_host() {
        Some(parsed.into())
    } else {
        None
    }
}

/// Opens the instance's authorization screen, or reports why it could not.
///
/// `result` and `busy` are the card's own signals; `alive` is checked before every
/// write because the user may navigate away while the instance is answering.
pub(super) fn start(result: RwSignal<Option<String>>, busy: RwSignal<bool>, alive: Alive) {
    wasm_bindgen_futures::spawn_local(async move {
        match authorize_sync().await {
            Ok(url) => {
                let Some(safe_url) = safe_popup_url(&url) else {
                    alive.if_alive(|| {
                        busy.set(false);
                        result.set(Some(
                            "Could not start the authorization: invalid authorization URL."
                                .to_owned(),
                        ));
                    });
                    return;
                };
                if let Some(window) = web_sys::window() {
                    // A new tab: the instance's consent screen is not part of this app,
                    // and leaving the current page would lose the card that explains it.
                    let _ = window.open_with_url_and_target_and_features(
                        &safe_url,
                        "_blank",
                        "noopener,noreferrer",
                    );
                }
                alive.if_alive(|| {
                    busy.set(false);
                    result.set(Some(
                        "Approve Nodus on your instance, then come back — this card updates when you do."
                            .to_owned(),
                    ));
                });
            }
            Err(message) => alive.if_alive(|| {
                busy.set(false);
                result.set(Some(format!(
                    "Could not start the authorization: {message}"
                )));
            }),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::safe_popup_url;

    #[test]
    fn allows_http_and_https_urls() {
        assert_eq!(
            safe_popup_url("https://mastodon.social/oauth/authorize?client_id=abc").as_deref(),
            Some("https://mastodon.social/oauth/authorize?client_id=abc")
        );
        assert_eq!(
            safe_popup_url("  http://localhost:3000/oauth/authorize  ").as_deref(),
            Some("http://localhost:3000/oauth/authorize")
        );
    }

    #[test]
    fn rejects_non_http_and_malformed_urls() {
        for bad in [
            "",
            "javascript:alert(1)",
            "JAVASCRIPT:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "vbscript:msgbox(1)",
            "file:///etc/passwd",
            "https://",
            "/relative/path",
        ] {
            assert!(
                safe_popup_url(bad).is_none(),
                "expected safe_popup_url({bad:?}) to be None"
            );
        }
    }
}
