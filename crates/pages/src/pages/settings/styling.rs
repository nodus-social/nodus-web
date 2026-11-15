//! Dedicated session styling editor & live layout preview (`/settings/styling`).

use std::sync::Arc;

use leptos::prelude::*;
use nodus_ui::html::safe_url;
use nodus_ui::session_style::{
    BannerRegion, BannerRegionKind, ImageSlot, MAX_SLOTS_PER_REGION, NavItemConfig, NavLayout,
    NavPlacement, SessionStyle, SplitDirection,
};

use crate::components::panel::Panel;

/// Optional client-provided file dropzone / picker rendered inside each banner region editor.
pub type UploadSlotRenderer = Arc<dyn Fn(BannerRegionKind) -> AnyView + Send + Sync>;

/// Last `(account_key, synced_style)` snapshot observed by an interactive styling editor.
///
/// Used to apply incoming background refreshes (e.g., `refresh_active()` completing after initial
/// mount) without discarding local unsaved edits, while still resetting on account switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleDraftSnapshot {
    pub account_key: Option<String>,
    pub synced: SessionStyle,
}

impl StyleDraftSnapshot {
    /// Returns `true` when incoming `active_style` updates should replace `current_draft`.
    #[must_use]
    pub fn should_apply_update(
        prev: Option<&Self>,
        account_key: &Option<String>,
        current_draft: &SessionStyle,
    ) -> bool {
        match prev {
            None => true,
            Some(last) => last.account_key != *account_key || *current_draft == last.synced,
        }
    }
}

fn preview_region_block(
    region: BannerRegion,
    empty_label: &'static str,
    height_class: &'static str,
) -> impl IntoView {
    let valid_slots: Vec<_> = region
        .slots
        .into_iter()
        .filter(|slot| safe_url(&slot.url))
        .collect();
    if valid_slots.is_empty() {
        return view! {
            <div class=format!(
                "flex {height_class} items-center justify-center rounded-box border border-dashed border-base-300 bg-base-200/70 px-2 text-center text-xs opacity-60"
            )>
                {empty_label}
            </div>
        }
        .into_any();
    }
    let container_class = match region.split {
        SplitDirection::Horizontal => "flex flex-row gap-1",
        SplitDirection::Vertical => "flex flex-col gap-1",
    };
    view! {
        <div class=container_class>
            {valid_slots
                .into_iter()
                .map(|slot| {
                    view! {
                        <img
                            src=slot.url
                            alt=slot.alt.unwrap_or_default()
                            class=format!("{height_class} min-w-0 flex-1 rounded-box object-cover")
                        />
                    }
                })
                .collect_view()}
        </div>
    }
    .into_any()
}

fn preview_nav_pills(items: Vec<NavItemConfig>) -> impl IntoView {
    view! {
        <ul class="flex flex-col gap-1">
            {items
                .into_iter()
                .map(|item| {
                    view! {
                        <li class="flex items-center justify-between gap-1 rounded bg-base-200 px-2 py-1 text-xs">
                            <span class="truncate">{item.label}</span>
                            {item.pinned.then(|| view! {
                                <span class="badge badge-xs badge-primary">"Pin"</span>
                            })}
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
}

/// Visual miniature preview of the active session shell reflecting image slots & nav placement.
#[component]
pub fn StylingPreview(#[prop(into)] style: Signal<SessionStyle>) -> impl IntoView {
    view! {
        <Panel title="Example layout preview">
            <p class="text-xs opacity-70">
                "Live preview of how your top, left, and right session banners frame navigation and your timeline."
            </p>
            {move || {
                let current = style.get();
                let left_items: Vec<_> = current
                    .nav
                    .items_in(NavPlacement::Left)
                    .into_iter()
                    .cloned()
                    .collect();
                let submenu_items: Vec<_> = current
                    .nav
                    .items_in(NavPlacement::Submenu)
                    .into_iter()
                    .cloned()
                    .collect();
                let right_items: Vec<_> = current
                    .nav
                    .items_in(NavPlacement::Right)
                    .into_iter()
                    .cloned()
                    .collect();
                let submenu_title = current.nav.submenu_title().to_owned();
                view! {
                    <div
                        class="flex flex-col gap-2 rounded-box border border-base-300 bg-base-200 p-3"
                        data-styling-preview=""
                    >
                        {preview_region_block(current.top, "Top banner slot", "h-16")}
                        <div class="grid grid-cols-1 gap-2 md:grid-cols-[9rem_minmax(0,1fr)_9rem]">
                            <div class="flex flex-col gap-2 rounded-box bg-base-100 p-2">
                                <span class="text-[11px] font-semibold uppercase tracking-wide opacity-60">
                                    "Left rail"
                                </span>
                                {preview_nav_pills(left_items)}
                                {(!submenu_items.is_empty()).then(|| {
                                    view! {
                                        <div class="rounded bg-base-200/70 p-1.5">
                                            <div class="mb-1 text-[11px] font-medium opacity-70">
                                                {format!("{submenu_title} ▾")}
                                            </div>
                                            {preview_nav_pills(submenu_items)}
                                        </div>
                                    }
                                })}
                                {preview_region_block(current.left, "Left image", "h-16")}
                            </div>
                            <div class="flex flex-col justify-between gap-2 rounded-box bg-base-100 p-3">
                                <div class="flex flex-col gap-1">
                                    <span class="text-xs font-semibold">"Timeline column"</span>
                                    <span class="text-xs opacity-60">
                                        "Each account keeps its own banner framing and navigation order."
                                    </span>
                                </div>
                                <div class="h-14 rounded-box bg-base-200/80"/>
                            </div>
                            <div class="flex flex-col gap-2 rounded-box bg-base-100 p-2">
                                <span class="text-[11px] font-semibold uppercase tracking-wide opacity-60">
                                    "Right rail"
                                </span>
                                {if right_items.is_empty() {
                                    view! {
                                        <p class="text-[11px] opacity-50">"No right nav items"</p>
                                    }
                                        .into_any()
                                } else {
                                    preview_nav_pills(right_items).into_any()
                                }}
                                {preview_region_block(current.right, "Right image", "h-16")}
                            </div>
                        </div>
                    </div>
                }
            }}
        </Panel>
    }
}

#[component]
fn RegionEditor(
    kind: BannerRegionKind,
    style: RwSignal<SessionStyle>,
    upload_zone: Option<UploadSlotRenderer>,
) -> impl IntoView {
    let url_input = RwSignal::new(String::new());
    let alt_input = RwSignal::new(String::new());
    let local_error = RwSignal::new(None::<String>);
    let dragged_slot = RwSignal::new(None::<usize>);

    view! {
        <div
            class="flex flex-col gap-3 rounded-box border border-base-300 bg-base-100 p-3"
            data-region-editor=kind.metadata_key()
        >
            <div class="flex flex-wrap items-center justify-between gap-2">
                <div>
                    <h3 class="text-sm font-semibold">{kind.label()}</h3>
                    <p class="text-xs opacity-60">
                        {move || {
                            let count = style.with(|s| s.region(kind).slots.len());
                            format!("{count}/{MAX_SLOTS_PER_REGION} slots")
                        }}
                    </p>
                </div>
                <div class="join">
                    <button
                        type="button"
                        class="btn btn-xs join-item"
                        class:btn-primary=move || style.with(|s| s.region(kind).split == SplitDirection::Horizontal)
                        on:click=move |_| {
                            style.update(|s| s.region_mut(kind).split = SplitDirection::Horizontal);
                        }
                    >
                        "Side-by-side"
                    </button>
                    <button
                        type="button"
                        class="btn btn-xs join-item"
                        class:btn-primary=move || style.with(|s| s.region(kind).split == SplitDirection::Vertical)
                        on:click=move |_| {
                            style.update(|s| s.region_mut(kind).split = SplitDirection::Vertical);
                        }
                    >
                        "Stacked"
                    </button>
                </div>
            </div>

            {upload_zone.map(|render| render(kind))}

            {move || {
                let region = style.with(|s| s.region(kind).clone());
                let count = region.slots.len();
                if region.slots.is_empty() {
                    return view! {
                        <p class="text-xs opacity-60">
                            "No image slots yet. Upload an image above or add an HTTPS image URL below."
                        </p>
                    }
                        .into_any();
                }
                view! {
                    <ul class="flex flex-col gap-2">
                        {region
                            .slots
                            .into_iter()
                            .enumerate()
                            .map(|(idx, slot)| {
                                let can_up = idx > 0;
                                let can_down = idx + 1 < count;
                                view! {
                                    <li
                                        class="flex flex-wrap items-center gap-2 rounded-box bg-base-200 p-2"
                                        draggable="true"
                                        on:dragstart=move |_| dragged_slot.set(Some(idx))
                                        on:dragover=move |ev| ev.prevent_default()
                                        on:drop=move |ev| {
                                            ev.prevent_default();
                                            if let Some(from) = dragged_slot.get_untracked() {
                                                style.update(|s| s.region_mut(kind).move_slot(from, idx));
                                            }
                                            dragged_slot.set(None);
                                        }
                                    >
                                        <span class="cursor-grab text-xs opacity-50" title="Drag to reorder">
                                            "::"
                                        </span>
                                        <img
                                            src=slot.url.clone()
                                            alt=slot.alt.clone().unwrap_or_default()
                                            class="h-10 w-16 rounded object-cover"
                                        />
                                        <div class="flex min-w-0 flex-1 flex-col">
                                            <span class="truncate text-xs font-medium">{slot.url}</span>
                                            {slot.alt.map(|alt| view! {
                                                <span class="truncate text-[11px] opacity-60">{alt}</span>
                                            })}
                                        </div>
                                        <div class="flex items-center gap-1">
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs"
                                                disabled=!can_up
                                                on:click=move |_| {
                                                    if idx > 0 {
                                                        style.update(|s| s.region_mut(kind).move_slot(idx, idx - 1));
                                                    }
                                                }
                                            >
                                                "Earlier"
                                            </button>
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs"
                                                disabled=!can_down
                                                on:click=move |_| {
                                                    style.update(|s| s.region_mut(kind).move_slot(idx, idx + 1));
                                                }
                                            >
                                                "Later"
                                            </button>
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs text-error"
                                                on:click=move |_| {
                                                    style.update(|s| s.region_mut(kind).remove_slot(idx));
                                                }
                                            >
                                                "Remove"
                                            </button>
                                        </div>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                }
                    .into_any()
            }}

            <form
                class="flex flex-wrap items-end gap-2"
                on:submit=move |ev| {
                    ev.prevent_default();
                    let raw_url = url_input.get_untracked();
                    let raw_alt = alt_input.get_untracked();
                    let Some(slot) = ImageSlot::new(&raw_url, Some(&raw_alt), None) else {
                        local_error.set(Some(String::from("Enter a valid http(s) or relative image URL.")));
                        return;
                    };
                    let added = style.try_update(|s| s.region_mut(kind).add_slot(slot)).unwrap_or(false);
                    if added {
                        url_input.set(String::new());
                        alt_input.set(String::new());
                        local_error.set(None);
                    } else {
                        local_error.set(Some(format!("At most {MAX_SLOTS_PER_REGION} slots fit in this banner.")));
                    }
                }
            >
                <label class="flex min-w-[12rem] flex-1 flex-col gap-1">
                    <span class="text-xs opacity-70">"Add split slot URL"</span>
                    <input
                        type="url"
                        class="input input-bordered input-xs w-full"
                        placeholder="https://..."
                        prop:value=move || url_input.get()
                        on:input=move |ev| url_input.set(event_target_value(&ev))
                    />
                </label>
                <label class="flex w-36 flex-col gap-1">
                    <span class="text-xs opacity-70">"Alt text"</span>
                    <input
                        type="text"
                        class="input input-bordered input-xs w-full"
                        placeholder="Optional description"
                        prop:value=move || alt_input.get()
                        on:input=move |ev| alt_input.set(event_target_value(&ev))
                    />
                </label>
                <button
                    type="submit"
                    class="btn btn-outline btn-xs"
                    disabled=move || style.with(|s| s.region(kind).slots.len() >= MAX_SLOTS_PER_REGION)
                >
                    "Split / add slot"
                </button>
            </form>
            <Show when=move || local_error.get().is_some()>
                <p class="text-xs text-error">{move || local_error.get().unwrap_or_default()}</p>
            </Show>
        </div>
    }
}

#[component]
fn NavPlacementBucket(
    placement: NavPlacement,
    style: RwSignal<SessionStyle>,
    filter_text: ReadSignal<String>,
    dragged_nav_id: RwSignal<Option<String>>,
) -> impl IntoView {
    view! {
        <div
            class="flex flex-col gap-2 rounded-box border border-dashed border-base-300 bg-base-100 p-3"
            data-nav-dropzone=placement.label()
            on:dragover=move |ev| ev.prevent_default()
            on:drop=move |ev| {
                ev.prevent_default();
                if let Some(id) = dragged_nav_id.get_untracked() {
                    style.update(|s| s.nav.move_item_to(&id, placement));
                }
                dragged_nav_id.set(None);
            }
        >
            <div class="flex items-center justify-between gap-2">
                <h3 class="text-xs font-semibold uppercase tracking-wide opacity-70">
                    {placement.label()}
                </h3>
                <span class="badge badge-ghost badge-xs">
                    {move || style.with(|s| s.nav.items_in(placement).len())}
                </span>
            </div>
            {move || {
                let query = filter_text.get().trim().to_ascii_lowercase();
                let items: Vec<NavItemConfig> = style
                    .with(|s| {
                        s.nav
                            .items_in(placement)
                            .into_iter()
                            .filter(|item| {
                                query.is_empty()
                                    || item.label.to_ascii_lowercase().contains(&query)
                                    || item.path.to_ascii_lowercase().contains(&query)
                            })
                            .cloned()
                            .collect()
                    });
                if items.is_empty() {
                    return view! {
                        <p class="text-xs opacity-50">"Drop navigation items here."</p>
                    }
                        .into_any();
                }
                view! {
                    <ul class="flex flex-col gap-1.5">
                        {items
                            .into_iter()
                            .map(|item| {
                                let id = item.id.clone();
                                let id_drag = id.clone();
                                let id_drop = id.clone();
                                let id_up = id.clone();
                                let id_down = id.clone();
                                let id_pin = id.clone();
                                let id_remove = id.clone();
                                let id_move = id.clone();
                                let is_custom = id.starts_with("fav-");
                                view! {
                                    <li
                                        class="flex flex-wrap items-center justify-between gap-2 rounded-box bg-base-200 px-2.5 py-1.5"
                                        draggable="true"
                                        on:dragstart=move |_| dragged_nav_id.set(Some(id_drag.clone()))
                                        on:dragover=move |ev| ev.prevent_default()
                                        on:drop=move |ev| {
                                            ev.prevent_default();
                                            ev.stop_propagation();
                                            if let Some(from_id) = dragged_nav_id.get_untracked() {
                                                style.update(|s| s.nav.reorder_before(&from_id, &id_drop));
                                            }
                                            dragged_nav_id.set(None);
                                        }
                                    >
                                        <div class="flex min-w-0 items-center gap-2">
                                            <span class="cursor-grab text-xs opacity-50" title="Drag to reorder">
                                                "::"
                                            </span>
                                            <span class="truncate text-sm font-medium">{item.label}</span>
                                            <span class="hidden truncate text-xs opacity-50 sm:inline">
                                                {item.path}
                                            </span>
                                            {item.pinned.then(|| view! {
                                                <span class="badge badge-xs badge-primary">"Pinned"</span>
                                            })}
                                        </div>
                                        <div class="flex flex-wrap items-center gap-1">
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs"
                                                on:click=move |_| {
                                                    style.update(|s| s.nav.toggle_pin(&id_pin));
                                                }
                                            >
                                                {if item.pinned { "Unpin" } else { "Pin" }}
                                            </button>
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs"
                                                on:click=move |_| {
                                                    style.update(|s| s.nav.shift_in_placement(&id_up, false));
                                                }
                                            >
                                                "↑"
                                            </button>
                                            <button
                                                type="button"
                                                class="btn btn-ghost btn-xs"
                                                on:click=move |_| {
                                                    style.update(|s| s.nav.shift_in_placement(&id_down, true));
                                                }
                                            >
                                                "↓"
                                            </button>
                                            <div class="join">
                                                {NavPlacement::ALL
                                                    .into_iter()
                                                    .filter(|dest| *dest != placement)
                                                    .map(|dest| {
                                                        let id_dest = id_move.clone();
                                                        let short = match dest {
                                                            NavPlacement::Left => "Left",
                                                            NavPlacement::Right => "Right",
                                                            NavPlacement::Submenu => "Submenu",
                                                            NavPlacement::Hidden => "Hide",
                                                        };
                                                        view! {
                                                            <button
                                                                type="button"
                                                                class="btn btn-ghost btn-xs join-item"
                                                                on:click=move |_| {
                                                                    style.update(|s| s.nav.move_item_to(&id_dest, dest));
                                                                }
                                                            >
                                                                {short}
                                                            </button>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </div>
                                            {is_custom.then(|| view! {
                                                <button
                                                    type="button"
                                                    class="btn btn-ghost btn-xs text-error"
                                                    on:click=move |_| {
                                                        style.update(|s| s.nav.remove_item(&id_remove));
                                                    }
                                                >
                                                    "Remove"
                                                </button>
                                            })}
                                        </div>
                                    </li>
                                }
                            })
                            .collect_view()}
                    </ul>
                }
                    .into_any()
            }}
        </div>
    }
}

#[component]
fn NavLayoutEditor(
    style: RwSignal<SessionStyle>,
    saving: Signal<bool>,
    message: Signal<Option<String>>,
    error: Signal<Option<String>>,
    on_save: Option<Callback<SessionStyle>>,
    on_reset: Option<Callback<()>>,
) -> impl IntoView {
    let filter_text = RwSignal::new(String::new());
    let fav_label = RwSignal::new(String::new());
    let fav_path = RwSignal::new(String::from("/saved/favourites"));
    let fav_placement = RwSignal::new(NavPlacement::Left);
    let fav_error = RwSignal::new(None::<String>);
    let dragged_nav_id = RwSignal::new(None::<String>);

    view! {
        <Panel title="Navigation layout & pinned favorites">
            <p class="text-sm opacity-70">
                "Drag navigation items to reorder them or drop them into Left rail, Right rail, Expandable submenu, or Hidden."
            </p>

            <div class="flex flex-wrap items-end justify-between gap-3">
                <label class="flex min-w-[14rem] flex-1 flex-col gap-1">
                    <span class="text-xs opacity-70">"Filter navigation items"</span>
                    <input
                        type="search"
                        class="input input-bordered input-sm w-full"
                        placeholder="Filter by label or path..."
                        prop:value=move || filter_text.get()
                        on:input=move |ev| filter_text.set(event_target_value(&ev))
                    />
                </label>
                <label class="flex w-44 flex-col gap-1">
                    <span class="text-xs opacity-70">"Submenu title"</span>
                    <input
                        type="text"
                        class="input input-bordered input-sm w-full"
                        prop:value=move || style.with(|s| s.nav.submenu_title().to_owned())
                        on:input=move |ev| {
                            let value = event_target_value(&ev);
                            style.update(|s| s.nav.submenu_label = Some(value));
                        }
                    />
                </label>
            </div>

            <form
                class="flex flex-wrap items-end gap-2 rounded-box bg-base-200 p-3"
                on:submit=move |ev| {
                    ev.prevent_default();
                    let label = fav_label.get_untracked();
                    let path = fav_path.get_untracked();
                    let target = fav_placement.get_untracked();
                    let mut result = Ok(());
                    style.update(|s| {
                        result = s.nav.pin_favorite(&label, &path, target);
                    });
                    match result {
                        Ok(()) => {
                            fav_label.set(String::new());
                            fav_error.set(None);
                        }
                        Err(msg) => fav_error.set(Some(msg.to_owned())),
                    }
                }
            >
                <label class="flex min-w-[10rem] flex-1 flex-col gap-1">
                    <span class="text-xs opacity-70">"Favorite label"</span>
                    <input
                        type="text"
                        class="input input-bordered input-xs w-full"
                        placeholder="Bookmarks, Favourites, Lists..."
                        prop:value=move || fav_label.get()
                        on:input=move |ev| fav_label.set(event_target_value(&ev))
                    />
                </label>
                <label class="flex min-w-[10rem] flex-1 flex-col gap-1">
                    <span class="text-xs opacity-70">"Favorite path"</span>
                    <input
                        type="text"
                        class="input input-bordered input-xs w-full"
                        placeholder="/saved/bookmarks"
                        prop:value=move || fav_path.get()
                        on:input=move |ev| fav_path.set(event_target_value(&ev))
                    />
                </label>
                <label class="flex flex-col gap-1">
                    <span class="text-xs opacity-70">"Place in"</span>
                    <select
                        class="select select-bordered select-xs"
                        on:change=move |ev| {
                            let next = match event_target_value(&ev).as_str() {
                                "right" => NavPlacement::Right,
                                "submenu" => NavPlacement::Submenu,
                                _ => NavPlacement::Left,
                            };
                            fav_placement.set(next);
                        }
                    >
                        <option value="left">"Left rail"</option>
                        <option value="right">"Right rail"</option>
                        <option value="submenu">"Expandable submenu"</option>
                    </select>
                </label>
                <button type="submit" class="btn btn-outline btn-xs">
                    "Pin favorite"
                </button>
            </form>
            <Show when=move || fav_error.get().is_some()>
                <p class="text-xs text-error">{move || fav_error.get().unwrap_or_default()}</p>
            </Show>

            <div class="grid grid-cols-1 gap-3 lg:grid-cols-2">
                {NavPlacement::ALL
                    .into_iter()
                    .map(|placement| {
                        view! {
                            <NavPlacementBucket
                                placement=placement
                                style=style
                                filter_text=filter_text.read_only()
                                dragged_nav_id=dragged_nav_id
                            />
                        }
                    })
                    .collect_view()}
            </div>

            <div class="card-actions mt-2 flex-wrap items-center justify-between gap-2">
                <div class="flex flex-wrap items-center gap-2">
                    <button
                        type="button"
                        class="btn btn-primary btn-sm"
                        disabled=move || saving.get() || on_save.is_none()
                        on:click=move |_| {
                            if let Some(on_save) = on_save {
                                on_save.run(style.get_untracked());
                            }
                        }
                    >
                        "Save styling & layout"
                    </button>
                    <button
                        type="button"
                        class="btn btn-ghost btn-sm"
                        disabled=move || saving.get()
                        on:click=move |_| {
                            let reset = SessionStyle::default();
                            style.set(reset);
                            if let Some(on_reset) = on_reset {
                                on_reset.run(());
                            } else {
                                let _ = NavLayout::default();
                            }
                        }
                    >
                        "Reset to default"
                    </button>
                </div>
                <div class="text-xs">
                    <Show when=move || message.get().is_some()>
                        <span class="text-success">{move || message.get().unwrap_or_default()}</span>
                    </Show>
                    <Show when=move || error.get().is_some()>
                        <span class="text-error">{move || error.get().unwrap_or_default()}</span>
                    </Show>
                </div>
            </div>
        </Panel>
    }
}

/// Complete Styling & Navigation layout editor screen.
#[component]
pub fn StylingPage(
    style: RwSignal<SessionStyle>,
    #[prop(into)] saving: Signal<bool>,
    #[prop(into)] message: Signal<Option<String>>,
    #[prop(into)] error: Signal<Option<String>>,
    #[prop(optional_no_strip)] on_save: Option<Callback<SessionStyle>>,
    #[prop(optional_no_strip)] on_reset: Option<Callback<()>>,
    #[prop(optional)] upload_zone: Option<UploadSlotRenderer>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col gap-4">
            <StylingPreview style=style/>

            <Panel title="Session images & split banners">
                <p class="text-sm opacity-70">
                    "Upload or drop images into the top, left, or right session regions. Split any banner side-by-side or stacked with up to four slots."
                </p>
                <div class="flex flex-col gap-3">
                    {BannerRegionKind::ALL
                        .into_iter()
                        .map(|kind| {
                            view! {
                                <RegionEditor
                                    kind=kind
                                    style=style
                                    upload_zone=upload_zone.clone()
                                />
                            }
                        })
                        .collect_view()}
                </div>
            </Panel>

            <NavLayoutEditor
                style=style
                saving=saving
                message=message
                error=error
                on_save=on_save
                on_reset=on_reset
            />
        </div>
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn styling_page_renders_preview_regions_and_nav_zones() {
        let mut initial = SessionStyle::default();
        initial.top.split = SplitDirection::Vertical;
        initial
            .top
            .slots
            .push(ImageSlot::new("https://cdn.example/hero-a.png", Some("Hero A"), None).unwrap());
        initial.nav.move_item_to("lists", NavPlacement::Right);
        initial.nav.move_item_to("scheduled", NavPlacement::Submenu);

        let html = view! {
            <StylingPage
                style=RwSignal::new(initial)
                saving=Signal::derive(|| false)
                message=Signal::derive(|| None::<String>)
                error=Signal::derive(|| None::<String>)
            />
        }
        .to_html();

        assert!(html.contains("data-styling-preview"));
        assert!(html.contains("https://cdn.example/hero-a.png"));
        assert!(html.contains("nodus-top-banner"));
        assert!(html.contains("nodus-left-image"));
        assert!(html.contains("nodus-right-image"));
        assert!(html.contains("Pin favorite"));
        assert!(html.contains("Expandable submenu"));
    }

    #[test]
    fn style_draft_snapshot_hydrates_async_refresh_unless_locally_edited() {
        let alice_key = Some(String::from("alpha.example::1"));
        let bob_key = Some(String::from("alpha.example::2"));
        let initial = SessionStyle::default();

        // Initial mount always applies active style.
        assert!(StyleDraftSnapshot::should_apply_update(
            None, &alice_key, &initial
        ));
        let mut prev = StyleDraftSnapshot {
            account_key: alice_key.clone(),
            synced: initial.clone(),
        };

        // Background refresh arrives for the same account while draft is still untouched: must update draft.
        assert!(StyleDraftSnapshot::should_apply_update(
            Some(&prev),
            &alice_key,
            &initial
        ));
        let mut server_style = SessionStyle::default();
        server_style
            .top
            .slots
            .push(ImageSlot::new("https://cdn.example/saved.png", None, None).unwrap());
        prev.synced = server_style.clone();
        let mut draft = server_style.clone();

        // User makes a local unsaved edit; a subsequent refresh must preserve the local draft.
        draft.nav.move_item_to("lists", NavPlacement::Right);
        assert!(!StyleDraftSnapshot::should_apply_update(
            Some(&prev),
            &alice_key,
            &draft
        ));

        // Switching accounts resets the draft even when there were unsaved local edits.
        assert!(StyleDraftSnapshot::should_apply_update(
            Some(&prev),
            &bob_key,
            &draft
        ));
    }
}
