//! The administration area's pages, as surfaces.
//!
//! One row per section the area draws, in the order the strip shows them. The slug is the
//! `Section`'s own last path segment, because that is what the address and the strip key
//! carry; `system` is the one slug whose page is called topology, which is the name both the
//! address and the capability kept.

use super::Surface;

impl Surface {
    /// The page `slug` names inside the administration area, when it names one.
    #[must_use]
    pub fn admin_page(slug: &str) -> Option<Self> {
        ADMIN_PAGES
            .iter()
            .copied()
            .find(|page| page_slug(*page) == Some(slug))
    }
}

/// Every page of the administration area, in the strip's order.
pub const ADMIN_PAGES: [Surface; 13] = [
    Surface::AdminOverview,
    Surface::AdminScanners,
    Surface::AdminDomains,
    Surface::AdminEmail,
    Surface::AdminReplies,
    Surface::AdminEscalation,
    Surface::AdminFederation,
    Surface::AdminAnnouncements,
    Surface::AdminRules,
    Surface::AdminAudit,
    Surface::AdminTopology,
    Surface::AdminProtocol,
    Surface::AdminSettings,
];

/// The slug a page is addressed by.
fn page_slug(page: Surface) -> Option<&'static str> {
    match page {
        Surface::AdminOverview => Some("overview"),
        Surface::AdminScanners => Some("scanners"),
        Surface::AdminDomains => Some("domains"),
        Surface::AdminEmail => Some("email"),
        Surface::AdminReplies => Some("replies"),
        Surface::AdminEscalation => Some("escalation"),
        Surface::AdminFederation => Some("federation"),
        Surface::AdminAnnouncements => Some("announcements"),
        Surface::AdminRules => Some("rules"),
        Surface::AdminAudit => Some("audit"),
        Surface::AdminTopology => Some("system"),
        Surface::AdminProtocol => Some("protocol"),
        Surface::AdminSettings => Some("settings-transfer"),
        _ => None,
    }
}
