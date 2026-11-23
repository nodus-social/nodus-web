//! The rail: every destination, in the order a reader scans them, and the doors only some
//! readers have.
//!
//! A `Destination` is data, not markup, so both paints decide which doors to draw from a
//! session each of them knows about.

use leptos::prelude::*;
use std::sync::Arc;

use super::icons::Icon;
use crate::components::sidebar::{SidebarData, SidebarModule};
use crate::html::safe_url;
use crate::session_style::{
    BannerRegion, BannerRegionKind, ImageSlot, ModuleKind, ModuleRail, NavItemConfig, NavPlacement,
    PictureTarget, SessionStyle, SplitDirection, StyleEditor, is_required_nav_item,
};

mod session_slot;
use session_slot::{banner_slot, slot_image};
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

/// Doors only some readers have, plus whether any of them survived the gate.
///
/// The rows and the answer travel together because the rail paints a heading and a card
/// around them. A caller handed only the rows could not tell "no doors" from "not built
/// yet", and would draw the bare heading this type exists to prevent: the answer is
/// computed by whoever applied the door gates, and drawn by whoever draws the card.
pub struct ExtraDoors {
    /// Whether at least one door passed the extension, capability and role checks.
    pub present: Signal<bool>,
    /// The doors themselves, as the paint's own markup.
    pub rows: Arc<dyn Fn() -> AnyView + Send + Sync>,
}

impl Clone for ExtraDoors {
    fn clone(&self) -> Self {
        Self {
            present: self.present,
            rows: Arc::clone(&self.rows),
        }
    }
}

impl std::fmt::Debug for ExtraDoors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtraDoors").finish_non_exhaustive()
    }
}

/// The signed-in reader, as the rail card stands for them.
///
/// Plain data rather than an SDK type: the view crate draws what it is handed, and the
/// server, which has no SDK, hands it whatever the page already knew.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileCardData {
    /// What the account calls itself.
    pub display_name: String,
    /// `@user@host`, ready to print.
    pub handle: String,
    /// Avatar URL; empty draws a placeholder.
    pub avatar_url: String,
    /// The avatar's published shape, when the reader's account carried one.
    pub avatar_shape: Option<(u32, u32)>,
    /// Where the card goes: the reader's own profile.
    pub href: String,
}

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

/// Whether this account's instance serves notifications, and how many are unread.
///
/// The probe answers about one read, not about the layout: a 404 is equally what a proxy
/// returns for a route it has not been taught and what an instance returns when it has no
/// such endpoint, so it cannot be evidence that a destination does not exist. Only a count
/// turns the entry into a bell; every other answer leaves the layout's ordinary door in
/// place, and the capability answer is the only thing allowed to remove it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NoticeDoor {
    /// Not asked yet, or no session: draw the ordinary entry.
    #[default]
    Unknown,
    /// The probe did not answer (including a 404): we do not know yet, so draw the entry.
    Absent,
    /// The instance serves notifications; the number is what the reader has not read.
    Unread(usize),
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

/// One rail's cards, in the reader's order.
///
/// Both rails draw the same thing: a column of cards whose contents arrive from `sidebar` and
/// whose order and membership live in the session style. Which column a card is in is decided
/// by the style, not here — this component is handed the cards one rail holds.
#[component]
fn RailCards(
    #[prop(into)] kinds: Signal<Vec<ModuleKind>>,
    #[prop(optional_no_strip)] sidebar: Option<Signal<SidebarData>>,
    #[prop(optional_no_strip)] editor: Option<StyleEditor>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-3" data-rail-modules="">
            <For
                each=move || kinds.get()
                key=|kind| *kind
                children={
                    move |kind| {
                        let rows = Signal::derive(move || {
                            sidebar
                                .map(|data| data.get().items(kind).to_vec())
                                .unwrap_or_default()
                        });
                        view! {
                            <SidebarModule
                                kind=kind
                                items=rows
                                editor=editor
                            />
                        }
                    }
                }
            />
        </div>
    }
}

/// Render the slots of one `BannerRegion` (single image or side-by-side / stacked splits).
///
/// With an `editor`, the region also draws its own controls: one button keeps the pictures
/// but stops drawing them, another flips the arrangement, and each picture can be dragged to
/// another position or dropped. Those controls are only ever built for the editor, so the page
/// a reader sees has no way to change itself.
///
/// The tree is built once and kept: every change arrives through a `Memo` (and `For` for the
/// pictures), because rebuilding the subtree on each edit disposes the owners behind the
/// handlers still attached to the old nodes.
#[component]
#[expect(
    clippy::too_many_lines,
    reason = "one banner: the picture, its editing chrome and its empty target are one node tree"
)]
pub fn SessionBanner(
    #[prop(into)] region: Signal<BannerRegion>,
    kind: BannerRegionKind,
    #[prop(optional_no_strip)] editor: Option<StyleEditor>,
    /// Ask the browser for a picture. Absent on the server, where there is no file dialog —
    /// the add target is painted disabled rather than removed, so both renders agree about
    /// what the page offers.
    #[prop(optional_no_strip)]
    on_pick: Option<Callback<PictureTarget>>,
) -> impl IntoView {
    let data_kind = match kind {
        BannerRegionKind::Top => "top",
        BannerRegionKind::Left => "left",
        BannerRegionKind::Right => "right",
    };
    let img_class = match kind {
        BannerRegionKind::Top => "h-28 w-full min-w-0 flex-1 rounded-box object-cover sm:h-36",
        // A side picture is the column: it fills the full height and lets `object-cover`
        // crop it, instead of being a card floating on a strip.
        BannerRegionKind::Left | BannerRegionKind::Right => "h-full w-full min-w-0 object-cover",
    };
    let open: RwSignal<Option<usize>> = RwSignal::new(None);

    let slots = Memo::new(move |_| {
        region
            .get()
            .slots
            .into_iter()
            .filter(|slot| safe_url(&slot.url))
            .collect::<Vec<ImageSlot>>()
    });
    let hidden = Memo::new(move |_| region.get().is_hidden());
    let editing = Memo::new(move |_| editor.is_some_and(StyleEditor::is_active));
    let layout = Memo::new(move |_| {
        if hidden.get() {
            // A hidden banner is a bare strip: no split to describe.
            String::from("flex flex-col gap-2")
        } else if matches!(kind, BannerRegionKind::Left | BannerRegionKind::Right) {
            // The column is the split: pictures stack down the banner, so one picture
            // fills the height and several divide it between them.
            String::from("flex h-full flex-col")
        } else {
            match region.get().split {
                SplitDirection::Horizontal => String::from("flex flex-col gap-2 sm:flex-row"),
                SplitDirection::Vertical => String::from("flex flex-col gap-2"),
            }
        }
    });

    let rows = Memo::new(move |_| {
        slots
            .get()
            .into_iter()
            .enumerate()
            .collect::<Vec<(usize, ImageSlot)>>()
    });
    let strip = Signal::derive(move || match kind {
        BannerRegionKind::Top => {
            format!(
                "{} overflow-hidden rounded-box bg-base-100 p-1 shadow-sm",
                layout.get()
            )
        }
        // The side banner is an edge of the page, not a card on one: no padding, no rounded
        // corners, and it fills the column so the pictures reach both edges.
        BannerRegionKind::Left | BannerRegionKind::Right => {
            format!("pointer-events-auto h-full w-full {}", layout.get())
        }
    });

    let outer_class = Signal::derive(move || match kind {
        BannerRegionKind::Top => {
            if slots.get().is_empty() {
                if editor.is_some_and(StyleEditor::is_active) {
                    String::from("mx-auto w-full max-w-6xl px-2 pt-3")
                } else {
                    String::from("hidden")
                }
            } else {
                String::from("mx-auto w-full max-w-6xl px-2 pt-3")
            }
        }
        // The side pictures are columns of the page, outside both rails: they are what makes
        // the site feel framed rather than padded. They are grid tracks in the frame beside
        // the 72rem reading column, so the gutter between the column and the window edge is
        // the picture's width — not a fixed strip with empty space beyond it.
        //
        // The track is fluid, but the picture stops at 43rem: about 60% of the 72rem centre
        // (688 / 1152 = 59.7%). Past that a side picture starts competing with the reading
        // column for attention, and `object-cover` crops a portrait into a wider and wider
        // band. The cap first bites just above 2560px, where the raw gutter is 704px a side;
        // the banner is then drawn 688px wide, still at the window edge.
        BannerRegionKind::Left | BannerRegionKind::Right => {
            let placement = if matches!(kind, BannerRegionKind::Left) {
                "xl:col-start-1"
            } else {
                // The right column is capped, so it is pinned to its own outer edge rather
                // than growing inward toward the reading column.
                "ml-auto xl:col-start-3"
            };
            // No `pointer-events-none`: the banner is its own grid track, so it cannot
            // overlap the page and swallow a click meant for it. The controls inside still
            // take their own clicks.
            format!("hidden h-full w-full min-w-0 max-w-[43rem] {placement} xl:flex xl:flex-col")
        }
    });
    let side_empty = Memo::new(move |_| slots.get().is_empty());

    // Everything in this node tree is painted once and then shown or hidden with a class.
    // Swapping subtrees with `Show` disposes the owners behind the handlers that are still
    // attached to the old nodes, and a queued reactive read then panics with "Tried to
    // access a reactive value that has already been disposed" — so the empty target, the
    // controls and the strip all exist and only their visibility changes.
    view! {
        <div class=move || outer_class.get() data-session-banner=data_kind>
            <div class=move || if hidden.get() { "hidden" } else { "contents" }>
                // The side column has a banner's width now, so its verbs fit at every width
                // the banner exists at; they still wait for a picture to manage, and the
                // empty column offers the one thing it is for, adding one.
                {editor
                    .map(move |edit| {
                        view! {
                            <Show when=move || {
                                edit.is_active()
                                    && (!side_empty.get()
                                        || matches!(kind, BannerRegionKind::Top))
                            }>                                {region_controls(kind, hidden, region, edit)}
                            </Show>
                        }
                    })}
            </div>
            <div class=move || {
                if side_empty.get() {
                    "hidden"
                } else if matches!(kind, BannerRegionKind::Left | BannerRegionKind::Right) {
                    // The column is the split: the strip fills it, and the pictures divide
                    // whatever height the column has.
                    "flex h-full w-full flex-col"
                } else {
                    "flex w-full flex-col gap-2"
                }
            }>
                <div class=strip>
                    <For
                        each=move || rows.get()
                        key=|(index, slot)| (*index, slot.url.clone())
                        children=move |(index, slot)| {
                            banner_slot(
                                slot_image(slot, img_class),
                                kind,
                                index,
                                editor,
                                region,
                                open,
                                on_pick,
                            )
                        }
                    />
                </div>
            </div>
            <div
                class=move || {
                    if side_empty.get() && editing.get() {
                        "flex w-full flex-col"
                    } else {
                        "hidden"
                    }
                }
                on:dragover=move |event| event.prevent_default()
                on:drop=move |event| {
                    event.prevent_default();
                    if let Some(edit) = editor {
                        edit.drop_picture(kind, 0);
                    }
                }
            >
                <button
                    type="button"
                    class="pointer-events-auto flex h-full min-h-40 w-full flex-1 flex-col items-center justify-center gap-2 rounded-box border-2 border-dashed border-base-content/40 bg-base-100/70 px-0 py-4 text-center text-base-content hover:border-primary hover:bg-primary/10 hover:text-primary focus:outline-none focus-visible:ring-2 focus-visible:ring-primary disabled:cursor-not-allowed disabled:opacity-60"
                    data-style-add-picture=kind.metadata_key()
                    title="Upload a picture for this banner"
                    disabled=move || on_pick.is_none()
                    on:click=move |_| {
                        if let Some(pick) = on_pick {
                            pick.run(PictureTarget::Region(kind));
                        }
                    }
                >
                    <span class="text-xs font-semibold uppercase tracking-wide">
                        "Drag a picture here"
                    </span>
                    <span class="text-[10px] underline opacity-70">"or upload one"</span>
                </button>
            </div>
        </div>
    }
}

/// The toolbar a banner grows while the editor is open.
///
/// A top banner has a page's width for a row of verbs; a side banner has a column's, so it
/// stacks them and keeps only the two words a reader needs to choose between.
fn region_controls(
    kind: BannerRegionKind,
    hidden: Memo<bool>,
    region: Signal<BannerRegion>,
    edit: StyleEditor,
) -> AnyView {
    let stacked = !matches!(kind, BannerRegionKind::Top);
    let verb = move || {
        if stacked {
            "btn btn-ghost btn-xs w-full"
        } else {
            "btn btn-ghost btn-xs"
        }
    };
    view! {
        <div
            class=move || {
                let base = "pointer-events-auto mb-1 rounded-box bg-base-100 shadow-sm";
                if stacked {
                    // A banner is a column: the verbs stack under the picture they manage
                    // instead of sitting beside a label that no longer fits.
                    format!("{base} flex w-full flex-col gap-1 px-1 py-1 text-[10px]")
                } else {
                    format!("{base} flex items-center gap-2 px-2 py-1 text-xs")
                }
            }
            data-style-controls="region"
        >
            <span class=move || if stacked { "sr-only" } else { "font-semibold" }>
                {kind.label()}
            </span>
            <span class="badge badge-xs badge-ghost" class:hidden=move || !hidden.get()>
                "hidden"
            </span>
            <span class=move || {
                if stacked {
                    "flex w-full flex-col gap-1"
                } else {
                    "ml-auto flex items-center gap-1"
                }
            }>
                <button
                    type="button"
                    class=verb
                    title="Show or hide this banner without losing its pictures"
                    on:click=move |_| edit.set_region_hidden(kind, !hidden.get_untracked())
                >
                    {move || if hidden.get() { "Show" } else { "Hide" }}
                </button>
                <button
                    type="button"
                    class=verb
                    title="Switch between side-by-side and stacked"
                    on:click=move |_| edit.toggle_split(kind)
                >
                    {move || region.get().split.label()}
                </button>
            </span>
        </div>
    }
    .into_any()
}

#[expect(
    clippy::too_many_lines,
    reason = "one row: its link, its drag handle and the two verbs that ride with it"
)]
fn nav_item_row(
    item: NavItemConfig,
    active: Signal<String>,
    editor: Option<StyleEditor>,
) -> AnyView {
    let icon = item.icon();
    let pinned = item.pinned;
    let rail = StoredValue::new(item.rail);
    let id = Signal::derive({
        let id = item.id;
        move || id.clone()
    });
    // `Copy` signals, never owned strings: `Show` and `For` call their closures more than
    // once, and a closure that moves a `String` out is only good once.
    let label = Signal::derive({
        let text = item.label;
        move || text.clone()
    });
    let href = Signal::derive({
        let path = item.path;
        move || path.clone()
    });
    let editing = Signal::derive(move || editor.is_some_and(StyleEditor::is_active));
    let (over, set_over) = signal(false);
    view! {
        <li
            class="group relative"
            on:dragover=move |event| {
                if editor.is_some() {
                    event.prevent_default();
                    set_over.set(true);
                }
            }
            on:dragleave=move |_| set_over.set(false)
            on:drop=move |event| {
                if let Some(edit) = editor {
                    event.prevent_default();
                    set_over.set(false);
                    edit.drop_on_nav_item(&id.get_untracked());
                }
            }
        >
            <Show when=move || over.get()>
                <span
                    class="pointer-events-none absolute inset-x-0 -top-5 z-10 rounded-box bg-primary px-2 py-0.5 text-center text-xs text-primary-content shadow"
                    data-drop-says=""
                >
                    "Move here"
                </span>
            </Show>
            // While the editor is open a rail entry is a thing to move, not a door to walk
            // through: following a link mid-arrangement would throw the draft away to show a
            // page nobody asked for. The whole row is the handle, so the destination can be
            // picked up anywhere on it rather than on a glyph the size of a sesame seed.
            <Show
                when=move || !editing.get()
                fallback=move || {
                    view! {
                        <div
                            class="flex w-full cursor-grab select-none items-center gap-2 font-medium"
                            draggable="true"
                            data-nav-drag=""
                            title="Drag to move this entry"
                            on:dragstart=move |_| {
                                if let Some(edit) = editor {
                                    rail.with_value(|rail| {
                                        edit.drag_nav_item(
                                            &id.get_untracked(),
                                            (!rail.is_empty()).then_some(rail.as_str()),
                                        );
                                    });
                                }
                            }
                            on:dragend=move |_| {
                                if let Some(edit) = editor {
                                    edit.drag_end();
                                }
                            }
                        >
                            <Icon icon=icon/>
                            <span class="min-w-0 flex-1 truncate">{move || label.get()}</span>
                            {pinned
                                .then(|| {
                                    view! { <span class="badge badge-xs badge-primary">"Pin"</span> }
                                })}
                        </div>
                    }
                }
            >
                <a
                    class="flex w-full items-center gap-2 font-medium"
                    class:menu-active=move || within(&href.get(), &active.get())
                    href=move || href.get()
                >
                    <Icon icon=icon/>
                    <span class="min-w-0 flex-1 truncate">{move || label.get()}</span>
                    {pinned
                        .then(|| view! { <span class="badge badge-xs badge-primary">"Pin"</span> })}
                </a>
            </Show>
            // The verbs live outside the row: a button inside an anchor is a nested control
            // the browser cannot decide what to do with, and its width came out of the rail's
            // fixed column instead of wrapping.
            {editor
                .map(|edit| {
                    view! {
                        <Show when=move || edit.is_active()>
                            <span
                                class="absolute inset-y-0 right-0 hidden items-center gap-0.5 rounded-box bg-base-200 pl-2 group-hover:flex"
                                data-style-controls="nav-item"
                            >
                                <button
                                    type="button"
                                    class="btn btn-ghost btn-xs"
                                    title="Pin this entry to the top of its rail"
                                    on:click=move |_| {
                                        edit.toggle_nav_pin(&id.get_untracked());
                                    }
                                >
                                    "Pin"
                                </button>
                                // A required entry keeps its Hide button off the page: an
                                // offer that cannot be taken is a lie the reader has to
                                // discover by clicking it.
                                <Show when=move || !is_required_nav_item(&id.get())>
                                    <button
                                        type="button"
                                        class="btn btn-ghost btn-xs"
                                        title="Hide this entry"
                                        on:click=move |_| {
                                            edit.hide_nav_item(&id.get_untracked());
                                        }
                                    >
                                        "Hide"
                                    </button>
                                </Show>
                            </span>
                        </Show>
                    }
                })}
        </li>
    }
    .into_any()
}

/// The entries of one nav list.
///
/// A component rather than a helper that builds a list, because the list changes while the
/// reader is editing it: `For` keeps the rows it already drew and moves them, where a
/// rebuilt list would dispose the owners behind the handlers still attached to those rows.
#[component]
fn NavItems(
    #[prop(into)] items: Signal<Vec<NavItemConfig>>,
    #[prop(into)] active: Signal<String>,
    #[prop(optional_no_strip)] editor: Option<StyleEditor>,
) -> impl IntoView {
    // One choke point for every navigation list: the left rails, the right rail and the
    // submenu all draw through here, so a destination the server does not serve is dropped
    // once rather than in each list. An editor still sees it while arranging, because the
    // entry exists in the layout and hiding it there would make it unrecoverable.
    let at = crate::capabilities::serves();
    let shown = Signal::derive(move || {
        served_entries(
            items.get(),
            at.map(|at| at.get()),
            editor.is_some_and(StyleEditor::is_active),
        )
    });
    view! {
        <For
            each=move || shown.get()
            key=|item| item.id.clone()
            children=move |item| nav_item_row(item, active, editor)
        />
    }
}

/// The entries of a list that this paint may draw, in their layout order.
///
/// A card's heading and the rows beneath it ask this one function, so an empty category and
/// a category whose every door the instance refused look the same: nothing to draw. An
/// editor still sees refused entries, because the entry is in the layout and the reader is
/// arranging what is there rather than what answers today.
#[must_use]
fn served_entries(
    items: Vec<NavItemConfig>,
    at: Option<crate::capabilities::Capabilities>,
    editing: bool,
) -> Vec<NavItemConfig> {
    if editing {
        return items;
    }
    items
        .into_iter()
        .filter(|item| {
            crate::capabilities::Surface::of_path(&item.path)
                .is_none_or(|surface| crate::capabilities::admits(at.as_ref(), surface))
        })
        .collect()
}

/// Whether a section earns its heading: it has something under it, or its reader is
/// arranging the layout and needs the empty slot to fill.
///
/// Every group the rail draws asks this one question, so a group whose doors were all
/// filtered out by the capability gate loses its heading and the padding around it. A
/// heading with no doors under it reads as a section that failed to load, which is the
/// opposite of a door that was correctly not served.
fn section_is_drawn(has_rows: bool, editing: bool) -> bool {
    has_rows || editing
}

/// The staff doors as one card, so either rail can hold them.
///
/// A door is furniture like a card is: a moderator who wants the moderation queue beside
/// the reading column should not have to keep it on the left because the app said so.
///
/// The card is drawn only when a door survived the gate, for the same reason a nav card is:
/// the heading names the doors under it, and there are none to name.
fn door_card(doors: Option<ExtraDoors>, editor: Option<StyleEditor>) -> AnyView {
    let Some(doors) = doors else {
        return ().into_any();
    };
    let editing = Signal::derive(move || editor.is_some_and(StyleEditor::is_active));
    let present = doors.present;
    view! {
        <Show when=move || section_is_drawn(present.get(), editing.get())>
            <div class="rounded-box bg-base-100 p-1 shadow-sm" data-nav-rail-doors="">
                <p class="flex items-center gap-1 px-2 py-1 text-xs font-semibold uppercase tracking-wide opacity-60">
                    <Show when=move || editing.get()>
                        <span
                            class="cursor-grab select-none"
                            draggable="true"
                            data-staff-drag=""
                            title="Drag the staff doors to the other rail"
                            on:dragstart=move |_| {
                                if let Some(edit) = editor {
                                    edit.drag_staff();
                                }
                            }
                            on:dragend=move |_| {
                                if let Some(edit) = editor {
                                    edit.drag_end();
                                }
                            }
                        >
                            "\u{28FF}"
                        </span>
                    </Show>
                    "Staff"
                </p>
                <ul class="menu w-full flex-col gap-1 p-0">{(doors.rows)()}</ul>
            </div>
        </Show>
    }
    .into_any()
}

/// One category card of the rail's navigation.
///
/// Categories are drawn as separate cards rather than as headings inside one long list,
/// because a card is something a reader can move, rename and delete, and a heading is not.
#[component]
fn NavRailCard(
    rail_id: String,
    /// Which column holds this card, so dragging it can say where it is now.
    side: ModuleRail,
    #[prop(into)] label: Signal<String>,
    #[prop(into)] items: Signal<Vec<NavItemConfig>>,
    #[prop(into)] active: Signal<String>,
    #[prop(optional_no_strip)] editor: Option<StyleEditor>,
) -> impl IntoView {
    let editing = Signal::derive(move || editor.is_some_and(StyleEditor::is_active));
    // The heading asks what will actually be drawn under it, not what the layout holds: a
    // category whose every door was refused is empty and earns no heading.
    let at = crate::capabilities::serves();
    let shown = Signal::derive(move || {
        section_is_drawn(
            !served_entries(items.get(), at.map(|at| at.get()), editing.get()).is_empty(),
            editing.get(),
        )
    });
    // Signals, not strings, in every handler: they are `Copy`, so the closures below can be
    // called more than once, which is what `Show` and `For` require of them.
    let id = {
        let id = rail_id.clone();
        Signal::derive(move || id.clone())
    };
    let id_for_drop = id;
    let id_for_empty = id;
    let handle = move || editor;
    let _ = handle;
    let (over, set_over) = signal(false);

    view! {
        <Show when=move || shown.get()>
            <section class="rounded-box bg-base-100 p-1 shadow-sm" data-nav-rail=rail_id.clone()>
                <Show
                    when=move || editing.get()
                    fallback=move || {
                        view! {
                            <p class="px-2 pb-1 pt-2 text-xs font-semibold uppercase tracking-wide opacity-60">
                                {move || label.get()}
                            </p>
                        }
                    }
                >
                    <div class="flex items-center gap-1 px-1 pt-1" data-nav-rail-header="">
                        <span
                            class="cursor-grab select-none px-1 text-xs opacity-60"
                            draggable="true"
                            data-nav-rail-drag=""
                            title="Drag this category to the other rail"
                            on:dragstart=move |_| {
                                if let Some(edit) = editor {
                                    edit.drag_rail(&id.get_untracked(), side);
                                }
                            }
                            on:dragend=move |_| {
                                if let Some(edit) = editor {
                                    edit.drag_end();
                                }
                            }
                        >
                            "\u{28FF}"
                        </span>
                        <span class="sr-only">{move || format!("{} rail", side.label())}</span>
                        <input
                            type="text"
                            class="input input-ghost input-xs min-w-0 flex-1 px-1 text-xs font-semibold uppercase tracking-wide"
                            title="Rename this category"
                            value=move || label.get_untracked()
                            on:change=move |event| {
                                if let Some(edit) = editor {
                                    edit.rename_nav_rail(&id.get_untracked(), &event_target_value(&event));
                                }
                            }
                        />
                        <button
                            type="button"
                            class="btn btn-ghost btn-xs px-1"
                            title="Move this category up"
                            disabled=move || editor.is_none()
                            on:click=move |_| {
                                if let Some(edit) = editor {
                                    edit.move_nav_rail(&id.get_untracked(), false);
                                }
                            }
                        >
                            "\u{2191}"
                        </button>
                        <button
                            type="button"
                            class="btn btn-ghost btn-xs px-1"
                            title="Move this category down"
                            disabled=move || editor.is_none()
                            on:click=move |_| {
                                if let Some(edit) = editor {
                                    edit.move_nav_rail(&id.get_untracked(), true);
                                }
                            }
                        >
                            "\u{2193}"
                        </button>
                        <button
                            type="button"
                            class="btn btn-ghost btn-xs px-1"
                            title="Delete this category; its links stay in the rail"
                            disabled=move || editor.is_none()
                            on:click=move |_| {
                                if let Some(edit) = editor {
                                    edit.remove_nav_rail(&id.get_untracked());
                                }
                            }
                        >
                            "\u{2715}"
                        </button>
                    </div>
                </Show>
                <ul
                    class="menu relative w-full flex-col gap-1 p-0"
                    on:dragover=move |event| {
                        if editor.is_some() {
                            event.prevent_default();
                            set_over.set(true);
                        }
                    }
                    on:dragleave=move |_| set_over.set(false)
                    on:drop=move |event| {
                        if let Some(edit) = editor {
                            event.prevent_default();
                            set_over.set(false);
                            edit.drop_in_category(&id_for_drop.get_untracked());
                        }
                    }
                >
                    <Show when=move || over.get()>
                        <li class="pointer-events-none absolute inset-x-0 -top-1 z-10 rounded-box bg-primary px-2 py-0.5 text-center text-xs text-primary-content shadow" data-drop-says="">
                            "Move to this category"
                        </li>
                    </Show>
                    <NavItems items=items active=active editor=editor/>
                    <Show when=move || items.get().is_empty()>
                        <li class="px-2 py-1">
                            <button
                                type="button"
                                class="w-full cursor-default rounded border border-dashed border-base-content/40 px-2 py-2 text-xs leading-snug opacity-80"
                                title="Drag a link onto this category to file it here"
                                data-nav-rail-empty=move || id_for_empty.get()
                            >
                                "Empty — drag a link here"
                            </button>
                        </li>
                    </Show>
                </ul>
            </section>
        </Show>
    }
}

/// The reader's own card: who they are signed in as, one click from their profile.
///
/// It sits at the top of the rail because that is the question a reader asks first when a
/// session has more than one account in it ("which one am I?").
#[component]
pub fn ProfileCard(#[prop(into)] profile: Signal<Option<ProfileCardData>>) -> impl IntoView {
    view! {
        <Show when=move || profile.get().is_some()>
            {move || {
                profile
                    .get()
                    .map_or_else(|| ().into_any(), |card| {
                        let name = if card.display_name.trim().is_empty() {
                            card.handle.clone()
                        } else {
                            card.display_name.clone()
                        };
                        let has_avatar = !card.avatar_url.is_empty();
                        let avatar = crate::picture::avatar(&card.avatar_url);
                        let still = crate::picture::avatar_still(&card.avatar_url);
                        let shape = crate::picture::avatar_box(card.avatar_shape);
                        view! {
                            <a
                                class="flex items-center gap-2 rounded-box bg-base-100 p-2 shadow-sm transition hover:bg-base-300"
                                href=card.href
                                title="Your profile"
                                data-rail-profile=""
                            >
                                <Show
                                    when=move || has_avatar
                                    fallback=|| {
                                        view! {
                                            <span class="grid size-9 shrink-0 place-items-center rounded-full bg-base-300 text-xs font-semibold opacity-70">
                                                "@"
                                            </span>
                                        }
                                    }
                                >
                                    <crate::picture::Picture
                                        src=avatar.clone()
                                        still=still.clone()
                                        width=36
                                        height=36
                                        shape=shape
                                        class="size-9 shrink-0 rounded-full object-cover"
                                        // The card's visible name already names this link, so the
                                        // picture is decorative here.
                                        alt=String::new()
                                    />
                                </Show>
                                <span class="min-w-0">
                                    <span class="block truncate text-sm font-semibold">{name}</span>
                                    <span class="block truncate text-xs opacity-60">
                                        {card.handle}
                                    </span>
                                </span>
                            </a>
                        }
                            .into_any()
                    })
            }}
        </Show>
    }
}

/// The rail.
///
/// The icon box is drawn at the icon's size rather than as an SVG: `view` deliberately has
/// no icon set, and the client swaps in the real paths when it takes over — the box is what
/// keeps the column from moving.
#[expect(
    clippy::too_many_lines,
    reason = "one rail: the nav and its cards; splitting it would pass four props through a wrapper that adds nothing"
)]
#[component]
pub fn Rail(
    /// The address being shown, so the matching destination is marked.
    #[prop(into)]
    active: Signal<String>,
    /// The reader themselves, at the top of the rail where a reader looks first.
    #[prop(into)]
    profile: Signal<Option<ProfileCardData>>,
    /// Doors that only some readers have: the moderation and administration ones.
    extra: Option<ExtraDoors>,
    /// Per-account session style controlling left-rail images and navigation layout.
    #[prop(optional)]
    style: Option<Signal<SessionStyle>>,
    /// What the rail's cards may draw from. Absent means every card is empty, which is what
    /// a server render that knew nothing has to offer.
    #[prop(optional_no_strip)]
    sidebar: Option<Signal<SidebarData>>,
    /// Present only while the reader edits the site itself.
    #[prop(optional_no_strip)]
    editor: Option<StyleEditor>,
    /// Whether the instance serves notifications, and how many are unread. Absent keeps
    /// the layout's own entry, which is what a server paint can honestly say.
    #[prop(optional_no_strip)]
    notices: Option<Signal<NoticeDoor>>,
) -> impl IntoView {
    let door = notices.unwrap_or_else(|| Signal::derive(|| NoticeDoor::Unknown));
    let at = crate::capabilities::serves();
    // The bell is the door, so it is drawn only when the probe has a count *and* the
    // instance serves the surface: a failed probe says "we do not know yet", never "there
    // is nothing here", and the capability answer remains the only refusal. While the bell
    // is drawn the catalogue copy is suppressed, because two doors to one page read as two
    // pages.
    let bell = Signal::derive(move || {
        matches!(door.get(), NoticeDoor::Unread(_))
            && crate::capabilities::admits(
                at.map(|at| at.get()).as_ref(),
                crate::capabilities::Surface::Notifications,
            )
    });
    let cards = Memo::new(move |_| {
        style
            .map(|s| s.get().modules_in(ModuleRail::Left))
            .unwrap_or_default()
    });
    let doors = extra;
    // Everything that changes lives in a signal, and the tree below is built once: a subtree
    // rebuilt on each style change would dispose the owners behind the rows it replaces
    // while a queued update is still holding them.
    let nav = Memo::new(move |_| {
        let layout = style.map(|s| s.get().nav).unwrap_or_default();
        // A layout with no items at all is an answer this build cannot use: the account's
        // instance has no Nodus metadata endpoint (a plain Mastodon server 404s it, and
        // `/api/v1/nodus/layout` does not exist anywhere), or the document was written by a
        // client whose schema this one cannot read. Drawing its categories would leave the
        // rail as headings with nothing under them — a column that looks empty is a column
        // the reader cannot leave the page by, so the defaults stand in.
        if layout.items.is_empty() {
            return crate::session_style::NavLayout::default();
        }
        layout
    });
    let rails = Memo::new(move |_| {
        let layout = nav.get();
        let rails = layout
            .rails_in(ModuleRail::Left)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        // An account whose layout never arrived has no cards to draw, but a reader who
        // expected a rail and found an empty column cannot find their way out of it: the
        // default categories stand in until a layout says otherwise.
        if rails.is_empty() && layout.rails.is_empty() {
            return crate::session_style::NavLayout::default()
                .rails_in(ModuleRail::Left)
                .into_iter()
                .cloned()
                .collect();
        }
        rails
    });
    let staff_here =
        Memo::new(move |_| style.is_none_or(|s| s.get().staff_rail == ModuleRail::Left));
    let submenu_items = Memo::new(move |_| {
        nav.get()
            .items_in(NavPlacement::Submenu)
            .into_iter()
            .cloned()
            .collect::<Vec<NavItemConfig>>()
    });
    let submenu_title = Memo::new(move |_| nav.get().submenu_title().to_owned());

    view! {
        <aside
            class="hidden shrink-0 xl:flex xl:w-44 xl:flex-col xl:gap-3"
            data-rail="left"
            on:dragover=move |event| {
                if editor.is_some_and(StyleEditor::is_active) {
                    event.prevent_default();
                }
            }
            on:drop=move |event| {
                if let Some(edit) = editor {
                    event.prevent_default();
                    edit.drop_on(ModuleRail::Left);
                }
            }
        >
            <ProfileCard profile=profile/>
            <nav aria-label="Primary navigation">
                // The bell stands beside the categories rather than inside one: it is the
                // one entry whose badge changes while the page is open, and a reader who
                // filed Notifications under a category of their own still gets the count.
                <Show when=move || bell.get()>
                    <ul class="menu w-full flex-col gap-1 p-0" data-notice-bell="">
                        <li>
                            <a
                                class="flex w-full items-center gap-2 font-medium"
                                class:menu-active=move || active.get().starts_with("/notifications")
                                href="/notifications"
                                title="Mentions, boosts, favourites, follows and moderation notices"
                                data-notice-door
                            >
                                <Icon icon=Icon::Bell/>
                                <span class="truncate">"Notifications"</span>
                                {move || match door.get() {
                                    NoticeDoor::Unread(count) if count > 0 => view! {
                                        <span class="badge badge-primary badge-sm ml-auto" data-notice-count>
                                            {count}
                                        </span>
                                    }.into_any(),
                                    _ => ().into_any(),
                                }}
                            </a>
                        </li>
                    </ul>
                </Show>
                // The rail is built out of category cards in every render, not only while
                // editing: a reader arranges the site they actually see, and the server
                // paints the same cards with the verbs switched off.
                <div class="flex flex-col gap-2" data-nav-rails="">
                        // One card per category, each its own component: a reader can move,
                        // rename and delete a card, and its links travel with the entries
                        // rather than being renumbered.
                        <For
                            each=move || rails.get()
                            key=|rail| rail.id.clone()
                            children=move |rail| {
                                let rail_id = rail.id.clone();
                                let rail_for_items = rail_id.clone();
                                let label = Signal::derive(move || rail.label.clone());
                                let items = Signal::derive(move || {
                                    nav.get()
                                        .items_of_rail(rail_for_items.as_str())
                                        .into_iter()
                                        .filter(|item| {
                                            !(bell.get() && item.path == "/notifications")
                                        })
                                        .cloned()
                                        .collect::<Vec<NavItemConfig>>()
                                });
                                view! {
                                    <NavRailCard
                                        rail_id=rail_id
                                        side=ModuleRail::Left
                                        label=label
                                        items=items
                                        active=active
                                        editor=editor
                                    />
                                }
                            }
                        />
                        <Show when=move || editor.is_some_and(StyleEditor::is_active)>
                            <button
                                type="button"
                                class="w-full rounded-box border border-dashed border-base-content/50 bg-base-100 px-2 py-2 text-xs font-semibold uppercase tracking-wide opacity-80 hover:border-primary hover:text-primary"
                                data-nav-new-rail=""
                                draggable="true"
                                on:dragstart=move |_| {
                                    if let Some(edit) = editor {
                                        edit.drag(crate::session_style::Dragged::NewCategory);
                                    }
                                }
                                on:dragend=move |_| {
                                    if let Some(edit) = editor {
                                        edit.drag_end();
                                    }
                                }
                                on:click=move |_| {
                                    if let Some(edit) = editor {
                                        edit.add_nav_rail_in("New category", ModuleRail::Left);
                                    }
                                }
                            >
                                "+ Category"
                            </button>
                        </Show>
                        <Show when=move || staff_here.get()>{door_card(doors.clone(), editor)}</Show>
                        <Show when=move || !submenu_items.get().is_empty()>
                            <details class="rounded-box bg-base-100 p-2 shadow-sm" data-session-submenu="">
                                <summary class="cursor-pointer select-none px-2 py-1 text-xs font-semibold uppercase tracking-wide opacity-70">
                                    {move || submenu_title.get()}
                                </summary>
                                <ul class="menu mt-1 w-full flex-col gap-1 p-0">
                                    <NavItems
                                        items=Signal::derive(move || submenu_items.get())
                                        active=active
                                        editor=editor
                                    />
                                </ul>
                            </details>
                        </Show>
                </div>
            </nav>
            // Cards in this rail sit under the navigation: the rail is one column of
            // furniture, and which column a card belongs to is the reader's call.
            <RailCards
                kinds=Signal::derive(move || cards.get())
                sidebar=sidebar
                editor=editor
            />
        </aside>
    }
}

/// Optional right-hand session rail rendering items moved to `Right` and right session images.
#[component]
#[expect(
    clippy::too_many_lines,
    reason = "one rail: its categories, the doors and its cards, painted in one place"
)]
pub fn RightRail(
    #[prop(into)] active: Signal<String>,
    #[prop(into)] style: Signal<SessionStyle>,
    #[prop(optional_no_strip)] editor: Option<StyleEditor>,
    /// The data every card draws from; absent means every card is empty, which is what a
    /// server render that knew nothing has to offer.
    #[prop(optional_no_strip)]
    sidebar: Option<Signal<SidebarData>>,
) -> impl IntoView {
    let right_items = Memo::new(move |_| {
        style
            .get()
            .nav
            .items_in(NavPlacement::Right)
            .into_iter()
            .cloned()
            .collect::<Vec<NavItemConfig>>()
    });
    let modules = Memo::new(move |_| style.get().modules_in(ModuleRail::Right));
    let nav = Memo::new(move |_| style.get().nav);
    let rails = Memo::new(move |_| {
        nav.get()
            .rails_in(ModuleRail::Right)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
    });
    let staff_here = Memo::new(move |_| style.get().staff_rail == ModuleRail::Right);
    let shown = Memo::new(move |_| {
        !right_items.get().is_empty()
            || !modules.get().is_empty()
            || !rails.get().is_empty()
            || staff_here.get()
            || editor.is_some_and(StyleEditor::is_active)
    });

    view! {
        <Show when=move || shown.get()>
            <aside
                class="hidden shrink-0 xl:flex xl:w-52 xl:flex-col xl:gap-3"
                data-session-right=""
                data-rail="right"
                on:dragover=move |event| {
                    if editor.is_some_and(StyleEditor::is_active) {
                        event.prevent_default();
                    }
                }
                on:drop=move |event| {
                    if let Some(edit) = editor {
                        event.prevent_default();
                        edit.drop_on(ModuleRail::Right);
                    }
                }
            >
                <div class="flex flex-col gap-2" data-nav-rails="">
                    <For
                        each=move || rails.get()
                        key=|rail| rail.id.clone()
                        children=move |rail| {
                            let rail_id = rail.id.clone();
                            let rail_for_items = rail_id.clone();
                            let label = Signal::derive(move || rail.label.clone());
                            let items = Signal::derive(move || {
                                nav.get()
                                    .items_of_rail(rail_for_items.as_str())
                                    .into_iter()
                                    .cloned()
                                    .collect::<Vec<NavItemConfig>>()
                            });
                            view! {
                                <NavRailCard
                                    rail_id=rail_id
                                    side=ModuleRail::Right
                                    label=label
                                    items=items
                                    active=active
                                    editor=editor
                                />
                            }
                        }
                    />
                    <Show when=move || staff_here.get()>
                        {door_card(None, editor)}
                    </Show>
                    <Show when=move || editor.is_some_and(StyleEditor::is_active)>
                        <button
                            type="button"
                            class="w-full rounded-box border border-dashed border-base-content/50 bg-base-100 px-2 py-2 text-xs font-semibold uppercase tracking-wide opacity-80 hover:border-primary hover:text-primary"
                            data-nav-new-rail="right"
                            draggable="true"
                            on:dragstart=move |_| {
                                if let Some(edit) = editor {
                                    edit.drag(crate::session_style::Dragged::NewCategory);
                                }
                            }
                            on:dragend=move |_| {
                                if let Some(edit) = editor {
                                    edit.drag_end();
                                }
                            }
                            on:click=move |_| {
                                if let Some(edit) = editor {
                                    edit.add_nav_rail_in("New category", ModuleRail::Right);
                                }
                            }
                        >
                            "+ Category"
                        </button>
                    </Show>
                </div>
                <Show when=move || !right_items.get().is_empty()>
                    <nav aria-label="Right rail navigation">
                        <ul class="menu w-full flex-col gap-1 rounded-box bg-base-100 p-2 shadow-sm">
                            <NavItems
                                items=Signal::derive(move || right_items.get())
                                active=active
                                editor=editor
                            />
                        </ul>
                    </nav>
                </Show>
                <RailCards
                    kinds=Signal::derive(move || modules.get())
                    sidebar=sidebar
                    editor=editor

                />
            </aside>
        </Show>
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

#[cfg(test)]
mod tests;
