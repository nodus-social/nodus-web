//! The account's data export.
//!
//! Nodus answers the export as one JSON document. Mastodon's export is an archive that
//! is assembled in the background, so there is no progress to watch here and no reason
//! to draw one: the action is a link, and the server's answer is the file.

use leptos::prelude::*;
use nodus_pages::components::panel::Panel;

use crate::state::bridge::{revision, with_sdk};

/// A download link for the signed-in account's export.
#[component]
pub fn ExportCard() -> impl IntoView {
    let revision = revision();
    let href = Signal::derive(move || {
        revision.get();
        with_sdk(|sdk| {
            sdk.state()
                .session
                .get()
                .map(|session| format!("/api/v1/accounts/{}/export", session.account_id))
        })
    });

    view! {
        <Panel title="Export">
            <p class="text-sm opacity-70">
                "Your posts, media descriptions, follows and account activities as this instance holds them, in one JSON document."
            </p>

            <Show
                when=move || href.get().is_some()
                fallback=|| view! { <p class="text-sm opacity-60">"Sign in to download your data."</p> }
            >
                <a
                    class="btn btn-primary btn-sm w-fit"
                    href=move || href.get().unwrap_or_default()
                    download="nodus-export.json"
                >
                    "Download your data"
                </a>
            </Show>
        </Panel>
    }
}
