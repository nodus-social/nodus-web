//! Posting defaults, shown as the instance reports them.
//!
//! This screen is read-only on purpose: Mastodon answers `GET /api/v1/preferences` with
//! values the account stored, but the instance this app talks to may answer with the
//! defaults it applies instead. Offering a save button would promise a write that has
//! nowhere to land, so the card states what it is looking at and shows the values.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::Preferences;
use nodus_pages::components::panel::Panel;

use crate::state::alive::Alive;
use crate::state::bridge::actions;

use super::refusal;

/// The values the instance applies to a new post and to reading.
#[component]
pub fn PreferencesCard() -> impl IntoView {
    let alive = Alive::new();
    let (values, set_values) = signal(None::<Preferences>);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            let outcome = actions::preferences().await;
            alive.if_alive(|| {
                match outcome {
                    Ok(found) => set_values.set(Some(found)),
                    Err(message) => set_error.set(Some(refusal::sentence(&message))),
                }
                set_loading.set(false);
            });
        });
    });

    view! {
        <Panel title="Preferences">
            <p class="text-sm opacity-70">
                "This screen is read-only: it never writes preferences back. The values below are what the instance reports, and a Nodus instance answers with the defaults it applies because it has no preference store."
            </p>

            <Show when=move || loading.get()>
                <p class="text-sm opacity-60">"Reading your preferences…"</p>
            </Show>

            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error">{move || error.get().unwrap_or_default()}</p>
            </Show>

            {move || {
                values
                    .get()
                    .map(|values| {
                        view! {
                            <dl class="grid grid-cols-1 gap-2 text-sm sm:grid-cols-2">
                                {row(
                                    "Default visibility",
                                    values
                                        .posting_default_visibility
                                        .clone()
                                        .unwrap_or_else(|| "server default".to_owned()),
                                )}
                                {row("New posts start sensitive", yes_no(values.posting_default_sensitive))}
                                {row(
                                    "Default language",
                                    values
                                        .posting_default_language
                                        .clone()
                                        .unwrap_or_else(|| "server default".to_owned()),
                                )}
                                {row("Media starts expanded", media(&values.reading_expand_media))}
                                {row("Content warnings start open", yes_no(values.reading_expand_spoilers))}
                            </dl>
                        }
                    })
            }}
        </Panel>
    }
}

/// One name and value pair.
fn row(name: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="rounded-box bg-base-200 px-3 py-2">
            <dt class="text-xs opacity-60">{name}</dt>
            <dd class="truncate">{value}</dd>
        </div>
    }
}

fn yes_no(value: bool) -> String {
    if value {
        "Yes".to_owned()
    } else {
        "No".to_owned()
    }
}

/// The media preference is a word (`default`, `show_all`, `hide_all`), not a boolean.
fn media(value: &str) -> String {
    value.replace('_', " ")
}
