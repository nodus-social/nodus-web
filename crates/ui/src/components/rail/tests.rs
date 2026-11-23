//! A rail group is a heading and the doors under it, and either can lose the other.
//!
//! The staff group is the one both paints assemble from a capability answer plus a role, so
//! it is where an empty heading shows up: the doors are correctly dropped and the heading
//! is left naming a section that has nothing in it.

use std::sync::Arc;

use leptos::prelude::*;

use super::{ExtraDoors, Rail};
use crate::capabilities::{Capabilities, provide_serves, staff_door};
use crate::icons::Icon;
use crate::session_style::{
    BannerRegionKind, ImageSlot, NavItemConfig, NavPlacement, NavRail, SessionStyle, StyleEditor,
};

/// One door, drawn the way the client draws its staff rows.
fn door(label: &'static str, href: &'static str) -> AnyView {
    view! {
        <super::RailEntry
            label=label
            hint=label
            href=href
            icon=Icon::Shield
            active=Signal::derive(|| false)
        />
    }
    .into_any()
}

/// The staff doors a paint builds from the three facts its gate reads.
///
/// The heading's answer and the rows are computed through the same `staff_door` calls, as
/// both paints do: an absent capability must leave neither a door nor the "Staff" heading
/// that would have named it. The third fact is eligibility — positive evidence of a staff
/// role — and not a grant, which the pages behind the door ask for themselves.
fn staff_doors(extensions: bool, served: [bool; 2], eligible: bool) -> ExtraDoors {
    let [moderation, administration] = served;
    ExtraDoors {
        present: Signal::derive(move || {
            staff_door(extensions, moderation, eligible)
                || staff_door(extensions, administration, eligible)
        }),
        rows: Arc::new(move || {
            let moderation = staff_door(extensions, moderation, eligible)
                .then(|| door("Moderation", "/moderation"));
            let administration = staff_door(extensions, administration, eligible)
                .then(|| door("Administration", "/admin"));
            view! { {moderation} {administration} }.into_any()
        }),
    }
}

fn rendered(extra: ExtraDoors) -> String {
    Owner::new().with(|| {
        view! {
            <Rail
                active=Signal::derive(|| "/home".to_owned())
                profile=Signal::derive(|| None)
                extra=Some(extra)
            />
        }
        .to_html()
    })
}

/// An instance that serves neither staff surface draws no doors and no heading.
#[test]
fn absent_staff_capabilities_leave_no_heading() {
    let html = rendered(staff_doors(true, [false, false], true));
    assert!(!html.contains("Staff"), "{html}");
    assert!(!html.contains("Moderation"), "{html}");
    assert!(!html.contains("Administration"), "{html}");
}

/// Both surfaces served by an account with a role: the heading and both doors.
#[test]
fn both_staff_doors_are_drawn_under_their_heading() {
    let html = rendered(staff_doors(true, [true, true], true));
    assert!(html.contains("Staff"), "{html}");
    assert!(html.contains("Moderation"), "{html}");
    assert!(html.contains("Administration"), "{html}");
}

/// Only the moderation surface served: the heading stays with its one door, and the door
/// that was not served does not appear.
#[test]
fn one_staff_door_keeps_the_heading_with_one_door() {
    let html = rendered(staff_doors(true, [true, false], true));
    assert!(html.contains("Staff"), "{html}");
    assert!(html.contains("Moderation"), "{html}");
    assert!(!html.contains("Administration"), "{html}");
}

/// The door's truth table, one row per answer the account's role read can give. The rail is
/// an advertisement, so every arm that is not positive evidence of a staff role draws no
/// door: a room that answers with a refusal is worse than no door at all.
///
/// The grant is deliberately absent from this table. An eligible account that has not granted
/// yet sees the door — that is who the grant button is for — and the page behind it reads the
/// grant itself, so the two facts stay separate.
#[test]
fn the_door_is_drawn_only_on_positive_role_evidence() {
    // (what the read answered, extensions, area served, a door is drawn)
    let table = [
        ("eligible, no grant yet", true, true, true),
        ("eligible, grant in hand", true, true, true),
        ("no role on the account", true, true, false),
        ("role read refused", true, true, false),
        ("role read unsupported", true, true, false),
        ("instance serves no area", true, false, false),
        ("instance speaks no extensions", false, true, false),
    ];
    for (answer, extensions, served, drawn) in table {
        let html = rendered(staff_doors(
            extensions,
            [served, served],
            // The eligibility signal is `false` for every arm but the first two; this is the
            // single bit the rail reads, and the arms above name the reads that produce it.
            matches!(answer, "eligible, no grant yet" | "eligible, grant in hand"),
        ));
        assert_eq!(html.contains("Staff"), drawn, "{answer}: {html}");
        assert_eq!(html.contains("/admin"), drawn, "{answer}: {html}");
        // And never an empty heading: the heading is painted from the same answer the rows are.
        assert_eq!(
            html.contains("Staff"),
            html.contains("/moderation"),
            "{answer}"
        );
    }
}

/// A layout whose Library card holds only doors an instance may refuse, so a paint that
/// serves none of them has nothing left to put under the heading.
fn refusable_library_style() -> SessionStyle {
    let mut style = SessionStyle::default();
    style.nav.rails = vec![NavRail::new("library", "Library")];
    style.nav.items = ["/drive", "/lists", "/saved/bookmarks"]
        .into_iter()
        .enumerate()
        .map(|(index, path)| NavItemConfig {
            id: format!("library-{index}"),
            label: path.trim_start_matches('/').to_owned(),
            path: path.to_owned(),
            pinned: false,
            placement: NavPlacement::Left,
            rail: String::from("library"),
        })
        .collect();
    style
}

/// Paints the whole rail with a style and one capability answer in scope.
fn painted(style: SessionStyle, at: Capabilities) -> String {
    Owner::new().with(|| {
        provide_serves(Signal::stored(at));
        view! {
            <Rail
                active=Signal::derive(|| "/drive".to_owned())
                profile=Signal::derive(|| None)
                extra=None
                style=Signal::stored(style)
            />
        }
        .to_html()
    })
}

/// No staff surfaces and no other group warranted by the instance: the Library card loses
/// its heading exactly as the staff card does, because both ask whether anything survives.
#[test]
fn a_group_whose_only_doors_are_refused_loses_its_heading() {
    let empty = painted(
        refusable_library_style(),
        Capabilities::from_parts("0.1.0", &[]),
    );
    assert!(!empty.contains("Library"), "{empty}");
    assert!(!empty.contains("data-nav-rail="), "{empty}");

    let served = painted(
        refusable_library_style(),
        Capabilities::from_parts("0.1.0", &["drive".to_owned()]),
    );
    assert!(served.contains("Library"), "{served}");
    assert!(served.contains("data-nav-rail=\"library\""), "{served}");
}

/// A side picture is a banner: the column is the split, and the picture fills it.
///
/// The verbs are built only when an editor is installed, and a reader's paint carries none
/// of them — the same node tree, so both paints agree about what the page offers.
#[test]
fn a_side_banner_fills_its_column_and_offers_its_picture_to_the_editor() {
    let mut style = SessionStyle::default();
    style
        .left
        .add_slot(ImageSlot::new("https://cdn.example/left.png", Some("Left"), None).unwrap());
    let region = Signal::stored(style.left.clone());

    let reading = Owner::new().with(|| {
        view! { <super::SessionBanner region=region kind=BannerRegionKind::Left/> }.to_html()
    });
    assert!(reading.contains("h-full w-full min-w-0 object-cover"));
    assert!(
        !reading.contains("max-h-[70vh]"),
        "a banner has no card's height cap"
    );
    assert!(!reading.contains("data-style-slot-edit"));

    let editing = Owner::new().with(|| {
        view! {
            <super::SessionBanner
                region=region
                kind=BannerRegionKind::Left
                editor=Some(StyleEditor::new(
                    RwSignal::new(style.clone()),
                    RwSignal::new(true),
                ))
            />
        }
        .to_html()
    });
    assert!(
        editing.contains("max-w-[43rem]")
            && editing.contains("xl:col-start-1")
            && !editing.contains("fixed inset-y-0"),
        "the side column is a grid track with a reading-measure cap, not a fixed strip"
    );
    assert!(editing.contains("data-style-slot-edit"));
    assert!(editing.contains("data-style-slot-editor"));
    assert!(editing.contains("data-style-slot-alt"));
    assert!(editing.contains("Replace picture"));
    // The controls are not waiting for `2xl` any more: the column is wide enough at `xl`.
    assert!(!editing.contains("hidden 2xl:flex"));
}

/// Paints the whole rail with one notifications answer and, optionally, a capability answer.
fn painted_notice(door: super::NoticeDoor, served: Option<&[&str]>) -> String {
    Owner::new().with(|| {
        if let Some(names) = served {
            let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
            provide_serves(Signal::stored(Capabilities::from_parts("0.1.0", &names)));
        }
        view! {
            <Rail
                active=Signal::derive(|| "/home".to_owned())
                profile=Signal::derive(|| None)
                extra=None
                notices=Some(Signal::stored(door))
            />
        }
        .to_html()
    })
}

/// A probe that answered 404 is one failed read, not a capability refusal: the directory
/// road stays in its category, because a transient 404 must not delete a nav destination.
#[test]
fn an_absent_notification_probe_keeps_the_door() {
    let html = painted_notice(super::NoticeDoor::Absent, Some(&["notifications"]));
    assert!(html.contains("href=\"/notifications\""), "{html}");
    assert!(!html.contains("data-notice-bell"), "{html}");
}

/// The capability answer is the only thing that removes the door: when the instance says it
/// serves no notifications, neither the row nor a bell is drawn.
#[test]
fn only_a_capability_refusal_removes_the_notification_door() {
    let html = painted_notice(super::NoticeDoor::Absent, Some(&["lists"]));
    assert!(!html.contains("href=\"/notifications\""), "{html}");
    assert!(!html.contains("data-notice-bell"), "{html}");
}

/// A served notifications surface replaces the catalogue row with the bell; a zero count
/// draws no badge, and a positive count draws it.
#[test]
fn a_served_notification_probe_draws_the_bell_and_its_count() {
    let zero = painted_notice(super::NoticeDoor::Unread(0), Some(&["notifications"]));
    assert!(zero.contains("data-notice-bell"), "{zero}");
    assert!(!zero.contains("data-notice-count"), "{zero}");
    assert_eq!(zero.matches("href=\"/notifications\"").count(), 1, "{zero}");

    let unread = painted_notice(super::NoticeDoor::Unread(3), Some(&["notifications"]));
    let compact: String = unread.split_whitespace().collect();
    assert!(compact.contains("data-notice-count"), "{unread}");
    assert!(compact.contains(">3<"), "{unread}");
}
