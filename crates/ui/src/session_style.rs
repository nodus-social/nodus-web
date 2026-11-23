//! Per-account custom session styling and navigation layout.
//!
//! Serializes into the four fine-grained `nodus-backend` client metadata keys
//! (`nodus-top-banner`, `nodus-left-image`, `nodus-right-image`, `nodus-nav-layout`)
//! and is shared identically by SSR (`nodus-web-server`), CSR (`nodus-web-client`),
//! and UI/page components (`nodus-ui`, `nodus-pages`).

use std::collections::{BTreeMap, HashSet};

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::components::icons::Icon;
use crate::components::rail::icon_of;
use crate::html::safe_url;

/// Metadata key for the top session banner (single or split image slots).
pub const KEY_TOP_BANNER: &str = "nodus-top-banner";
/// Metadata key for the left sidebar session image(s).
pub const KEY_LEFT_IMAGE: &str = "nodus-left-image";
/// Metadata key for the right sidebar session image(s).
pub const KEY_RIGHT_IMAGE: &str = "nodus-right-image";
/// Metadata key for the custom navigation layout configuration.
pub const KEY_NAV_LAYOUT: &str = "nodus-nav-layout";

/// Metadata key for the right rail's card list.
pub const KEY_RAIL_MODULES: &str = "nodus-rail-modules";

/// Where the staff doors sit, so an account that moved them gets them back.
pub const KEY_STAFF_RAIL: &str = "nodus-staff-rail";

/// Reads a stored rail name, treating anything unexpected as the left rail the doors start on.
fn parse_rail_side(value: Option<&Value>) -> ModuleRail {
    match value.and_then(Value::as_str) {
        Some("right") => ModuleRail::Right,
        _ => ModuleRail::Left,
    }
}

/// All fine-grained metadata keys managed by Nodus Web for session styling.
pub const STYLE_KEYS: [&str; 6] = [
    KEY_TOP_BANNER,
    KEY_LEFT_IMAGE,
    KEY_RIGHT_IMAGE,
    KEY_NAV_LAYOUT,
    KEY_RAIL_MODULES,
    KEY_STAFF_RAIL,
];

/// Maximum number of image slots allowed inside one banner region.
pub const MAX_SLOTS_PER_REGION: usize = 4;
/// Maximum number of navigation items allowed in a custom layout.
pub const MAX_NAV_ITEMS: usize = 24;
/// Maximum number of category cards the rail's navigation may hold.
pub const MAX_NAV_RAILS: usize = 6;

/// Entries a reader may not take off the rail.
///
/// Settings is the way back to every other control on the site, so a layout that hides it
/// is a layout with no way out of itself. Everything else on the rail is taste.
pub const REQUIRED_NAV_ITEMS: [&str; 1] = ["settings"];

/// Whether this entry has to stay placed.
#[must_use]
pub fn is_required_nav_item(id: &str) -> bool {
    REQUIRED_NAV_ITEMS.contains(&id)
}
/// Longest description a banner picture may carry.
///
/// Public because the editor checks typed text against it and has to say which limit it
/// refused: cutting a description short would leave a picture described by half a sentence.
pub const MAX_ALT_LEN: usize = 200;
const MAX_LABEL_LEN: usize = 48;
const MAX_ID_LEN: usize = 48;

/// Which region of the application shell a banner belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BannerRegionKind {
    Top,
    Left,
    Right,
}

impl BannerRegionKind {
    /// All three banner regions in display order.
    pub const ALL: [Self; 3] = [Self::Top, Self::Left, Self::Right];

    /// Human-readable title for the region.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Top => "Top banner",
            Self::Left => "Left rail image",
            Self::Right => "Right rail image",
        }
    }

    /// Fine-grained `nodus-backend` metadata key for this region.
    #[must_use]
    pub const fn metadata_key(self) -> &'static str {
        match self {
            Self::Top => KEY_TOP_BANNER,
            Self::Left => KEY_LEFT_IMAGE,
            Self::Right => KEY_RIGHT_IMAGE,
        }
    }
}

/// How multiple image slots inside a banner region are arranged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    /// Place slots side-by-side in a row.
    #[default]
    Horizontal,
    /// Stack slots vertically.
    Vertical,
}

/// What a picture pick is for: a region that wants one, or a placed picture being replaced.
///
/// The pick travels through the browser's file dialog, so it has to carry the destination
/// itself: a replacement that appended a second picture would be a different edit from the
/// one the reader asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureTarget {
    /// Add a picture to the region.
    Region(BannerRegionKind),
    /// Put a different picture where this one is, keeping its place in the split.
    Slot(BannerRegionKind, usize),
}

impl SplitDirection {
    /// Human-readable label for the split mode.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "Side-by-side",
            Self::Vertical => "Stacked",
        }
    }
}

/// One image slot inside a session banner region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSlot {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_id: Option<String>,
}

impl ImageSlot {
    /// Construct a sanitized image slot, returning `None` if `url` is unsafe or empty.
    #[must_use]
    pub fn new(url: &str, alt: Option<&str>, media_id: Option<&str>) -> Option<Self> {
        let trimmed = url.trim();
        if trimmed.is_empty() || !safe_url(trimmed) {
            return None;
        }
        Some(Self {
            url: trimmed.to_owned(),
            alt: alt
                .map(|text| sanitize_text(text, MAX_ALT_LEN))
                .filter(|text| !text.is_empty()),
            media_id: media_id
                .map(|id| sanitize_text(id, MAX_ID_LEN))
                .filter(|id| !id.is_empty()),
        })
    }
}

/// Images and split orientation for one session banner region.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BannerRegion {
    #[serde(default)]
    pub split: SplitDirection,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<ImageSlot>,
    /// Whether the region is hidden while keeping its pictures.
    ///
    /// Hiding and removing are different verbs: a reader tidying the site hides a banner and
    /// expects to bring it back, so the pictures have to survive the round trip.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
}

impl BannerRegion {
    /// Whether this region currently has no valid image slots.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Whether the region holds pictures but is asked not to draw them.
    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        self.hidden && !self.slots.is_empty()
    }

    /// Show or hide the region, keeping its slots.
    pub const fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
    }

    /// First image URL in this region, used for previews and account thumbnails.
    #[must_use]
    pub fn first_url(&self) -> Option<&str> {
        self.slots.first().map(|slot| slot.url.as_str())
    }

    /// Filter out unsafe URLs, sanitize metadata fields, and enforce slot limits.
    pub fn sanitize(&mut self) {
        self.slots = self
            .slots
            .iter()
            .filter_map(|slot| {
                ImageSlot::new(&slot.url, slot.alt.as_deref(), slot.media_id.as_deref())
            })
            .take(MAX_SLOTS_PER_REGION)
            .collect();
    }

    /// Append a sanitized image slot if the region is below `MAX_SLOTS_PER_REGION`.
    pub fn add_slot(&mut self, slot: ImageSlot) -> bool {
        if self.slots.len() >= MAX_SLOTS_PER_REGION {
            return false;
        }
        let Some(clean) = ImageSlot::new(
            slot.url.as_str(),
            slot.alt.as_deref(),
            slot.media_id.as_deref(),
        ) else {
            return false;
        };
        drop(slot);
        self.slots.push(clean);
        true
    }

    /// Replace a slot at `index` or append if `index >= self.slots.len()`.
    pub fn set_slot(&mut self, index: usize, slot: ImageSlot) -> bool {
        let Some(clean) = ImageSlot::new(
            slot.url.as_str(),
            slot.alt.as_deref(),
            slot.media_id.as_deref(),
        ) else {
            return false;
        };
        drop(slot);
        if index < self.slots.len() {
            self.slots[index] = clean;
            true
        } else {
            self.add_slot(clean)
        }
    }

    /// Put `slot` where the picture at `index` is; `false` if that place is gone.
    ///
    /// Replacement is not addition: the region keeps its length and the new picture takes
    /// the old one's place in the split.
    pub fn replace_slot(&mut self, index: usize, slot: ImageSlot) -> bool {
        if index >= self.slots.len() {
            return false;
        }
        self.set_slot(index, slot)
    }

    /// Set the description of one picture, or clear it.
    ///
    /// Empty is a real answer — a decorative banner is drawn with `alt=""` — so an empty
    /// string stores `None`. Text past [`MAX_ALT_LEN`] is refused instead of cut: a
    /// description that silently lost its end is worse than no description at all.
    pub fn set_alt(&mut self, index: usize, alt: &str) -> Result<(), String> {
        let clean: String = alt
            .chars()
            .filter(|ch| !ch.is_control())
            .collect::<String>()
            .trim()
            .to_owned();
        let length = clean.chars().count();
        if length > MAX_ALT_LEN {
            return Err(format!(
                "Alt text is {length} characters; the limit is {MAX_ALT_LEN}. Shorten it — nothing was changed."
            ));
        }
        let Some(slot) = self.slots.get_mut(index) else {
            return Err(String::from("That picture is no longer on the page."));
        };
        slot.alt = (!clean.is_empty()).then_some(clean);
        Ok(())
    }

    /// Remove the slot at `index`, if present.
    pub fn remove_slot(&mut self, index: usize) {
        if index < self.slots.len() {
            self.slots.remove(index);
        }
    }

    /// Move a slot from `from` to `to`.
    pub fn move_slot(&mut self, from: usize, to: usize) {
        if from >= self.slots.len() || to >= self.slots.len() || from == to {
            return;
        }
        let item = self.slots.remove(from);
        self.slots.insert(to, item);
    }
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if hands the field over by reference"
)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// Where a navigation item is placed in the shell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavPlacement {
    #[default]
    Left,
    Right,
    Submenu,
    Hidden,
}

impl NavPlacement {
    /// Placements shown to users as destinations for moving navigation items.
    pub const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Submenu, Self::Hidden];

    /// Human-readable label for this placement.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "Left rail",
            Self::Right => "Right rail",
            Self::Submenu => "Expandable submenu",
            Self::Hidden => "Hidden",
        }
    }
}

/// One category card of the rail's navigation.
///
/// Categories are the reader's, not the app's: the default layout ships three (Read,
/// Find, Library) because a dozen links in one column is a wall, but a reader who wants
/// one long list, or five small ones, can say so. Items name their rail by id, so moving
/// an entry between cards never renumbers anything else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavRail {
    pub id: String,
    pub label: String,
    /// Which column the card sits in. Categories are rail furniture like cards are, so a
    /// reader can keep navigation on the left and everything else on the right, or the
    /// other way round.
    #[serde(default)]
    pub rail: ModuleRail,
}

impl NavRail {
    /// A category with an already-unique id.
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            rail: ModuleRail::Left,
        }
    }

    /// Id derived from a human label, so a new card arrives with a stable key.
    #[must_use]
    pub fn id_for(label: &str) -> String {
        let mut id = String::new();
        for ch in label.chars().take(MAX_ID_LEN) {
            if ch.is_ascii_alphanumeric() {
                id.push(ch.to_ascii_lowercase());
            } else if !id.is_empty() && !id.ends_with('-') {
                id.push('-');
            }
        }
        let trimmed = id.trim_matches('-').to_owned();
        if trimmed.is_empty() {
            "rail".to_owned()
        } else {
            trimmed
        }
    }
}

/// One navigation item in the account's custom layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavItemConfig {
    pub id: String,
    pub label: String,
    pub path: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub placement: NavPlacement,
    /// Id of the [`NavRail`] card this entry is filed under; empty means unfiled.
    #[serde(default)]
    pub rail: String,
}

impl NavItemConfig {
    /// Icon for this item, favoring `Star` for custom pinned entries whose label
    /// does not match a built-in destination.
    #[must_use]
    pub fn icon(&self) -> Icon {
        if self.pinned && matches!(icon_of(&self.label), Icon::Settings) && self.label != "Settings"
        {
            Icon::Star
        } else {
            icon_of(&self.label)
        }
    }
}

/// Per-account navigation ordering, pinning, placement, and rail categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavLayout {
    #[serde(default = "default_nav_items")]
    pub items: Vec<NavItemConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submenu_label: Option<String>,
    /// The rail's category cards, in display order.
    #[serde(default = "default_nav_rails")]
    pub rails: Vec<NavRail>,
}

/// The category cards a layout starts with.
#[must_use]
pub fn default_nav_rails() -> Vec<NavRail> {
    vec![
        NavRail::new("read", "Read"),
        NavRail::new("find", "Find"),
        NavRail::new("library", "Library"),
    ]
}

impl Default for NavLayout {
    fn default() -> Self {
        let mut layout = Self {
            items: default_nav_items(),
            submenu_label: Some(String::from("More")),
            rails: default_nav_rails(),
        };
        layout.normalize_rails();
        layout
    }
}

impl NavLayout {
    /// Drops categories with no id or a repeated one, gives them usable labels, and files
    /// every orphaned item under the first card — so a layout written before categories
    /// existed still opens with its links where the reader left them.
    fn normalize_rails(&mut self) {
        let mut seen = HashSet::new();
        self.rails
            .retain(|rail| !rail.id.trim().is_empty() && seen.insert(rail.id.trim().to_owned()));
        for rail in &mut self.rails {
            rail.id = rail.id.trim().to_owned();
            let label = rail.label.trim();
            rail.label = if label.is_empty() {
                rail.id.clone()
            } else {
                label.chars().take(MAX_LABEL_LEN).collect()
            };
        }
        self.rails.truncate(MAX_NAV_RAILS);
        if self.rails.is_empty() {
            self.rails = default_nav_rails();
        }
        let known = self
            .rails
            .iter()
            .map(|rail| rail.id.clone())
            .collect::<HashSet<_>>();
        let first = self
            .rails
            .first()
            .map(|rail| rail.id.clone())
            .unwrap_or_default();
        for item in &mut self.items {
            if item.rail.trim().is_empty() || !known.contains(item.rail.trim()) {
                item.rail.clone_from(&first);
            } else {
                item.rail = item.rail.trim().to_owned();
            }
        }
    }

    /// Id of the category card an item is filed under.
    #[must_use]
    pub fn rail_of(&self, item: &NavItemConfig) -> Option<&NavRail> {
        self.rails.iter().find(|rail| rail.id == item.rail)
    }

    /// Every category in display order with its left-rail entries, pinned ones first.
    ///
    /// Cards with nothing in them are included: the editor has to draw the one a reader
    /// has just added, and hiding it would look like the button did nothing.
    #[must_use]
    pub fn left_rails(&self) -> Vec<(&NavRail, Vec<&NavItemConfig>)> {
        self.rails
            .iter()
            .map(|rail| (rail, self.items_of_rail(rail.id.as_str())))
            .collect()
    }

    /// Left-rail entries filed under `rail_id`, pinned ones first.
    #[must_use]
    pub fn items_of_rail(&self, rail_id: &str) -> Vec<&NavItemConfig> {
        let mut pinned = Vec::new();
        let mut regular = Vec::new();
        for item in &self.items {
            if item.placement != NavPlacement::Left || item.rail != rail_id {
                continue;
            }
            if item.pinned {
                pinned.push(item);
            } else {
                regular.push(item);
            }
        }
        pinned.extend(regular);
        pinned
    }

    /// Adds a category card and returns its id (or `None` when the list is full).
    pub fn add_rail(&mut self, label: &str) -> Option<String> {
        if self.rails.len() >= MAX_NAV_RAILS {
            return None;
        }
        let label = label.trim();
        let base = if label.is_empty() {
            "New rail".to_owned()
        } else {
            label.chars().take(MAX_LABEL_LEN).collect()
        };
        let mut id = NavRail::id_for(&base);
        let mut suffix = 2;
        while self.rails.iter().any(|rail| rail.id == id) {
            id = format!("{}-{suffix}", NavRail::id_for(&base));
            suffix += 1;
        }
        self.rails.push(NavRail::new(id.clone(), base));
        Some(id)
    }

    /// Removes a category card, keeping its entries under the next card that remains.
    ///
    /// Deleting a card is a statement about the card, not about the links in it, so the
    /// entries move instead of disappearing.
    pub fn remove_rail(&mut self, rail_id: &str) -> bool {
        if self.rails.len() <= 1 {
            return false;
        }
        let Some(index) = self.rails.iter().position(|rail| rail.id == rail_id) else {
            return false;
        };
        self.rails.remove(index);
        let fallback = self
            .rails
            .first()
            .map(|rail| rail.id.clone())
            .unwrap_or_default();
        for item in &mut self.items {
            if item.rail == rail_id {
                item.rail.clone_from(&fallback);
            }
        }
        true
    }

    /// Renames a category card; blank labels fall back to the id.
    pub fn rename_rail(&mut self, rail_id: &str, label: &str) {
        let trimmed = label.trim();
        if let Some(rail) = self.rails.iter_mut().find(|rail| rail.id == rail_id) {
            rail.label = if trimmed.is_empty() {
                rail.id.clone()
            } else {
                trimmed.chars().take(MAX_LABEL_LEN).collect()
            };
        }
    }

    /// Files an entry under another category card.
    pub fn set_item_rail(&mut self, item_id: &str, rail_id: &str) {
        if !self.rails.iter().any(|rail| rail.id == rail_id) {
            return;
        }
        if let Some(item) = self.items.iter_mut().find(|item| item.id == item_id) {
            rail_id.clone_into(&mut item.rail);
        }
    }

    /// The category cards one rail holds, in display order.
    #[must_use]
    pub fn rails_in(&self, rail: ModuleRail) -> Vec<&NavRail> {
        self.rails.iter().filter(|held| held.rail == rail).collect()
    }

    /// Adds a category card to `rail` and returns its id.
    pub fn add_rail_in(&mut self, label: &str, rail: ModuleRail) -> Option<String> {
        let id = self.add_rail(label)?;
        self.set_rail_side(&id, rail);
        Some(id)
    }

    /// Puts a category card in `rail`, keeping its entries and its place.
    pub fn set_rail_side(&mut self, rail_id: &str, rail: ModuleRail) -> bool {
        let Some(held) = self.rails.iter_mut().find(|held| held.id == rail_id) else {
            return false;
        };
        if held.rail == rail {
            return false;
        }
        held.rail = rail;
        true
    }

    /// Moves a category card earlier (`forward == false`) or later in the column.
    pub fn move_rail(&mut self, rail_id: &str, forward: bool) -> bool {
        let Some(index) = self.rails.iter().position(|rail| rail.id == rail_id) else {
            return false;
        };
        let target = if forward {
            index + 1
        } else {
            index.checked_sub(1).unwrap_or(index)
        };
        if target >= self.rails.len() || target == index {
            return false;
        }
        self.rails.swap(index, target);
        true
    }

    /// Display title for the expandable submenu trigger.
    #[must_use]
    pub fn submenu_title(&self) -> &str {
        self.submenu_label
            .as_deref()
            .filter(|text| !text.trim().is_empty())
            .unwrap_or("More")
    }

    /// Items placed in `placement`, ordered with pinned items first within that region.
    #[must_use]
    pub fn items_in(&self, placement: NavPlacement) -> Vec<&NavItemConfig> {
        let mut pinned = Vec::new();
        let mut regular = Vec::new();
        for item in &self.items {
            if item.placement != placement {
                continue;
            }
            if item.pinned {
                pinned.push(item);
            } else {
                regular.push(item);
            }
        }
        pinned.extend(regular);
        pinned
    }

    /// Move `id` to `placement`.
    pub fn move_item_to(&mut self, id: &str, placement: NavPlacement) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.placement = placement;
        }
    }

    /// Toggle whether `id` is pinned at the front of its placement.
    pub fn toggle_pin(&mut self, id: &str) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.pinned = !item.pinned;
        }
    }

    /// Move `dragged_id` onto the position of `target_id` (and adopt `target_id`'s placement).
    pub fn reorder_before(&mut self, dragged_id: &str, target_id: &str) {
        if dragged_id == target_id {
            return;
        }
        let Some(from_idx) = self.items.iter().position(|item| item.id == dragged_id) else {
            return;
        };
        let Some(target_idx) = self.items.iter().position(|item| item.id == target_id) else {
            return;
        };
        let target_placement = self.items[target_idx].placement;
        let mut dragged = self.items.remove(from_idx);
        dragged.placement = target_placement;
        let insert_idx = if from_idx < target_idx {
            target_idx - 1
        } else {
            target_idx
        };
        self.items.insert(insert_idx.min(self.items.len()), dragged);
    }

    /// Shift `id` earlier (`delta < 0`) or later (`delta > 0`) among items in the same placement.
    pub fn shift_in_placement(&mut self, id: &str, forward: bool) {
        let Some(idx) = self.items.iter().position(|item| item.id == id) else {
            return;
        };
        let placement = self.items[idx].placement;
        let peer_idx = if forward {
            ((idx + 1)..self.items.len()).find(|&pos| self.items[pos].placement == placement)
        } else {
            (0..idx).rfind(|&pos| self.items[pos].placement == placement)
        };
        if let Some(other) = peer_idx {
            self.items.swap(idx, other);
        }
    }

    /// Pin a custom favorite destination directly into navigation (or update/pin an existing path).
    pub fn pin_favorite(
        &mut self,
        label: &str,
        path: &str,
        placement: NavPlacement,
    ) -> Result<(), &'static str> {
        let clean_label = sanitize_text(label, MAX_LABEL_LEN);
        if clean_label.is_empty() {
            return Err("Favorite label cannot be empty.");
        }
        let trimmed_path = path.trim();
        if !safe_app_path(trimmed_path) {
            return Err("Favorite path must start with `/` and contain no spaces or `//`.");
        }
        if let Some(existing) = self.items.iter_mut().find(|item| item.path == trimmed_path) {
            existing.label = clean_label;
            existing.pinned = true;
            existing.placement = if placement == NavPlacement::Hidden {
                NavPlacement::Left
            } else {
                placement
            };
            return Ok(());
        }
        if self.items.len() >= MAX_NAV_ITEMS {
            return Err("Maximum number of navigation items reached.");
        }
        let base_id = slugify_id(&clean_label);
        let mut id = format!("fav-{base_id}");
        let mut suffix = 2_usize;
        while self.items.iter().any(|item| item.id == id) {
            id = format!("fav-{base_id}-{suffix}");
            suffix += 1;
        }
        self.items.push(NavItemConfig {
            id,
            label: clean_label,
            path: trimmed_path.to_owned(),
            pinned: true,
            placement: if placement == NavPlacement::Hidden {
                NavPlacement::Left
            } else {
                placement
            },
            rail: self
                .rails
                .first()
                .map_or_else(|| String::from("read"), |rail| rail.id.clone()),
        });
        Ok(())
    }

    /// Remove a custom favorite item (items not in the default rail).
    pub fn remove_item(&mut self, id: &str) {
        if !default_nav_items().iter().any(|item| item.id == id) {
            self.items.retain(|item| item.id != id);
        }
    }

    /// Validate and clean all navigation items, preserving safe ordering.
    pub fn sanitize(&mut self) {
        let mut seen = HashSet::new();
        let mut clean = Vec::new();
        for item in self.items.drain(..) {
            let id = sanitize_id(&item.id);
            let label = sanitize_text(&item.label, MAX_LABEL_LEN);
            let path = item.path.trim().to_owned();
            if id.is_empty() || label.is_empty() || !safe_app_path(&path) {
                continue;
            }
            if !seen.insert(id.clone()) {
                continue;
            }
            clean.push(NavItemConfig {
                id,
                label,
                path,
                pinned: item.pinned,
                placement: item.placement,
                rail: item.rail,
            });
            if clean.len() >= MAX_NAV_ITEMS {
                break;
            }
        }
        if clean.is_empty() {
            clean = default_nav_items();
        }
        self.items = clean;
        self.submenu_label = self
            .submenu_label
            .as_deref()
            .map(|label| sanitize_text(label, MAX_LABEL_LEN))
            .filter(|label| !label.is_empty())
            .or_else(|| Some(String::from("More")));
    }
}

/// Default navigation items mirroring the standard Nodus rail.
#[must_use]
pub fn default_nav_items() -> Vec<NavItemConfig> {
    [
        ("home", "Home", "/home"),
        ("messages", "Messages", "/messages"),
        ("notifications", "Notifications", "/notifications"),
        ("search", "Search", "/search"),
        ("people", "People", "/directory"),
        ("saved", "Saved", "/saved/favourites"),
        ("scheduled", "Scheduled", "/scheduled"),
        ("lists", "Lists", "/lists"),
        ("settings", "Settings", "/settings/account"),
    ]
    .into_iter()
    .map(|(id, label, path)| NavItemConfig {
        id: id.to_owned(),
        label: label.to_owned(),
        path: path.to_owned(),
        pinned: false,
        placement: NavPlacement::Left,
        rail: default_rail_for(id).to_owned(),
    })
    .collect()
}

/// Which starter category a default destination belongs to.
fn default_rail_for(id: &str) -> &'static str {
    match id {
        "home" | "messages" | "notifications" => "read",
        "search" | "people" | "saved" => "find",
        _ => "library",
    }
}

/// One card a reader can place in the right rail.
///
/// The list is a catalogue, not a menu: every entry is a small read of the instance, so a
/// reader builds the column they want out of parts and the ones nobody wants cost nothing
/// because they are not there. The unpersonalised entries are deliberately in the same
/// catalogue as the rest — "what is happening here" and "what your own feed says" are both
/// legitimate things to want above the fold, and only the reader knows which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleKind {
    /// The public timeline, newest first.
    LiveFeed,
    /// The reader's own notifications.
    Notifications,
    /// Tags that people on this instance are using.
    TrendingTags,
    /// Accounts the instance points at.
    TrendingUsers,
    /// Posts the instance is promoting.
    TrendingPosts,
    /// Links pulled out of those posts.
    TrendingLinks,
    /// Accounts the recommender thinks this reader will want.
    RecommendedAccounts,
    /// Posts the recommender picked for this reader.
    RecommendedPosts,
}

impl ModuleKind {
    /// Every card this build can draw, in the order the picker offers them.
    ///
    /// The recommender cards and the link roll are deliberately absent: the client has no
    /// read that answers them yet, and offering a card that can only ever say "nothing here"
    /// is worse than not offering it. The variants stay so a layout written by a build that
    /// has them still parses, and so adding the read is a one-line change here.
    pub const ALL: [Self; 5] = [
        Self::LiveFeed,
        Self::Notifications,
        Self::TrendingTags,
        Self::TrendingUsers,
        Self::TrendingPosts,
    ];

    /// Stable identifier used in the stored layout and in drag targets.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::LiveFeed => "live_feed",
            Self::Notifications => "notifications",
            Self::TrendingTags => "trending_tags",
            Self::TrendingUsers => "trending_users",
            Self::TrendingPosts => "trending_posts",
            Self::TrendingLinks => "trending_links",
            Self::RecommendedAccounts => "recommended_accounts",
            Self::RecommendedPosts => "recommended_posts",
        }
    }

    /// The card's title.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::LiveFeed => "Live feed",
            Self::Notifications => "Notifications",
            Self::TrendingTags => "Trending tags",
            Self::TrendingUsers => "Trending people",
            Self::TrendingPosts => "Trending posts",
            Self::TrendingLinks => "Trending links",
            Self::RecommendedAccounts => "Suggested by the recommender",
            Self::RecommendedPosts => "Picked by the recommender",
        }
    }

    /// What the card shows, in one line, for the picker.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::LiveFeed => "Everything posted publicly, as it arrives",
            Self::Notifications => "Mentions, replies, boosts and follows aimed at you",
            Self::TrendingTags => "Hashtags gaining users this week",
            Self::TrendingUsers => "Accounts this instance is pointing at",
            Self::TrendingPosts => "Posts with the most replies, boosts and favourites",
            Self::TrendingLinks => "Links shared the most in those posts",
            Self::RecommendedAccounts => "Who your own reading suggests you follow",
            Self::RecommendedPosts => "From the people you follow, ranked for you",
        }
    }

    /// Whether the card needs the recommender rather than the instance API.
    #[must_use]
    pub const fn nods_to_recommender(self) -> bool {
        matches!(self, Self::RecommendedAccounts | Self::RecommendedPosts)
    }

    /// The kind behind a stored identifier.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }
}

/// Which column a card sits in.
///
/// Cards are rail furniture, not page furniture: a reader who wants the trends beside the
/// navigation puts them on the left, and one who wants them out of the reading column's way
/// puts them on the right. The rail is chosen per card so the two columns can be as
/// different as the reader's attention is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleRail {
    /// The right-hand column, where cards start.
    #[default]
    Right,
    /// The left rail, under the navigation.
    Left,
}

impl ModuleRail {
    /// Stable identifier used in the stored layout.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
        }
    }

    /// The rail as a reader would name it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Right => "right rail",
            Self::Left => "left rail",
        }
    }
}

/// One card and the column it lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlacedModule {
    /// What the card draws.
    pub kind: ModuleKind,
    /// Which rail holds it.
    #[serde(default)]
    pub rail: ModuleRail,
}

impl PlacedModule {
    /// A card in the right rail, which is where a card starts.
    #[must_use]
    pub const fn new(kind: ModuleKind) -> Self {
        Self {
            kind,
            rail: ModuleRail::Right,
        }
    }

    /// The same card in `rail`.
    #[must_use]
    pub const fn in_rail(kind: ModuleKind, rail: ModuleRail) -> Self {
        Self { kind, rail }
    }
}

/// The cards a reader starts with: the two that need no permission and no taste to be
/// useful on a first visit.
#[must_use]
pub fn default_modules() -> Vec<PlacedModule> {
    // Nothing is on until the reader asks for it: a column of trending cards is a decision
    // about what the site is for, and a new account has not made it.
    Vec::new()
}

/// Full per-account session styling and navigation layout state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStyle {
    #[serde(default)]
    pub top: BannerRegion,
    #[serde(default)]
    pub left: BannerRegion,
    #[serde(default)]
    pub right: BannerRegion,
    #[serde(default)]
    pub nav: NavLayout,
    /// Cards and the rail each one lives in, in the reader's order.
    #[serde(default = "default_modules")]
    pub modules: Vec<PlacedModule>,
    /// Which column the staff doors sit in. They are furniture like everything else, so
    /// they move like everything else.
    #[serde(default)]
    pub staff_rail: ModuleRail,
}

impl Default for SessionStyle {
    fn default() -> Self {
        Self {
            top: BannerRegion::default(),
            left: BannerRegion::default(),
            right: BannerRegion::default(),
            nav: NavLayout::default(),
            modules: default_modules(),
            staff_rail: ModuleRail::Left,
        }
    }
}

impl SessionStyle {
    /// Access a banner region by kind.
    #[must_use]
    pub const fn region(&self, kind: BannerRegionKind) -> &BannerRegion {
        match kind {
            BannerRegionKind::Top => &self.top,
            BannerRegionKind::Left => &self.left,
            BannerRegionKind::Right => &self.right,
        }
    }

    /// Mutably access a banner region by kind.
    pub fn region_mut(&mut self, kind: BannerRegionKind) -> &mut BannerRegion {
        match kind {
            BannerRegionKind::Top => &mut self.top,
            BannerRegionKind::Left => &mut self.left,
            BannerRegionKind::Right => &mut self.right,
        }
    }

    /// The entries a reader may not take off the rail.
    ///
    /// Settings is the way back to every other control on the site, so hiding it leaves a
    /// reader with no way to undo what they just did. Everything else is taste.
    #[must_use]
    pub fn unplaced_required(&self) -> Option<String> {
        let missing: Vec<&str> = self
            .nav
            .items
            .iter()
            .filter(|item| item.placement == NavPlacement::Hidden)
            .filter(|item| is_required_nav_item(&item.id))
            .filter_map(|item| match item.id.as_str() {
                "settings" => Some("Settings"),
                _ => None,
            })
            .collect();
        if missing.is_empty() {
            return None;
        }
        Some(format!(
            "{} has to stay on a rail. Put it back before saving.",
            missing.join(", ")
        ))
    }

    /// Whether a rail already holds this card.
    #[must_use]
    pub fn has_module(&self, kind: ModuleKind) -> bool {
        self.modules.iter().any(|held| held.kind == kind)
    }

    /// Which rail holds this card, if any.
    #[must_use]
    pub fn module_rail(&self, kind: ModuleKind) -> Option<ModuleRail> {
        self.modules
            .iter()
            .find(|held| held.kind == kind)
            .map(|held| held.rail)
    }

    /// The cards one rail holds, in order.
    #[must_use]
    pub fn modules_in(&self, rail: ModuleRail) -> Vec<ModuleKind> {
        self.modules
            .iter()
            .filter(|held| held.rail == rail)
            .map(|held| held.kind)
            .collect()
    }

    /// Add a card at the bottom of `rail`, unless a rail already holds it.
    pub fn add_module(&mut self, kind: ModuleKind, rail: ModuleRail) {
        if !self.has_module(kind) {
            self.modules.push(PlacedModule::in_rail(kind, rail));
        }
    }

    /// Put a card in `rail`, keeping its place if it is already on a rail.
    pub fn set_module_rail(&mut self, kind: ModuleKind, rail: ModuleRail) -> bool {
        match self.modules.iter_mut().find(|held| held.kind == kind) {
            Some(held) => {
                let moved = held.rail != rail;
                held.rail = rail;
                moved
            }
            None => false,
        }
    }

    /// Take a card off its rail, answering whether it was there.
    pub fn remove_module(&mut self, kind: ModuleKind) -> bool {
        match self.modules.iter().position(|held| held.kind == kind) {
            Some(index) => {
                self.modules.remove(index);
                true
            }
            None => false,
        }
    }

    /// Put the cards back to the starting set.
    pub fn reset_modules(&mut self) {
        self.modules = default_modules();
    }

    /// Move `dragged` to `target`'s position in the rail.
    ///
    /// Cards are a stack, not a grid, so a drag only ever reorders: the drop target decides
    /// where the dragged card lands, and everything after it shifts down.
    pub fn reorder_module(&mut self, dragged: ModuleKind, target: ModuleKind) {
        if dragged == target {
            return;
        }
        let (Some(from), Some(to)) = (
            self.modules.iter().position(|held| held.kind == dragged),
            self.modules.iter().position(|held| held.kind == target),
        ) else {
            return;
        };
        // Only two cards in the same rail can swap places: dropping one on a card in the
        // other column is what `set_module_rail` is for, and silently moving it here would
        // take it out of the column the reader dropped it in.
        if self.modules[from].rail != self.modules[to].rail {
            return;
        }
        let moved = self.modules.remove(from);
        let to = if from < to { to - 1 } else { to };
        self.modules.insert(to.min(self.modules.len()), moved);
    }

    /// Primary thumbnail image for account switchers and summaries.
    #[must_use]
    pub fn thumbnail_url(&self) -> Option<&str> {
        self.top
            .first_url()
            .or_else(|| self.left.first_url())
            .or_else(|| self.right.first_url())
    }

    /// Whether this style has no custom images and keeps the default navigation layout.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.top.is_empty()
            && self.left.is_empty()
            && self.right.is_empty()
            && self.nav == NavLayout::default()
            && self.modules == default_modules()
    }

    /// Whether the region of `kind` is hidden with its pictures kept.
    #[must_use]
    pub fn region_is_hidden(&self, kind: BannerRegionKind) -> bool {
        self.region(kind).is_hidden()
    }

    /// Show or hide a region without discarding its pictures.
    pub fn set_region_hidden(&mut self, kind: BannerRegionKind, hidden: bool) {
        self.region_mut(kind).set_hidden(hidden);
    }

    /// Validate all URLs, paths, and limits inside this style.
    pub fn sanitize(&mut self) {
        self.top.sanitize();
        self.left.sanitize();
        self.right.sanitize();
        self.nav.sanitize();
    }

    /// Return a sanitized clone of this style.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.sanitize();
        self
    }

    /// Parse fine-grained `nodus-backend` metadata keys into a sanitized `SessionStyle`.
    #[must_use]
    pub fn from_metadata_map(metadata: &BTreeMap<String, Value>) -> Self {
        let mut style = Self {
            top: parse_region(metadata.get(KEY_TOP_BANNER)),
            left: parse_region(metadata.get(KEY_LEFT_IMAGE)),
            right: parse_region(metadata.get(KEY_RIGHT_IMAGE)),
            nav: parse_nav(metadata.get(KEY_NAV_LAYOUT)),
            modules: parse_modules(metadata.get(KEY_RAIL_MODULES)),
            staff_rail: parse_rail_side(metadata.get(KEY_STAFF_RAIL)),
        };
        style.sanitize();
        style
    }

    /// Serialize into a `PATCH /api/v1/nodus/account/metadata` payload map.
    ///
    /// Regions or layouts back at their default values are emitted as `Value::Null` so
    /// `nodus-backend` removes cleared keys without bloating account metadata.
    #[must_use]
    pub fn to_patch_metadata(&self) -> BTreeMap<String, Value> {
        let clean = self.clone().sanitized();
        let mut out = BTreeMap::new();
        for kind in BannerRegionKind::ALL {
            let region = clean.region(kind);
            let value = if region.is_empty() {
                Value::Null
            } else {
                serde_json::to_value(region).unwrap_or(Value::Null)
            };
            out.insert(kind.metadata_key().to_owned(), value);
        }
        let nav_value = if clean.nav == NavLayout::default() {
            Value::Null
        } else {
            serde_json::to_value(&clean.nav).unwrap_or(Value::Null)
        };
        out.insert(KEY_NAV_LAYOUT.to_owned(), nav_value);
        let modules_value = if clean.modules == default_modules() {
            Value::Null
        } else {
            // `left:trending_tags` is a card in the left rail; a bare id is the right one,
            // which keeps every layout written before cards could move readable.
            serde_json::to_value(
                clean
                    .modules
                    .iter()
                    .map(|held| match held.rail {
                        ModuleRail::Right => held.kind.id().to_owned(),
                        ModuleRail::Left => format!("left:{}", held.kind.id()),
                    })
                    .collect::<Vec<String>>(),
            )
            .unwrap_or(Value::Null)
        };
        out.insert(KEY_RAIL_MODULES.to_owned(), modules_value);
        // The doors start on the left, so only a move needs saying.
        let staff_value = if clean.staff_rail == ModuleRail::Left {
            Value::Null
        } else {
            Value::String(clean.staff_rail.id().to_owned())
        };
        out.insert(KEY_STAFF_RAIL.to_owned(), staff_value);
        out
    }

    /// Serialize for safe embedding inside an HTML `<script type="application/json">` tag.
    #[must_use]
    pub fn to_safe_json_island(&self) -> String {
        serde_json::to_string(&self.clone().sanitized())
            .unwrap_or_else(|_| String::from("{}"))
            .replace("</", "<\\/")
    }
}

fn parse_region(value: Option<&Value>) -> BannerRegion {
    let Some(value) = value else {
        return BannerRegion::default();
    };
    if value.is_null() {
        return BannerRegion::default();
    }
    serde_json::from_value::<BannerRegion>(value.clone()).unwrap_or_default()
}

fn parse_nav(value: Option<&Value>) -> NavLayout {
    let Some(value) = value else {
        return NavLayout::default();
    };
    if value.is_null() {
        return NavLayout::default();
    }
    let mut layout =
        serde_json::from_value::<NavLayout>(value.clone()).unwrap_or_else(|_| NavLayout::default());
    layout.normalize_rails();
    layout
}

/// The cards a stored metadata value names, skipping anything this build does not know.
///
/// Unknown identifiers are dropped rather than rejected: a layout written by a newer client
/// should still open here, minus the cards this one cannot draw.
fn parse_modules(value: Option<&Value>) -> Vec<PlacedModule> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return default_modules();
    };
    let Ok(ids) = serde_json::from_value::<Vec<String>>(value.clone()) else {
        return default_modules();
    };
    let mut modules = Vec::new();
    for id in ids {
        let (rail, name) = match id.split_once(':') {
            Some(("left", name)) => (ModuleRail::Left, name),
            Some(("right", name)) => (ModuleRail::Right, name),
            _ => (ModuleRail::Right, id.as_str()),
        };
        if let Some(kind) = ModuleKind::from_id(name)
            .filter(|kind| !modules.iter().any(|held: &PlacedModule| held.kind == *kind))
        {
            modules.push(PlacedModule::in_rail(kind, rail));
        }
    }
    modules
}

/// Whether `path` is a safe in-app relative route path.
#[must_use]
pub fn safe_app_path(path: &str) -> bool {
    let trimmed = path.trim();
    trimmed.starts_with('/')
        && !trimmed.starts_with("//")
        && !trimmed.starts_with("/\\")
        && trimmed.len() <= 256
        && !trimmed
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
}

fn sanitize_text(input: &str, max_len: usize) -> String {
    input
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(max_len)
        .collect()
}

fn sanitize_id(input: &str) -> String {
    input
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .take(MAX_ID_LEN)
        .collect()
}

fn slugify_id(label: &str) -> String {
    let slug: String = label
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        String::from("item")
    } else {
        trimmed.chars().take(24).collect()
    }
}

/// Live handle an editor uses to change the draft style from inside the painted site.
///
/// The shell stays a painting: it reads a `SessionStyle` and draws it. An editor is the same
/// shell plus this handle, which is why editing happens on the real site rather than on a
/// second copy of it that could drift.
#[derive(Clone, Copy)]
pub struct StyleEditor {
    draft: RwSignal<SessionStyle>,
    active: RwSignal<bool>,
    dragging: RwSignal<Option<Dragged>>,
    /// How a collection picture becomes a slot. A drive file has no public address until
    /// the instance mints one, and only the browser can ask; the shell holds the door open
    /// rather than the address.
    library_placer: RwSignal<Option<Callback<(LibraryPicture, PictureTarget)>>>,
}

/// One picture in the account's own collection, ready to be dragged onto the page.
///
/// `thumbnail` is what the tray draws; `id` names the drive file the drop has to resolve
/// into a permanent address before it can be stored in the style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPicture {
    /// The drive file's id.
    pub id: String,
    /// The file's name, used as the picture's description when it lands.
    pub name: String,
    /// An address the browser can draw on this page only.
    pub thumbnail: String,
}

/// A file dropped on the collection's upload target, already read into memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadRequest {
    /// The file's name.
    pub name: String,
    /// The type the browser reported, which the server checks again.
    pub mime: String,
    /// The file's bytes.
    pub bytes: Vec<u8>,
}

/// What a drag in the editor is carrying.
///
/// One payload type for every movable thing, held by [`StyleEditor`] so every component reads
/// and clears the same signal. A drop handler that does not accept a variant refuses it by
/// type: a picture dragged onto a rail reaches a handler that only knows pictures, so the
/// picture stays where it was instead of landing in the navigation.
///
/// `Library` is the collection's payload, the one thing here that is not already in the
/// draft: a drive picture that has not been placed yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dragged {
    /// A card, by kind.
    Module { kind: ModuleKind },
    /// A navigation entry and the category it came from, if any.
    NavItem { id: String, rail: Option<String> },
    /// A navigation category card, by id and by the rail it came from.
    Category { id: String, rail: ModuleRail },
    /// A picture already in a region.
    Picture {
        region: BannerRegionKind,
        index: usize,
    },
    /// A picture from the collection, not yet placed.
    Library { picture: LibraryPicture },
    /// A brand-new category card, waiting to be dropped on a rail.
    NewCategory,
    /// The staff doors, which are one card whatever they contain.
    Staff,
}

impl std::fmt::Debug for StyleEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Signals have no contents worth printing beyond whether they exist.
        f.debug_struct("StyleEditor")
            .field("active", &self.is_active())
            .finish_non_exhaustive()
    }
}

impl StyleEditor {
    /// Wrap the draft the editor writes into and the switch that turns it on.
    ///
    /// The switch is a signal rather than a flag read once at mount time: the editor is a
    /// mode of the page that is already on screen, so entering and leaving it must repaint
    /// the same components instead of rebuilding them.
    #[must_use]
    pub fn new(draft: RwSignal<SessionStyle>, active: RwSignal<bool>) -> Self {
        Self {
            draft,
            active,
            dragging: RwSignal::new(None),
            library_placer: RwSignal::new(None),
        }
    }

    /// Whether the editor is on, tracked so a `Show`/`Memo` around it repaints on a toggle.
    #[must_use]
    pub fn is_active(self) -> bool {
        self.active.get()
    }

    /// Whether the editor is on right now, without subscribing the caller.
    ///
    /// Used where a decision is made once while building a node (draw this picture with
    /// handles or not), so a later toggle does not leave a stale subscription behind.
    #[must_use]
    pub fn is_active_now(self) -> bool {
        self.active.get_untracked()
    }

    /// The draft signal itself, for controls that need to read it.
    #[must_use]
    pub const fn draft(self) -> RwSignal<SessionStyle> {
        self.draft
    }

    /// Show or hide a region, keeping its pictures.
    pub fn set_region_hidden(self, kind: BannerRegionKind, hidden: bool) {
        self.draft
            .update(|style| style.set_region_hidden(kind, hidden));
    }

    /// Drop one picture from a region.
    pub fn remove_slot(self, kind: BannerRegionKind, index: usize) {
        self.draft
            .update(|style| style.region_mut(kind).remove_slot(index));
    }

    /// Replace one picture, keeping its place in the region.
    pub fn replace_slot(self, kind: BannerRegionKind, index: usize, slot: ImageSlot) -> bool {
        self.draft
            .try_update(|style| style.region_mut(kind).replace_slot(index, slot))
            .unwrap_or(false)
    }

    /// Set the description of one picture; see [`BannerRegion::set_alt`].
    pub fn set_slot_alt(
        self,
        kind: BannerRegionKind,
        index: usize,
        alt: &str,
    ) -> Result<(), String> {
        self.draft
            .try_update(|style| style.region_mut(kind).set_alt(index, alt))
            .unwrap_or_else(|| Err(String::from("That picture is no longer on the page.")))
    }

    /// Move a picture within its region.
    pub fn move_slot(self, kind: BannerRegionKind, from: usize, to: usize) {
        self.draft
            .update(|style| style.region_mut(kind).move_slot(from, to));
    }

    /// Flip a region between side-by-side and stacked.
    pub fn toggle_split(self, kind: BannerRegionKind) {
        self.draft.update(|style| {
            let region = style.region_mut(kind);
            region.split = match region.split {
                SplitDirection::Horizontal => SplitDirection::Vertical,
                SplitDirection::Vertical => SplitDirection::Horizontal,
            };
        });
    }

    /// Hide a navigation item without losing its position.
    pub fn hide_nav_item(self, id: &str) -> bool {
        if is_required_nav_item(id) {
            return false;
        }
        let id = id.to_owned();
        self.draft
            .update(|style| style.nav.move_item_to(&id, NavPlacement::Hidden));
        true
    }

    /// Drop `dragged` onto `target`'s position, adopting its placement.
    pub fn reorder_nav_item(self, dragged: &str, target: &str) {
        let (dragged, target) = (dragged.to_owned(), target.to_owned());
        self.draft
            .update(|style| style.nav.reorder_before(&dragged, &target));
    }

    /// Put a card on a rail.
    pub fn add_module(self, kind: ModuleKind, rail: ModuleRail) {
        self.draft.update(|style| style.add_module(kind, rail));
    }

    /// Move a card to the other rail, keeping its order among that rail's cards.
    pub fn set_module_rail(self, kind: ModuleKind, rail: ModuleRail) {
        self.draft.update(|style| {
            style.set_module_rail(kind, rail);
        });
    }

    /// Take a card off the rail.
    pub fn remove_module(self, kind: ModuleKind) {
        self.draft.update(|style| {
            style.remove_module(kind);
        });
    }

    /// Add a card that is absent, remove one that is present — what the tray's one button
    /// does, so a reader does not have to know which side of the list they are looking at.
    pub fn toggle_module(self, kind: ModuleKind) {
        self.draft.update(|style| {
            if !style.remove_module(kind) {
                style.add_module(kind, ModuleRail::Right);
            }
        });
    }

    /// Reorder two cards on the rail.
    pub fn reorder_module(self, dragged: ModuleKind, target: ModuleKind) {
        self.draft
            .update(|style| style.reorder_module(dragged, target));
    }

    /// Drop a card on another card: adopt its rail, then its place.
    ///
    /// A card that is not on a rail yet is the tray's Add door, so a drop on it adds to the
    /// rail the Add button would have chosen; the place is only adopted when both cards
    /// share a rail.
    pub fn drop_on_module(self, target: ModuleKind) -> bool {
        let Some(Dragged::Module { kind }) = self.take_drag() else {
            return false;
        };
        if kind == target {
            return false;
        }
        let mut moved = false;
        self.draft.update(|style| {
            let rail = style.module_rail(target).unwrap_or(ModuleRail::Right);
            if style.has_module(kind) {
                style.set_module_rail(kind, rail);
            } else {
                style.add_module(kind, rail);
            }
            if style.module_rail(target) == Some(rail) {
                style.reorder_module(kind, target);
            }
            moved = true;
        });
        moved
    }

    /// Drop a navigation entry on another entry, adopting its place.
    pub fn drop_on_nav_item(self, target: &str) -> bool {
        match self.take_drag() {
            Some(Dragged::NavItem { id, .. }) => {
                self.reorder_nav_item(&id, target);
                true
            }
            _ => false,
        }
    }

    /// File a dragged navigation entry under `rail_id`.
    pub fn drop_in_category(self, rail_id: &str) -> bool {
        match self.take_drag() {
            Some(Dragged::NavItem { id, .. }) => {
                self.set_nav_item_rail(&id, rail_id);
                true
            }
            _ => false,
        }
    }

    /// Put a dragged navigation entry away, in the state the layout keeps off the rail.
    pub fn drop_to_hidden(self) -> bool {
        match self.take_drag() {
            Some(Dragged::NavItem { id, .. }) => self.hide_nav_item(&id),
            _ => false,
        }
    }

    /// Move a navigation item to another placement.
    pub fn move_nav_item(self, id: &str, placement: NavPlacement) {
        let id = id.to_owned();
        self.draft
            .update(|style| style.nav.move_item_to(&id, placement));
    }

    /// Flip whether a navigation item leads its placement.
    pub fn toggle_nav_pin(self, id: &str) {
        let id = id.to_owned();
        self.draft.update(|style| style.nav.toggle_pin(&id));
    }

    /// Add a category card to the rail's navigation and return its id.
    pub fn add_nav_rail(self, label: &str) -> Option<String> {
        let label = label.to_owned();
        let mut id = None;
        self.draft.update(|style| {
            id = style.nav.add_rail(&label);
        });
        id
    }

    /// Add a category card straight into `rail` and return its id.
    pub fn add_nav_rail_in(self, label: &str, rail: ModuleRail) -> Option<String> {
        let label = label.to_owned();
        let mut id = None;
        self.draft.update(|style| {
            id = style.nav.add_rail_in(&label, rail);
        });
        id
    }

    /// Remove a category card, keeping its entries under the card that remains.
    pub fn remove_nav_rail(self, id: &str) {
        let id = id.to_owned();
        self.draft.update(|style| {
            style.nav.remove_rail(&id);
        });
    }

    /// Rename a category card.
    pub fn rename_nav_rail(self, id: &str, label: &str) {
        let (id, label) = (id.to_owned(), label.to_owned());
        self.draft
            .update(|style| style.nav.rename_rail(&id, &label));
    }

    /// File a navigation entry under another category card.
    pub fn set_nav_item_rail(self, item_id: &str, rail_id: &str) {
        let (item_id, rail_id) = (item_id.to_owned(), rail_id.to_owned());
        self.draft
            .update(|style| style.nav.set_item_rail(&item_id, &rail_id));
    }

    /// The signal every draggable node writes and every drop target reads.
    #[must_use]
    pub const fn drag_signal(self) -> RwSignal<Option<Dragged>> {
        self.dragging
    }

    /// Start a drag with `payload`, replacing whatever was there.
    pub fn drag(self, payload: Dragged) {
        self.dragging.set(Some(payload));
    }

    /// Clear the drag on `dragend`, so a cancelled drag cannot leave a stale target.
    pub fn drag_end(self) {
        self.dragging.set(None);
    }

    /// Remember what a drag started on, so a rail can take it when it is dropped.
    pub fn drag_module(self, kind: ModuleKind) {
        self.drag(Dragged::Module { kind });
    }

    /// The same, for a category card.
    pub fn drag_rail(self, id: &str, rail: ModuleRail) {
        self.drag(Dragged::Category {
            id: id.to_owned(),
            rail,
        });
    }

    /// The same, for the staff doors.
    pub fn drag_staff(self) {
        self.drag(Dragged::Staff);
    }

    /// The same, for a navigation entry.
    pub fn drag_nav_item(self, id: &str, rail: Option<&str>) {
        self.drag(Dragged::NavItem {
            id: id.to_owned(),
            rail: rail.map(str::to_owned),
        });
    }

    /// The same, for a picture that is already on a banner.
    pub fn drag_picture(self, region: BannerRegionKind, index: usize) {
        self.drag(Dragged::Picture { region, index });
    }

    /// The same, for a picture the collection offers.
    pub fn drag_library(self, picture: LibraryPicture) {
        self.drag(Dragged::Library { picture });
    }

    /// Install the browser's answer to "where can this drive picture live?".
    pub fn set_library_placer(self, place: Callback<(LibraryPicture, PictureTarget)>) {
        self.library_placer.set(Some(place));
    }

    /// Put a collection picture in the top banner, the same place a click in the tray
    /// promises. The keyboard door onto the same edit a drag makes.
    pub fn place_library(self, picture: LibraryPicture) -> bool {
        match self.library_placer.get_untracked() {
            Some(place) => {
                place.run((picture, PictureTarget::Region(BannerRegionKind::Top)));
                true
            }
            None => false,
        }
    }

    /// Put a resolved collection picture on the page.
    pub fn place_library_slot(self, target: PictureTarget, slot: ImageSlot) -> bool {
        self.draft
            .try_update(|style| match target {
                PictureTarget::Region(kind) => style.region_mut(kind).add_slot(slot),
                PictureTarget::Slot(kind, index) => {
                    style.region_mut(kind).replace_slot(index, slot)
                }
            })
            .unwrap_or(false)
    }

    /// Take the payload a drop should apply, clearing it.
    pub fn take_drag(self) -> Option<Dragged> {
        self.dragging
            .get_untracked()
            .inspect(|_| self.dragging.set(None))
    }

    /// Drop a picture on `target` at `index`.
    ///
    /// A picture from another region moves there (keeping its slot in the draft), a
    /// collection picture is added, and anything else is refused rather than rerouted: a
    /// module or a category dropped on a banner does nothing.
    pub fn drop_picture(self, target: BannerRegionKind, index: usize) -> bool {
        match self.take_drag() {
            Some(Dragged::Picture {
                region,
                index: from,
            }) => {
                if region == target {
                    self.move_slot(target, from, index);
                    return true;
                }
                self.draft
                    .try_update(|style| {
                        let Some(image) = style.region_mut(region).slots.get(from).cloned() else {
                            return false;
                        };
                        if !style.region_mut(target).add_slot(image) {
                            return false;
                        }
                        style.region_mut(region).remove_slot(from);
                        let last = style.region_mut(target).slots.len().saturating_sub(1);
                        style.region_mut(target).move_slot(last, index.min(last));
                        true
                    })
                    .unwrap_or(false)
            }
            Some(Dragged::Library { picture }) => {
                // A drop on an occupied place replaces that picture, the same edit the
                // picture's own replace control asks for; a drop past the end adds one.
                let occupied = self
                    .draft
                    .with_untracked(|style| index < style.region(target).slots.len());
                let place = if occupied {
                    PictureTarget::Slot(target, index)
                } else {
                    PictureTarget::Region(target)
                };
                match self.library_placer.get_untracked() {
                    Some(place_it) => {
                        place_it.run((picture, place));
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        }
    }

    /// Take a picture off the page when it is dropped back on the collection.
    pub fn drop_on_collection(self) -> bool {
        match self.take_drag() {
            Some(Dragged::Picture { region, index }) => {
                self.remove_slot(region, index);
                true
            }
            _ => false,
        }
    }

    /// Apply whatever is being dragged to `rail`. Returns whether anything moved.
    pub fn drop_on(self, rail: ModuleRail) -> bool {
        match self.take_drag() {
            Some(Dragged::Module { kind }) => {
                let mut placed = false;
                self.draft.update(|style| {
                    if style.has_module(kind) {
                        placed = style.set_module_rail(kind, rail);
                    } else {
                        style.add_module(kind, rail);
                        placed = true;
                    }
                });
                placed
            }
            Some(Dragged::Category { id, .. }) => {
                let mut moved = false;
                self.draft.update(|style| {
                    moved = style.nav.set_rail_side(&id, rail);
                });
                moved
            }
            Some(Dragged::NavItem { id, .. }) => {
                let mut moved = false;
                let placement = match rail {
                    ModuleRail::Left => NavPlacement::Left,
                    ModuleRail::Right => NavPlacement::Right,
                };
                self.draft.update(|style| {
                    if let Some(item) = style.nav.items.iter_mut().find(|item| item.id == id) {
                        item.placement = placement;
                        item.rail.clear();
                        moved = true;
                    }
                });
                moved
            }
            Some(Dragged::NewCategory) => self.add_nav_rail_in("New category", rail).is_some(),
            Some(Dragged::Staff) => {
                let mut moved = false;
                self.draft.update(|style| {
                    moved = style.staff_rail != rail;
                    style.staff_rail = rail;
                });
                moved
            }
            Some(Dragged::Picture { .. } | Dragged::Library { .. }) | None => false,
        }
    }

    /// Put a category card in `rail` without a drag.
    pub fn set_nav_rail_side(self, id: &str, rail: ModuleRail) {
        let id = id.to_owned();
        self.draft.update(|style| {
            style.nav.set_rail_side(&id, rail);
        });
    }

    /// Put the staff doors in `rail` without a drag.
    pub fn set_staff_rail(self, rail: ModuleRail) {
        self.draft.update(|style| {
            style.staff_rail = rail;
        });
    }

    /// Move a category card earlier or later in the column.
    pub fn move_nav_rail(self, id: &str, forward: bool) {
        let id = id.to_owned();
        self.draft.update(|style| {
            style.nav.move_rail(&id, forward);
        });
    }
}

#[cfg(test)]
mod hidden_tests {
    use super::*;

    #[test]
    fn hiding_a_banner_keeps_its_pictures_and_round_trips() {
        let mut style = SessionStyle::default();
        assert!(
            style.top.add_slot(
                ImageSlot::new("https://cdn.example/top.png", Some("Top"), None).unwrap()
            )
        );
        style.set_region_hidden(BannerRegionKind::Top, true);
        assert!(style.region_is_hidden(BannerRegionKind::Top));

        let json = serde_json::to_string(&style).unwrap();
        assert!(json.contains("\"hidden\":true"));
        let restored: SessionStyle = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.top.slots.len(), 1);
        assert!(restored.region_is_hidden(BannerRegionKind::Top));
    }

    #[test]
    fn cards_are_added_once_and_reordered_by_drag() {
        let mut style = SessionStyle::default();
        assert_eq!(style.modules, default_modules());

        style.add_module(ModuleKind::TrendingTags, ModuleRail::Right);
        assert_eq!(
            style
                .modules
                .iter()
                .filter(|held| held.kind == ModuleKind::TrendingTags)
                .count(),
            1
        );

        style.add_module(ModuleKind::Notifications, ModuleRail::Right);
        assert_eq!(
            style.modules.last().map(|held| held.kind),
            Some(ModuleKind::Notifications)
        );

        // Dragging the last card onto the first puts it first, and nothing is lost.
        style.reorder_module(ModuleKind::Notifications, ModuleKind::TrendingTags);
        assert_eq!(
            style.modules.first().map(|held| held.kind),
            Some(ModuleKind::Notifications)
        );
        assert_eq!(style.modules.len(), 2);

        assert!(style.remove_module(ModuleKind::Notifications));
        assert!(!style.remove_module(ModuleKind::Notifications));
        assert!(!style.has_module(ModuleKind::Notifications));
    }

    #[test]
    fn the_rail_is_made_of_category_cards() {
        let layout = NavLayout::default();
        let rails = layout.left_rails();
        assert_eq!(
            rails
                .iter()
                .map(|(rail, _)| rail.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Read", "Find", "Library"]
        );
        assert_eq!(
            rails[0]
                .1
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Home", "Messages", "Notifications"]
        );

        // A card that empties out stays addressable while editing.
        let mut layout = layout;
        for item in layout
            .items_of_rail("read")
            .iter()
            .map(|item| item.id.clone())
            .collect::<Vec<_>>()
        {
            layout.set_item_rail(&item, "find");
        }
        let (emptied, entries) = &layout.left_rails()[0];
        assert_eq!(emptied.id, "read");
        assert!(
            entries.is_empty(),
            "an emptied card stays addressable while editing"
        );
    }

    #[test]
    fn a_new_rail_gets_an_id_nobody_else_uses() {
        let mut layout = NavLayout::default();
        assert_eq!(
            layout.add_rail("Watch later").as_deref(),
            Some("watch-later")
        );
        assert_eq!(
            layout.add_rail("Watch later").as_deref(),
            Some("watch-later-2")
        );
        assert_eq!(layout.add_rail("  ").as_deref(), Some("new-rail"));
        assert_eq!(layout.rails.len(), 6);
        assert!(layout.add_rail("One more").is_none());
    }

    #[test]
    fn removing_a_card_keeps_its_entries_in_the_rail() {
        let mut layout = NavLayout::default();
        assert!(layout.remove_rail("find"));
        assert_eq!(
            layout
                .items_of_rail("read")
                .iter()
                .filter(|item| item.label == "Search")
                .count(),
            1
        );
        // The last card cannot be removed: there is nowhere for its entries to go.
        let mut single = NavLayout::default();
        while single.remove_rail(single.rails[0].id.clone().as_str()) {}
        assert_eq!(single.rails.len(), 1);
    }

    #[test]
    fn a_layout_written_before_categories_existed_still_opens() {
        let json = serde_json::json!({
            "items": [
                { "id": "home", "label": "Home", "path": "/home" },
                { "id": "lists", "label": "Lists", "path": "/lists", "pinned": true }
            ],
            "submenu_label": "More"
        });
        let layout = parse_nav(Some(&json));
        assert_eq!(layout.rails, default_nav_rails());
        assert!(layout.items.iter().all(|item| item.rail == "read"));
        assert_eq!(layout.items_of_rail("read").len(), 2);
    }

    #[test]
    fn a_card_moves_between_rails_without_leaving_the_list() {
        let mut style = SessionStyle::default();
        style.add_module(ModuleKind::LiveFeed, ModuleRail::Left);
        assert_eq!(
            style.module_rail(ModuleKind::LiveFeed),
            Some(ModuleRail::Left)
        );
        assert!(
            style
                .modules_in(ModuleRail::Left)
                .contains(&ModuleKind::LiveFeed)
        );
        assert!(
            !style
                .modules_in(ModuleRail::Right)
                .contains(&ModuleKind::LiveFeed)
        );

        // A card on one rail is not dragged onto a card in the other: that is a rail move,
        // and a drag that silently did it would move a card out of the column it was
        // dropped in.
        let before = style.modules.clone();
        style.reorder_module(ModuleKind::LiveFeed, ModuleKind::TrendingTags);
        assert_eq!(style.modules, before);

        assert!(style.set_module_rail(ModuleKind::LiveFeed, ModuleRail::Right));
        assert!(!style.set_module_rail(ModuleKind::LiveFeed, ModuleRail::Right));
        assert_eq!(
            style.module_rail(ModuleKind::LiveFeed),
            Some(ModuleRail::Right)
        );
    }

    #[test]
    fn a_card_list_survives_metadata_but_unknown_cards_are_dropped() {
        let style = SessionStyle {
            modules: vec![
                PlacedModule::new(ModuleKind::Notifications),
                PlacedModule::in_rail(ModuleKind::LiveFeed, ModuleRail::Left),
            ],
            ..SessionStyle::default()
        };
        let patch = style.to_patch_metadata();
        let stored = patch.get(KEY_RAIL_MODULES).cloned().unwrap();
        assert_eq!(
            stored,
            serde_json::json!(["notifications", "left:live_feed"])
        );

        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert(
            KEY_RAIL_MODULES.to_owned(),
            serde_json::json!([
                "live_feed",
                "from_a_newer_build",
                "live_feed",
                "left:trending_tags"
            ]),
        );
        let restored = SessionStyle::from_metadata_map(&metadata);
        assert_eq!(
            restored.modules,
            vec![
                PlacedModule::new(ModuleKind::LiveFeed),
                PlacedModule::in_rail(ModuleKind::TrendingTags, ModuleRail::Left),
            ]
        );

        // The starting set is not written back: a default layout stores nothing.
        let default_patch = SessionStyle::default().to_patch_metadata();
        assert!(default_patch.get(KEY_RAIL_MODULES).unwrap().is_null());
    }

    #[test]
    fn an_empty_region_is_never_reported_hidden() {
        let mut style = SessionStyle::default();
        style.set_region_hidden(BannerRegionKind::Left, true);
        assert!(!style.region_is_hidden(BannerRegionKind::Left));
        let json = serde_json::to_string(&style).unwrap();
        assert!(json.contains("\"hidden\":true"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn metadata_roundtrip_preserves_split_banners_and_nav_layout() {
        let mut style = SessionStyle::default();
        style.top.split = SplitDirection::Horizontal;
        assert!(
            style.top.add_slot(
                ImageSlot::new(
                    "https://cdn.example/top-a.png",
                    Some("Left half"),
                    Some("m1")
                )
                .unwrap()
            )
        );
        assert!(style.top.add_slot(
            ImageSlot::new("https://cdn.example/top-b.png", Some("Right half"), None).unwrap()
        ));
        style.left.split = SplitDirection::Vertical;
        assert!(
            style
                .left
                .add_slot(ImageSlot::new("https://cdn.example/left.png", None, None).unwrap())
        );
        style.nav.move_item_to("lists", NavPlacement::Right);
        style.nav.move_item_to("scheduled", NavPlacement::Submenu);
        style.nav.move_item_to("people", NavPlacement::Hidden);
        style.nav.toggle_pin("saved");
        style
            .nav
            .pin_favorite("Bookmarks", "/saved/bookmarks", NavPlacement::Right)
            .unwrap();

        let patch = style.to_patch_metadata();
        assert!(patch.get(KEY_TOP_BANNER).unwrap().is_object());
        assert!(patch.get(KEY_LEFT_IMAGE).unwrap().is_object());
        assert_eq!(patch.get(KEY_RIGHT_IMAGE), Some(&Value::Null));
        assert!(patch.get(KEY_NAV_LAYOUT).unwrap().is_object());

        let restored = SessionStyle::from_metadata_map(&patch);
        assert_eq!(restored, style);
        assert_eq!(
            restored.thumbnail_url(),
            Some("https://cdn.example/top-a.png")
        );
    }

    #[test]
    fn from_metadata_map_drops_unsafe_urls_and_paths() {
        let mut metadata = BTreeMap::new();
        metadata.insert(
            KEY_TOP_BANNER.to_owned(),
            json!({
                "split": "vertical",
                "slots": [
                    { "url": "javascript:alert(1)" },
                    { "url": "//evil.example/a.png" },
                    { "url": "https://cdn.example/ok.png", "alt": "ok\nline" }
                ]
            }),
        );
        metadata.insert(
            KEY_NAV_LAYOUT.to_owned(),
            json!({
                "items": [
                    { "id": "evil", "label": "Evil", "path": "//evil.example", "placement": "left" },
                    { "id": "home", "label": "Home", "path": "/home", "pinned": true, "placement": "right" }
                ]
            }),
        );

        let parsed = SessionStyle::from_metadata_map(&metadata);
        assert_eq!(parsed.top.split, SplitDirection::Vertical);
        assert_eq!(parsed.top.slots.len(), 1);
        assert_eq!(parsed.top.slots[0].url, "https://cdn.example/ok.png");
        assert_eq!(parsed.top.slots[0].alt.as_deref(), Some("okline"));
        assert_eq!(parsed.nav.items.len(), 1);
        assert_eq!(parsed.nav.items[0].id, "home");
        assert_eq!(parsed.nav.items[0].placement, NavPlacement::Right);
    }

    #[test]
    fn nav_layout_reorder_pin_and_favorites_behave_consistently() {
        let mut nav = NavLayout::default();
        nav.move_item_to("lists", NavPlacement::Right);
        nav.toggle_pin("saved");
        nav.reorder_before("search", "lists");

        let right_ids: Vec<&str> = nav
            .items_in(NavPlacement::Right)
            .into_iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(right_ids, vec!["search", "lists"]);

        let left_ids: Vec<&str> = nav
            .items_in(NavPlacement::Left)
            .into_iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(left_ids.first().copied(), Some("saved"));

        assert!(
            nav.pin_favorite("Bad", "https://evil.example", NavPlacement::Left)
                .is_err()
        );
        nav.pin_favorite("Faves", "/saved/favourites", NavPlacement::Submenu)
            .unwrap();
        assert_eq!(
            nav.items.iter().find(|item| item.id == "saved").map(|i| (
                i.label.as_str(),
                i.pinned,
                i.placement
            )),
            Some(("Faves", true, NavPlacement::Submenu))
        );
    }

    #[test]
    fn safe_json_island_escapes_closing_script_tag() {
        let mut style = SessionStyle::default();
        style.top.add_slot(ImageSlot {
            url: String::from("https://cdn.example/banner.png"),
            alt: Some(String::from("</script><script>alert(1)</script>")),
            media_id: None,
        });
        let island = style.to_safe_json_island();
        assert!(!island.contains("</script>"));
        assert!(island.contains("<\\/script>"));
    }

    #[test]
    fn a_placed_picture_is_replaced_where_it_stands() {
        let mut style = SessionStyle::default();
        assert!(
            style
                .left
                .add_slot(ImageSlot::new("https://cdn.example/a.png", Some("A"), None).unwrap())
        );
        assert!(
            style
                .left
                .add_slot(ImageSlot::new("https://cdn.example/b.png", Some("B"), None).unwrap())
        );
        let editor = StyleEditor::new(RwSignal::new(style), RwSignal::new(true));
        assert!(editor.replace_slot(
            BannerRegionKind::Left,
            1,
            ImageSlot::new("https://cdn.example/c.png", Some("C"), Some("m3")).unwrap()
        ));
        let left = editor.draft().get_untracked().left;
        assert_eq!(left.slots.len(), 2, "a replacement does not add a picture");
        assert_eq!(left.slots[0].alt.as_deref(), Some("A"));
        assert_eq!(left.slots[1].url, "https://cdn.example/c.png");
        assert_eq!(left.slots[1].media_id.as_deref(), Some("m3"));
        assert!(
            !editor.replace_slot(
                BannerRegionKind::Left,
                2,
                ImageSlot::new("https://cdn.example/d.png", None, None).unwrap()
            ),
            "there is no third place to fill"
        );
    }

    #[test]
    fn alt_text_is_trimmed_cleared_when_empty_and_refused_past_the_limit() {
        let mut style = SessionStyle::default();
        assert!(
            style
                .right
                .add_slot(ImageSlot::new("https://cdn.example/a.png", Some("Old"), None).unwrap())
        );
        let editor = StyleEditor::new(RwSignal::new(style), RwSignal::new(true));
        editor
            .set_slot_alt(BannerRegionKind::Right, 0, "  New\nline  ")
            .unwrap();
        assert_eq!(
            editor.draft().get_untracked().right.slots[0].alt.as_deref(),
            Some("Newline")
        );

        editor
            .set_slot_alt(BannerRegionKind::Right, 0, "   ")
            .unwrap();
        assert_eq!(
            editor.draft().get_untracked().right.slots[0].alt,
            None,
            "an empty description is a decorative picture, not a failure"
        );

        let too_long = "a".repeat(MAX_ALT_LEN + 1);
        let refused = editor
            .set_slot_alt(BannerRegionKind::Right, 0, &too_long)
            .unwrap_err();
        assert!(refused.contains(&MAX_ALT_LEN.to_string()), "{refused}");
        assert_eq!(
            editor.draft().get_untracked().right.slots[0].alt,
            None,
            "refused text is not cut and not stored"
        );
        assert!(
            editor
                .set_slot_alt(BannerRegionKind::Right, 9, "x")
                .is_err()
        );
    }

    #[test]
    fn picture_dragged_between_regions_keeps_its_slot() {
        let mut style = SessionStyle::default();
        for url in ["https://cdn.example/a.png", "https://cdn.example/b.png"] {
            assert!(
                style
                    .left
                    .add_slot(ImageSlot::new(url, None, None).unwrap())
            );
        }
        let editor = StyleEditor::new(RwSignal::new(style), RwSignal::new(true));
        editor.drag_picture(BannerRegionKind::Left, 1);
        assert!(editor.drop_picture(BannerRegionKind::Right, 0));
        let style = editor.draft().get_untracked();
        assert_eq!(style.left.slots.len(), 1);
        assert_eq!(style.left.slots[0].url, "https://cdn.example/a.png");
        assert_eq!(style.right.slots.len(), 1);
        assert_eq!(style.right.slots[0].url, "https://cdn.example/b.png");
    }

    #[test]
    fn module_dragged_from_the_tray_lands_on_a_rail_and_leaves_the_tray() {
        let mut style = SessionStyle::default();
        style.remove_module(ModuleKind::TrendingTags);
        let editor = StyleEditor::new(RwSignal::new(style), RwSignal::new(true));
        editor.drag_module(ModuleKind::TrendingTags);
        assert!(editor.drop_on(ModuleRail::Right));
        assert!(
            editor
                .draft()
                .get_untracked()
                .has_module(ModuleKind::TrendingTags)
        );
        assert!(
            editor
                .draft()
                .get_untracked()
                .modules_in(ModuleRail::Right)
                .contains(&ModuleKind::TrendingTags)
        );
    }

    #[test]
    fn a_picture_dropped_on_a_module_holder_is_refused() {
        let mut style = SessionStyle::default();
        assert!(
            style
                .left
                .add_slot(ImageSlot::new("https://cdn.example/a.png", None, None).unwrap())
        );
        let editor = StyleEditor::new(RwSignal::new(style), RwSignal::new(true));
        editor.drag_picture(BannerRegionKind::Left, 0);
        assert!(!editor.drop_on_module(ModuleKind::TrendingTags));
        assert!(
            editor.take_drag().is_none(),
            "a refused drop still clears the drag"
        );
        assert_eq!(editor.draft().get_untracked().left.slots.len(), 1);
    }

    #[test]
    fn module_dragged_between_categories_lands_only_in_the_destination() {
        let mut style = SessionStyle::default();
        style.add_module(ModuleKind::TrendingTags, ModuleRail::Left);
        let editor = StyleEditor::new(RwSignal::new(style), RwSignal::new(true));
        assert!(
            editor
                .draft()
                .get_untracked()
                .modules_in(ModuleRail::Left)
                .contains(&ModuleKind::TrendingTags)
        );
        editor.drag_module(ModuleKind::TrendingTags);
        assert!(editor.drop_on(ModuleRail::Right));
        let style = editor.draft().get_untracked();
        assert!(style.modules_in(ModuleRail::Right).contains(&ModuleKind::TrendingTags));
        assert!(
            !style.modules_in(ModuleRail::Left).contains(&ModuleKind::TrendingTags),
            "a moved card is moved, not copied"
        );
        assert_eq!(
            style
                .modules
                .iter()
                .filter(|held| held.kind == ModuleKind::TrendingTags)
                .count(),
            1,
            "one card cannot be in two rails at once"
        );
    }

    #[test]
    fn a_collection_picture_dropped_on_a_slot_replaces_that_slot() {
        let mut style = SessionStyle::default();
        for url in ["https://cdn.example/a.png", "https://cdn.example/b.png"] {
            assert!(
                style
                    .top
                    .add_slot(ImageSlot::new(url, None, None).unwrap())
            );
        }
        let editor = StyleEditor::new(RwSignal::new(style), RwSignal::new(true));
        // The browser normally resolves the drive file asynchronously; the test supplies
        // that answer directly, which is the only part of the drop that is not the shell's.
        editor.set_library_placer(Callback::new(move |(_, target): (LibraryPicture, PictureTarget)| {
            editor.place_library_slot(
                target,
                ImageSlot::new("https://cdn.example/c.png", None, Some("attached"))
                    .expect("a safe address"),
            );
        }));
        editor.drag_library(LibraryPicture {
            id: String::from("file-1"),
            name: String::from("c.png"),
            thumbnail: String::from("blob:test"),
        });
        assert!(editor.drop_picture(BannerRegionKind::Top, 1));
        let top = editor.draft().get_untracked().top;
        assert_eq!(top.slots.len(), 2, "a collection drop replaces, never grows");
        assert_eq!(top.slots[0].url, "https://cdn.example/a.png");
        assert_eq!(top.slots[1].url, "https://cdn.example/c.png");
    }

    #[test]
    fn an_add_target_accepts_the_same_edit_by_drop_and_by_click() {
        // The tray's Add button and a drop on the same card are one edit; the button is
        // the keyboard door onto it, so both have to end in the same placement.
        let dropped = StyleEditor::new(RwSignal::new(SessionStyle::default()), RwSignal::new(true));
        dropped.drag_module(ModuleKind::LiveFeed);
        assert!(dropped.drop_on_module(ModuleKind::TrendingTags));
        let clicked = StyleEditor::new(RwSignal::new(SessionStyle::default()), RwSignal::new(true));
        clicked.toggle_module(ModuleKind::LiveFeed);
        assert_eq!(
            dropped
                .draft()
                .get_untracked()
                .module_rail(ModuleKind::LiveFeed),
            clicked.draft().get_untracked().module_rail(ModuleKind::LiveFeed),
            "the drop and the click must choose the same rail"
        );
    }

    #[test]
    fn a_collection_drag_is_cleared_when_the_browser_cancels_the_drop() {
        let editor = StyleEditor::new(RwSignal::new(SessionStyle::default()), RwSignal::new(true));
        editor.drag_library(LibraryPicture {
            id: String::from("file-1"),
            name: String::from("c.png"),
            thumbnail: String::from("blob:test"),
        });
        // No drop target accepts it, and the browser then fires `dragend`: the shared
        // signal must be empty, or the next hover promises a drop that cannot happen.
        editor.drag_end();
        assert!(editor.take_drag().is_none());
    }

    #[test]
    fn a_link_dragged_to_the_hidden_list_leaves_the_rail() {
        let editor = StyleEditor::new(RwSignal::new(SessionStyle::default()), RwSignal::new(true));
        let id = editor
            .draft()
            .get_untracked()
            .nav
            .items
            .iter()
            .find(|item| item.placement == NavPlacement::Left)
            .map(|item| item.id.clone())
            .expect("the default layout puts a link on the left");
        editor.drag_nav_item(&id, Some(""));
        assert!(editor.drop_to_hidden());
        let nav = editor.draft().get_untracked().nav;
        assert!(
            nav.items_in(NavPlacement::Hidden)
                .iter()
                .any(|item| item.id == id)
        );
        assert!(
            !nav.items_in(NavPlacement::Left)
                .iter()
                .any(|item| item.id == id)
        );
    }

    #[test]
    fn a_cancelled_drag_leaves_no_phantom_target() {
        let mut style = SessionStyle::default();
        assert!(
            style
                .left
                .add_slot(ImageSlot::new("https://cdn.example/a.png", None, None).unwrap())
        );
        let editor = StyleEditor::new(RwSignal::new(style), RwSignal::new(true));
        editor.drag_picture(BannerRegionKind::Left, 0);
        assert!(editor.take_drag().is_some(), "the drag really started");
        editor.drag_end();
        assert!(editor.take_drag().is_none());
    }
}
