//! What the screen says when the account's instance would not answer.
//!
//! A stored session whose instance is down is not a signed-out reader: the account is
//! still theirs, the timeline simply has nothing to show. The card names the instance and
//! points at the settings screen, where another account can be picked.

use leptos::prelude::*;

use crate::state::bridge::feed_ui;

/// The failure card, or nothing when the last read succeeded.
#[component]
pub fn Trouble() -> impl IntoView {
    let message = feed_ui().error;
    view! {
        <Show when=move || message.get().is_some()>
            <div class="alert alert-warning mb-3" role="alert" data-trouble="">
                <div class="flex flex-col gap-1">
                    <span class="font-medium">{move || message.get().unwrap_or_default()}</span>
                    <a class="link link-hover text-sm" href="/settings">
                        "Switch account or sign in again"
                    </a>
                </div>
            </div>
        </Show>
    }
}
