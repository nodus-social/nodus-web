//! The media library: every picture this account uploaded, and what still holds it.
//!
//! `GET /api/v1/media/library` answers each row with a `usage` of `status`, `drive`, `style`
//! or `none`, and `DELETE /api/v1/media/library/{id}` refuses (422, naming the use) anything
//! that is not `none`. The card draws that decision twice: it offers no Delete where the
//! answer already says no, and the caller's refusal — the sentence the server sent when a
//! row went stale between the read and the click — is printed in the row it happened to,
//! because a refusal shown anywhere else sends the reader looking for it.
//!
//! The rows arrive already reduced: the caller formats `created_at` against its own clock
//! (this crate reads no clock), and reduces `duplicate_of` to a flag, because the row says
//! a picture is a copy once and does not need to name the original.

use leptos::prelude::*;

use crate::components::panel::Panel;

/// What still holds one picture, in the server's four words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaUsage {
    /// Attached to a post.
    Status,
    /// A file in the account's own drive.
    Drive,
    /// A banner on the account's profile.
    Style,
    /// Nothing refers to it, so the server will delete it.
    Unused,
}

impl MediaUsage {
    /// Every word, in the order the filter offers them.
    pub const ALL: [Self; 4] = [Self::Status, Self::Drive, Self::Style, Self::Unused];

    /// The word the wire used.
    #[must_use]
    pub fn from_wire(name: &str) -> Self {
        match name {
            "status" => Self::Status,
            "drive" => Self::Drive,
            "style" => Self::Style,
            _ => Self::Unused,
        }
    }

    /// What the row says holds this picture.
    #[must_use]
    pub const fn sentence(self) -> &'static str {
        match self {
            Self::Status => "posted",
            Self::Drive => "in your drive",
            Self::Style => "on your profile banners",
            Self::Unused => "not used anywhere",
        }
    }

    /// Whether the row offers a Delete button.
    ///
    /// Only `none` does: the server refuses every other use with a 422, and a button that is
    /// known to fail is a promise the screen cannot keep. The row shows [`Self::refusal`]
    /// instead.
    #[must_use]
    pub const fn removable(self) -> bool {
        matches!(self, Self::Unused)
    }

    /// Why this picture cannot be deleted, or `None` when it can.
    #[must_use]
    pub const fn refusal(self) -> Option<&'static str> {
        match self {
            Self::Status => Some("still used by a post"),
            Self::Drive => Some("still in your drive"),
            Self::Style => Some("still on your profile banners"),
            Self::Unused => None,
        }
    }
}

/// One picture, reduced to what the row shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaItem {
    /// The id a delete names.
    pub id: String,
    /// The picture itself, falling back to the full size when the server sent no preview.
    pub preview_url: String,
    /// The account's own description, empty when they wrote none.
    pub description: String,
    /// How long ago it was uploaded, already spoken (`3h ago`, `just now`).
    pub uploaded: String,
    /// What still holds it.
    pub usage: MediaUsage,
    /// Whether another row already holds these bytes.
    pub duplicate: bool,
}

/// What the library holds, in one line: `12 pictures, 3 unused`.
#[must_use]
pub fn summary(items: &[MediaItem]) -> String {
    let total = items.len();
    let unused = items.iter().filter(|item| item.usage.removable()).count();
    let pictures = if total == 1 {
        "1 picture".to_owned()
    } else {
        format!("{total} pictures")
    };
    if unused == 0 {
        pictures
    } else {
        format!("{pictures}, {unused} unused")
    }
}

/// What an empty library means, rather than an empty box.
pub const EMPTY: &str = "You have not uploaded a picture yet.";

/// Whether a row survives the filter; `None` is every picture.
#[must_use]
pub fn admits(filter: Option<MediaUsage>, usage: MediaUsage) -> bool {
    filter.is_none_or(|wanted| wanted == usage)
}

/// The media library: the count, the usage filter, and one row per picture.
#[component]
pub fn MediaLibraryPage(
    #[prop(into)] items: Signal<Vec<MediaItem>>,
    #[prop(into)] filter: Signal<Option<MediaUsage>>,
    #[prop(into)] loading: Signal<bool>,
    #[prop(into)] error: Signal<Option<String>>,
    /// The server's refusal, paired with the row it belongs to.
    #[prop(into)]
    refusal: Signal<Option<(String, String)>>,
    #[prop(optional_no_strip)] on_filter: Option<Callback<Option<MediaUsage>>>,
    #[prop(optional_no_strip)] on_delete: Option<Callback<String>>,
) -> impl IntoView {
    let shown = Signal::derive(move || {
        let filter = filter.get();
        items
            .get()
            .into_iter()
            .filter(|item| admits(filter, item.usage))
            .collect::<Vec<_>>()
    });

    view! {
        <Panel title="Media">
            <p class="text-sm opacity-70">
                "Every picture this account uploaded, and what still holds each one. A picture can be deleted once nothing uses it."
            </p>

            <p class="text-xs opacity-70">{move || summary(&items.get())}</p>

            <div role="group" aria-label="Filter by use" class="flex flex-wrap gap-1">
                <button
                    class="btn btn-xs"
                    class:btn-active=move || filter.get().is_none()
                    aria-pressed=move || if filter.get().is_none() { "true" } else { "false" }
                    disabled=on_filter.is_none()
                    on:click=move |_| {
                        if let Some(on_filter) = on_filter {
                            on_filter.run(None);
                        }
                    }
                >
                    "All"
                </button>
                {MediaUsage::ALL
                    .into_iter()
                    .map(|usage| {
                        view! {
                            <button
                                class="btn btn-xs"
                                class:btn-active=move || filter.get() == Some(usage)
                                aria-pressed=move || {
                                    if filter.get() == Some(usage) { "true" } else { "false" }
                                }
                                disabled=on_filter.is_none()
                                on:click=move |_| {
                                    if let Some(on_filter) = on_filter {
                                        on_filter.run(Some(usage));
                                    }
                                }
                            >
                                {usage.sentence()}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>

            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error">{move || error.get().unwrap_or_default()}</p>
            </Show>

            // A read still on its way and an empty library look the same to the list below;
            // the empty sentence is left to the answer that really is empty.
            <Show when=move || loading.get() && items.get().is_empty()>
                <p class="text-sm opacity-60">"Reading your pictures…"</p>
            </Show>
            <Show when=move || !loading.get() && items.get().is_empty()>
                <p class="text-sm opacity-70">{EMPTY}</p>
            </Show>

            <ul class="flex flex-col gap-2">
                <For
                    each=move || shown.get()
                    key=|item| item.id.clone()
                    let:item
                >
                    {
                        let id = item.id.clone();
                        let description = if item.description.trim().is_empty() {
                            "No description".to_owned()
                        } else {
                            item.description.clone()
                        };
                        let alt = item.description.clone();
                        let refusal_id = id.clone();
                        view! {
                            <li class="flex items-start gap-3 rounded-box bg-base-200 p-2">
                                <img
                                    class="size-16 rounded-box object-cover"
                                    src=item.preview_url.clone()
                                    alt=alt
                                    loading="lazy"
                                />
                                <div class="min-w-0 flex-1">
                                    <p class="truncate text-sm font-medium">{description}</p>
                                    <p class="text-xs opacity-70">
                                        "Uploaded "{item.uploaded.clone()}
                                    </p>
                                    <p class="text-xs opacity-70">{item.usage.sentence()}</p>
                                    <Show when=move || item.duplicate>
                                        <p class="text-xs opacity-50">
                                            "A copy of another picture here."
                                        </p>
                                    </Show>
                                    {match item.usage.refusal() {
                                        Some(reason) => view! {
                                            <p class="text-xs opacity-60">
                                                {format!("Cannot be deleted: {reason}.")}
                                            </p>
                                        }
                                            .into_any(),
                                        None => view! {
                                            <button
                                                class="btn btn-ghost btn-xs"
                                                disabled=on_delete.is_none()
                                                on:click=move |_| {
                                                    if let Some(on_delete) = on_delete {
                                                        on_delete.run(id.clone());
                                                    }
                                                }
                                            >
                                                "Delete"
                                            </button>
                                        }
                                            .into_any(),
                                    }}
                                    // The refusal is shown where it happened, not as a
                                    // toast: this is the row the reader clicked.
                                    <Show when=move || {
                                        refusal
                                            .get()
                                            .is_some_and(|(row, _)| row == refusal_id)
                                    }>
                                        <p role="alert" class="text-xs text-error">
                                            {move || {
                                                refusal
                                                    .get()
                                                    .map(|(_, message)| message)
                                                    .unwrap_or_default()
                                            }}
                                        </p>
                                    </Show>
                                </div>
                            </li>
                        }
                    }
                </For>
            </ul>
        </Panel>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, usage: MediaUsage) -> MediaItem {
        MediaItem {
            id: id.to_owned(),
            preview_url: String::new(),
            description: String::new(),
            uploaded: String::new(),
            usage,
            duplicate: false,
        }
    }

    #[test]
    fn every_use_is_one_human_sentence() {
        assert_eq!(MediaUsage::Status.sentence(), "posted");
        assert_eq!(MediaUsage::Drive.sentence(), "in your drive");
        assert_eq!(MediaUsage::Style.sentence(), "on your profile banners");
        assert_eq!(MediaUsage::Unused.sentence(), "not used anywhere");
    }

    #[test]
    fn the_wire_word_is_the_one_the_server_sends() {
        assert_eq!(MediaUsage::from_wire("status"), MediaUsage::Status);
        assert_eq!(MediaUsage::from_wire("drive"), MediaUsage::Drive);
        assert_eq!(MediaUsage::from_wire("style"), MediaUsage::Style);
        assert_eq!(MediaUsage::from_wire("none"), MediaUsage::Unused);
    }

    /// The button exists exactly when the server would accept the delete; every other use
    /// names what holds it instead.
    #[test]
    fn only_an_unused_picture_offers_a_delete() {
        assert!(MediaUsage::Unused.removable());
        assert_eq!(MediaUsage::Unused.refusal(), None);
        for usage in [MediaUsage::Status, MediaUsage::Drive, MediaUsage::Style] {
            assert!(!usage.removable(), "{usage:?}");
            assert!(usage.refusal().is_some(), "{usage:?}");
        }
        assert_eq!(
            MediaUsage::Status.refusal(),
            Some("still used by a post"),
            "the reason names the thing that holds it"
        );
    }

    #[test]
    fn the_count_names_the_library_and_how_much_is_free() {
        let mut items = vec![
            item("a", MediaUsage::Status),
            item("b", MediaUsage::Drive),
            item("c", MediaUsage::Unused),
        ];
        assert_eq!(summary(&items), "3 pictures, 1 unused");
        items.clear();
        assert_eq!(summary(&items), "0 pictures");
        assert_eq!(
            summary(&[item("a", MediaUsage::Unused)]),
            "1 picture, 1 unused"
        );
    }

    #[test]
    fn a_filter_admits_one_use_and_no_filter_admits_every_picture() {
        assert!(admits(None, MediaUsage::Status));
        assert!(admits(None, MediaUsage::Unused));
        assert!(admits(Some(MediaUsage::Unused), MediaUsage::Unused));
        assert!(!admits(Some(MediaUsage::Unused), MediaUsage::Status));
    }

    #[test]
    fn an_empty_library_says_what_that_means() {
        assert_eq!(EMPTY, "You have not uploaded a picture yet.");
    }
}
