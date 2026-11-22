//! What this server serves, as the shared screens ask it.
//!
//! One value decides every door in this crate, and it lives here rather than in the client
//! because the same page code paints on both sides: a rule written twice would be a rule
//! that disagrees with itself the first time one side learns a new surface.
//!
//! The answer itself comes from the SDK ([`nodus_protocol::Capabilities`]): a Nodus server
//! publishes its list at `/api/v1/meta`, and Mastodon and Misskey publish nothing, so the
//! SDK answers those from a probed baseline. This module adds only what the views need on
//! top of it — the gate itself, the answer both paints share, and what to say when a surface
//! is missing; [`surface::Surface`] holds the names a page is built from and the row each is
//! gated on.
//!
//! ## No surface is presumed served
//!
//! Every [`Surface`] maps to a [`Feature`], and every protocol's answer is one bit test over
//! that row, so nothing here is a default. That was not always true: while the enum could
//! spell only 29 of the 42 published names, a surface the protocol had no word for was
//! answered `true` for Mastodon and Misskey — a guess that drew Misskey's bookmarks and
//! conversations, which that server answers with a 404. The vocabulary is complete now, and
//! the guess is gone; see [`Surface::served`].
//!
//! ## A paint that does not know the answer yet
//!
//! [`admits`] still draws on `None`, because a missing answer is not a refusal yet. The
//! browser always has an answer. The server render does not, and it is the paint that could
//! have one: `web`'s document layer has already proved the token's dialect
//! (`session.protocol`), so `Capabilities::misskey()` or `Capabilities::mastodon()` — the
//! same baseline the browser uses, from no request — is available to it before it renders.
//! Supplying it is a `provide_serves` at that paint, and that paint is in `crates/web-server`,
//! outside this change: until it calls one, a Misskey session's server render still draws a
//! bookmarks tab that the browser then filters out of the rail on hydration. The accepting
//! side already exists here, so the missing half is one call, not a redesign.

use leptos::prelude::*;

pub use nodus_protocol::{Capabilities, Feature};

mod surface;

pub use surface::Surface;

/// Whether a paint that may not know the answer yet should draw a door.
///
/// `None` is "nobody has asked": before `/api/v1/meta` answers in the browser, and on a
/// server render, which is handed no capability list. Drawing the door there is still the
/// honest choice — hiding a surface the server does serve makes it unreachable for as long as
/// a probe takes — and a route that turns out to be absent explains itself instead of
/// failing. It is also still a narrower concession than it was: every surface now has a row,
/// so `None` is the only answer that is not decided. See this module's documentation for why
/// the server render is not yet given the baseline it could have.
#[must_use]
pub fn admits(at: Option<&Capabilities>, surface: Surface) -> bool {
    at.is_none_or(|at| surface.served(at))
}

/// What the paints hand the shared screens: one value, or nothing known yet.
pub type Serves = Option<Signal<Capabilities>>;

/// Put the negotiated answer in scope for the screens below.
///
/// A context rather than a prop because the frame is built by both artifacts, and only one
/// of them has an SDK to ask: a prop would have to be threaded through every screen and
/// every server call site for a value the server cannot know.
pub fn provide_serves(at: impl Into<Signal<Capabilities>>) {
    provide_context(at.into());
}

/// The answer in scope, if this paint has one.
#[must_use]
pub fn serves() -> Serves {
    use_context::<Signal<Capabilities>>()
}

/// What a failed command should be shown as.
///
/// A command that fails because the server does not serve it arrives as the SDK's own
/// display text, naming the feature; the reader gets the sentence a page for that surface
/// would have said, because a control drawn before the answer arrived is not a fault they
/// can act on. Anything else is left exactly as the transport said it.
#[must_use]
pub fn explain(error: Option<String>) -> String {
    error.map_or_else(String::new, |error| {
        Surface::named_by(&error).map_or(error.clone(), |surface| surface.absent().to_owned())
    })
}

/// A page for a surface the server does not serve.
///
/// It is a page and not a toast: the address is real, the reader followed a link to it, and
/// what they need is one sentence about this server plus the way back into the site.
#[component]
pub fn UnsupportedSurface(surface: Surface) -> impl IntoView {
    view! {
        <crate::Screen>
            <crate::PageHeader
                title=surface.title()
                description="Not served by this instance"
            />
            <crate::Note>
                {surface.absent()}
                " "
                <a class="link link-hover" href="/home">
                    "Go to the home timeline"
                </a>
            </crate::Note>
        </crate::Screen>
    }
}

#[cfg(test)]
mod tests;
