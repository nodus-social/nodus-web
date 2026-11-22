//! The moderation area's sections, as surfaces.
//!
//! One row per section the area draws, in the order the strip shows them. The slug is the
//! `Section`'s own last path segment. `scans` is the slug of the page the capability calls
//! `moderation_flags`: the panel lists what the scanners flagged, and the two words have
//! meant the same page since it was written.

use super::Surface;

impl Surface {
    /// The section `slug` names inside the moderation area, when it names one.
    #[must_use]
    pub fn moderation_section(slug: &str) -> Option<Self> {
        MODERATION_SECTIONS
            .iter()
            .copied()
            .find(|page| section_slug(*page) == Some(slug))
    }
}

/// Every section of the moderation area, in the strip's order.
pub const MODERATION_SECTIONS: [Surface; 8] = [
    Surface::ModerationReports,
    Surface::ModerationAppeals,
    Surface::ModerationFlags,
    Surface::ModerationMedia,
    Surface::ModerationAccounts,
    Surface::ModerationRegistrations,
    Surface::ModerationFederation,
    Surface::ModerationRules,
];

/// The slug a section is addressed by.
fn section_slug(section: Surface) -> Option<&'static str> {
    match section {
        Surface::ModerationReports => Some("reports"),
        Surface::ModerationAppeals => Some("appeals"),
        Surface::ModerationFlags => Some("scans"),
        Surface::ModerationMedia => Some("media"),
        Surface::ModerationAccounts => Some("accounts"),
        Surface::ModerationRegistrations => Some("registrations"),
        Surface::ModerationFederation => Some("federation"),
        Surface::ModerationRules => Some("rules"),
        _ => None,
    }
}
