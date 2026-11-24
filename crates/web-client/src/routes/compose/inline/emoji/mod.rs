//! Emoji for the composer: shortcode suggestions while typing, and a picker.
#![allow(
    dead_code,
    reason = "the controls are wired into the textarea in the next step"
)]
//!
//! Both sources are the instance's own catalogue (`/api/v1/custom_emojis`), which is the
//! set Mastodon has cached — its own emoji plus every federated emoji it has seen. That
//! matters for the picker *and* for the suggestion list: an emoji copied from another
//! server is only renderable if this instance knows it, so offering anything else would be
//! offering a shortcode that arrives as plain text.
//!
//! What is left here is the part that is not markup: the token scanner, the fetch, and the
//! text edit that replaces a pending shortcode. The buttons and the suggestion list are the
//! view's ([`Suggestions`] is re-exported from there), because they are drawn the same way
//! wherever a composer appears.

use crate::state::bridge::actions;
use leptos::prelude::*;
use nodus_client_core::model::CustomEmoji;

pub use nodus_pages::pages::compose::inline::emoji::{complete, pending};

/// The catalogue, fetched the first time a composer needs it.
///
/// A signal rather than a local variable because the picker and the suggestion list read
/// the same list, and a second fetch would double the instance's largest metadata request.
pub fn catalogue() -> ReadSignal<Vec<CustomEmoji>> {
    let (emojis, set_emojis) = signal(Vec::<CustomEmoji>::new());
    leptos::task::spawn_local(async move {
        let all = actions::custom_emojis().await;
        set_emojis.set(all);
    });
    emojis
}
