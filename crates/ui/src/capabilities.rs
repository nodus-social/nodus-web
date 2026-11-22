//! What this server serves, as the shared screens ask it.
//!
//! One value decides every door in this crate, and it lives here rather than in the client
//! because the same page code paints on both sides: a rule written twice would be a rule
//! that disagrees with itself the first time one side learns a new surface.
//!
//! The answer itself comes from the SDK ([`nodus_protocol::Capabilities`]): a Nodus server
//! publishes its list at `/api/v1/meta`, and Mastodon and Misskey publish nothing, so the
//! SDK answers those from a probed baseline. This module adds only what the views need on
//! top of it — the names of the surfaces a page is built from, and what to say when one is
//! missing.

use leptos::prelude::*;

pub use nodus_protocol::{Capabilities, Feature, Provider};

/// One surface a page can draw, named by the capability that decides whether it exists.
///
/// The wire name is the string a server publishes in `features`, which is what lets a Nodus
/// server answer for surfaces this build's [`Feature`] enum does not spell (lists, bookmarks,
/// conversations, …): those arrive in [`Capabilities::unknown`] and are still an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// Recommended posts, which only a server with a recommender serves.
    Feed,
    /// The administration area.
    Admin,
    /// The moderation queue.
    Moderation,
    /// The account's own file storage.
    Drive,
    /// Lists of accounts, each with a timeline.
    Lists,
    /// Filters over the reader's timeline.
    Filters,
    /// Saved posts.
    Bookmarks,
    /// Favourited posts.
    Favourites,
    /// Posts waiting to be published.
    Scheduled,
    /// Mentions, boosts, favourites and follow requests.
    Notifications,
    /// Announcements from the instance.
    Announcements,
    /// Accounts, posts and hashtags.
    Search,
    /// The accounts an instance suggests.
    Directory,
    /// Read positions across devices.
    Markers,
    /// Direct conversations.
    Conversations,
}

impl Surface {
    /// The capability this surface is gated on, when this build can spell it.
    ///
    /// `None` is a surface the protocol enum has no name for; it is answered from the
    /// published name instead, and from a baseline on the two protocols that publish none.
    #[must_use]
    pub const fn capability(self) -> Option<Feature> {
        match self {
            Self::Feed => Some(Feature::FeedAccounts),
            Self::Admin => Some(Feature::Admin),
            Self::Moderation => Some(Feature::Moderation),
            Self::Drive => Some(Feature::Drive),
            Self::Favourites => Some(Feature::Favourites),
            Self::Lists
            | Self::Filters
            | Self::Bookmarks
            | Self::Scheduled
            | Self::Notifications
            | Self::Announcements
            | Self::Search
            | Self::Directory
            | Self::Markers
            | Self::Conversations => None,
        }
    }

    /// The name a server publishes for this surface, matching the backend's `FEATURES`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Feed => "feed_accounts",
            Self::Admin => "admin",
            Self::Moderation => "moderation",
            Self::Drive => "drive",
            Self::Lists => "lists",
            Self::Filters => "filters",
            Self::Bookmarks => "bookmarks",
            Self::Favourites => "favourites",
            Self::Scheduled => "scheduled_statuses",
            Self::Notifications => "notifications",
            Self::Announcements => "announcements",
            Self::Search => "search",
            Self::Directory => "directory",
            Self::Markers => "markers",
            Self::Conversations => "conversations",
        }
    }

    /// What the page calls this surface.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Feed => "Recommended posts",
            Self::Admin => "Administration",
            Self::Moderation => "Moderation",
            Self::Drive => "Drive",
            Self::Lists => "Lists",
            Self::Filters => "Filters",
            Self::Bookmarks => "Bookmarks",
            Self::Favourites => "Favourites",
            Self::Scheduled => "Scheduled posts",
            Self::Notifications => "Notifications",
            Self::Announcements => "Announcements",
            Self::Search => "Search",
            Self::Directory => "Directory",
            Self::Markers => "Read positions",
            Self::Conversations => "Messages",
        }
    }

    /// The one sentence a page says instead of the surface, in the reader's terms.
    ///
    /// It names the server, not the reader: nothing here is something the reader did wrong,
    /// and nothing they can retry.
    #[must_use]
    pub const fn absent(self) -> &'static str {
        match self {
            Self::Feed => {
                "This instance has no recommendation engine, so there is nothing to rank here."
            }
            Self::Admin => "This instance does not serve its administration API to this app.",
            Self::Moderation => "This instance does not serve a moderation API to this app.",
            Self::Drive => "This instance has no file storage API.",
            Self::Lists => "This instance does not serve lists.",
            Self::Filters => "This instance does not serve filters.",
            Self::Bookmarks => "This instance does not keep bookmarks.",
            Self::Favourites => "This instance does not keep favourites.",
            Self::Scheduled => "This instance does not keep scheduled posts.",
            Self::Notifications => "This instance does not serve a notifications API.",
            Self::Announcements => "This instance publishes no announcements.",
            Self::Search => "This instance does not serve a search API.",
            Self::Directory => "This instance does not suggest accounts.",
            Self::Markers => "This instance does not keep read positions.",
            Self::Conversations => "This instance does not serve direct conversations.",
        }
    }

    /// Whether `at` says the server serves this surface.
    #[must_use]
    pub fn served(self, at: &Capabilities) -> bool {
        match at.provider() {
            // Nodus publishes the list, so a name it omits is not served. A name this build
            // cannot spell is still in `unknown`, and the server meant it.
            Provider::Nodus => match self.capability() {
                Some(feature) => at.features.contains(feature),
                None => at.unknown.iter().any(|name| name == self.name()),
            },
            // Mastodon and Misskey publish nothing. Their `Feature` rows are the SDK's, so
            // the answer for a named capability is the SDK's; the surfaces the protocol has
            // no name for are served by both, which is why the baseline below is `true`.
            Provider::Mastodon | Provider::Misskey => match self.capability() {
                Some(feature) => at.supports(feature),
                None => true,
            },
        }
    }

    /// The surface a navigation entry leads to, when it leads to one.
    ///
    /// `None` is a destination every server answers: the reader's home timeline, the
    /// composer, and settings, which is where an account is edited rather than a service.
    #[must_use]
    pub fn of_path(path: &str) -> Option<Self> {
        let path = path.trim_end_matches('/');
        let first = path.split('/').nth(1).unwrap_or_default();
        match first {
            "feed" => Some(Self::Feed),
            "admin" => Some(Self::Admin),
            "moderation" => Some(Self::Moderation),
            "drive" => Some(Self::Drive),
            "lists" => Some(Self::Lists),
            "filters" => Some(Self::Filters),
            "scheduled" => Some(Self::Scheduled),
            "notifications" => Some(Self::Notifications),
            "announcements" => Some(Self::Announcements),
            "search" => Some(Self::Search),
            "directory" => Some(Self::Directory),
            "markers" => Some(Self::Markers),
            "messages" => Some(Self::Conversations),
            "saved" => match path.split('/').nth(2) {
                Some("bookmarks") => Some(Self::Bookmarks),
                _ => Some(Self::Favourites),
            },
            _ => None,
        }
    }

    /// The surface an error names, when it is the SDK's capability refusal.
    ///
    /// A control drawn before the server said what it serves still fails; this is what turns
    /// that failure into the same sentence the page itself would have said, instead of a
    /// fault the reader cannot act on.
    #[must_use]
    pub fn named_by(error: &str) -> Option<Self> {
        let name = error
            .split_once("this server does not support ")
            .and_then(|(_, rest)| rest.split_whitespace().next())?;
        Self::all()
            .into_iter()
            .find(|surface| surface.name() == name)
    }

    /// Every surface, for the lookups above.
    #[must_use]
    pub const fn all() -> [Self; 15] {
        [
            Self::Feed,
            Self::Admin,
            Self::Moderation,
            Self::Drive,
            Self::Lists,
            Self::Filters,
            Self::Bookmarks,
            Self::Favourites,
            Self::Scheduled,
            Self::Notifications,
            Self::Announcements,
            Self::Search,
            Self::Directory,
            Self::Markers,
            Self::Conversations,
        ]
    }
}

/// Whether a paint that may not know the answer yet should draw a door.
///
/// `None` is "nobody has asked": before `/api/v1/meta` answers, and on a server render whose
/// page carried no capability list. Drawing the door there is the honest choice — hiding a
/// surface the server does serve makes it unreachable for as long as a probe takes — and a
/// route that turns out to be absent explains itself instead of failing.
#[must_use]
pub fn admits(at: Option<&Capabilities>, surface: Surface) -> bool {
    at.is_none_or(|at| surface.served(at))
}

/// What the paints hand the shared screens: one value, or nothing known yet.
pub type Serves = Option<Signal<Capabilities>>;

/// Put the negotiated answer in scope for the screens below.
///
/// A context rather than a prop because the frame is built by both artifacts, and only one
/// of them has an SDK to ask: a prop would have to be threaded through every screen and
/// every server call site for a value the server cannot know.
pub fn provide_serves(at: impl Into<Signal<Capabilities>>) {
    provide_context(at.into());
}

/// The answer in scope, if this paint has one.
#[must_use]
pub fn serves() -> Serves {
    use_context::<Signal<Capabilities>>()
}

/// What a failed command should be shown as.
///
/// A command that fails because the server does not serve it arrives as the SDK's own
/// display text, naming the feature; the reader gets the sentence a page for that surface
/// would have said, because a control drawn before the answer arrived is not a fault they
/// can act on. Anything else is left exactly as the transport said it.
#[must_use]
pub fn explain(error: Option<String>) -> String {
    error.map_or_else(String::new, |error| {
        Surface::named_by(&error).map_or(error.clone(), |surface| surface.absent().to_owned())
    })
}

/// A page for a surface the server does not serve.
///
/// It is a page and not a toast: the address is real, the reader followed a link to it, and
/// what they need is one sentence about this server plus the way back into the site.
#[component]
pub fn UnsupportedSurface(surface: Surface) -> impl IntoView {
    view! {
        <crate::Screen>
            <crate::PageHeader
                title=surface.title()
                description="Not served by this instance"
            />
            <crate::Note>
                {surface.absent()}
                " "
                <a class="link link-hover" href="/home">
                    "Go to the home timeline"
                </a>
            </crate::Note>
        </crate::Screen>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Nodus answer as `/api/v1/meta` publishes it: the backend's `FEATURES` names.
    fn nodus(features: &[&str]) -> Capabilities {
        let names = features
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        Capabilities::from_parts("0.1.0", &names)
    }

    fn full_nodus() -> Capabilities {
        nodus(&[
            "favourites",
            "bookmarks",
            "conversations",
            "lists",
            "filters",
            "scheduled_statuses",
            "notifications",
            "announcements",
            "search",
            "directory",
            "markers",
            "admin",
            "moderation",
            "drive",
            "feed_accounts",
        ])
    }

    /// The three answers a gate has to hold for: our own list, and the two baselines the SDK
    /// probed. What the surfaces are worth is the point — a door the server serves is drawn,
    /// and nothing else is.
    #[test]
    fn the_three_answers_decide_the_same_surfaces_differently() {
        let nodus = full_nodus();
        let mastodon = Capabilities::mastodon();
        let misskey = Capabilities::misskey();

        // A Nodus server that lists them serves them, including the names this build's
        // `Feature` enum cannot spell.
        for surface in [
            Surface::Lists,
            Surface::Favourites,
            Surface::Admin,
            Surface::Feed,
        ] {
            assert!(surface.served(&nodus), "{} is listed", surface.name());
        }

        // Both baselines serve administration, moderation and favourites; neither serves the
        // recommendation feed, and the SDK's table is what says so.
        for at in [&mastodon, &misskey] {
            assert!(Surface::Admin.served(at));
            assert!(Surface::Moderation.served(at));
            assert!(Surface::Favourites.served(at));
            assert!(!Surface::Feed.served(at), "no recommender behind either");
        }

        // The drive is the one row the two disagree on: a Mastodon server has no storage API
        // at all (`/api/v1/drive` is a 404 there), a Misskey server has one.
        assert!(!Surface::Drive.served(&mastodon));
        assert!(Surface::Drive.served(&misskey));

        // Lists, bookmarks and conversations are served by both even though the protocol
        // enum has no name for them: they are baseline rows, not guesses.
        assert!(Surface::Lists.served(&mastodon));
        assert!(Surface::Bookmarks.served(&misskey));
        assert!(Surface::Conversations.served(&mastodon));
    }

    /// A Nodus instance that serves lists but not the drive draws the first and explains the
    /// second, and an answer nobody asked for yet hides nothing.
    #[test]
    fn an_absent_name_hides_its_door_and_unknown_hides_nothing() {
        let at = nodus(&["lists", "favourites", "notifications"]);
        assert!(Surface::Lists.served(&at));
        assert!(!Surface::Drive.served(&at));
        // Nothing asked yet: the door stays, because a missing answer is not a refusal.
        assert!(admits(None, Surface::Drive));
        assert!(admits(Some(&at), Surface::Lists));
        assert!(!admits(Some(&at), Surface::Drive));
    }

    /// The navigation entry a path leads to is what the rail filters on.
    #[test]
    fn every_navigation_entry_names_its_surface() {
        assert_eq!(
            Surface::of_path("/saved/bookmarks"),
            Some(Surface::Bookmarks)
        );
        assert_eq!(
            Surface::of_path("/saved/favourites"),
            Some(Surface::Favourites)
        );
        assert_eq!(Surface::of_path("/scheduled"), Some(Surface::Scheduled));
        assert_eq!(Surface::of_path("/lists"), Some(Surface::Lists));
        assert_eq!(Surface::of_path("/admin/reports"), Some(Surface::Admin));
        assert_eq!(Surface::of_path("/home"), None);
        assert_eq!(Surface::of_path("/settings/account"), None);
    }

    /// A refusal is the same sentence a page would have said, so a stale control explains
    /// itself instead of reporting a fault.
    #[test]
    fn a_capability_refusal_becomes_the_explanation() {
        let error = "GET /api/v1/lists is not served: this server does not support lists";
        assert_eq!(Surface::named_by(error), Some(Surface::Lists));
        assert_eq!(
            Surface::Lists.absent(),
            "This instance does not serve lists."
        );
        assert_eq!(Surface::named_by("404 Not Found"), None);
    }

    /// The page a reader lands on for an absent surface says so, and still offers the way
    /// out of it: no toast, no failure.
    #[test]
    fn the_absent_surface_paints_an_explanation() {
        use leptos::prelude::*;
        use leptos::tachys::view::RenderHtml as _;
        let props = UnsupportedSurfaceProps {
            surface: Surface::Lists,
        };
        let html = Owner::new().with(|| UnsupportedSurface(props).to_html());
        assert!(html.contains("Lists"), "{html}");
        assert!(
            html.contains("This instance does not serve lists."),
            "{html}"
        );
        assert!(html.contains("href=\"/home\""), "{html}");
    }
}
