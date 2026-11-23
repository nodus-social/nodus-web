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
//! [`admits`] draws on `None`, because a missing answer is not a refusal. The server render
//! used to be the paint that always passed `None`; it now supplies the answer from the same
//! facts the browser has. The dialect was proved at sign-in (`Session::protocol`), so a
//! Mastodon or Misskey session gets the SDK's baseline with no request, and a Nodus session
//! gets the list published at `/api/v1/meta` (read once per host, then cached in the server
//! process). That value goes in through [`provide_serves`] at the document layer, so the
//! frame, the rail and this gate decide identically before and after hydration.
//!
//! `None` therefore means one of two honest things, never "this instance serves nothing":
//! nobody is signed in, so there is no account to ask about; or the one read failed for this
//! render and the page is painted permissively rather than replaced with an error. A known
//! answer is `Some`, and a surface missing from it is a known absence.

use leptos::prelude::*;

pub use nodus_protocol::{Capabilities, Feature};

mod surface;

pub use surface::Surface;

/// Whether a paint that may not know the answer yet should draw a door.
///
/// `None` is "no capability answer is possible for this paint": an anonymous page with no
/// account to ask about, or a signed-in render whose one meta read failed. Drawing the door
/// there is still the honest choice — hiding a surface the server does serve makes it
/// unreachable for as long as a probe takes, and an anonymous public profile must keep the
/// chrome it always drew. It is also the narrower concession it has always been: every
/// surface has a row, so a known answer (`Some`) that omits one is a refusal, and `None` is
/// the only answer that is not decided. See this module's documentation.
#[must_use]
pub fn admits(at: Option<&Capabilities>, surface: Surface) -> bool {
    at.is_none_or(|at| surface.served(at))
}

/// Whether a door gated by a raw [`Feature`] is drawn.
///
/// The same rule as [`admits`], for a control that is not one of [`Surface`]'s rows: a gate
/// of `None` asks for nothing and is always drawn, and a known answer that omits the name is
/// a refusal. Settings tabs use this because most of their gates are account-list names
/// (`mutes`, `blocks`) with no page of their own to register as a surface.
#[must_use]
pub fn admits_feature(at: Option<&Capabilities>, gate: Option<Feature>) -> bool {
    gate.is_none_or(|gate| at.is_none_or(|at| at.supports(gate)))
}

/// Whether a staff door is served to this reader, from the three facts both paints know.
///
/// A staff door is narrower than an ordinary one: the instance must speak this protocol's
/// extensions (a staff screen is an extension), it must serve the surface, and the account
/// must be eligible — positive evidence of a staff role, which is not the same as already
/// holding a grant. The rail draws the heading and the door under it from this one answer, so
/// a door that is not served cannot leave a heading behind, and this function says nothing
/// about any other door: the three-part gate lives here, not in the card that draws it.
///
/// Eligibility rather than a grant, because the door is an advertisement: a staff account
/// that has not granted yet is exactly who the grant button is for. The page behind the door
/// asks the stricter question, so an eligible account that opens it reads the gate's own arm
/// instead of an empty console.
#[must_use]
pub const fn staff_door(extensions: bool, served: bool, eligible: bool) -> bool {
    extensions && served && eligible
}

/// What the paints hand the shared screens: one value, or nothing known yet.
pub type Serves = Option<Signal<Capabilities>>;

/// Put the negotiated answer in scope for the screens below.
///
/// A context rather than a prop because the frame is built by both artifacts: a prop would
/// have to be threaded through every screen and every server call site for a value that is
/// one fact about the session, and the two paints would then be free to pass different ones.
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

/// A page inside an area this instance does serve, which it does not serve itself.
///
/// The area's door stays — the operator's rule is to omit the screen, not the room — and the
/// address the reader followed still has to say what happened, in the same shape
/// [`UnsupportedSurface`] gives a whole area. One function for both staff areas, so an
/// administrator and a moderator read the same sentence about the same missing page.
#[component]
pub fn UnservedPage(surface: Surface) -> impl IntoView {
    view! { <UnservedNotice sentence=surface.absent()/> }
}

/// The sentence-shaped half of [`UnservedPage`], for a gate with no surface row.
///
/// A settings section is gated on a [`Feature`] rather than a [`Surface`], so it has no
/// `absent()` sentence of its own; it still says the same thing, in the same place, rather
/// than leaving the tab body blank.
#[component]
pub fn UnservedNotice(sentence: &'static str) -> impl IntoView {
    view! { <crate::Note class="text-sm opacity-70">{sentence}</crate::Note> }
}

#[cfg(test)]
mod tests;
