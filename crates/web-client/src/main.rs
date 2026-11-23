//! Entry point of the wasm client.
//!
//! This package owns everything that can only run in a browser: `app` (the shell and its
//! screens), `boot` (the installs and the mount), `state` (the bridge into the SDK), and the
//! DOM-facing helpers (`media`, `platform`, `router`, `scroll`, `shell`, `theme`, `ui`).
//! `nodus-web` keeps only the route table, which the server must agree with.

// Hydration monomorphises a second pass over the view tree (`hydrate_async`), which nests
// the already-deep `AnyView` types one level further. The default recursion limit overflows
// while computing the layout of the status body, so the limit is raised here.
#![recursion_limit = "256"]


// The route table lives in `nodus-web`, which both artifacts depend on. Re-exported here so
// the screens keep naming it as `crate::shared::router::…` — the same path the server uses.
pub(crate) use nodus_web::shared;

mod app;
mod boot;
mod media;
mod platform;
mod public;
mod router;
mod routes;
mod screen;
mod scroll;
mod shell;
mod state;
mod theme;
mod ui;

fn main() {
    boot::start();
}
