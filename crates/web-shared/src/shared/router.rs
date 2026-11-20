//! The route vocabulary: one enum, the address-bar path it maps to, and the sections a bare
//! path stands for.
//!
//! Both paints name routes — the client to navigate, the server to know which screen a URL
//! asks for — so the enum lives here, where neither `web_sys` nor axum may be named. Only
//! parsing and formatting are here; pushing history and reading the address bar are
//! `browser::router`'s job.
//!
//! The path is the only source of truth. Unknown paths render the feed, which is what a
//! static host serving `index.html` for every URL will produce anyway.

/// Where the instance sends the browser back.
///
/// The same string is registered as `redirect_uris` and echoed to `/oauth/token`:
/// Mastodon requires all three to match exactly, so it is defined once, here. The route
/// table and `state::oauth` both read it, so the address bar's path and the registered URI
/// cannot drift apart.
pub const CALLBACK_PATH: &str = "/oauth/callback";

/// A screen the app can show.
///
/// Not `Copy`: a thread and a profile carry an id, and an id is a `String` the
/// address bar produced. Everything that compared routes by value now compares them
/// by reference or clones, which is the honest cost of having routes with arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// The recommendation Feed tab.
    Feed,
    /// The home timeline.
    Home,
    /// Posts from accounts on this instance.
    Local,
    /// Posts this instance received from elsewhere.
    Federated,
    /// One status with its ancestors and replies.
    ///
    /// The host is part of the address because an id is local to the instance that issued
    /// it: without the host a link is ambiguous, and a reader who is not signed in cannot
    /// be told where to look. Empty only for a legacy `/status/{id}` link.
    Thread {
        host: String,
        id: String,
    },
    /// One account's profile and statuses.
    Account(String),
    /// One hashtag: its header and its timeline, open at `/tags/{name}`.
    ///
    /// A hashtag is a place a reader can link, reload and come back to, so its name
    /// is in the address rather than in a filter over another screen.
    Tag(String),
    /// Notifications for the signed-in account.
    Messages,
    Notifications,
    /// Search over accounts, statuses and hashtags.
    Search,
    /// People the instance suggests.
    Directory,
    /// Favourites and bookmarks; the shelf is part of the address (`/saved/bookmarks`).
    Saved(String),
    /// Posts waiting to be published.
    Scheduled,
    /// Hand-picked accounts with a timeline each.
    Lists,
    /// One of those lists, open. The list is in the address so a roster or a timeline can
    /// be linked, reloaded and returned to.
    List(String),
    /// The phone's door to everything the bottom bar does not hold.
    More,
    /// Writing a new status.
    Compose,
    /// Editing one of the viewer's own statuses.
    ComposeEdit(String),
    /// Accounts the viewer muted.
    Mutes,
    /// Accounts the viewer blocked.
    Blocks,
    /// The cloud drive: this account's own folders and files.
    Drive,
    Admin(String),
    Moderation(String),
    /// Settings, with the section in the address (`/settings/privacy`). Empty is the
    /// account section, so `/settings` stays a working link.
    Settings(String),
    /// The fullscreen styling editor. An address of its own rather than a flag on the page
    /// that launches it: the editor is a place the reader is in, so it must be linkable,
    /// reloadable, and left by the back button like any other screen.
    StyleEditor,
    /// Where the instance returns the browser after authorization. The query string
    /// carries the code and `state`, so this route is matched regardless of sign-in.
    OauthCallback,
}

impl Route {
    /// The URL path for this route.
    #[must_use]
    pub fn path(&self) -> String {
        let path = self.suffix();
        if BASE.is_empty() {
            return path;
        }
        if path == "/" {
            return BASE.to_owned();
        }
        format!("{BASE}{path}")
    }

    /// Parses an internal link `href` into its [`Route`] and canonical URL with query/fragment
    /// preserved, so navigating to `/search?q=%23rust` keeps `?q=%23rust` in the address bar.
    #[must_use]
    pub fn parse_href(href: &str) -> (Self, String) {
        let split_at = href.find(['?', '#']).unwrap_or(href.len());
        let (path, suffix) = href.split_at(split_at);
        let route = Self::from_path(path);
        let url = format!("{}{suffix}", route.path());
        (route, url)
    }

    /// The route below the mount point.
    fn suffix(&self) -> String {
        match self {
            Self::Feed => "/home/feed".to_owned(),
            Self::Home => "/home".to_owned(),
            Self::Local => "/home/local".to_owned(),
            Self::Federated => "/home/federated".to_owned(),
            Self::Thread { host, id } => {
                if host.is_empty() {
                    format!("/status/{id}")
                } else {
                    format!("/status/{host}/{id}")
                }
            }
            Self::Account(id) => format!("/accounts/{id}"),
            Self::Tag(name) => format!("/tags/{}", encode_segment(name)),
            Self::Messages => "/messages".to_owned(),
            Self::Notifications => "/notifications".to_owned(),
            Self::Search => "/search".to_owned(),
            Self::Directory => "/directory".to_owned(),
            Self::Saved(which) => format!("/saved/{which}"),
            Self::Scheduled => "/scheduled".to_owned(),
            Self::Lists => "/lists".to_owned(),
            Self::List(id) => format!("/lists/{id}"),
            Self::More => "/more".to_owned(),
            Self::Compose => "/compose".to_owned(),
            Self::ComposeEdit(id) => format!("/compose/{id}"),
            Self::Mutes => "/mutes".to_owned(),
            Self::Blocks => "/blocks".to_owned(),
            Self::Drive => "/drive".to_owned(),
            Self::Admin(section) => format!("/admin/{section}"),
            Self::Moderation(section) => format!("/moderation/{section}"),
            Self::Settings(section) => format!("/settings/{section}"),
            Self::StyleEditor => STYLE_EDITOR_PATH.to_owned(),
            // The OAuth flow registers this path as `redirect_uris`; sharing the
            // constant is what keeps the address bar and the registered URI identical.
            Self::OauthCallback => CALLBACK_PATH.to_owned(),
        }
    }

    /// The route for an address-bar pathname; unknown paths fall back to the feed.
    ///
    /// A prefix match is how a route with an argument is parsed: the id is whatever
    /// follows, unencoded, because an instance id is a digit string and encoding it
    /// would only make the address bar lie about what the id is.
    #[must_use]
    pub fn from_path(mut path: &str) -> Self {
        // A build mounted under a prefix sees the prefix in the address bar but not in
        // its own routes; stripping it here is what lets one bundle serve both the root
        // and a diagnostic mount without a second copy of every match arm.
        if !BASE.is_empty() {
            path = path.strip_prefix(BASE).unwrap_or(path);
        }
        let path = if path.is_empty() { "/" } else { path };
        let trimmed = path.trim_end_matches('/');
        if trimmed == CALLBACK_PATH {
            return Self::OauthCallback;
        }
        // `/@name` and `/@name@host` are how a handle is written everywhere else on the
        // network, and `/@name@host/{id}` is what a shared remote-status link looks like.
        // They name the same screens the canonical `/accounts/{host}/{name}` and
        // `/status/{host}/{id}` do, so a pasted link lands instead of falling home.
        if let Some(rest) = trimmed.strip_prefix("/@") {
            let (handle, id) = match rest.split_once('/') {
                Some((handle, id)) => (handle, Some(id)),
                None => (rest, None),
            };
            let id = id.map(str::trim).filter(|id| !id.is_empty());
            let handle = handle.trim_start_matches('@');
            match handle.split_once('@') {
                Some((name, host)) => {
                    let (name, host) = (name.trim(), host.trim());
                    if !name.is_empty() && !host.is_empty() {
                        return match id {
                            Some(id) => Self::Thread {
                                host: host.to_owned(),
                                id: id.to_owned(),
                            },
                            None => Self::Account(format!("{host}/{name}")),
                        };
                    }
                }
                None => {
                    let name = handle.trim();
                    if !name.is_empty() {
                        return match id {
                            Some(id) => Self::Thread {
                                host: String::new(),
                                id: id.to_owned(),
                            },
                            None => Self::Account(name.to_owned()),
                        };
                    }
                }
            }
        }
        if let Some(rest) = trimmed.strip_prefix("/status/") {
            // `/status/{host}/{id}` is the shareable form; `/status/{id}` stays the
            // in-session one, because an id alone says nothing about where to read it.
            if let Some((host, id)) = rest.split_once('/') {
                if !host.is_empty() && !id.is_empty() {
                    return Self::Thread {
                        host: host.to_owned(),
                        id: id.to_owned(),
                    };
                }
            } else if !rest.is_empty() {
                // Legacy shape: the view resolves the host, or asks for it.
                return Self::Thread {
                    host: String::new(),
                    id: rest.to_owned(),
                };
            }
        }
        // The editor's own address, tested before the `/settings/` prefix so it is a screen
        // rather than a section named `styling/editor`.
        // The styling tab is gone; its address is the editor's, so a bookmark or a link
        // written before the change opens the thing that replaced it.
        if trimmed == "/settings/styling" {
            return Self::StyleEditor;
        }
        if trimmed == STYLE_EDITOR_PATH {
            return Self::StyleEditor;
        }
        // Submenus are addresses, not client state: `/settings/privacy` and
        // `/saved/bookmarks` stay readable, shareable and back-button-friendly.
        if let Some(section) = trimmed.strip_prefix("/settings/") {
            if !section.is_empty() {
                return Self::Settings(section.to_owned());
            }
        }
        if let Some(section) = trimmed.strip_prefix("/moderation/") {
            if !section.is_empty() {
                return Self::Moderation(section.to_owned());
            }
        }
        if let Some(section) = trimmed.strip_prefix("/admin/") {
            if !section.is_empty() {
                return Self::Admin(section.to_owned());
            }
        }
        if let Some(shelf) = trimmed.strip_prefix("/saved/") {
            if !shelf.is_empty() {
                return Self::Saved(shelf.to_owned());
            }
        }
        if let Some(id) = trimmed.strip_prefix("/lists/") {
            if !id.is_empty() {
                return Self::List(id.to_owned());
            }
        }
        if let Some(id) = trimmed.strip_prefix("/accounts/") {
            if !id.is_empty() {
                return Self::Account(id.to_owned());
            }
        }
        // A hashtag is read by name, so the segment is percent-decoded back into the
        // name the instance knows; a stray leading `#` is the same tag.
        if let Some(name) = trimmed.strip_prefix("/tags/") {
            let name = decode_query(name).trim_start_matches('#').to_owned();
            if !name.is_empty() {
                return Self::Tag(name);
            }
        }
        // After `/accounts/`, so a status id that looks like a path cannot shadow a
        // profile: the longer prefix wins by being tested first.
        if let Some(id) = trimmed.strip_prefix("/compose/") {
            if !id.is_empty() {
                return Self::ComposeEdit(id.to_owned());
            }
        }
        match trimmed {
            "/home" => Self::Home,
            "/home/feed" => Self::Feed,
            "/home/local" => Self::Local,
            "/home/federated" => Self::Federated,
            "/messages" => Self::Messages,
            "/notifications" => Self::Notifications,
            "/search" => Self::Search,
            "/directory" => Self::Directory,
            "/saved" => Self::Saved(SHELF_DEFAULT.to_owned()),
            "/scheduled" => Self::Scheduled,
            "/lists" => Self::Lists,
            "/more" => Self::More,
            "/compose" => Self::Compose,
            "/mutes" => Self::Mutes,
            "/blocks" => Self::Blocks,
            "/drive" => Self::Drive,
            "/admin" => Self::Admin(ADMIN_DEFAULT.to_owned()),
            "/moderation" => Self::Moderation(MODERATION_DEFAULT.to_owned()),
            "/settings" => Self::Settings(SECTION_DEFAULT.to_owned()),
            _ => Self::Home,
        }
    }
}

/// Where this bundle is mounted: the root for a release build, a prefix for a diagnostic
/// one (`NODUS_BASE=/debug`). Every route path is composed from it, so an asset or a link
/// cannot point outside the mount by accident.
/// The section `/settings` means when no section is named.
pub const SECTION_DEFAULT: &str = "account";
/// Where the fullscreen styling editor lives, below the screen that launches it.
pub const STYLE_EDITOR_PATH: &str = "/settings/styling/editor";
/// The shelf `/saved` means when no shelf is named.
pub const SHELF_DEFAULT: &str = "favourites";

/// The administration section `/admin` lands on. Declared here rather than in the admin
/// module so a build without the moderation screens can still parse the address.
pub const ADMIN_DEFAULT: &str = "overview";

/// The moderation section `/moderation` lands on: the queue is what a moderator opens.
pub const MODERATION_DEFAULT: &str = "reports";

pub const BASE: &str = match option_env!("NODUS_BASE") {
    Some(base) => base,
    None => "",
};

/// Reads one query parameter from `search` (with or without a leading `?`), skipping
/// valueless flags and percent-decoding UTF-8 and `+` spaces.
#[must_use]
pub fn parse_query_param(search: &str, name: &str) -> Option<String> {
    let pairs = search.strip_prefix('?').unwrap_or(search);
    if pairs.is_empty() {
        return None;
    }
    for pair in pairs.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key == name {
            return Some(decode_query(value));
        }
    }
    None
}

/// Percent-encodes one path segment, leaving the characters a hashtag is usually
/// written with alone so `/tags/rust` stays readable.
#[must_use]
pub fn encode_segment(value: &str) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

/// Decodes percent-encoded UTF-8 sequences and `+` spaces in a query-string value.
#[must_use]
pub fn decode_query(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                    decoded.push(byte);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{Route, decode_query, parse_query_param};

    #[test]
    fn a_tag_is_a_place_and_its_name_survives_the_round_trip() {
        assert_eq!(Route::from_path("/tags/rust"), Route::Tag("rust".to_owned()));
        assert_eq!(
            Route::from_path("/tags/%23rust"),
            Route::Tag("rust".to_owned())
        );
        assert_eq!(
            Route::from_path("/tags/caf%C3%A9"),
            Route::Tag("café".to_owned())
        );
        assert_eq!(
            Route::Tag("café".to_owned()).path(),
            format!("{}/tags/caf%C3%A9", super::BASE)
        );
    }

    #[test]
    fn parse_href_preserves_query_and_fragment() {
        let (route, url) = Route::parse_href("/search?q=%23rust");
        assert_eq!(route, Route::Search);
        assert_eq!(url, format!("{}/search?q=%23rust", super::BASE));

        let (route, url) = Route::parse_href("/search?q=%40alice%40example.org#top");
        assert_eq!(route, Route::Search);
        assert_eq!(
            url,
            format!("{}/search?q=%40alice%40example.org#top", super::BASE)
        );
    }

    #[test]
    fn a_handle_is_a_profile_and_a_handle_with_an_id_is_a_thread() {
        assert_eq!(
            Route::from_path("/@luxuride"),
            Route::Account("luxuride".to_owned())
        );
        assert_eq!(
            Route::from_path("/@luxuride@nodus.luxuride.eu"),
            Route::Account("nodus.luxuride.eu/luxuride".to_owned())
        );
        assert_eq!(
            Route::from_path("/@Gargron@mastodon.social/115"),
            Route::Thread {
                host: "mastodon.social".to_owned(),
                id: "115".to_owned()
            }
        );
        assert_eq!(
            Route::from_path("/@luxuride/01M7"),
            Route::Thread {
                host: String::new(),
                id: "01M7".to_owned()
            }
        );
    }

    #[test]
    fn a_handle_path_keeps_its_canonical_address_so_links_are_stable() {
        let (route, url) = Route::parse_href("/@Gargron@mastodon.social");
        assert_eq!(url, format!("{}/accounts/mastodon.social/Gargron", super::BASE));
        assert_eq!(route, Route::Account("mastodon.social/Gargron".to_owned()));
    }

    #[test]
    fn query_param_decodes_hashtags_mentions_utf8_and_skips_flags() {
        assert_eq!(
            parse_query_param("?flag&q=%23rust+%E2%9C%A8", "q").as_deref(),
            Some("#rust ✨")
        );
        assert_eq!(
            parse_query_param("?q=%40alice%40example.org", "q").as_deref(),
            Some("@alice@example.org")
        );
        assert_eq!(decode_query("hello+world%21"), "hello world!");
    }
}
