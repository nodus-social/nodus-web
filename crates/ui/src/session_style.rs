//! Per-account custom session styling and navigation layout.
//!
//! Serializes into the four fine-grained `nodus-backend` client metadata keys
//! (`nodus-top-banner`, `nodus-left-image`, `nodus-right-image`, `nodus-nav-layout`)
//! and is shared identically by SSR (`nodus-web-server`), CSR (`nodus-web-client`),
//! and UI/page components (`nodus-ui`, `nodus-pages`).

use std::collections::{BTreeMap, HashSet};

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

/// All fine-grained metadata keys managed by Nodus Web for session styling.
pub const STYLE_KEYS: [&str; 4] = [
    KEY_TOP_BANNER,
    KEY_LEFT_IMAGE,
    KEY_RIGHT_IMAGE,
    KEY_NAV_LAYOUT,
];

/// Maximum number of image slots allowed inside one banner region.
pub const MAX_SLOTS_PER_REGION: usize = 4;
/// Maximum number of navigation items allowed in a custom layout.
pub const MAX_NAV_ITEMS: usize = 24;
const MAX_ALT_LEN: usize = 200;
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
}

impl BannerRegion {
    /// Whether this region currently has no valid image slots.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
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

/// Per-account navigation ordering, pinning, and placement layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavLayout {
    #[serde(default = "default_nav_items")]
    pub items: Vec<NavItemConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submenu_label: Option<String>,
}

impl Default for NavLayout {
    fn default() -> Self {
        Self {
            items: default_nav_items(),
            submenu_label: Some(String::from("More")),
        }
    }
}

impl NavLayout {
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
    })
    .collect()
}

/// Full per-account session styling and navigation layout state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStyle {
    #[serde(default)]
    pub top: BannerRegion,
    #[serde(default)]
    pub left: BannerRegion,
    #[serde(default)]
    pub right: BannerRegion,
    #[serde(default)]
    pub nav: NavLayout,
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
    serde_json::from_value::<NavLayout>(value.clone()).unwrap_or_default()
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
}
