#![recursion_limit = "256"]

//! The page frame both paints render.
//!
//! The SSR host and the browser mount the *same* component tree here: the host turns it into
//! a string, the browser adopts those nodes. That is the one property hydration needs and the
//! reason the frame cannot stay in `nodus-web-client` (wasm-only) or be hand-assembled in
//! `nodus-web-server` (an `inner_html` blob has no node sequence to adopt).
//!
//! The only differences between the two paints are seams, not trees: [`ShellSeed`] carries
//! the values the document already knows, and a `nodus_route::Runtime` answers reads. Both
//! are data, so the rendered markup does not depend on which side built it.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_route::route::{RouteView, Runtime};
use nodus_ui::components::CardRenderer;
use nodus_ui::components::rail::{NoticeDoor, ProfileCardData};
use nodus_ui::components::sidebar::SidebarData;
use nodus_ui::session_style::SessionStyle;
use nodus_web::shared::router::Route;
use serde::{Deserialize, Serialize};

/// The browser's own screens, keyed by route.
///
/// `None` means "this document already carries that route's screen": the frame then draws
/// the paint's [`RouteView`] instead. That is what keeps the first render identical to the
/// document the host wrote, while every route a click reaches is drawn by the dispatcher.
pub type ScreenFn = Arc<dyn Fn(Route) -> Option<AnyView> + Send + Sync>;

/// The staff answer the document already holds, so the adopted tree draws the doors the
/// paint drew.
///
/// The inputs are the client's own — `nodus_ui::capabilities::staff_door(extensions,
/// served, eligible)` — answered by the host from the same capability row and the same role
/// read the hand-painted chrome uses. The browser takes them over once its own probes answer,
/// which is why this is a seed and not an instruction: an instance that omits the surface
/// still closes the door after the mount, and one whose role read is refused still opens
/// none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaffDoors {
    /// The answer knows the extensions and serves the moderation screens.
    pub moderation: bool,
    /// The answer knows the extensions and serves the administration screens.
    pub administration: bool,
    /// The role read named a staff role, or one whose grant is still missing.
    pub eligible: bool,
}

impl StaffDoors {
    /// Whether the rail draws the Moderation row.
    #[must_use]
    pub fn moderation_door(self) -> bool {
        self.eligible && self.moderation
    }

    /// Whether the rail draws the Administration row.
    #[must_use]
    pub fn administration_door(self) -> bool {
        self.eligible && self.administration
    }

    /// Whether either row survived, which is what draws the card at all.
    #[must_use]
    pub fn drawn(self) -> bool {
        self.moderation_door() || self.administration_door()
    }
}

/// The staff rows, each drawn only while its surface is served.
///
/// Both paints call this: the hand-painted chrome passes its two answers as stored signals,
/// the shell passes the seed's. One function, so the adopted card cannot drift from the
/// painted one — a row whose markup differed by a byte would be a node the browser could not
/// adopt.
#[must_use]
pub fn staff_doors(path: &str, moderation: Signal<bool>, administration: Signal<bool>) -> AnyView {
    let moderation_path = path.to_owned();
    let administration_path = path.to_owned();
    view! {
        {move || {
            let path = moderation_path.clone();
            moderation.get().then(move || {
                view! {
                    <nodus_ui::RailEntry
                        label="Moderation"
                        href="/moderation"
                        hint="The queue, the accounts and what the server allows"
                        icon=nodus_ui::icons::Icon::Shield
                        active=Signal::derive(move || nodus_ui::within("/moderation", &path))
                    />
                }
            })
        }}
        {move || {
            let path = administration_path.clone();
            administration.get().then(move || {
                view! {
                    <nodus_ui::RailEntry
                        label="Administration"
                        href="/admin"
                        hint="What this instance publishes and allows"
                        icon=nodus_ui::icons::Icon::Globe
                        active=Signal::derive(move || nodus_ui::within("/admin", &path))
                    />
                }
            })
        }}
    }
    .into_any()
}

/// What the document already knows about the frame.
///
/// Written into the page as `#nodus-shell`, so the client starts its first pass from exactly
/// the values the paint used. A field the host could not answer stays at its default: "not
/// yet known" must hydrate, which it does because both sides draw that state.
///
/// The editor and the callbacks are deliberately not here: they are browser-only actions
/// with no markup of their own, so a paint that does not know them still draws the same
/// nodes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShellSeed {
    /// Whether the reader has a session. The rail exists only for someone who can use it.
    pub signed_in: bool,
    /// The address being shown.
    pub path: String,
    /// The reader's own card, when the paint knew the account.
    pub profile: Option<ProfileCardData>,
    /// The rail's cards. Empty is a drawn, empty state rather than a missing one.
    pub sidebar: SidebarData,
    /// Whether the instance serves notifications. `None` is "not asked yet", which the
    /// painted rail draws as its ordinary door.
    pub notices: Option<NoticeDoor>,
    /// The staff doors, from the capability answer and the role read the paint resolved.
    pub staff: StaffDoors,
    /// Per-account session styling, when one is active.
    pub style: Option<SessionStyle>,
}

/// The frame and one route screen, as one tree.
///
/// `route` is the screen the address names; `runtime` answers its reads. `on_home` is a
/// browser action and changes no markup, so the host passes `None`.
#[component]
pub fn ShellPage(
    seed: ShellSeed,
    runtime: Arc<dyn Runtime>,
    /// A signal, not a value: the rail navigates in place, and the screen must follow.
    route: Signal<Route>,
    card: CardRenderer,
    /// The block above a timeline's posts. Both paints pass the same slot, so the
    /// composer's first frame is the one the document already carries.
    #[prop(optional_no_strip)]
    header: Option<ChildrenFn>,
    /// Goes home without a document load when the app can answer.
    #[prop(optional_no_strip)]
    on_home: Option<Callback<()>>,
    /// The staff answer the browser took over after its first pass. Absent on the host,
    /// which draws the seed's answer: both sides then render the same rows.
    #[prop(optional_no_strip)]
    staff: Option<Signal<StaffDoors>>,
    /// The notice state the browser took over after its first pass. Absent on the host.
    #[prop(optional_no_strip)]
    notices: Option<Signal<Option<NoticeDoor>>>,
    /// The browser's own route table — the screens a click can reach without a document
    /// load. Absent on the host, which has no navigation to answer: the paint is the only
    /// screen there is. A browser passes it and returns `None` for the route this document
    /// already carries, so its first render stays the paint's, node for node.
    #[prop(optional_no_strip)]
    screen: Option<ScreenFn>,
) -> impl IntoView {
    let signed_in = seed.signed_in;
    let path = seed.path.clone();
    let profile = seed.profile.clone();
    let sidebar = seed.sidebar.clone();
    let notices = match notices {
        // The live answer is the browser's; the seed's state is what the paint drew, and
        // `None` there means "not asked", which the rail draws as its ordinary door.
        Some(live) => Some(Signal::derive(move || live.get().unwrap_or_default())),
        None => seed.notices.map(Signal::stored),
    };
    let style = seed.style.clone();
    // Taken before `path` moves into the signal below: the rows mark themselves active from
    // the address they were painted at.
    let door_path = path.clone();
    let staff = staff.unwrap_or_else(|| Signal::stored(seed.staff));
    let moderation = Signal::derive(move || staff.get().moderation_door());
    let administration = Signal::derive(move || staff.get().administration_door());
    // The card is handed both rows and the signal that decides whether the heading exists,
    // so an answer that flips after the mount hides the card without changing the nodes the
    // paint drew.
    let extra = Some(nodus_ui::ExtraDoors {
        present: Signal::derive(move || moderation.get() || administration.get()),
        rows: Arc::new(move || staff_doors(&door_path, moderation, administration)),
    });

    let inner: ChildrenFn = Arc::new(move || {
        let runtime = Arc::clone(&runtime);
        let card = Arc::clone(&card);
        let header = header.clone();
        let screen = screen.clone();
        view! {
            {move || {
                let route = route.get();
                match screen.clone().and_then(|screen| screen(route.clone())) {
                    Some(drawn) => drawn,
                    // No dispatcher, or one that declined this route: the shell draws the
                    // paint's own screen, which is the tree the document carries.
                    None => view! {
                        <RouteView
                            runtime=Arc::clone(&runtime)
                            route=route
                            card=Arc::clone(&card)
                            header=header.clone()
                        />
                    }
                    .into_any(),
                }
            }}
        }
        .into_any()
    });

    // The prop *struct*, not the builder: `extra`, `on_home`, `style`, `profile` and
    // `notices` are `Option`s whose absent state is part of the tree, and the builder's
    // "optional" shorthand erases exactly that distinction.
    nodus_ui::Frame(nodus_ui::FrameProps {
        signed_in: Signal::derive(move || signed_in),
        path: Signal::derive(move || path.clone()),
        inner,
        extra,
        on_home,
        style: style.map(Signal::stored),
        profile: profile.map(|card| Signal::stored(Some(card))),
        sidebar: Signal::stored(sidebar),
        notices: notices,
        editor: None,
        on_save: None,
        on_cancel: None,
        on_pick: None,
        notice: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodus_ui::capabilities::staff_door;

    #[test]
    fn a_seed_with_no_staff_answer_draws_no_door() {
        // The default is every screen the lab runs today: the hand-painted chrome of a
        // signed-out reader, and every shell page whose paint resolved no role.
        let none = StaffDoors::default();
        assert!(!none.drawn());
        assert!(!none.moderation_door());
        assert!(!none.administration_door());
    }

    #[test]
    fn the_seed_door_is_the_clients_own_rule() {
        // The client's gate is `staff_door(extensions, served, eligible)`. The seed folds
        // the extension check into the two `served` bits the host already applies
        // (`staff_door_served` in `nodus-web-server`), so what is left to check is that the
        // role is applied in the same place — and only there.
        for extensions in [false, true] {
            for served in [false, true] {
                for eligible in [false, true] {
                    let seeded = StaffDoors {
                        moderation: extensions && served,
                        administration: extensions && served,
                        eligible,
                    };
                    assert_eq!(
                        seeded.moderation_door(),
                        staff_door(extensions, served, eligible),
                        "moderation: extensions={extensions} served={served} eligible={eligible}"
                    );
                    assert_eq!(
                        seeded.administration_door(),
                        staff_door(extensions, served, eligible),
                        "administration: extensions={extensions} served={served} eligible={eligible}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_door_the_answer_omits_stays_shut_whatever_the_role_says() {
        let doors = StaffDoors {
            moderation: false,
            administration: true,
            eligible: true,
        };
        assert!(!doors.moderation_door());
        assert!(doors.administration_door());
        assert!(doors.drawn());
    }
}
