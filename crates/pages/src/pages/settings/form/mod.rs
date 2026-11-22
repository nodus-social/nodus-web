//! The form rows the settings cards are built from.
//!
//! A row owns what it draws and nothing else: the value arrives as a `Signal` and every
//! edit leaves as a `Callback`, so the caller keeps the draft it was already keeping and
//! this crate stays free of storage and transport. Where a callback is absent the control
//! renders disabled rather than pretending to be editable — the server paints the same
//! tree without verbs.
//!
//! The bio counter is here, not in the caller, because it is a display rule: the limit
//! belongs to the instance, and a paste longer than it should be visibly over budget
//! instead of silently truncated.

use leptos::prelude::*;
use std::sync::Arc;

/// Which of the two pictures a picker fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileImage {
    /// The small picture beside the name.
    Avatar,
    /// The wide picture behind it.
    Header,
}

impl ProfileImage {
    /// The label on the button that replaces it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Avatar => "Change picture",
            Self::Header => "Change banner",
        }
    }

    /// What shape survives the crop, so the choice is made before the upload, not after.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Avatar => "Square works best: it is cropped to a circle.",
            Self::Header => "Wide works best: it is cropped to a shallow strip.",
        }
    }

    /// The field name the instance expects, for the media upload's alt text.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Avatar => "avatar",
            Self::Header => "header",
        }
    }
}

/// A file picker the client renders, because only the browser can read a chosen file.
///
/// Absent when nothing can upload: the server paints the same card with the pictures and no
/// buttons, rather than a control that looks live and does nothing.
pub type ProfileImagePicker = Arc<dyn Fn(ProfileImage) -> AnyView + Send + Sync>;

/// The two pictures a profile carries, the profile picture and the banner.
#[component]
pub fn ProfileImages(
    /// The picture as the instance currently serves it; empty when there is none.
    #[prop(into)]
    avatar: Signal<String>,
    /// The banner as the instance currently serves it; empty when there is none.
    #[prop(into)]
    header: Signal<String>,
    /// Whether a picture is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// Upload progress, when one is running.
    #[prop(into)]
    progress: Signal<Option<u8>>,
    /// The last upload's answer, when there was one.
    #[prop(into)]
    message: Signal<Option<String>>,
    /// Draws the picker for one picture.
    #[prop(optional_no_strip)]
    picker: Option<ProfileImagePicker>,
) -> impl IntoView {
    let buttons = move || {
        [ProfileImage::Avatar, ProfileImage::Header]
            .into_iter()
            .map(|kind| match picker.as_ref() {
                Some(picker) => picker(kind).into_any(),
                None => view! {
                    <button class="btn btn-outline btn-xs" disabled=true>
                        {kind.label()}
                    </button>
                }
                .into_any(),
            })
            .collect_view()
    };

    let avatar_image = move || {
        view! {
            <Show
                when=move || !avatar.get().is_empty()
                fallback=|| view! { <div class="h-16 w-16 rounded-full border-4 border-base-100 bg-base-300"></div> }
            >
                <img
                    class="h-16 w-16 rounded-full border-4 border-base-100 bg-base-200 object-cover"
                    alt="Profile picture"
                    src=move || nodus_ui::picture::avatar(&avatar.get())
                />
            </Show>
        }
    };

    view! {
        <div class="flex flex-col gap-3">
            // No banner is drawn as no banner: an empty strip of colour reads as a picture
            // that failed to load, which is a different message from "there is none".
            <Show when=move || !header.get().is_empty()>
                <div class="relative">
                    <img
                        class="h-32 w-full rounded-box border border-base-300 bg-base-200 object-cover"
                        alt="Banner"
                        src=move || nodus_ui::picture::banner(&header.get())
                    />
                    <div class="absolute -bottom-8 left-4">{avatar_image()}</div>
                </div>
            </Show>
            <Show when=move || header.get().is_empty()>
                <div class="flex">{avatar_image()}</div>
            </Show>
            <div class="flex flex-wrap gap-2">{buttons}</div>
            <Show when=move || progress.get().is_some()>
                <progress
                    class="progress progress-primary h-1.5 w-full"
                    value=move || f64::from(progress.get().unwrap_or(0))
                    max="100"
                ></progress>
            </Show>
            <p class="text-xs opacity-60">
                {move || if busy.get() {
                    "Uploading…"
                } else {
                    "Pictures are applied as soon as they upload."
                }}
            </p>
            <span class="text-sm">{move || message.get().unwrap_or_default()}</span>
        </div>
    }
}

mod fields;
mod filter;
mod metadata;
mod profile;

pub use fields::{
    FieldError, MAX_FIELDS, MAX_NAME_CHARS, ProfileField, is_link, parse_field_error,
    validate_fields,
};
pub use filter::{FeaturedTagField, FilterFields};
pub use metadata::MetadataFields;
pub use profile::ProfileFields;
