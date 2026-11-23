//! The banners an account published for other readers, drawn in the profile's content column.
//!
//! A published banner belongs to the account's page, not to the reader's shell: it sits in
//! normal flow above the profile card, never fixed and never in a gutter, because a reader
//! scrolling past the account should leave its banner behind with the rest of the page. The
//! cover a sensitive slot wears is the shell's own session-slot cover — blur, overlay,
//! "show" button — but the reveal signal is this view's: it is per view, and nothing about
//! it is written back.

use leptos::prelude::*;

use crate::html::safe_url;
use crate::session_style::{
    BannerRegion, BannerRegionKind, ImageSlot, PublishedStyle, SplitDirection,
};

/// The published banners of one account, or nothing while it has published none.
///
/// Drawing nothing for `None` is deliberate: a 404, a failed read and an instance without
/// the capability all look like this, and none of them is a broken profile.
#[component]
pub fn PublishedBanners(#[prop(into)] style: Signal<Option<PublishedStyle>>) -> impl IntoView {
    view! {
        <div class="flex w-full flex-col gap-3" data-profile-banners="">
            {move || {
                style
                    .get()
                    .map(|style| {
                        view! {
                            {drawable(&style.top)
                                .then(|| view! { <PublishedRegion kind=BannerRegionKind::Top region=style.top /> })}
                            {drawable(&style.left)
                                .then(|| view! { <PublishedRegion kind=BannerRegionKind::Left region=style.left /> })}
                            {drawable(&style.right)
                                .then(|| view! { <PublishedRegion kind=BannerRegionKind::Right region=style.right /> })}
                        }
                    })
            }}
        </div>
    }
}

/// Whether a region has a picture a reader may draw.
///
/// A hidden banner, an empty one and one whose only address this client refuses are all
/// "nothing to draw": they must not leave a gap where a banner would have been.
fn drawable(region: &BannerRegion) -> bool {
    !region.is_hidden() && region.slots.iter().any(|slot| safe_url(&slot.url))
}

/// One published banner that already passed [`drawable`].
#[component]
fn PublishedRegion(kind: BannerRegionKind, region: BannerRegion) -> impl IntoView {
    let slots = region
        .slots
        .into_iter()
        .filter(|slot| safe_url(&slot.url))
        .collect::<Vec<_>>();
    let hidden = region.hidden || slots.is_empty();
    // The two side banners are read as columns even when the stored split says horizontal:
    // they occupy a reading column here, not a gutter, and a row of full-width pictures
    // would shrink each one to a thumbnail. The top banner keeps the owner's split.
    let split = match kind {
        BannerRegionKind::Top => region.split,
        BannerRegionKind::Left | BannerRegionKind::Right => SplitDirection::Vertical,
    };
    let layout = match split {
        SplitDirection::Horizontal => "flex flex-row flex-wrap gap-2",
        SplitDirection::Vertical => "flex flex-col gap-2",
    };
    view! {
        <div class=layout data-profile-banner=banner_name(kind) class:hidden=hidden>
            {slots
                .into_iter()
                .map(|slot| {
                    view! {
                        <div class="min-w-0 flex-1 overflow-hidden rounded-box border border-base-300 bg-base-100">
                            <PublishedPicture image=slot />
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// One picture, covered before it is drawn when its slot was published as sensitive.
#[component]
fn PublishedPicture(image: ImageSlot) -> impl IntoView {
    let src = image.url;
    let alt = image.alt.unwrap_or_default();
    if !image.sensitive {
        return view! {
            <img src=src alt=alt class="h-auto w-full object-cover" loading="lazy" />
        }
        .into_any();
    }
    let revealed = RwSignal::new(false);
    view! {
        <div class="relative flex w-full min-w-0" data-style-sensitive="">
            <img
                src=src
                alt=alt
                class="h-auto w-full object-cover"
                class=("blur-2xl", move || !revealed.get())
                loading="lazy"
            />
            <button
                type="button"
                class="absolute inset-0 z-10 flex items-center justify-center bg-base-100/70 text-xs font-medium"
                data-style-sensitive-reveal=""
                class:hidden=move || revealed.get()
                title="This picture is marked sensitive; it stays covered until you ask"
                on:click=move |_| revealed.set(true)
            >
                "Sensitive picture — show"
            </button>
        </div>
    }
    .into_any()
}

/// The attribute value each region carries, named after the backend's own document keys.
fn banner_name(kind: BannerRegionKind) -> &'static str {
    match kind {
        BannerRegionKind::Top => "top",
        BannerRegionKind::Left => "left",
        BannerRegionKind::Right => "right",
    }
}

#[cfg(test)]
mod tests {
    use leptos::prelude::{Owner, RenderHtml as _, Signal};

    use super::{PublishedBanners, PublishedBannersProps};
    use crate::session_style::{BannerRegion, ImageSlot, PublishedStyle};

    fn slot(url: &str, sensitive: bool) -> ImageSlot {
        let mut slot = ImageSlot::new(url, Some("A banner"), None).expect("the test URL is safe");
        slot.sensitive = sensitive;
        slot
    }

    fn painted(style: PublishedStyle) -> String {
        Owner::new().with(|| {
            PublishedBanners(PublishedBannersProps {
                style: Signal::derive(move || Some(style.clone())),
            })
            .to_html()
        })
    }

    /// Both published banners are drawn in the page, and an empty published style draws no
    /// banner at all — the shape a never-published account returns.
    #[test]
    fn published_banners_are_drawn_in_the_content_column() {
        let mut style = PublishedStyle::default();
        style
            .left
            .add_slot(slot("https://cdn.example/left.png", false));
        style
            .right
            .add_slot(slot("https://cdn.example/right.png", false));
        let html = painted(style);
        assert!(html.contains(r#"data-profile-banner="left""#), "{html}");
        assert!(html.contains(r#"data-profile-banner="right""#), "{html}");
        assert!(html.contains("https://cdn.example/left.png"), "{html}");
        assert!(html.contains("https://cdn.example/right.png"), "{html}");
        assert!(!html.contains("fixed"), "{html}");
        assert!(!html.contains("sticky"), "{html}");

        let empty = painted(PublishedStyle::default());
        assert!(!empty.contains("data-profile-banner=\""), "{empty}");
    }

    /// The cover is the session slot's: blurred, labelled, and revealed only by the button.
    /// The click itself is a browser event, exercised by `e2e/tests/profile-banners.spec.mjs`.
    #[test]
    fn a_sensitive_published_picture_starts_covered_and_offers_the_reveal() {
        let mut style = PublishedStyle::default();
        style
            .left
            .add_slot(slot("https://cdn.example/left.png", true));
        let html = painted(style);
        assert!(html.contains("data-style-sensitive"), "{html}");
        assert!(html.contains("blur-2xl"), "{html}");
        assert!(html.contains("Sensitive picture — show"), "{html}");
        assert!(html.contains("data-style-sensitive-reveal"), "{html}");
    }

    /// A region that was hidden or that holds only an unsafe address is not an empty box.
    #[test]
    fn a_hidden_or_unsafe_banner_reserves_no_box() {
        let mut style = PublishedStyle::default();
        style
            .top
            .add_slot(slot("https://cdn.example/top.png", false));
        style.top.set_hidden(true);
        let mut unsafe_region = BannerRegion::default();
        unsafe_region.slots.push(ImageSlot {
            url: "javascript:alert(1)".to_owned(),
            ..ImageSlot::new("https://cdn.example/x.png", None, None).expect("safe URL")
        });
        style.left = unsafe_region;
        let html = painted(style);
        assert!(!html.contains("data-profile-banner=\""), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
    }
}
