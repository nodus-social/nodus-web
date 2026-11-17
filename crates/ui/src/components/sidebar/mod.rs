//! The rail cards, and the tray a reader builds them from.
//!
//! A card is a small read of the instance — what is trending, what arrived while the reader
//! was away, what the recommender suggests. They are drawn from data the caller hands over,
//! so the same component paints a server render (with whatever the page already knew) and a
//! hydrated one (with what the browser fetched), and neither has to know where it came from.
//!
//! While the editor is open every card grows a handle and a button that puts it back in the
//! tray, and the tray in the middle of the page lists every card with the rail it sits in.
//! That is the whole build loop: one list, two rails, one Save.

use leptos::prelude::*;

use crate::session_style::{ModuleKind, ModuleRail, NavLayout, PlacedModule, StyleEditor};

/// One line inside a card: a tag, a person, a post, a notification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SidebarItem {
    /// The row's text, already plain (never HTML from a remote author).
    pub label: String,
    /// A short qualifier: use counts, a handle, a timestamp, an author.
    pub meta: String,
    /// Where the row goes when clicked.
    pub href: String,
    /// A picture, for rows that have one.
    pub avatar_url: Option<String>,
}

impl SidebarItem {
    /// A text row.
    #[must_use]
    pub fn new(label: impl Into<String>, meta: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            meta: meta.into(),
            href: href.into(),
            avatar_url: None,
        }
    }

    /// The same row, with a picture.
    #[must_use]
    pub fn with_picture(mut self, avatar_url: impl Into<String>) -> Self {
        let avatar_url = avatar_url.into();
        self.avatar_url = (!avatar_url.is_empty()).then_some(avatar_url);
        self
    }
}

/// Everything the cards can show, keyed by the card that shows it.
///
/// Empty is a legitimate answer and each card says so in its own words: an instance with
/// nothing trending, a reader with no notifications, or a browser that has not fetched yet
/// are three different silences, and the caption is what tells them apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SidebarData {
    /// The public timeline.
    pub live_feed: Vec<SidebarItem>,
    /// The reader's own notifications, newest first.
    pub notifications: Vec<SidebarItem>,
    /// Tags in use on this instance.
    pub trending_tags: Vec<SidebarItem>,
    /// Accounts the instance points at.
    pub trending_users: Vec<SidebarItem>,
    /// Posts the instance is promoting.
    pub trending_posts: Vec<SidebarItem>,
    /// Links lifted from those posts.
    pub trending_links: Vec<SidebarItem>,
    /// Accounts the recommender suggests.
    pub recommended_accounts: Vec<SidebarItem>,
    /// Posts the recommender picked.
    pub recommended_posts: Vec<SidebarItem>,
}

impl SidebarData {
    /// The rows for one card.
    #[must_use]
    pub fn items(&self, kind: ModuleKind) -> &[SidebarItem] {
        match kind {
            ModuleKind::LiveFeed => &self.live_feed,
            ModuleKind::Notifications => &self.notifications,
            ModuleKind::TrendingTags => &self.trending_tags,
            ModuleKind::TrendingUsers => &self.trending_users,
            ModuleKind::TrendingPosts => &self.trending_posts,
            ModuleKind::TrendingLinks => &self.trending_links,
            ModuleKind::RecommendedAccounts => &self.recommended_accounts,
            ModuleKind::RecommendedPosts => &self.recommended_posts,
        }
    }

    /// Whether any card has something to show.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        ModuleKind::ALL
            .into_iter()
            .all(|kind| self.items(kind).is_empty())
    }
}

/// What a card says when it has nothing.
#[must_use]
pub fn empty_caption(kind: ModuleKind) -> &'static str {
    match kind {
        ModuleKind::LiveFeed => "Nothing has arrived yet. It will, as soon as anyone posts.",
        ModuleKind::Notifications => "Nothing new. Mentions and replies land here.",
        ModuleKind::TrendingTags => "Nothing is trending here yet.",
        ModuleKind::TrendingUsers => "No accounts to suggest right now.",
        ModuleKind::TrendingPosts => "Nothing is getting attention here yet.",
        ModuleKind::TrendingLinks => "No links shared yet.",
        ModuleKind::RecommendedAccounts => {
            "Follow a few people and the recommender will have something to suggest."
        }
        ModuleKind::RecommendedPosts => {
            "Reading a little is how the recommender learns what to pick."
        }
    }
}

/// One card in the rail.
#[component]
pub fn SidebarModule(
    kind: ModuleKind,
    #[prop(into)] items: Signal<Vec<SidebarItem>>,
    #[prop(optional_no_strip)] editor: Option<StyleEditor>,
    /// Which card is being dragged right now, shared by the cards in one rail.
    #[prop(optional_no_strip)]
    dragging: Option<RwSignal<Option<ModuleKind>>>,
) -> impl IntoView {
    let editing = Memo::new(move |_| editor.is_some_and(StyleEditor::is_active));
    let rows = Memo::new(move |_| items.get());
    let id = kind.id();
    view! {
        <section
            class="rounded-box border border-base-300 bg-base-100 p-2 shadow-sm"
            data-rail-module=id
            draggable=move || editing.get().to_string()
            on:dragstart=move |_| {
                if let Some(dragging) = dragging {
                    dragging.set(Some(kind));
                }
            }
            on:dragover=move |event| {
                if editing.get_untracked() {
                    event.prevent_default();
                }
            }
            on:drop=move |event| {
                event.prevent_default();
                let from = dragging.and_then(|signal| signal.get_untracked());
                if let (Some(dragging), Some(editor), Some(from)) = (dragging, editor, from) {
                    editor.reorder_module(from, kind);
                    dragging.set(None);
                }
            }
        >
            <header class="mb-1 flex items-center gap-1">
                <Show when=move || editing.get()>
                    <span class="cursor-grab text-xs opacity-50" title="Drag to reorder">"⣿"</span>
                </Show>
                <h2 class="flex-1 truncate text-xs font-semibold tracking-wide uppercase opacity-60">
                    {kind.label()}
                </h2>
                <Show when=move || editing.get()>
                    <button
                        type="button"
                        class="btn btn-ghost btn-xs"
                        title="Take this card off the rail"
                        on:click=move |_| {
                            if let Some(editor) = editor {
                                editor.remove_module(kind);
                            }
                        }
                    >
                        "✕"
                    </button>
                </Show>
            </header>
            <ul class="flex flex-col gap-1">
                <For
                    each=move || rows.get()
                    key=|item| (item.href.clone(), item.label.clone())
                    children=move |item| {
                        let picture = item.avatar_url.clone();
                        let label = item.label.clone();
                        let meta = item.meta.clone();
                        let has_meta = !item.meta.is_empty();
                        view! {
                            <li>
                                <a
                                    class="flex items-center gap-2 rounded-box p-1 text-sm transition hover:bg-base-300"
                                    href=item.href
                                    title=item.label
                                >
                                    {picture.map(|url| {
                                        view! {
                                            <img
                                                class="size-6 shrink-0 rounded-full object-cover"
                                                src=url
                                                alt=""
                                                loading="lazy"
                                            />
                                        }
                                    })}
                                    <span class="min-w-0 flex-1">
                                        <span class="block truncate">{label}</span>
                                        <Show when=move || has_meta>
                                            <span class="block truncate text-xs opacity-60">
                                                {meta.clone()}
                                            </span>
                                        </Show>
                                    </span>
                                </a>
                            </li>
                        }
                    }
                />
            </ul>
            <Show when=move || rows.get().is_empty()>
                <p class="px-1 py-2 text-xs opacity-60">{empty_caption(kind)}</p>
            </Show>
        </section>
    }
}

/// The middle of the editor: every card that exists, with the rail it lives in on one tap.
///
/// It is the dumping ground the editor page needs: the rails are where the cards live, so the
/// largest area of the screen is the best place to see what is not there, pick it up, put it
/// back, and say which side of the page it belongs to. Nothing here is a preview — each row
/// says what it reads and where it is right now.
#[component]
#[expect(
    clippy::too_many_lines,
    reason = "one catalogue: cards, categories and the doors, each a row of the same list"
)]
pub fn ModuleTray(
    #[prop(into)] layout: Signal<Vec<PlacedModule>>,
    /// The navigation layout: its category cards are movable furniture like the cards are.
    #[prop(into)]
    nav: Signal<NavLayout>,
    /// Which rail currently holds the staff doors.
    #[prop(into)]
    staff_rail: Signal<ModuleRail>,
    /// Whether this reader has staff doors at all; a reader without them is not offered a
    /// way to move them.
    #[prop(default = false)]
    has_staff: bool,
    /// Present in the browser, where there is a draft to write into. A server render has
    /// none, so it paints the catalogue with the buttons disabled — the same tree either way.
    #[prop(optional_no_strip)]
    editor: Option<StyleEditor>,
) -> impl IntoView {
    // The list is the catalogue, not the placement: a row whose placement changed is the
    // same row, so `For` keeps it and the buttons below read the placement reactively.
    // Keying on the placement instead would rebuild the row — and its captured state would
    // then be a snapshot of the placement at the moment it was drawn.
    let placed = move |kind: ModuleKind| {
        layout
            .get()
            .iter()
            .find(|held| held.kind == kind)
            .map(|held| held.rail)
    };
    let editable = Signal::derive(move || editor.is_some());
    view! {
        <section class="flex flex-col gap-4" data-module-tray="">
            <header class="flex flex-col gap-1">
                <h1 class="text-lg font-semibold">"This page"</h1>
                <p class="text-sm opacity-70">
                    "Everything here is furniture you can move: drag a card or a category onto either rail, or use the two rail buttons. Nothing changes for anyone else until you Save."
                </p>
            </header>

            <h2 class="text-sm font-semibold uppercase tracking-wide opacity-70">"Cards"</h2>
            <ul class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
                <For
                    each=move || ModuleKind::ALL
                    key=|kind| *kind
                    children=move |kind| {
                        let chosen = Signal::derive(move || placed(kind));
                        view! {
                            <li
                                class="flex flex-col gap-2 rounded-box border border-base-300 bg-base-100 p-3 shadow-sm"
                                data-module-option=kind.id()
                                draggable=move || editable.get().to_string()
                                on:dragstart=move |_| {
                                    if let Some(edit) = editor {
                                        edit.drag_module(kind);
                                    }
                                }
                            >
                                <div class="min-w-0">
                                    <h3 class="truncate text-sm font-semibold">{kind.label()}</h3>
                                    <p class="text-xs opacity-70">{kind.hint()}</p>
                                </div>
                                <div class="mt-auto flex flex-wrap items-center gap-2">
                                    <button
                                        type="button"
                                        class=move || {
                                            if chosen.get().is_some() {
                                                "btn btn-ghost btn-xs"
                                            } else {
                                                "btn btn-primary btn-xs"
                                            }
                                        }
                                        disabled=editor.is_none()
                                        on:click=move |_| {
                                            if let Some(editor) = editor {
                                                editor.toggle_module(kind);
                                            }
                                        }
                                    >
                                        {move || if chosen.get().is_some() { "Take off" } else { "Add" }}
                                    </button>
                                    <Show when=move || chosen.get().is_some()>
                                        <span class="ml-auto flex items-center gap-1" data-module-rail-picker="">
                                            <RailButton kind=kind rail=ModuleRail::Left chosen=chosen editor=editor/>
                                            <RailButton kind=kind rail=ModuleRail::Right chosen=chosen editor=editor/>
                                        </span>
                                    </Show>
                                </div>
                            </li>
                        }
                    }
                />
            </ul>

            <h2 class="text-sm font-semibold uppercase tracking-wide opacity-70">"Navigation"</h2>
            <ul class="flex flex-col gap-2" data-tray-rails="">
                <For
                    each=move || nav.get().rails
                    key=|rail| rail.id.clone()
                    children=move |rail| {
                        let id = Signal::derive({
                            let id = rail.id.clone();
                            move || id.clone()
                        });
                        let label = Signal::derive({
                            let label = rail.label.clone();
                            move || label.clone()
                        });
                        let side = Signal::derive({
                            let side = rail.rail;
                            move || side
                        });
                        let entries = Signal::derive({
                            let id = rail.id.clone();
                            move || nav.get().items_of_rail(&id).len()
                        });
                        view! {
                            <li
                                class="flex flex-wrap items-center gap-2 rounded-box border border-base-300 bg-base-100 p-2 shadow-sm"
                                data-tray-rail=move || id.get()
                                draggable=move || editable.get().to_string()
                                on:dragstart=move |_| {
                                    if let Some(edit) = editor {
                                        edit.drag_rail(&id.get_untracked());
                                    }
                                }
                            >
                                <span class="cursor-grab select-none opacity-60" title="Drag this category to a rail">
                                    "\u{28FF}"
                                </span>
                                <span class="min-w-0 flex-1 truncate text-sm font-semibold">{move || label.get()}</span>
                                <span class="text-xs opacity-60">{move || format!("{} links", entries.get())}</span>
                                <span class="flex items-center gap-1">
                                    <RailSideButton
                                        side=ModuleRail::Left
                                        chosen=side
                                        label="left rail"
                                        editor=editor
                                        rail_id=id
                                    />
                                    <RailSideButton
                                        side=ModuleRail::Right
                                        chosen=side
                                        label="right rail"
                                        editor=editor
                                        rail_id=id
                                    />
                                </span>
                            </li>
                        }
                    }
                />
            </ul>

            <Show when=move || has_staff>
                <h2 class="text-sm font-semibold uppercase tracking-wide opacity-70">"Staff"</h2>
                <div
                    class="flex flex-wrap items-center gap-2 rounded-box border border-base-300 bg-base-100 p-2 shadow-sm"
                    data-tray-staff=""
                    draggable=move || editable.get().to_string()
                    on:dragstart=move |_| {
                        if let Some(edit) = editor {
                            edit.drag_staff();
                        }
                    }
                >
                    <span class="cursor-grab select-none opacity-60">"\u{28FF}"</span>
                    <span class="min-w-0 flex-1 truncate text-sm font-semibold">"Moderation and administration"</span>
                    <span class="flex items-center gap-1">
                        <StaffSideButton side=ModuleRail::Left chosen=staff_rail editor=editor/>
                        <StaffSideButton side=ModuleRail::Right chosen=staff_rail editor=editor/>
                    </span>
                </div>
            </Show>

            <p class="text-xs opacity-60">
                "Cards that read the instance work as soon as you add them. The two that ask the recommender need a minute of reading before they have anything to say. Settings stays on a rail: it is how you come back."
            </p>
        </section>
    }
}

/// One of the two rail buttons beside a category card in the tray.
#[component]
fn RailSideButton(
    side: ModuleRail,
    #[prop(into)] chosen: Signal<ModuleRail>,
    label: &'static str,
    #[prop(optional_no_strip)] editor: Option<StyleEditor>,
    #[prop(into)] rail_id: Signal<String>,
) -> impl IntoView {
    view! {
        <button
            type="button"
            class=move || {
                if chosen.get() == side {
                    "btn btn-xs btn-active"
                } else {
                    "btn btn-xs btn-ghost"
                }
            }
            data-tray-rail-picker=side.id()
            disabled=editor.is_none()
            title=format!("Put this category on the {label}")
            on:click=move |_| {
                if let Some(edit) = editor {
                    edit.set_nav_rail_side(&rail_id.get_untracked(), side);
                }
            }
        >
            {label}
        </button>
    }
}

/// The same button, for the one card the staff doors are.
#[component]
fn StaffSideButton(
    side: ModuleRail,
    #[prop(into)] chosen: Signal<ModuleRail>,
    #[prop(optional_no_strip)] editor: Option<StyleEditor>,
) -> impl IntoView {
    view! {
        <button
            type="button"
            class=move || {
                if chosen.get() == side {
                    "btn btn-xs btn-active"
                } else {
                    "btn btn-xs btn-ghost"
                }
            }
            data-tray-staff-picker=side.id()
            disabled=editor.is_none()
            title=format!("Put the staff doors on the {}", side.label())
            on:click=move |_| {
                if let Some(edit) = editor {
                    edit.set_staff_rail(side);
                }
            }
        >
            {side.label()}
        </button>
    }
}

/// One of the two rail buttons on a card: says where the card is, and moves it there.
#[component]
fn RailButton(
    kind: ModuleKind,
    rail: ModuleRail,
    #[prop(into)] chosen: Signal<Option<ModuleRail>>,
    #[prop(optional_no_strip)] editor: Option<StyleEditor>,
) -> impl IntoView {
    view! {
        <button
            type="button"
            class=move || {
                if chosen.get() == Some(rail) {
                    "btn btn-xs btn-active"
                } else {
                    "btn btn-xs btn-ghost"
                }
            }
            title=format!("Put this card in the {}", rail.label())
            disabled=editor.is_none()
            on:click=move |_| {
                if let Some(editor) = editor {
                    editor.set_module_rail(kind, rail);
                }
            }
        >
            {rail.label()}
        </button>
    }
}
