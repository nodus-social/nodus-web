//! The page frame: the rail, the main column, and whatever the route paints inside it.

use leptos::prelude::*;

use super::rail::{ExtraDoors, Rail, RightRail, SessionBanner};
use crate::session_style::{BannerRegionKind, SessionStyle};

#[component]
pub fn Frame(
    /// Whether the reader has a session. The rail is navigation, so it is only there for
    /// someone who can use it.
    #[prop(into)]
    signed_in: Signal<bool>,
    /// The address being shown.
    #[prop(into)]
    path: Signal<String>,
    /// The screen itself. A function, not a value, so a route change repaints the content
    /// without rebuilding the frame around it (which would drop the screen's state).
    inner: ChildrenFn,
    /// Doors only some readers have.
    extra: Option<ExtraDoors>,
    /// Goes home without a document load when the app can answer.
    #[prop(optional)]
    on_home: Option<Callback<()>>,
    /// Per-account session styling (top/left/right banners and custom navigation layout).
    #[prop(optional)]
    style: Option<Signal<SessionStyle>>,
) -> impl IntoView {
    let inner = StoredValue::new(inner);
    let active_style = Signal::derive(move || style.map(|s| s.get()).unwrap_or_default());
    let top_region = Signal::derive(move || active_style.get().top);
    view! {
        <div class="relative min-h-screen bg-base-200 text-base-content">
            <Show when=move || !signed_in.get()>
                <nav class="absolute right-4 top-4 hidden items-center gap-3 text-sm xl:flex">
                    <a class="opacity-70 hover:opacity-100" href="/">"Home"</a>
                    <a class="btn btn-sm" href="/sign-in">"Sign in"</a>
                </nav>
            </Show>
            <header class="sticky top-0 z-30 flex items-center justify-between gap-2 border-b border-base-300 bg-base-100 px-3 py-2 xl:hidden">
                <a class="font-semibold tracking-tight" href=move || if signed_in.get() { "/home" } else { "/" }
                    on:click=move |event| {
                        if signed_in.get_untracked()
                            && let Some(on_home) = on_home
                        {
                                #[cfg(target_arch = "wasm32")]
                                event.prevent_default();
                                #[cfg(not(target_arch = "wasm32"))]
                                let _ = &event;
                            on_home.run(());
                        }
                    }>"Nodus"</a>
                <Show when=move || !signed_in.get()>
                    <nav class="flex items-center gap-3 text-sm">
                        <a class="opacity-70 hover:opacity-100" href="/">"Home"</a>
                        <a class="btn btn-sm" href="/sign-in">"Sign in"</a>
                    </nav>
                </Show>
            </header>
            <Show when=move || signed_in.get()>
                <SessionBanner region=top_region kind=BannerRegionKind::Top/>
            </Show>
            <div class="mx-auto flex w-full max-w-6xl flex-col gap-4 px-2 py-4 xl:flex-row">
                <Show when=move || signed_in.get()>
                    <Rail active=path extra=extra.clone() style=active_style/>
                </Show>
                <main class="min-w-0 flex-1 pb-[calc(5.5rem+env(safe-area-inset-bottom))] xl:pb-0">
                    {move || inner.with_value(|inner| inner())}
                </main>
                <Show when=move || signed_in.get()>
                    <RightRail active=path style=active_style/>
                </Show>
            </div>
        </div>
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::session_style::{BannerRegion, ImageSlot, NavPlacement, SplitDirection};

    #[test]
    fn frame_renders_split_banners_and_nav_placements() {
        let mut style = SessionStyle {
            top: BannerRegion {
                split: SplitDirection::Horizontal,
                slots: vec![
                    ImageSlot::new("https://cdn.example/top-left.png", Some("Top Left"), None)
                        .unwrap(),
                    ImageSlot::new("https://cdn.example/top-right.png", Some("Top Right"), None)
                        .unwrap(),
                ],
            },
            ..SessionStyle::default()
        };
        style.left.slots.push(
            ImageSlot::new("https://cdn.example/left.png", Some("Left Banner"), None).unwrap(),
        );
        style.right.slots.push(
            ImageSlot::new("https://cdn.example/right.png", Some("Right Banner"), None).unwrap(),
        );
        style.nav.move_item_to("lists", NavPlacement::Right);
        style.nav.move_item_to("scheduled", NavPlacement::Submenu);
        style.nav.move_item_to("people", NavPlacement::Hidden);
        style
            .nav
            .pin_favorite("Bookmarks", "/saved/bookmarks", NavPlacement::Right)
            .unwrap();

        let style_sig = Signal::stored(style);
        let html = view! {
            <Frame
                signed_in=true
                path=String::from("/home")
                extra=None
                style=style_sig
                inner=Arc::new(|| view! { <p>"Body"</p> }.into_any())
            />
        }
        .to_html();

        assert!(html.contains(r#"data-session-banner="top""#));
        assert!(html.contains("https://cdn.example/top-left.png"));
        assert!(html.contains("https://cdn.example/top-right.png"));
        assert!(html.contains(r#"data-session-banner="left""#));
        assert!(html.contains("https://cdn.example/left.png"));
        assert!(html.contains(r#"data-session-banner="right""#));
        assert!(html.contains("https://cdn.example/right.png"));
        assert!(html.contains("data-session-right"));
        assert!(html.contains("data-session-submenu"));
        assert!(html.contains("/saved/bookmarks"));
        assert!(html.contains("Bookmarks"));
        assert!(!html.contains("/directory"));
    }

    #[test]
    fn frame_hides_session_banners_when_signed_out() {
        let mut style = SessionStyle::default();
        style
            .top
            .slots
            .push(ImageSlot::new("https://cdn.example/top.png", Some("Top"), None).unwrap());
        let html = view! {
            <Frame
                signed_in=false
                path=String::from("/")
                extra=None
                style=Signal::stored(style)
                inner=Arc::new(|| view! { <p>"Public"</p> }.into_any())
            />
        }
        .to_html();

        assert!(!html.contains("https://cdn.example/top.png"));
    }
}
