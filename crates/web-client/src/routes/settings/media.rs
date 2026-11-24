//! The media library screen, wired to the instance.
//!
//! The card is handed rows reduced to what a row shows; this module owns the read, the
//! filter, and the one write. A refused delete keeps the row it belongs to: the server's
//! sentence is paired with that id and the card prints it there, which is the same rule the
//! refusal has everywhere else in this client — a refusal is shown where it happened, never
//! as a toast that slides away before the reader reads it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::pages::settings::media::{MediaItem, MediaLibraryPage, MediaUsage};

use crate::routes::format::relative_time;
use crate::state::alive::Alive;
use crate::state::bridge::media;

use super::refusal;

/// One row, in the words the card shows.
fn item(row: &media::Row) -> MediaItem {
    MediaItem {
        id: row.id.clone(),
        preview_url: row.picture().to_owned(),
        description: row.description.clone(),
        uploaded: row
            .created_at
            .as_deref()
            .map_or_else(String::new, age_sentence),
        usage: MediaUsage::from_wire(&row.usage),
        duplicate: row.duplicate_of.is_some(),
    }
}

/// When it was uploaded, as the row says it: `just now`, `3h ago`, `12d ago`.
fn age_sentence(created_at: &str) -> String {
    let age = relative_time(created_at);
    if age.is_empty() {
        return String::new();
    }
    if age == "now" {
        "just now".to_owned()
    } else {
        format!("{age} ago")
    }
}

/// The media library, read from the instance.
#[component]
pub fn MediaLibraryScreen() -> impl IntoView {
    // One flag for the screen: a flag made inside a callback belongs to the block that
    // invoked it, and that block is gone by the time the reply lands.
    let alive = Alive::new();
    let (rows, set_rows) = signal(Vec::<media::Row>::new());
    let (filter, set_filter) = signal(None::<MediaUsage>);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (refusal_for, set_refusal) = signal(None::<(String, String)>);

    Effect::new(move |_| {
        spawn_local(async move {
            match media::load().await {
                Ok(found) => alive.if_alive(|| {
                    set_rows.set(found);
                    set_error.set(None);
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(refusal::sentence(&message)))),
            }
            alive.if_alive(|| set_loading.set(false));
        });
    });

    let items = Signal::derive(move || rows.get().iter().map(item).collect::<Vec<_>>());

    let remove = Callback::new(move |id: String| {
        let row = id.clone();
        spawn_local(async move {
            match media::delete(&id).await {
                Ok(()) => alive.if_alive(|| {
                    // The row goes because the server no longer holds it, not because the
                    // screen hid it: a local hide would come back on the next read.
                    set_rows.update(|rows| rows.retain(|row| row.id != id));
                    set_refusal.set(None);
                }),
                Err(message) => alive.if_alive(|| set_refusal.set(Some((row.clone(), message)))),
            }
        });
    });

    view! {
        <MediaLibraryPage
            items=items
            filter=Signal::derive(move || filter.get())
            loading=Signal::derive(move || loading.get())
            error=Signal::derive(move || error.get())
            refusal=Signal::derive(move || refusal_for.get())
            on_filter=Some(Callback::new(move |next: Option<MediaUsage>| set_filter.set(next)))
            on_delete=Some(remove)
        />
    }
}
