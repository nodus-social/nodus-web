//! What the client does once, before the first render.
//!
//! Every `install` below registers something that outlives a screen (a signal with no
//! owner, a listener, a tick), so it has to run before the first mount; the mount itself is
//! the takeover of the page the server painted.

use crate::app::App;
use wasm_bindgen::JsCast as _;

/// Installs the app-lifetime services, then mounts the app into the page.
pub fn start() {
    console_error_panic_hook::set_once();
    // Before the first mount, so app-lifetime signals have no owner to be disposed
    // with (see `routes::settings::sync::install`).
    crate::routes::settings::sync::install();
    // Same reason: the revision signal is written by socket frames and ticks, which
    // outlive every screen, so it must not belong to one.
    crate::state::bridge::install_reactive_root();
    crate::state::clock_format::install();
    crate::state::feeds::install();
    // The rail cards' contents are read by components that mount and unmount as the rail
    // does, so the signal holding them must not belong to any of them.
    crate::state::bridge::sidebar::install();
    crate::state::bridge::notifications::install();
    // The expander's signal, created with no owner for the same reason.
    crate::media::install();
    // The operating system decides light or dark, and may change its mind while the tab
    // is open; `index.html` also sets the attribute before wasm arrives, so the first
    // paint is already correct.
    crate::theme::follow_system();
    // The server painted the first page into `#app`; the client replaces it, using the data
    // that page carried (`state::bridge::seed` read it a line earlier). Mounting *into* the
    // node instead of appending to `<body>` is what makes the takeover a replacement rather
    // than a second copy of the page underneath the first.
    crate::state::bridge::seed::install();
    mount_app();
    // Started after the mount so the first render never waits on a network tick.
    crate::state::bridge::start_background_tick();
    // Impressions are written when a card leaves the screen, so the last screen's
    // worth of them is flushed on page hide (D36).
    crate::state::bridge::install_impression_flush();
}

/// Adopts the server-painted tree, or mounts into `<body>` when there is none.
///
/// A static deployment (no SSR host) has no `#app`, and `mount_to_body` is the right answer
/// there; the SSR document always has one.
///
/// A printed document whose `#app` carries `data-hydrate` was rendered from *this* view
/// tree, so the nodes are adopted: `hydrate_from` walks `#app`'s children looking for the
/// exact element and marker sequence `App` renders and binds the signals to them. A document
/// without the attribute is not a Leptos tree (it is the hand-painted shell), so the client
/// still builds its own tree and swaps the paint out.
fn mount_app() {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(host) = document
        .get_element_by_id("app")
        .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        // A static bundle: there is no server paint to stand aside for.
        leptos::mount::mount_to_body(App);
        mark_hydrated();
        return;
    };
    // A shell document names its root: `#app` was rendered by `nodus_shell::ShellPage`, and
    // `#nodus-shell` carries the seed that tree was built from, so the first pass can start
    // from the paint's own values instead of guessing them.
    if host.has_attribute("data-shell") {
        let seed = shell_seed(&document).unwrap_or_default();
        leptos::mount::hydrate_from(host, move || {
            crate::shell_root::ShellRoot(crate::shell_root::ShellRootProps { seed })
        }).forget();
        mark_hydrated();
        return;
    }
    if host.has_attribute("data-hydrate") {
        leptos::mount::hydrate_from(host, App).forget();
        mark_hydrated();
        return;
    }

    // Fallback: the paint is not a Leptos tree, so nothing can be adopted from it.
    //
    // Clearing `#app` and mounting into it paints one frame of nothing, and the reader sees
    // the server's page blink out and come back — which reads as the interface resetting
    // itself. Rendering beside the document and replacing the children in one step keeps the
    // server paint on screen until the client tree is ready to stand in its place.
    let staging = document
        .create_element("div")
        .ok()
        .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok());
    if let Some(staging) = staging {
        leptos::mount::mount_to(staging.clone(), App).forget();
        if let Ok(node) = staging.dyn_into::<web_sys::Node>() {
            host.replace_children_with_node_1(&node);
        }
        mark_hydrated();
        return;
    }
    host.set_inner_html("");
    leptos::mount::mount_to(host, App).forget();
    mark_hydrated();
}

/// Says, in a way a test (or a curious reader) can see, that the client tree is live.
///
/// Until this runs, the rail and the page are the **server's** anchors: clicking one is a
/// full document load. A test that clicks before this point is testing a static site.
fn mark_hydrated() {
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Reflect::set(&window, &"__nodus".into(), &wasm_bindgen::JsValue::TRUE);
    }
}

/// The seed the shell paint used, if this document carries one.
///
/// A document without the island is not a shell tree (it predates the shell, or the host
/// could not serialise the seed), so the defaults are used — both sides draw that state.
fn shell_seed(document: &web_sys::Document) -> Option<nodus_shell::ShellSeed> {
    let text = document.get_element_by_id("nodus-shell")?.text_content()?;
    serde_json::from_str(&text).ok()
}
