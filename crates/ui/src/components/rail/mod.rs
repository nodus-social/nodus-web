//! The rail: every destination, in the order a reader scans them, and the doors only some
//! readers have.
//!
//! A `Destination` is data, not markup, so both paints decide which doors to draw from a
//! session each of them knows about.

use leptos::prelude::*;
use std::sync::Arc;

use super::icons::Icon;
use crate::html::safe_url;
use crate::session_style::{
    BannerRegion, BannerRegionKind, NavItemConfig, NavLayout, NavPlacement, SessionStyle,
    SplitDirection,
};
/// The drawing for a destination, by name.
///
/// The label is the key rather than a field on `Destination`, because the label is what a
/// reader sees and what the list is ordered by; a second field would be one more thing to
/// keep in step for no gain.
#[must_use]
pub fn icon_of(label: &str) -> Icon {
    match label {
        "Home" => Icon::Home,
        "Feed" => Icon::Sparkle,
        "Local" | "People" => Icon::Users,
        "Federated" => Icon::Globe,
        "Messages" => Icon::Mail,
        "Notifications" => Icon::Bell,
        "Search" => Icon::Search,
        "Saved" => Icon::Bookmark,
        "Scheduled" => Icon::Clock,
        "Lists" => Icon::List,
        "Compose" => Icon::Pen,
        "Moderation" | "Administration" => Icon::Shield,
        _ => Icon::Settings,
    }
}

/// Whether `path` is this destination or one of its sections.
///
/// `/settings/profile` belongs to Settings, and `/saved/bookmarks` belongs to Saved; the
/// rail marks the section, not only the one address that happens to be its default.
#[must_use]
pub fn within(href: &str, path: &str) -> bool {
    if href == "/" {
        return path == "/";
    }
    if path == href {
        return true;
    }
    for section in ["/settings", "/saved", "/admin", "/moderation"] {
        if let Some(index) = href.find(&format!("{section}/")) {
            let root = &href[..index + section.len()];
            if path == root || path.starts_with(&format!("{root}/")) {
                return true;
            }
        }
    }
    path.starts_with(&format!("{href}/"))
}

/// Doors only some readers have, as a function so the same value can be drawn by both
/// paints: the server closes over server markup, the browser closes over live components.
pub type ExtraDoors = Arc<dyn Fn() -> AnyView + Send + Sync>;

/// One destination in the rail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    /// What the reader sees.
    pub label: &'static str,
    /// Where it goes.
    pub href: &'static str,
    /// One sentence, shown as the hover title.
    pub hint: &'static str,
}

/// A named group of destinations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The section heading, hidden on narrow screens.
    pub title: &'static str,
    /// Its destinations, in order.
    pub destinations: &'static [Destination],
}

/// The rail the client shows, in its order.
#[must_use]
pub fn rail() -> Vec<Group> {
    vec![
        Group {
            title: "Read",
            destinations: &[
                Destination {
                    label: "Home",
                    href: "/home",
                    hint: "Your timeline, the feed, and the instance's",
                },
                Destination {
                    label: "Messages",
                    href: "/messages",
                    hint: "Direct conversations",
                },
                Destination {
                    label: "Notifications",
                    href: "/notifications",
                    hint: "Mentions, boosts, favourites and follow requests",
                },
            ],
        },
        Group {
            title: "Find",
            destinations: &[
                Destination {
                    label: "Search",
                    href: "/search",
                    hint: "Accounts, posts and hashtags, including remote ones",
                },
                Destination {
                    label: "People",
                    href: "/directory",
                    hint: "Accounts this instance suggests",
                },
                Destination {
                    label: "Saved",
                    href: "/saved/favourites",
                    hint: "Your favourites and bookmarks",
                },
            ],
        },
        Group {
            title: "Library",
            destinations: &[
                Destination {
                    label: "Scheduled",
                    href: "/scheduled",
                    hint: "Posts waiting to be published",
                },
                Destination {
                    label: "Lists",
                    href: "/lists",
                    hint: "Hand-picked accounts with a timeline each",
                },
                Destination {
                    label: "Settings",
                    href: "/settings/account",
                    hint: "Profile, tags, filters, sync and this instance",
                },
            ],
        },
    ]
}

/// Render the slots of one `BannerRegion` (single image or side-by-side / stacked splits).
#[component]
pub fn SessionBanner(
    #[prop(into)] region: Signal<BannerRegion>,
    kind: BannerRegionKind,
) -> impl IntoView {
    let data_kind = match kind {
        BannerRegionKind::Top => "top",
        BannerRegionKind::Left => "left",
        BannerRegionKind::Right => "right",
    };
    let outer_class = match kind {
        BannerRegionKind::Top => "mx-auto w-full max-w-6xl px-2 pt-3",
        BannerRegionKind::Left | BannerRegionKind::Right => "w-full",
    };
    let img_class = match kind {
        BannerRegionKind::Top => "h-28 w-full min-w-0 flex-1 rounded-box object-cover sm:h-36",
        BannerRegionKind::Left | BannerRegionKind::Right => {
            "h-36 w-full min-w-0 flex-1 rounded-box object-cover"
        }
    };

    view! {
        {move || {
            let current = region.get();
            let valid_slots: Vec<_> = current
                .slots
                .into_iter()
                .filter(|slot| safe_url(&slot.url))
                .collect();
            if valid_slots.is_empty() {
                return ().into_any();
            }
            let layout_class = match current.split {
                SplitDirection::Horizontal => "flex flex-col gap-2 sm:flex-row",
                SplitDirection::Vertical => "flex flex-col gap-2",
            };
            view! {
                <div class=outer_class data-session-banner=data_kind>
                    <div class=format!(
                        "{layout_class} overflow-hidden rounded-box bg-base-100 p-1 shadow-sm"
                    )>
                        {valid_slots
                            .into_iter()
                            .map(|slot| {
                                view! {
                                    <img
                                        src=slot.url
                                        alt=slot.alt.unwrap_or_default()
                                        class=img_class
                                        loading="lazy"
                                    />
                                }
                            })
                            .collect_view()}
                    </div>
                </div>
            }
                .into_any()
        }}
    }
}

fn render_nav_items(items: Vec<NavItemConfig>, active: Signal<String>) -> impl IntoView {
    items
        .into_iter()
        .map(|item| {
            let icon = item.icon();
            let href = item.path;
            let pinned = item.pinned;
            let href_for_active = href.clone();
            view! {
                <li>
                    <a
                        class="flex w-full items-center gap-2 font-medium"
                        class:menu-active=move || within(&href_for_active, &active.get())
                        href=href
                    >
                        <Icon icon=icon/>
                        <span class="min-w-0 flex-1 truncate">{item.label}</span>
                        {pinned.then(|| view! { <span class="badge badge-xs badge-primary">"Pin"</span> })}
                    </a>
                </li>
            }
        })
        .collect_view()
}

/// The rail.
///
/// The icon box is drawn at the icon's size rather than as an SVG: `view` deliberately has
/// no icon set, and the client swaps in the real paths when it takes over — the box is what
/// keeps the column from moving.
#[component]
pub fn Rail(
    /// The address being shown, so the matching destination is marked.
    #[prop(into)]
    active: Signal<String>,
    /// Doors that only some readers have: the moderation and administration ones.
    extra: Option<ExtraDoors>,
    /// Per-account session style controlling left-rail images and navigation layout.
    #[prop(optional)]
    style: Option<Signal<SessionStyle>>,
) -> impl IntoView {
    let left_region = Signal::derive(move || style.map(|s| s.get().left).unwrap_or_default());
    let default_nav = NavLayout::default();

    view! {
        <aside class="hidden shrink-0 xl:flex xl:w-44 xl:flex-col xl:gap-3">
            <nav aria-label="Primary navigation">
                {move || {
                    let nav = style.map(|s| s.get().nav).unwrap_or_default();
                    let extra_doors = extra.as_ref().map(|doors| doors());
                    if nav == default_nav {
                        view! {
                            <ul class="menu w-full flex-row flex-wrap gap-1 rounded-box bg-base-100 p-2 shadow-sm xl:flex-col xl:flex-nowrap">
                                {rail()
                                    .into_iter()
                                    .map(|group| {
                                        view! {
                                            <li class="menu-title hidden xl:block">{group.title}</li>
                                            {group
                                                .destinations
                                                .iter()
                                                .map(|destination| {
                                                    let href = destination.href;
                                                    view! {
                                                        <li>
                                                            <a
                                                                class="flex w-full items-center gap-2 font-medium"
                                                                class:menu-active=move || within(href, &active.get())
                                                                href=href
                                                                title=destination.hint
                                                            >
                                                                <Icon icon=icon_of(destination.label)/>
                                                                <span class="truncate">{destination.label}</span>
                                                            </a>
                                                        </li>
                                                    }
                                                })
                                                .collect_view()}
                                        }
                                    })
                                    .collect_view()}
                                {extra_doors}
                            </ul>
                        }
                            .into_any()
                    } else {
                        let left_items: Vec<NavItemConfig> =
                            nav.items_in(NavPlacement::Left).into_iter().cloned().collect();
                        let submenu_items: Vec<NavItemConfig> =
                            nav.items_in(NavPlacement::Submenu).into_iter().cloned().collect();
                        let submenu_title = nav.submenu_title().to_owned();
                        view! {
                            <div class="flex flex-col gap-2">
                                <ul class="menu w-full flex-col gap-1 rounded-box bg-base-100 p-2 shadow-sm">
                                    {render_nav_items(left_items, active)}
                                    {extra_doors}
                                </ul>
                                {(!submenu_items.is_empty()).then(|| {
                                    view! {
                                        <details class="rounded-box bg-base-100 p-2 shadow-sm" data-session-submenu="">
                                            <summary class="cursor-pointer select-none px-2 py-1 text-xs font-semibold uppercase tracking-wide opacity-70">
                                                {submenu_title}
                                            </summary>
                                            <ul class="menu mt-1 w-full flex-col gap-1 p-0">
                                                {render_nav_items(submenu_items, active)}
                                            </ul>
                                        </details>
                                    }
                                })}
                            </div>
                        }
                            .into_any()
                    }
                }}
            </nav>
            <SessionBanner region=left_region kind=BannerRegionKind::Left/>
        </aside>
    }
}

/// Optional right-hand session rail rendering items moved to `Right` and right session images.
#[component]
pub fn RightRail(
    #[prop(into)] active: Signal<String>,
    #[prop(into)] style: Signal<SessionStyle>,
) -> impl IntoView {
    let right_region = Signal::derive(move || style.get().right);
    view! {
        {move || {
            let current = style.get();
            let right_items: Vec<NavItemConfig> = current
                .nav
                .items_in(NavPlacement::Right)
                .into_iter()
                .cloned()
                .collect();
            let has_nav = !right_items.is_empty();
            let has_banner = current.right.slots.iter().any(|slot| safe_url(&slot.url));
            if !has_nav && !has_banner {
                return ().into_any();
            }
            view! {
                <aside class="hidden shrink-0 xl:flex xl:w-52 xl:flex-col xl:gap-3" data-session-right="">
                    {has_nav.then(|| {
                        view! {
                            <nav aria-label="Right rail navigation">
                                <ul class="menu w-full flex-col gap-1 rounded-box bg-base-100 p-2 shadow-sm">
                                    {render_nav_items(right_items, active)}
                                </ul>
                            </nav>
                        }
                    })}
                    <SessionBanner region=right_region kind=BannerRegionKind::Right/>
                </aside>
            }
                .into_any()
        }}
    }
}

/// One rail entry whose destination is the caller's route, not one of `rail()`'s.
///
/// The address is a real `href`, so a modified click still opens a new tab; only a plain
/// primary click is taken over, and only when the caller has somewhere to send it.
#[component]
pub fn RailEntry(
    #[prop(into)] label: String,
    #[prop(into)] hint: String,
    #[prop(into)] href: String,
    icon: Icon,
    #[prop(into)] active: Signal<bool>,
    #[prop(optional_no_strip)] on_click: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <li>
            <a
                class="flex w-full items-center gap-2 font-medium"
                class:menu-active=move || active.get()
                href=href
                title=hint
                on:click=move |event| {
                    if event.button() != 0
                        || event.meta_key()
                        || event.ctrl_key()
                        || event.shift_key()
                        || event.alt_key()
                    {
                        return;
                    }
                    event.prevent_default();
                    if let Some(on_click) = on_click {
                        on_click.run(());
                    }
                }
            >
                <Icon icon=icon/>
                <span class="truncate">{label}</span>
            </a>
        </li>
    }
}
