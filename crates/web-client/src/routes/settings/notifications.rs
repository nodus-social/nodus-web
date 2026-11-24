//! The Notifications tab: the instance's policy, or the explanation it already had.
//!
//! The tab stays on the strip when the capability answer is empty — a missing name is a
//! refusal of the editor, not of the destination — and this card is what the destination
//! says instead: no controls that cannot save, and the same sentence the section has always
//! shown. A 404 from the policy read is treated the same way, because the lab's image
//! predates the endpoint and a 404 there means "not advertised", not "broken".

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::Feature;
use nodus_pages::Note;
use nodus_pages::pages::settings::notifications::{
    NotificationPolicyCard, PolicyKind, PolicyRow, PolicyValue,
};
use nodus_pages::pages::settings::push::{BrowserPushCard, PushKind};

use crate::state::alive::Alive;
use crate::state::bridge::{self, capabilities, settings as policy};

/// Browser push notifications and notification policy settings.
#[component]
pub fn NotificationsCard() -> impl IntoView {
    let at = capabilities::serves();
    let push_feature = Signal::derive(move || at.get().supports(Feature::Push));

    let (push_supported, set_push_supported) = signal(bridge::push::supported());
    let (push_keyed, set_push_keyed) = signal(true);
    let (push_subscribed, set_push_subscribed) = signal(false);
    let (push_working, set_push_working) = signal(false);
    let (push_alerts, set_push_alerts) = signal(
        PushKind::ALL
            .into_iter()
            .map(|kind| (kind, true))
            .collect::<Vec<_>>(),
    );
    let (push_error, set_push_error) = signal(None::<String>);

    // The server's row decides what the card says: it is the side that delivers, and a
    // browser subscription the server has no row for receives nothing. What the card may
    // offer is a separate fact — a subscription needs a key, and an instance that published
    // none to this client is told so instead of being shown a toggle that cannot work.
    Effect::new(move |_| {
        if !push_feature.get() {
            return;
        }
        let supported = bridge::push::supported();
        set_push_supported.set(supported);
        if !supported {
            return;
        }
        spawn_local(async move {
            match bridge::push::state().await {
                Ok(state) => {
                    set_push_keyed.set(state.keyed);
                    set_push_subscribed.set(state.subscription.is_some());
                    if let Some(subscription) = state.subscription {
                        set_push_alerts.set(
                            PushKind::ALL
                                .into_iter()
                                .map(|kind| (kind, subscription.alerts.get(kind.wire())))
                                .collect(),
                        );
                    }
                }
                // A read that failed says nothing about the subscription: print it where the
                // card is rather than offering a toggle whose first request would repeat it.
                Err(message) => set_push_error.set(Some(message)),
            }
        });
    });

    let on_push_toggle = Callback::new(move |enabled: bool| {
        set_push_working.set(true);
        set_push_error.set(None);
        // The checkbox has already moved in the DOM; the signal moving with it is what makes
        // a refusal able to put it back. Without this the browser's own click state and the
        // card's state disagree until the next reload.
        set_push_subscribed.set(enabled);
        spawn_local(async move {
            let outcome = if enabled {
                bridge::push::subscribe(bridge::push::Alerts::default()).await
            } else {
                bridge::push::unsubscribe().await
            };
            match outcome {
                Ok(()) => {
                    set_push_alerts.set(
                        PushKind::ALL
                            .into_iter()
                            .map(|kind| (kind, enabled))
                            .collect(),
                    );
                }
                // The browser's refusal and the server's sentence both land here, in place,
                // and the toggle goes back to what is actually true.
                Err(message) => {
                    set_push_subscribed.set(!enabled);
                    set_push_error.set(Some(message));
                }
            }
            set_push_working.set(false);
        });
    });

    let on_push_alert_change = Callback::new(move |(kind, enabled): (PushKind, bool)| {
        let mut next = bridge::push::Alerts::default();
        for (other, value) in push_alerts.get_untracked() {
            next.set(other.wire(), value);
        }
        next.set(kind.wire(), enabled);
        set_push_alerts.update(|rows| {
            if let Some(row) = rows.iter_mut().find(|(other, _)| *other == kind) {
                row.1 = enabled;
            }
        });
        spawn_local(async move {
            if let Err(message) = bridge::push::update(next).await {
                set_push_error.set(Some(message));
                set_push_alerts.update(|rows| {
                    if let Some(row) = rows.iter_mut().find(|(other, _)| *other == kind) {
                        row.1 = !enabled;
                    }
                });
            }
        });
    });

    // Notification policy state
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

    let push_error_text: Signal<Option<String>> = push_error.into();
    let policy_rows: Signal<Vec<PolicyRow>> = rows.into();
    let policy_saved: Signal<bool> = saved.into();

    view! {
        <Show when=move || push_feature.get()>
            <BrowserPushCard
                supported=Signal::from(push_supported)
                keyed=Signal::from(push_keyed)
                subscribed=Signal::from(push_subscribed)
                working=Signal::from(push_working)
                alerts=Signal::from(push_alerts)
                error=push_error_text
                on_toggle=Some(on_push_toggle)
                on_alert_change=Some(on_push_alert_change)
            />
        </Show>

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
