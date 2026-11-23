//! The Settings page shell: its title, its tab strip and the slot the cards fill.

use leptos::prelude::*;

use crate::capabilities::UnservedNotice;
use crate::components::screen::Screen;

use super::nav::{SettingsSection, SettingsTabs};

/// The page around the settings cards.
///
/// The section is gated here rather than in every card: a direct visit to a tab the strip
/// did not draw must still say why, and one guard covers both paints and every section.
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
            {move || {
                if drawn() {
                    children().into_any()
                } else {
                    view! { <UnservedNotice sentence=sentence()/> }.into_any()
                }
            }}
        </Screen>
    }
}
