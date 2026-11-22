//! Profile pictures for a reader who asked not to be shown motion.
//!
//! Mastodon splits a profile picture in two: `avatar` may animate, `avatar_static` must not,
//! and the same pair exists for the banner (`header` / `header_static`). A reader whose
//! system says `prefers-reduced-motion: reduce` should be handed the still; every other
//! reader the animation.
//!
//! The choice is made here once rather than at each call site, because the drawing sites are
//! many and the rule is one — and because the answer has to be safe when nothing can answer
//! it. Server rendering has no media query to read, so the server paints the still; the
//! client re-renders the same tree on hydration and swaps the animation in for a reader who
//! has no preference. A still is never wrong; an animation can be.

#[cfg(all(not(target_arch = "wasm32"), test))]
use std::cell::Cell;

/// The media query a reader's motion preference is read from.
#[cfg(target_arch = "wasm32")]
const QUERY: &str = "(prefers-reduced-motion: reduce)";

/// The profile picture to draw for `url`.
///
/// `url` is the account's `avatar`, the layer that may animate; the still twin is returned
/// when the reader asked for it.
#[must_use]
pub fn avatar(url: &str) -> String {
    chosen(url, "/avatar", asked())
}

/// The profile banner to draw for `url`.
///
/// `url` is the account's `header`; the rule is the avatar's.
#[must_use]
pub fn banner(url: &str) -> String {
    chosen(url, "/header", asked())
}

/// The still when the reader asked for it, the animation otherwise.
///
/// An unreadable preference counts as a request for the still: an animation the reader did
/// not ask for can hurt, a still they did not need cannot.
fn chosen(url: &str, layer: &str, reduce: Option<bool>) -> String {
    if reduce.unwrap_or(true) {
        still(url, layer)
    } else {
        url.to_owned()
    }
}

/// The still twin of a profile layer, when the URL has one.
///
/// The instance names the pair by a suffix on the same path — `/media/{id}/avatar` and
/// `/media/{id}/avatar_static` — which is the rule the backend applies when it fills the
/// `*_static` fields. A remote account's URL is not on that path and has no twin, so the
/// animation comes back unchanged; the backend reports the same URL for that account, which
/// keeps the two paints telling the same story.
fn still(url: &str, layer: &str) -> String {
    let Some(base) = url.strip_suffix(layer) else {
        return url.to_owned();
    };
    format!("{base}{layer}_static")
}

/// Whether the reader asked for reduced motion, or `None` when nothing can answer.
#[cfg(target_arch = "wasm32")]
fn asked() -> Option<bool> {
    let window = web_sys::window()?;
    let media = window.match_media(QUERY).ok()??;
    Some(media.matches())
}

/// Whether the reader asked for reduced motion, or `None` when nothing can answer.
///
/// This is the server's answer: a page is built before it reaches a browser, and a media
/// query belongs to the browser. See the module comment for why the unknown answer is the
/// still.
#[cfg(all(not(target_arch = "wasm32"), not(test)))]
fn asked() -> Option<bool> {
    None
}

#[cfg(all(not(target_arch = "wasm32"), test))]
thread_local! {
    /// A test has no browser to ask either, so it states the answer a reader would give.
    static ASKED: Cell<Option<bool>> = const { Cell::new(None) };
}

#[cfg(all(not(target_arch = "wasm32"), test))]
fn asked() -> Option<bool> {
    ASKED.with(Cell::get)
}

// No `wasm_bindgen_test` runner exists in this workspace, so the tests stay on the host and
// reach the preference through the seam above rather than through a browser.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use leptos::prelude::{Owner, RenderHtml as _};
    use nodus_client_core::model::Account;

    use super::{ASKED, avatar, banner, chosen, still};
    use crate::components::lists::account_row::{AccountRow, AccountRowProps};

    #[test]
    fn a_reader_who_asked_for_reduced_motion_gets_the_still() {
        assert_eq!(
            chosen("https://social.test/media/1/avatar", "/avatar", Some(true)),
            "https://social.test/media/1/avatar_static"
        );
    }

    #[test]
    fn a_reader_who_expressed_no_preference_gets_the_animation() {
        assert_eq!(
            chosen("https://social.test/media/1/avatar", "/avatar", Some(false)),
            "https://social.test/media/1/avatar"
        );
    }

    #[test]
    fn an_unreadable_preference_gets_the_still() {
        assert_eq!(
            chosen("https://social.test/media/1/avatar", "/avatar", None),
            "https://social.test/media/1/avatar_static"
        );
    }

    #[test]
    fn a_url_without_a_still_twin_is_returned_unchanged() {
        let remote = "https://far.example/avatars/original/ada.gif";
        assert_eq!(still(remote, "/avatar"), remote);
        assert_eq!(
            banner("https://social.test/media/2/header"),
            "https://social.test/media/2/header_static"
        );
    }

    fn account() -> Account {
        Account {
            id: "1".to_owned(),
            acct: "alice@example.org".to_owned(),
            display_name: "Alice".to_owned(),
            note: String::new(),
            avatar: "https://social.test/media/1/avatar".to_owned(),
            header: String::new(),
            bot: false,
            locked: false,
            followers_count: 0,
            following_count: 0,
            statuses_count: 0,
            last_status_at: None,
            discoverable: None,
            indexable: None,
            noindex: None,
            fields: Vec::new(),
        }
    }

    fn draw(prefers_still: bool) -> String {
        ASKED.with(|asked| asked.set(Some(prefers_still)));
        let rendered = Owner::new().with(|| {
            AccountRow(AccountRowProps {
                account: account(),
                note: String::new(),
                on_open: None,
                after: None,
            })
            .to_html()
        });
        ASKED.with(|asked| asked.set(None));
        rendered
    }

    #[test]
    fn account_row_draws_the_avatar_from_the_static_field_under_reduced_motion() {
        assert!(
            draw(true).contains("src=\"https://social.test/media/1/avatar_static\""),
            "the still is not drawn under reduced motion"
        );
        assert!(
            draw(false).contains("src=\"https://social.test/media/1/avatar\""),
            "the animation is not drawn when the reader has no preference"
        );
        assert_eq!(
            avatar("https://social.test/media/3/avatar"),
            "https://social.test/media/3/avatar_static"
        );
    }
}
