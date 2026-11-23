//! The page frame: the rail, the main column, and whatever the route paints inside it.

use leptos::prelude::*;

use super::rail::{ExtraDoors, NoticeDoor, Rail, RightRail, SessionBanner};
use crate::session_style::{BannerRegionKind, PictureTarget, SessionStyle, StyleEditor};

#[component]
#[expect(
    clippy::too_many_lines,
    reason = "the shell is one tree: splitting it would move props through wrappers that add nothing"
)]
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
    /// The reader's own card for the top of the rail. Absent when no account is known,
    /// which is what a server render without a session has.
    #[prop(optional)]
    profile: Option<Signal<Option<super::rail::ProfileCardData>>>,
    /// What the rails' cards may draw from. Absent means every card is empty, which is
    /// what a server render that knew nothing has to offer.
    #[prop(optional, into)]
    sidebar: Signal<crate::components::sidebar::SidebarData>,
    /// Present only while the reader edits the site: the shell then draws controls on the
    /// modules it is painting, so the thing being edited is the real page, not a copy of it.
    #[prop(optional_no_strip)]
    editor: Option<StyleEditor>,
    /// Publish the draft. Absent on the server, which paints the toolbar disabled.
    #[prop(optional_no_strip)]
    on_save: Option<Callback<()>>,
    /// Throw the draft away and leave the editor.
    #[prop(optional_no_strip)]
    on_cancel: Option<Callback<()>>,
    /// Ask for a picture for a banner region. The browser opens a file dialog and adds what
    /// comes back to the draft; the server has no dialog, so the control is painted disabled.
    #[prop(optional_no_strip)]
    on_pick: Option<Callback<PictureTarget>>,
    /// One line of feedback from the editor (an upload that failed, for instance).
    #[prop(optional_no_strip)]
    notice: Option<Signal<Option<String>>>,
    /// Whether this instance serves notifications, and how many are unread. Absent keeps
    /// the rail's own entry, which is what the server paints.
    #[prop(optional_no_strip)]
    notices: Option<Signal<NoticeDoor>>,
) -> impl IntoView {
    let inner = StoredValue::new(inner);
    let active_style = Signal::derive(move || style.map(|s| s.get()).unwrap_or_default());
    let top_region = Signal::derive(move || active_style.get().top);
    let left_region = Signal::derive(move || active_style.get().left);
    let right_region = Signal::derive(move || active_style.get().right);
    let profile = profile.unwrap_or_else(|| Signal::derive(|| None));
    // One layout in both modes. Editing must not move the page under the reader: the same
    // wrapper, the same paddings and gaps, so a column sits exactly where it sat a moment
    // before the editor opened. Only the empty side columns change width, and they do that
    // in the banner itself.
    // One layout in both modes: the columns sit where they sit whether or not the editor is
    // open, because the editor edits this page rather than a copy of it. The width is the
    // bounding width every page uses, so a page does not change measure when the frame
    // around it changes state.
    let page_class = "mx-auto flex w-full max-w-6xl flex-col gap-4 px-2 py-4 xl:flex-row";
    view! {
        <div
            class="relative min-h-screen bg-base-200 text-base-content"
            class:style-editing=editor.is_some()
            data-style-editing=editor.is_some().to_string()
        >
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
                <SessionBanner region=top_region kind=BannerRegionKind::Top editor=editor on_pick=on_pick/>
            </Show>
            <div class=page_class>
                // The tall pictures are the outer edges of the page: left picture, rail,
                // column, right rail, right picture. They are drawn here rather than inside
                // a rail so that they flank the reader's whole window, not one column of it.
                <Show when=move || signed_in.get()>
                    <SessionBanner region=left_region kind=BannerRegionKind::Left editor=editor on_pick=on_pick/>
                </Show>
                <Show when=move || signed_in.get()>
                    <Rail
                        active=path
                        extra=extra.clone()
                        style=active_style
                        editor=editor
                        profile=profile
                        sidebar=Some(Signal::derive(move || sidebar.get()))
                        notices=notices
                    />
                </Show>
                <main class="min-w-0 flex-1 pb-[calc(5.5rem+env(safe-area-inset-bottom))] xl:pb-0">
                    {move || inner.with_value(|inner| inner())}
                </main>
                <Show when=move || signed_in.get()>
                    <RightRail
                        active=path
                        style=active_style
                        editor=editor
                        sidebar=Some(Signal::derive(move || sidebar.get()))
                    />
                </Show>
                <Show when=move || signed_in.get()>
                    <SessionBanner region=right_region kind=BannerRegionKind::Right editor=editor on_pick=on_pick/>
                </Show>
            </div>
            {editor.map(|edit| {
                view! {
                    <div
                        class="fixed inset-x-0 bottom-0 z-50 border-t border-base-300 bg-base-100/95 px-3 py-2 shadow-lg backdrop-blur"
                        class:hidden=move || !edit.is_active()
                        role="toolbar"
                        data-style-editor=""
                    >
                        <div class="mx-auto flex w-full max-w-6xl flex-wrap items-center gap-2 text-sm">
                            <span class="font-semibold">"Editing the site"</span>
                            <span class="hidden text-xs opacity-70 sm:inline">
                                "Drag a picture or a rail entry to move it. Hide what you do not want; nothing changes for other readers until you Save."
                            </span>
                            {notice.map(|notice| {
                                view! {
                                    <Show when=move || notice.get().is_some()>
                                        <span class="badge badge-sm badge-error badge-soft" role="status">
                                            {move || notice.get().unwrap_or_default()}
                                        </span>
                                    </Show>
                                }
                            })}
                            <span class="ml-auto flex items-center gap-2">
                                <button
                                    type="button"
                                    class="btn btn-sm"
                                    disabled=on_cancel.is_none()
                                    on:click=move |_| {
                                        if let Some(cancel) = on_cancel {
                                            cancel.run(());
                                        }
                                    }
                                >
                                    "Cancel"
                                </button>
                                <button
                                    type="button"
                                    class="btn btn-sm btn-primary"
                                    disabled=on_save.is_none()
                                    on:click=move |_| {
                                        if let Some(save) = on_save {
                                            save.run(());
                                        }
                                    }
                                >
                                    "Save"
                                </button>
                            </span>
                        </div>
                    </div>
                }
            })}
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
                hidden: false,
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
        // The frame builds a keyed list for its banners, and a keyed list needs an owner:
        // `to_html` on its own does not make one, so the test lends it one the way the
        // browser does.
        let html = Owner::new().with(|| {
            view! {
                <Frame
                    signed_in=true
                    path=String::from("/home")
                    extra=None
                    style=style_sig
                    inner=Arc::new(|| view! { <p>"Body"</p> }.into_any())
                />
            }
            .to_html()
        });

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
    fn the_rail_shows_who_is_signed_in_first() {
        use crate::components::rail::ProfileCardData;

        let card = ProfileCardData {
            display_name: "Lux".to_owned(),
            handle: "@luxuride@nodus.luxuride.eu".to_owned(),
            avatar_url: "https://cdn.example/lux.png".to_owned(),
            avatar_shape: None,
            href: "/accounts/01EXAMPLE".to_owned(),
        };
        let html = Owner::new().with(|| {
            view! {
                <Frame
                    signed_in=true
                    path=String::from("/home")
                    extra=None
                    profile=Signal::stored(Some(card))
                    inner=Arc::new(|| view! { <p>"Body"</p> }.into_any())
                />
            }
            .to_html()
        });

        assert!(html.contains("data-rail-profile"));
        assert!(html.contains("Lux"));
        assert!(html.contains("@luxuride@nodus.luxuride.eu"));
        assert!(html.contains("https://cdn.example/lux.png"));
        assert!(html.contains(r#"href="/accounts/01EXAMPLE""#));
    }

    #[test]
    fn the_shell_only_offers_handles_while_editing() {
        let mut style = SessionStyle::default();
        style
            .top
            .slots
            .push(ImageSlot::new("https://cdn.example/top.png", Some("Top"), None).unwrap());

        let reading = Owner::new().with(|| {
            view! {
                <Frame
                    signed_in=true
                    path=String::from("/home")
                    extra=None
                    style=Signal::stored(style.clone())
                    editor=Some(StyleEditor::new(
                        RwSignal::new(style.clone()),
                        RwSignal::new(false),
                    ))
                    inner=Arc::new(|| view! { <p>"Body"</p> }.into_any())
                />
            }
            .to_html()
        });
        // The editor is installed, so the tools are wired, but closed: the bar is in the tree
        // and hidden, because a bar removed and re-added is a bar that once failed to come
        // back when the editor was re-entered without a reload.
        assert!(reading.contains("data-style-editor"));
        assert!(
            reading.contains(r#"class="hidden fixed inset-x-0 bottom-0"#)
                || reading.contains("hidden")
        );
        assert!(!reading.contains("data-style-controls"));
        // The bar's own words are in the hidden node; what a reader sees is the paint.
        assert!(reading.contains("Editing the site"));

        let editing = Owner::new().with(|| {
            view! {
                <Frame
                    signed_in=true
                    path=String::from("/home")
                    extra=None
                    style=Signal::stored(style.clone())
                    editor=Some(StyleEditor::new(RwSignal::new(style), RwSignal::new(true)))
                    on_save=Some(Callback::new(|()| {}))
                    on_cancel=Some(Callback::new(|()| {}))
                    inner=Arc::new(|| view! { <p>"Body"</p> }.into_any())
                />
            }
            .to_html()
        });

        assert!(editing.contains("data-style-editor"));
        assert!(editing.contains("Editing the site"));
        assert!(!editing.contains(r#"class="hidden fixed inset-x-0 bottom-0"#));
        assert!(editing.contains("data-style-controls=\"region\""));
        assert!(editing.contains(r#"data-style-module="banner-slot""#));
        assert!(editing.contains(">Save<"));
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
