//! Profile pictures whose form the browser chooses.
//!
//! Mastodon splits a profile picture in two: `avatar` may animate, `avatar_static` must not,
//! and the same pair exists for the banner (`header` / `header_static`). A reader whose
//! system says `prefers-reduced-motion: reduce` should be handed the still; every other
//! reader the animation.
//!
//! The choice is made by the document, not by a script. [`Picture`] emits a `<picture>` with
//! a `prefers-reduced-motion` `<source>` ahead of the animated `<img>`, so the browser picks
//! one URL while it parses the markup: there is no paint-still-then-swap, no signal read in
//! Rust, and no re-render when hydration attaches. A still is never wrong; an animation the
//! reader did not ask for can be.
//!
//! A URL that is not on the instance's `/media/{id}/` path — a remote account's avatar, for
//! instance — has no still twin and is drawn as the one layer the instance reported.

use leptos::prelude::*;

/// The profile picture to draw for `url`, unchanged.
///
/// `url` is the account's `avatar`, already absolute on the instance's media origin. The
/// animated layer is what `<img>` carries; the still is picked by [`Picture`] from
/// [`avatar_still`], so a call site that paints a bare `<img>` gets the animation for every
/// reader.
#[must_use]
pub fn avatar(url: &str) -> String {
    url.to_owned()
}

/// The profile banner to draw for `url`, unchanged; the rule is the avatar's.
#[must_use]
pub fn banner(url: &str) -> String {
    url.to_owned()
}

/// The still twin of an avatar, when the URL has one.
///
/// The instance names the pair by a suffix on the same path — `/media/{id}/avatar` and
/// `/media/{id}/avatar_static` — which is the rule the backend applies when it fills the
/// `*_static` fields. A remote account's URL is not on that path and has no twin, so the
/// animation is the only layer.
#[must_use]
pub fn avatar_still(url: &str) -> Option<String> {
    still(url, "/avatar")
}

/// The still twin of a banner, when the URL has one.
#[must_use]
pub fn banner_still(url: &str) -> Option<String> {
    still(url, "/header")
}

fn still(url: &str, layer: &str) -> Option<String> {
    let base = url.strip_suffix(layer)?;
    Some(format!("{base}{layer}_static"))
}

/// The intrinsic box a picture's known shape reserves.
///
/// An avatar and a banner are both profile pictures; the only difference for a layout is
/// which shape a site reads from the account. Both helpers below answer `None` when the
/// instance published no shape, so a caller cannot mistake an unknown box for a square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PictureBox {
    /// Width in pixels, as the instance named it.
    pub width: u32,
    /// Height in pixels, as the instance named it.
    pub height: u32,
}

impl PictureBox {
    fn from_shape(shape: Option<(u32, u32)>) -> Option<Self> {
        let (width, height) = shape?;
        (width > 0 && height > 0).then_some(Self { width, height })
    }

    fn style(self) -> String {
        format!("aspect-ratio: {} / {}", self.width, self.height)
    }
}

/// The box to reserve for an avatar, from [`nodus_client_core::model::Account::avatar_shape`].
#[must_use]
pub fn avatar_box(shape: Option<(u32, u32)>) -> Option<PictureBox> {
    PictureBox::from_shape(shape)
}

/// The box to reserve for a banner, from [`nodus_client_core::model::Account::header_shape`].
#[must_use]
pub fn banner_box(shape: Option<(u32, u32)>) -> Option<PictureBox> {
    PictureBox::from_shape(shape)
}

/// One profile layer, drawn so the browser picks its source before any script runs.
///
/// The `<img>` declares the box's intrinsic size, so the page reserves it before a byte
/// arrives. When the instance published the picture's shape, that shape — not the
/// layout's guess — supplies the `width`, `height` and `aspect-ratio`. Without one the
/// picture keeps the fallback box its caller passed, exactly as before.
///
/// `priority` marks the first picture on a page — the reader's own avatar, a profile
/// header — which loads eagerly and ahead of the rest; everything else is lazy and decoded
/// off the main thread.
#[component]
pub fn Picture(
    /// The layer that may animate, as an absolute URL on the instance origin.
    src: String,
    /// The single-frame twin drawn under reduced motion, when the instance publishes one.
    still: Option<String>,
    /// The layout's width in CSS pixels, when it has a fallback box.
    #[prop(optional)]
    width: Option<u32>,
    /// The layout's height in CSS pixels, when it has a fallback box.
    #[prop(optional)]
    height: Option<u32>,
    /// The intrinsic box the instance published, when it published one.
    #[prop(optional_no_strip)]
    shape: Option<PictureBox>,
    /// The layout classes the `<img>` draws with.
    #[prop(into)]
    class: String,
    /// Whether this is the first picture on the page.
    #[prop(default = false)]
    priority: bool,
    /// The alternative text; empty for a picture whose neighbour names it.
    #[prop(default = String::new())]
    alt: String,
) -> impl IntoView {
    let (width, height, style) = match shape {
        Some(shape) => (Some(shape.width), Some(shape.height), Some(shape.style())),
        None => (width, height, None),
    };
    view! {
        <picture>
            {still
                .map(|still| {
                    view! { <source media="(prefers-reduced-motion: reduce)" srcset=still/> }
                })}
            <img
                src=src
                width=width
                height=height
                style=style
                class=class
                alt=alt
                loading=if priority { "eager" } else { "lazy" }
                fetchpriority=if priority { "high" } else { "auto" }
                decoding="async"
                referrerpolicy="no-referrer"
            />
        </picture>
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use leptos::prelude::{Owner, RenderHtml as _};
    use nodus_client_core::model::Account;

    use super::{
        Picture, PictureBox, PictureProps, avatar, avatar_box, avatar_still, banner_box,
        banner_still,
    };
    use crate::components::lists::account_row::{AccountRow, AccountRowProps};

    fn draw(url: &str, still: Option<String>, priority: bool) -> String {
        Owner::new().with(|| {
            Picture(PictureProps {
                src: url.to_owned(),
                still,
                width: Some(40),
                height: Some(40),
                shape: None,
                class: "h-10 w-10 rounded-full object-cover".to_owned(),
                priority,
                alt: String::new(),
            })
            .to_html()
        })
    }

    fn draw_with(shape: Option<PictureBox>, class: &str) -> String {
        Owner::new().with(|| {
            Picture(PictureProps {
                src: "https://social.test/media/1/avatar".to_owned(),
                still: None,
                width: None,
                height: None,
                shape,
                class: class.to_owned(),
                priority: false,
                alt: String::new(),
            })
            .to_html()
        })
    }

    fn still_draw() -> String {
        draw(
            "https://social.test/media/1/avatar",
            avatar_still("https://social.test/media/1/avatar"),
            false,
        )
    }

    #[test]
    fn the_reduced_motion_source_comes_before_the_animation() {
        let html = still_draw();
        let source = html.find("<source").expect("no source element");
        let img = html.find("<img").expect("no img element");
        assert!(source < img, "the source must come first: {html}");
        assert!(
            html.contains("media=\"(prefers-reduced-motion: reduce)\""),
            "the source is not gated on the motion preference: {html}"
        );
        assert!(
            html.contains("srcset=\"https://social.test/media/1/avatar_static\""),
            "the source does not name the still: {html}"
        );
        assert!(
            html.contains("src=\"https://social.test/media/1/avatar\""),
            "the animation is not the img's src: {html}"
        );
    }

    #[test]
    fn a_picture_without_a_still_twin_names_one_url() {
        let html = draw("https://far.example/avatars/ada.gif", None, false);
        assert!(!html.contains("<source"), "a twin was invented: {html}");
        assert_eq!(html.matches("<img").count(), 1);
    }

    #[test]
    fn every_picture_carries_its_intrinsic_size() {
        let html = still_draw();
        assert!(html.contains("width=\"40\""), "no width: {html}");
        assert!(html.contains("height=\"40\""), "no height: {html}");
    }

    #[test]
    fn a_known_shape_replaces_the_layouts_guess() {
        let html = draw_with(avatar_box(Some((400, 300))), "rounded-full object-cover");
        assert!(
            html.contains("width=\"400\""),
            "width is not the shape's: {html}"
        );
        assert!(
            html.contains("height=\"300\""),
            "height is not the shape's: {html}"
        );
        assert!(
            html.contains("style=\"aspect-ratio: 400 / 300;\""),
            "the shape's ratio is not declared: {html}"
        );
    }

    #[test]
    fn an_absent_shape_leaves_the_layout_box_untouched() {
        let html = draw("https://social.test/media/1/avatar", None, false);
        assert!(html.contains("width=\"40\""), "no fallback width: {html}");
        assert!(html.contains("height=\"40\""), "no fallback height: {html}");
        assert!(
            !html.contains("aspect-ratio"),
            "a ratio was invented: {html}"
        );
        assert!(!html.contains("style="), "a style was invented: {html}");
        assert_eq!(avatar_box(None), None);
        assert_eq!(banner_box(None), None);
    }

    #[test]
    fn a_banner_keeps_full_width_and_takes_its_height_from_the_shape() {
        let html = draw_with(banner_box(Some((1500, 500))), "w-full object-cover");
        assert!(html.contains("class=\"w-full object-cover\""), "{html}");
        assert!(
            !html.contains("h-32"),
            "the guessed height survived: {html}"
        );
        assert!(html.contains("width=\"1500\""), "no shape width: {html}");
        assert!(html.contains("height=\"500\""), "no shape height: {html}");
        assert!(
            html.contains("style=\"aspect-ratio: 1500 / 500;\""),
            "no three-to-one box: {html}"
        );
    }

    #[test]
    fn the_first_picture_is_eager_and_the_rest_are_lazy() {
        let first = draw("https://social.test/media/1/avatar", None, true);
        assert!(first.contains("loading=\"eager\""), "not eager: {first}");
        assert!(
            first.contains("fetchpriority=\"high\""),
            "not high priority: {first}"
        );
        let later = draw("https://social.test/media/2/avatar", None, false);
        assert!(later.contains("loading=\"lazy\""), "not lazy: {later}");
        assert!(
            later.contains("decoding=\"async\""),
            "not decoded async: {later}"
        );
    }

    /// The attributes that decide which file is fetched and how big its box is must be the
    /// same on the server's paint and on the client's, or hydration flips the picture and
    /// the browser fetches a second file.
    #[test]
    fn hydration_does_not_change_the_picture() {
        let server = still_draw();
        let client = still_draw();
        assert_eq!(server, client, "the two paints differ");
        for attribute in [
            "src=\"https://social.test/media/1/avatar\"",
            "srcset=\"https://social.test/media/1/avatar_static\"",
            "width=\"40\"",
            "height=\"40\"",
            "loading=\"lazy\"",
            "decoding=\"async\"",
        ] {
            assert!(
                server.contains(attribute),
                "{attribute} is missing from the paint"
            );
        }
        assert!(
            !server.contains("data-hk"),
            "the picture carries a hydration marker: {server}"
        );
    }

    #[test]
    fn only_instance_media_urls_have_a_still_twin() {
        assert_eq!(
            avatar_still("https://social.test/media/1/avatar").as_deref(),
            Some("https://social.test/media/1/avatar_static")
        );
        assert_eq!(
            banner_still("https://social.test/media/2/header").as_deref(),
            Some("https://social.test/media/2/header_static")
        );
        assert_eq!(avatar_still("https://far.example/avatars/ada.gif"), None);
        assert_eq!(
            avatar("https://social.test/media/1/avatar"),
            "https://social.test/media/1/avatar"
        );
    }

    fn account() -> Account {
        Account {
            id: "1".to_owned(),
            acct: "alice@example.org".to_owned(),
            display_name: "Alice".to_owned(),
            note: String::new(),
            avatar: "https://social.test/media/1/avatar".to_owned(),
            avatar_meta: None,
            header: String::new(),
            header_meta: None,
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

    #[test]
    fn the_account_row_draws_the_avatar_through_the_picture_element() {
        let html = Owner::new().with(|| {
            AccountRow(AccountRowProps {
                account: account(),
                note: String::new(),
                on_open: None,
                after: None,
            })
            .to_html()
        });
        assert!(html.contains("<picture>"), "no picture element: {html}");
        let source = html.find("<source").expect("no reduced-motion source");
        let img = html.find("<img").expect("no image");
        assert!(
            source < img,
            "the source must come before the image: {html}"
        );
        assert!(
            html.contains("src=\"https://social.test/media/1/avatar\""),
            "the animation is not painted: {html}"
        );
    }
}
