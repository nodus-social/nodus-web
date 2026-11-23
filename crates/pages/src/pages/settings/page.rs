//! The Settings page shell: its title, its tab strip and the slot the cards fill.

use leptos::prelude::*;

use crate::capabilities::UnservedNotice;
use crate::components::screen::Screen;

use super::nav::{SettingsSection, SettingsTabs};

/// The page around the settings cards.
///
/// The section is gated here rather than in every card: a direct visit to a tab the strip
/// did not draw must still say why, and one guard covers both paints and every section.
///
/// The gate answers a `bool` through [`Show`], which rebuilds its children only when that
/// boolean flips. The capability answer is derived from the SDK revision, so it is re-read
/// on every state change — but a revision that leaves this section drawn must not cost the
/// card its identity, because a card's own state (a save confirmation, a suggestion it has
/// already taken) lives in its components. Deciding the gate in the render closure instead
/// built a fresh subtree on every revision, so a save or a POST remounted the card it had
/// just updated. A late-arriving answer still flips the gate: `Show`'s memo recomputes with
/// the revision, and unmounts the card only when the answer really says the section is
/// absent.
#[component]
pub fn SettingsPage(
    #[prop(into)] section: Signal<SettingsSection>,
    #[prop(optional_no_strip)] on_navigate: Option<Callback<SettingsSection>>,
    children: ChildrenFn,
) -> impl IntoView {
    let at = crate::capabilities::serves();
    let drawn = move || section.get().drawn(at.as_ref().map(|at| at.get()).as_ref());
    let sentence = move || section.get().absent();
    view! {
        <Screen>
            <h1 class="mb-5 text-xl font-semibold tracking-tight">"Settings"</h1>
            <SettingsTabs section=section on_navigate=on_navigate/>
            <Show
                when=drawn
                fallback=move || view! { <UnservedNotice sentence=sentence()/> }
            >
                {children()}
            </Show>
        </Screen>
    }
}
