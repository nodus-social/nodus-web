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

/// What the document already knows about the frame.
///
/// Written into the page as `#nodus-shell`, so the client starts its first pass from exactly
/// the values the paint used. A field the host could not answer stays at its default: "not
/// yet known" must hydrate, which it does because both sides draw that state.
///
/// `editor`, `extra` (staff doors) and the callbacks are deliberately not here: they are
/// browser-only actions with no markup of their own, so a paint that does not know them
/// still draws the same nodes.
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
    /// Goes home without a document load when the app can answer.
    #[prop(optional_no_strip)]
    on_home: Option<Callback<()>>,
) -> impl IntoView {
    let signed_in = seed.signed_in;
    let path = seed.path.clone();
    let profile = seed.profile.clone();
    let sidebar = seed.sidebar.clone();
    let notices = seed.notices;
    let style = seed.style.clone();

    let inner: ChildrenFn = Arc::new(move || {
        let runtime = Arc::clone(&runtime);
        let card = Arc::clone(&card);
        view! {
            {move || {
                let route = route.get();
                view! { <RouteView runtime=Arc::clone(&runtime) route=route card=Arc::clone(&card)/> }
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
        extra: None,
        on_home,
        style: style.map(Signal::stored),
        profile: profile.map(|card| Signal::stored(Some(card))),
        sidebar: Signal::stored(sidebar),
        notices: notices.map(Signal::stored),
        editor: None,
        on_save: None,
        on_cancel: None,
        on_pick: None,
        notice: None,
    })
}
