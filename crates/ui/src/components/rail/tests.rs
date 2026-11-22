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
use crate::session_style::{NavItemConfig, NavPlacement, NavRail, SessionStyle};

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
/// that would have named it.
fn staff_doors(extensions: bool, served: [bool; 2], role: bool) -> ExtraDoors {
    let [moderation, administration] = served;
    ExtraDoors {
        present: Signal::derive(move || {
            staff_door(extensions, moderation, role) || staff_door(extensions, administration, role)
        }),
        rows: Arc::new(move || {
            let moderation =
                staff_door(extensions, moderation, role).then(|| door("Moderation", "/moderation"));
            let administration = staff_door(extensions, administration, role)
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
