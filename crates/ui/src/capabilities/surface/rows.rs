//! The table: one row per surface, and the four facts a page reads off it.
//!
//! A table rather than four match arms in the parent, because the rows are only ever read
//! together — the capability a row is gated on, the name a server publishes for it, the word
//! a page calls it, and the sentence it says when its server does not serve it. Keeping them
//! on one line is what keeps them from drifting apart, and it is what the parent's test pins.

use super::Surface;
use crate::capabilities::Feature;

/// The facts of one row: capability, wire name, page word, absent sentence.
pub(super) type Row = (Feature, &'static str, &'static str, &'static str);

/// The row that decides `surface`.
///
/// Total: every variant has a line, so a new surface cannot be drawn without a capability to
/// gate it on and a sentence to say when that capability is missing.
#[expect(
    clippy::too_many_lines,
    reason = "one line per row is the whole point: a surface's four facts are read together"
)]
pub(super) const fn row(surface: Surface) -> Row {
    match surface {
        Surface::Feed => (
            Feature::FeedAccounts,
            "feed_accounts",
            "Recommended posts",
            "This instance has no recommendation engine, so there is nothing to rank here.",
        ),
        Surface::Admin => (
            Feature::Admin,
            "admin",
            "Administration",
            "This instance does not serve its administration API to this app.",
        ),
        Surface::Moderation => (
            Feature::Moderation,
            "moderation",
            "Moderation",
            "This instance does not serve a moderation API to this app.",
        ),
        Surface::Drive => (
            Feature::Drive,
            "drive",
            "Drive",
            "This instance has no file storage API.",
        ),
        Surface::Lists => (
            Feature::Lists,
            "lists",
            "Lists",
            "This instance does not serve lists.",
        ),
        Surface::Filters => (
            Feature::Filters,
            "filters",
            "Filters",
            "This instance does not serve filters.",
        ),
        Surface::Bookmarks => (
            Feature::Bookmarks,
            "bookmarks",
            "Bookmarks",
            "This instance does not keep bookmarks.",
        ),
        Surface::Favourites => (
            Feature::Favourites,
            "favourites",
            "Favourites",
            "This instance does not keep favourites.",
        ),
        Surface::Scheduled => (
            Feature::ScheduledStatuses,
            "scheduled_statuses",
            "Scheduled posts",
            "This instance does not keep scheduled posts.",
        ),
        Surface::Notifications => (
            Feature::Notifications,
            "notifications",
            "Notifications",
            "This instance does not serve a notifications API.",
        ),
        Surface::Announcements => (
            Feature::Announcements,
            "announcements",
            "Announcements",
            "This instance publishes no announcements.",
        ),
        Surface::Search => (
            Feature::Search,
            "search",
            "Search",
            "This instance does not serve a search API.",
        ),
        Surface::Directory => (
            Feature::Directory,
            "directory",
            "Directory",
            "This instance does not suggest accounts.",
        ),
        Surface::Markers => (
            Feature::Markers,
            "markers",
            "Read positions",
            "This instance does not keep read positions.",
        ),
        Surface::Conversations => (
            Feature::Conversations,
            "conversations",
            "Messages",
            "This instance does not serve direct conversations.",
        ),
        Surface::AdminAnnouncements => (
            Feature::AdminAnnouncements,
            "admin_announcements",
            "Announcements",
            "This instance does not serve the announcements editor.",
        ),
        Surface::AdminAudit => (
            Feature::AdminAudit,
            "admin_audit",
            "Audit log",
            "This instance does not serve the audit log view.",
        ),
        Surface::AdminDomains => (
            Feature::AdminDomains,
            "admin_domains",
            "Allowed domains",
            "This instance does not serve the allowed-domains view.",
        ),
        Surface::AdminEmail => (
            Feature::AdminEmail,
            "admin_email",
            "Mail domains",
            "This instance does not serve the mail-domains view.",
        ),
        Surface::AdminEscalation => (
            Feature::AdminEscalation,
            "admin_escalation",
            "Escalation",
            "This instance does not serve the escalation view.",
        ),
        Surface::AdminFederation => (
            Feature::AdminFederation,
            "admin_federation",
            "Deliveries",
            "This instance does not serve the federation-deliveries view.",
        ),
        Surface::AdminOverview => (
            Feature::AdminOverview,
            "admin_overview",
            "Overview",
            "This instance does not serve the administration overview.",
        ),
        Surface::AdminProtocol => (
            Feature::AdminProtocol,
            "admin_protocol",
            "Protocol",
            "This instance does not serve the protocol view.",
        ),
        Surface::AdminReplies => (
            Feature::AdminReplies,
            "admin_replies",
            "Canned replies",
            "This instance does not serve the canned-replies view.",
        ),
        Surface::AdminRules => (
            Feature::AdminRules,
            "admin_rules",
            "Instance rules",
            "This instance does not serve the instance-rules view.",
        ),
        Surface::AdminScanners => (
            Feature::AdminScanners,
            "admin_scanners",
            "Scanners",
            "This instance does not serve the scanners view.",
        ),
        Surface::AdminSettings => (
            Feature::AdminSettings,
            "admin_settings",
            "Settings transfer",
            "This instance does not serve the settings-transfer view.",
        ),
        Surface::AdminTopology => (
            Feature::AdminTopology,
            "admin_topology",
            "System",
            "This instance does not serve the topology view.",
        ),
        Surface::ModerationAccounts => (
            Feature::ModerationAccounts,
            "moderation_accounts",
            "Accounts",
            "This instance does not serve the account directory.",
        ),
        Surface::ModerationAppeals => (
            Feature::ModerationAppeals,
            "moderation_appeals",
            "Appeals",
            "This instance does not serve the appeals queue.",
        ),
        Surface::ModerationFederation => (
            Feature::ModerationFederation,
            "moderation_federation",
            "Federation",
            "This instance does not serve the moderation federation view.",
        ),
        Surface::ModerationFlags => (
            Feature::ModerationFlags,
            "moderation_flags",
            "Scans",
            "This instance does not serve the scan queue.",
        ),
        Surface::ModerationMedia => (
            Feature::ModerationMedia,
            "moderation_media",
            "Media",
            "This instance does not serve the quarantined-media view.",
        ),
        Surface::ModerationRegistrations => (
            Feature::ModerationRegistrations,
            "moderation_registrations",
            "Registrations",
            "This instance does not serve the registration queue.",
        ),
        Surface::ModerationReports => (
            Feature::ModerationReports,
            "moderation_reports",
            "Reports",
            "This instance does not serve the report queue.",
        ),
        Surface::ModerationRules => (
            Feature::ModerationRules,
            "moderation_rules",
            "Rules",
            "This instance does not serve the moderation-rules view.",
        ),
    }
}
