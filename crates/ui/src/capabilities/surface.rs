//! The surfaces a page is built from, and the capability that decides each.
//!
//! A table, not a rule: one row says which of the server's published names gates a door, and
//! every gate in the crate reads it through [`Surface::served`]. It lives beside the gate
//! rather than in it so the mapping can grow a surface without the gate changing, and the
//! prose a reader sees when a door is missing stays next to the row that decides it.
//!
//! Two kinds of row live here. The first fifteen are the areas a rail draws a door for. The
//! rest are the pages inside the two staff areas: an instrument that serves `admin` may still
//! not serve one of its screens, and the operator's rule is to omit that screen rather than
//! the whole area. A page row is what lets one door stay while one tab goes.

use super::{Capabilities, Feature};

mod admin;
mod moderation;
mod rows;

// The slug tables live in the child modules; the lookups they feed are inherent methods on
// `Surface` (`admin_page`, `moderation_section`), so a strip in another crate names a page by
// the address it already has and no second import is needed.

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
    /// `/admin/announcements`.
    AdminAnnouncements,
    /// `/admin/audit`.
    AdminAudit,
    /// `/admin/domains`.
    AdminDomains,
    /// `/admin/email`.
    AdminEmail,
    /// `/admin/escalation`.
    AdminEscalation,
    /// `/admin/federation`.
    AdminFederation,
    /// `/admin/overview`.
    AdminOverview,
    /// `/admin/protocol`.
    AdminProtocol,
    /// `/admin/replies`.
    AdminReplies,
    /// `/admin/rules`.
    AdminRules,
    /// `/admin/scanners`.
    AdminScanners,
    /// `/admin/settings-transfer`.
    AdminSettings,
    /// `/admin/system`.
    AdminTopology,
    /// `/moderation/accounts`.
    ModerationAccounts,
    /// `/moderation/appeals`.
    ModerationAppeals,
    /// `/moderation/federation`.
    ModerationFederation,
    /// `/moderation/flags`.
    ModerationFlags,
    /// `/moderation/media`.
    ModerationMedia,
    /// `/moderation/registrations`.
    ModerationRegistrations,
    /// `/moderation/reports`.
    ModerationReports,
    /// `/moderation/rules`.
    ModerationRules,
}

impl Surface {
    /// The capability this surface is gated on.
    ///
    /// Total, and deliberately so: every row names one of the published capabilities, so
    /// every surface a page can draw is decided by a server's answer on every protocol, and
    /// there is no surface left for a per-protocol default to guess at. A surface without a
    /// row here would be a surface drawn blind.
    #[must_use]
    pub const fn capability(self) -> Feature {
        rows::row(self).0
    }

    /// The name a server publishes for this surface, matching the backend's `FEATURES`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        rows::row(self).1
    }

    /// What the page calls this surface.
    #[must_use]
    pub const fn title(self) -> &'static str {
        rows::row(self).2
    }

    /// The one sentence a page says instead of the surface, in the reader's terms.
    ///
    /// It names the server, not the reader: nothing here is something the reader did wrong,
    /// and nothing they can retry. It is also the sentence a page *inside* an area says when
    /// only that page is missing, which is why it names the screen rather than the area.
    #[must_use]
    pub const fn absent(self) -> &'static str {
        rows::row(self).3
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
    /// An area, not a page: the rail draws one door per area, and a door into a section the
    /// instrument no longer serves would be the strip's problem, not the rail's.
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
            .iter()
            .copied()
            .find(|surface| surface.name() == name)
    }

    /// Every surface, for the lookups above.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        ALL
    }
}

/// Every row, in the order the areas are listed and then the pages they hold.
const ALL: &[Surface] = &[
    Surface::Feed,
    Surface::Admin,
    Surface::Moderation,
    Surface::Drive,
    Surface::Lists,
    Surface::Filters,
    Surface::Bookmarks,
    Surface::Favourites,
    Surface::Scheduled,
    Surface::Notifications,
    Surface::Announcements,
    Surface::Search,
    Surface::Directory,
    Surface::Markers,
    Surface::Conversations,
    Surface::AdminAnnouncements,
    Surface::AdminAudit,
    Surface::AdminDomains,
    Surface::AdminEmail,
    Surface::AdminEscalation,
    Surface::AdminFederation,
    Surface::AdminOverview,
    Surface::AdminProtocol,
    Surface::AdminReplies,
    Surface::AdminRules,
    Surface::AdminScanners,
    Surface::AdminSettings,
    Surface::AdminTopology,
    Surface::ModerationAccounts,
    Surface::ModerationAppeals,
    Surface::ModerationFederation,
    Surface::ModerationFlags,
    Surface::ModerationMedia,
    Surface::ModerationRegistrations,
    Surface::ModerationReports,
    Surface::ModerationRules,
];

#[cfg(test)]
mod tests;
