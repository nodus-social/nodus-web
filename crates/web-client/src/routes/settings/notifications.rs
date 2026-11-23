//! The Notifications tab: the instance's policy, or the explanation it already had.
//!
//! The tab stays on the strip when the capability answer is empty — a missing name is a
//! refusal of the editor, not of the destination — and this card is what the destination
//! says instead: no controls that cannot save, and the same sentence the section has always
//! shown. A 404 from the policy read is treated the same way, because the lab's image
//! predates the endpoint and a 404 there means "not advertised", not "broken".

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::Note;
use nodus_pages::pages::settings::notifications::{
    NotificationPolicyCard, PolicyKind, PolicyRow, PolicyValue,
};

use crate::state::alive::Alive;
use crate::state::bridge::{self, settings as policy};

/// What this instance does with each kind of notification, edited in place.
#[component]
pub fn NotificationsCard() -> impl IntoView {
    let advertised = bridge::feature("notification_settings");
    let alive = Alive::new();
    let (rows, set_rows) = signal(Vec::<PolicyRow>::new());
    let (loaded, set_loaded) = signal(false);
    let (asking, set_asking) = signal(false);
    let (missing, set_missing) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let (saved, set_saved) = signal(false);

    Effect::new(move |_| {
        if !advertised.get()
            || loaded.get_untracked()
            || asking.get_untracked()
            || error.get_untracked().is_some()
        {
            return;
        }
        set_asking.set(true);
        spawn_local(async move {
            let outcome = policy::load().await;
            alive.if_alive(|| {
                set_asking.set(false);
                match outcome {
                    Ok(Some(policy_rows)) => {
                        set_rows.set(policy_rows);
                        set_loaded.set(true);
                    }
                    Ok(None) => set_missing.set(true),
                    Err(message) => set_error.set(Some(message)),
                }
            });
        });
    });

    // The row changes first and the request follows: a control that waited for the round
    // trip would feel broken on a slow instance. A refusal puts the old value back and
    // leaves the instance's sentence under the row it is about.
    let on_change = Callback::new(move |(kind, value): (PolicyKind, PolicyValue)| {
        let previous = rows
            .get_untracked()
            .into_iter()
            .find(|row| row.kind == kind)
            .map(|row| row.value);
        let Some(previous) = previous else {
            return;
        };
        if previous == value {
            return;
        }
        set_rows.update(|rows| {
            if let Some(row) = rows.iter_mut().find(|row| row.kind == kind) {
                row.value = value;
                row.saving = true;
                row.error = None;
            }
        });
        set_saved.set(false);
        policy::save(
            kind,
            value,
            Callback::new(move |outcome: Result<(), String>| {
                set_rows.update(|rows| {
                    if let Some(row) = rows.iter_mut().find(|row| row.kind == kind) {
                        row.saving = false;
                        if let Err(message) = &outcome {
                            row.value = previous;
                            row.error = Some(message.clone());
                        }
                    }
                });
                if outcome.is_ok() {
                    set_saved.set(true);
                }
            }),
        );
    });

    let policy_rows: Signal<Vec<PolicyRow>> = rows.into();
    let policy_saved: Signal<bool> = saved.into();

    view! {
        <Show when=move || !advertised.get() || missing.get()>
            <Note class="text-sm opacity-70">
                "This instance does not publish notification settings."
            </Note>
        </Show>

        <Show when=move || advertised.get() && !missing.get() && asking.get()>
            <p class="text-sm opacity-60" role="status">
                "Reading your notification settings…"
            </p>
        </Show>

        <Show when=move || error.get().is_some()>
            <p class="text-sm text-error" role="alert">
                {move || error.get().unwrap_or_default()}
            </p>
        </Show>

        <Show when=move || loaded.get() && !missing.get()>
            <NotificationPolicyCard
                rows=policy_rows
                saved=policy_saved
                on_change=Some(on_change)
            />
        </Show>
    }
}
