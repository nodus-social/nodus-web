//! The mapping itself: the wire name, the typed row, and the path each surface sits at.

use super::*;

/// The wire name and the typed row must spell the same capability: a Nodus server that
/// publishes `bookmarks` would otherwise light a different door than the one a baseline
/// gates. And no two surfaces may share a row, which would gate one on another's evidence.
#[test]
fn every_surface_names_its_own_capability_on_the_wire() {
    for surface in Surface::all() {
        assert_eq!(surface.name(), surface.capability().as_str(), "{surface:?}");
        let sharing = Surface::all()
            .into_iter()
            .filter(|other| other.capability() == surface.capability())
            .count();
        assert_eq!(sharing, 1, "{surface:?} shares its row");
    }
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

/// A refusal is the same sentence a page would have said, so a stale control explains itself
/// instead of reporting a fault.
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

/// A page is found by the address its own area draws it at, and only by that address: the
/// strip filters on this, so a slug that resolved to the wrong row would advertise one page
/// and gate another.
#[test]
fn every_page_is_found_by_its_own_slug() {
    for (slug, page) in [
        ("overview", Surface::AdminOverview),
        ("scanners", Surface::AdminScanners),
        ("domains", Surface::AdminDomains),
        ("email", Surface::AdminEmail),
        ("replies", Surface::AdminReplies),
        ("escalation", Surface::AdminEscalation),
        ("federation", Surface::AdminFederation),
        ("announcements", Surface::AdminAnnouncements),
        ("rules", Surface::AdminRules),
        ("audit", Surface::AdminAudit),
        ("system", Surface::AdminTopology),
        ("protocol", Surface::AdminProtocol),
        ("settings-transfer", Surface::AdminSettings),
    ] {
        assert_eq!(Surface::admin_page(slug), Some(page), "{slug}");
    }
    for (slug, page) in [
        ("reports", Surface::ModerationReports),
        ("appeals", Surface::ModerationAppeals),
        ("scans", Surface::ModerationFlags),
        ("media", Surface::ModerationMedia),
        ("accounts", Surface::ModerationAccounts),
        ("registrations", Surface::ModerationRegistrations),
        ("federation", Surface::ModerationFederation),
        ("rules", Surface::ModerationRules),
    ] {
        assert_eq!(Surface::moderation_section(slug), Some(page), "{slug}");
    }
    assert_eq!(Surface::admin_page("saved"), None);
    assert_eq!(Surface::moderation_section("overview"), None);
}

/// The strip is a list of surfaces, not of sections: every page an area can draw has a row
/// with its own capability, so no page can be shown on another page's evidence.
#[test]
fn every_page_row_is_inside_its_own_area() {
    assert_eq!(super::admin::ADMIN_PAGES.len(), 13);
    assert_eq!(super::moderation::MODERATION_SECTIONS.len(), 8);
    for page in super::admin::ADMIN_PAGES
        .iter()
        .chain(super::moderation::MODERATION_SECTIONS.iter())
    {
        assert!(Surface::all().contains(page), "{page:?} has no row");
        assert!(!page.absent().is_empty(), "{page:?} explains nothing");
    }
}
