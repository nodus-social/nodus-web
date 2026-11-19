//! Escalation: what a number of strikes adds, and in which order.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::admin::escalation::{Draft, EscalationPage, Policy, Step};

use crate::state::alive::Alive;
use crate::state::bridge::admin::{self, EscalationPolicy, EscalationStep};

/// The policy as the page draws it, from the instance's answer.
fn policy_of(answer: &EscalationPolicy) -> Policy {
    Policy {
        enabled: answer.enabled,
        window_days: answer.window_days,
        steps: answer
            .steps
            .iter()
            .map(|step| Step {
                strikes: step.strikes,
                action: step.action.clone(),
                duration_days: step.duration_days,
            })
            .collect(),
        updated_by: answer.updated_by.clone(),
        updated_at: answer.updated_at.clone(),
    }
}

/// What an administrator wrote, as the request carries it.
fn steps_of(draft: &Draft) -> Vec<EscalationStep> {
    draft
        .steps
        .iter()
        .map(|step| EscalationStep {
            strikes: step.strikes,
            action: step.action.clone(),
            duration_days: step.duration_days,
        })
        .collect()
}

/// The escalation policy, read once and written when an administrator saves it.
#[component]
pub fn Escalation() -> impl IntoView {
    let alive = Alive::new();
    let (policy, set_policy) = signal(Option::<Policy>::None);
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (notice, set_notice) = signal(None::<String>);

    let reload = Callback::new(move |()| {
        spawn_local(async move {
            let outcome = admin::escalation_policy().await;
            alive.if_alive(|| {
                match outcome {
                    Ok(answer) => {
                        set_policy.set(Some(policy_of(&answer)));
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| reload.run(()));

    let save = Callback::new(move |draft: Draft| {
        set_busy.set(true);
        set_notice.set(None);
        spawn_local(async move {
            let steps = steps_of(&draft);
            let outcome =
                admin::save_escalation_policy(draft.enabled, draft.window_days, &steps).await;
            alive.if_alive(|| {
                match outcome {
                    // The answer, not the draft, is what the page keeps: a policy the store
                    // normalised is shown as it was stored.
                    Ok(answer) => {
                        set_error.set(None);
                        set_notice.set(Some(
                            "Saved. This is the ladder the instance applies now.".to_owned(),
                        ));
                        set_policy.set(Some(policy_of(&answer)));
                    }
                    // A refusal leaves the draft where it is, so the rung its sentence names
                    // is still on screen to be corrected.
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    view! {
        <EscalationPage
            policy=policy
            busy=busy
            error=error
            notice=Some(Signal::from(notice))
            write=Some(admin::may_write())
            on_save=save
        />
    }
}
