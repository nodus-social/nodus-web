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
