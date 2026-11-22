//! The gate itself: one answer per protocol, and the door each one draws.

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

/// Every surface against the three answers, as the probes decided them:
/// `(surface, nodus, mastodon, misskey)`. Nodus is a server publishing the full list; the
/// two baselines are the SDK's rows, which answer whether the server has the route at all.
const SURFACES: &[(Surface, bool, bool, bool)] = &[
    (Surface::Feed, true, false, false),
    (Surface::Admin, true, true, true),
    (Surface::Moderation, true, true, true),
    (Surface::Drive, true, false, true),
    (Surface::Lists, true, true, true),
    (Surface::Filters, true, true, false),
    (Surface::Bookmarks, true, true, false),
    (Surface::Favourites, true, true, true),
    (Surface::Scheduled, true, true, false),
    (Surface::Notifications, true, true, true),
    (Surface::Announcements, true, true, true),
    (Surface::Search, true, true, true),
    (Surface::Directory, true, true, true),
    (Surface::Markers, true, true, false),
    (Surface::Conversations, true, true, false),
];

/// The three answers a gate has to hold for: our own list, and the two baselines the SDK
/// probed. What the surfaces are worth is the point — a door the server serves is drawn, and
/// nothing else is, on every one of the fifteen.
#[test]
fn the_three_answers_gate_all_fifteen_surfaces() {
    assert_eq!(SURFACES.len(), Surface::all().len());
    for surface in Surface::all() {
        assert!(
            SURFACES.iter().any(|(row, ..)| *row == surface),
            "{surface:?} is missing from the table"
        );
    }

    let nodus = full_nodus();
    let mastodon = Capabilities::mastodon();
    let misskey = Capabilities::misskey();
    for &(surface, in_nodus, in_mastodon, in_misskey) in SURFACES {
        assert_eq!(surface.served(&nodus), in_nodus, "nodus {}", surface.name());
        assert_eq!(
            surface.served(&mastodon),
            in_mastodon,
            "mastodon {}",
            surface.name()
        );
        assert_eq!(
            surface.served(&misskey),
            in_misskey,
            "misskey {}",
            surface.name()
        );
    }
}

/// The regression the completed vocabulary removes. While the enum could not spell these
/// names, `served` answered `true` for them on both baselines, so a Misskey session drew
/// bookmarks its server answers with a 404 and a Mastodon session drew a drive it has no
/// route for. Both doors are gone, and both paths still name the surface, which is what
/// makes the gate reach them.
#[test]
fn a_misskey_session_draws_no_bookmarks_and_a_mastodon_session_no_drive() {
    let misskey = Capabilities::misskey();
    assert_eq!(
        Surface::of_path("/saved/bookmarks"),
        Some(Surface::Bookmarks)
    );
    assert!(!Surface::Bookmarks.served(&misskey));
    assert!(!admits(Some(&misskey), Surface::Bookmarks));
    assert!(!Surface::Conversations.served(&misskey));

    let mastodon = Capabilities::mastodon();
    assert_eq!(Surface::of_path("/drive"), Some(Surface::Drive));
    assert!(!Surface::Drive.served(&mastodon));
    assert!(!admits(Some(&mastodon), Surface::Drive));
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

/// The page a reader lands on for an absent surface says so, and still offers the way out of
/// it: no toast, no failure.
#[test]
fn the_absent_surface_paints_an_explanation() {
    use leptos::prelude::*;
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
