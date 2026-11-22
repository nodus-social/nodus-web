//! The surfaces a page is built from, and the capability that decides each.
//!
//! A table, not a rule: one row says which of the server's published names gates a door, and
//! every gate in the crate reads it through [`Surface::served`]. It lives beside the gate
//! rather than in it so the mapping can grow a surface without the gate changing, and the
//! prose a reader sees when a door is missing stays next to the row that decides it.

use super::{Capabilities, Feature};

/// One surface a page can draw, named by the capability that decides whether it exists.
///
/// The name is the string a server publishes in `features`, which is why it is kept beside
/// the typed row: a refusal that reaches [`Surface::named_by`] is spelled that way, and the
/// test suite pins the two spellings to each other.
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
    /// The capability this surface is gated on.
    ///
    /// Total, and deliberately so: the enum names all 42 names a Nodus server publishes, so
    /// every surface a page can draw has one row on every protocol, and there is no surface
    /// left for a per-protocol default to guess at. A surface without a row here would be a
    /// surface drawn blind.
    #[must_use]
    pub const fn capability(self) -> Feature {
        match self {
            Self::Feed => Feature::FeedAccounts,
            Self::Admin => Feature::Admin,
            Self::Moderation => Feature::Moderation,
            Self::Drive => Feature::Drive,
            Self::Lists => Feature::Lists,
            Self::Filters => Feature::Filters,
            Self::Bookmarks => Feature::Bookmarks,
            Self::Favourites => Feature::Favourites,
            Self::Scheduled => Feature::ScheduledStatuses,
            Self::Notifications => Feature::Notifications,
            Self::Announcements => Feature::Announcements,
            Self::Search => Feature::Search,
            Self::Directory => Feature::Directory,
            Self::Markers => Feature::Markers,
            Self::Conversations => Feature::Conversations,
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
    ///
    /// One bit test for all three protocols. A Nodus answer is the list it published; the two
    /// that publish none are the SDK's probed baseline, which holds a decided row for this
    /// surface as well. A surface absent from either set is a refusal for this server, and it
    /// stays a refusal: a protocol being unable to name a surface is not evidence that it
    /// serves one, and the guess that it was is what drew Misskey's bookmarks and
    /// conversations — doors that server answers with a 404.
    #[must_use]
    pub fn served(self, at: &Capabilities) -> bool {
        at.supports(self.capability())
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

#[cfg(test)]
mod tests;
