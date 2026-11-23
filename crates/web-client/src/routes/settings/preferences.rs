//! Posting defaults, reading and notification switches, edited in place.
//!
//! Each change is sent on its own as a partial body built from the last confirmed map, so
//! two tabs editing different keys cannot overwrite each other with a whole-object write.
//! The control moves first; a refusal puts the confirmed value back and leaves the
//! instance's sentence under that row. Nothing here toasts: the row is where the answer
//! belongs.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::Preferences;
use nodus_pages::pages::settings::preferences::{
    PreferenceField, PreferenceValue, PreferencesCard as PreferencesForm, PreferencesEditor,
};

use crate::state::alive::Alive;
use crate::state::bridge::actions;

use super::refusal;

/// The nine values the instance stores for this account.
#[component]
pub fn PreferencesCard() -> impl IntoView {
    let alive = Alive::new();
    let editor = RwSignal::new(PreferencesEditor::new(Preferences::default()));
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            let outcome = actions::preferences().await;
            alive.if_alive(|| {
                match outcome {
                    Ok(found) => editor.set(PreferencesEditor::new(found)),
                    Err(message) => set_error.set(Some(refusal::sentence(&message))),
                }
                set_loading.set(false);
            });
        });
    });

    let on_change = Callback::new(move |(field, value): (PreferenceField, PreferenceValue)| {
        let mut patch = None;
        editor.update(|state| patch = state.begin(field, value));
        let Some(patch) = patch else {
            return;
        };

        spawn_local(async move {
            let outcome = actions::update_preferences(&patch).await;
            alive.if_alive(|| match outcome {
                Ok(server) => editor.update(|state| state.accepted(server)),
                Err(error) => editor.update(|state| {
                    state.refused(field, refusal::sentence(&error.sentence()));
                }),
            });
        });
    });

    view! {
        <Show when=move || loading.get()>
            <p class="text-sm opacity-60" role="status">
                "Reading your preferences…"
            </p>
        </Show>

        <Show when=move || error.get().is_some()>
            <p class="text-sm text-error" role="alert">
                {move || error.get().unwrap_or_default()}
            </p>
        </Show>

        <Show when=move || !loading.get() && error.get().is_none()>
            <PreferencesForm editor=editor on_change=Some(on_change) />
        </Show>
    }
}
