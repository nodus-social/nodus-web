//! The root the browser hydrates when the document was painted by `nodus-shell`.
//!
//! One tree, two paints: the host rendered `nodus_shell::ShellPage` and wrote the seed it
//! used into `#nodus-shell`; this root reads that seed back and renders the same component,
//! so `hydrate_from` finds the node sequence it expects. Anything the shell does not know
//! (the browser's own actions) is a seam, not a difference in markup.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_ui::components::CardRenderer;

use crate::router::{self, Route};

/// The shell, as the document described it.
///
/// `seed` is read from `#nodus-shell` before the mount; a document without one starts from
/// the same defaults the host would have used for a value it could not answer.
#[component]
pub fn ShellRoot(seed: nodus_shell::ShellSeed) -> impl IntoView {
    // The same installs `App` makes before its first render: the SDK must exist (the seed
    // and the island have already been read in `boot`) and the rail's clicks must navigate.
    crate::state::bridge::init();
    router::install();

    let card: CardRenderer =
        Arc::new(|status| view! { <crate::routes::status::StatusCard status/> }.into_any());
    // `on_home` changes no markup: it is an action the host cannot have, so passing it is
    // not a divergence — hydration only compares nodes.
    let on_home = Some(Callback::new(|()| router::navigate(Route::Home)));
    // The router's own signal, so a rail click repaints the screen inside the adopted frame
    // instead of only changing the address. Read it here, in the root's owner: created lazily
    // inside the screen's reactive block it would belong to that block and be disposed with
    // it, panicking on the next navigation.
    let current = router::route();
    let route = Signal::derive(move || current.get());
    let runtime = crate::state::runtime::runtime();
    // The same header the server painted for a timeline: on `/lists` the route ignores
    // it, and on `/home` its first frame is the shared preview, which is the tree the
    // document carried. The live composer arrives after hydration.
    let header: ChildrenFn = Arc::new(|| {
        view! { <crate::routes::home::TimelineHeader/> }.into_any()
    });

    view! {
        <nodus_shell::ShellPage
            seed=seed
            runtime=runtime
            route=route
            card=card
            header=Some(header)
            on_home=on_home
        />
    }
}
