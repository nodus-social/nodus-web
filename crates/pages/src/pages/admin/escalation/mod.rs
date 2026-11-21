//! Escalation: what a number of strikes adds, and in which order.
//!
//! A policy is a window and a ladder. A strike is a decision a moderator or a rule already
//! made ("this account was silenced at 12:04"); the ladder is what the instance does once
//! enough of them are still live. The ladder is read top to bottom, so the screen is a
//! ladder: each rung names the strike count it answers and says, in a sentence, what an
//! account carrying that many strikes gets.
//!
//! One rule holds above every rung, and the screen says it in words rather than leaving it
//! to be discovered from an audit entry: an escalation never weakens a restriction already
//! in force. A rung that reaches for less than the account already carries is recorded and
//! skipped — the instance will not lift a suspension because the ladder says "flag".
//!
//! The order check is the instance's, not this screen's. Its refusal names the offending
//! rung by position ("step 3: strike counts must ascend, 2 does not follow 4"), and that
//! sentence is the only guidance an administrator gets, so it is shown verbatim instead of
//! being replaced by a client-side guess. The rung it names is ringed, so the sentence and
//! the row it is about are read together.

use leptos::prelude::*;

use crate::Note;
use crate::components::panel::Panel;
use crate::props::{NO_WRITE, writable};

/// One rung: the live strike count it answers, what it applies, and for how long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The live strike count at which this rung applies.
    pub strikes: i64,
    /// `warn`, `flag`, `silence` or `suspend`, as the instance spells it.
    pub action: String,
    /// How long a temporary action lasts, in days. `None` means "until lifted".
    pub duration_days: Option<i64>,
}

/// The policy as the instance holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Whether the ladder is applied at all.
    pub enabled: bool,
    /// How long a strike counts, in days.
    pub window_days: i64,
    /// The ladder, in the order the instance reads it.
    pub steps: Vec<Step>,
    /// Who wrote it last.
    pub updated_by: Option<String>,
    /// When, in RFC 3339.
    pub updated_at: Option<String>,
}

/// What an administrator is editing, before the instance has judged it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    /// Whether the ladder should be applied.
    pub enabled: bool,
    /// How long a strike should count, in days.
    pub window_days: i64,
    /// The ladder as it stands on screen.
    pub steps: Vec<Step>,
}

impl Draft {
    /// The rung a policy starts from: a warning at the first strike, in a thirty-day window.
    #[must_use]
    pub fn new() -> Self {
        Self {
            enabled: true,
            window_days: 30,
            steps: vec![Step {
                strikes: 1,
                action: "flag".to_owned(),
                duration_days: None,
            }],
        }
    }

    /// The draft an existing policy starts from.
    #[must_use]
    pub fn of(policy: &Policy) -> Self {
        Self {
            enabled: policy.enabled,
            window_days: policy.window_days,
            steps: policy.steps.clone(),
        }
    }
}

impl Default for Draft {
    fn default() -> Self {
        Self::new()
    }
}

/// Every action a rung may apply, as the instance accepts them.
const ACTIONS: [&str; 4] = ["warn", "flag", "silence", "suspend"];

/// Whether an action has a length: only a restriction taken away can be handed back.
#[must_use]
pub fn is_timed(action: &str) -> bool {
    matches!(action, "silence" | "suspend")
}

/// An action in a sentence, after the strike count.
fn action_line(action: &str) -> &'static str {
    match action {
        "warn" => "warns the author",
        "flag" => "flags the account for a moderator",
        "silence" => "silences the account",
        "suspend" => "suspends the account",
        _ => "does something this screen does not know",
    }
}

/// How long one rung lasts, in a sentence.
fn duration_line(step: &Step) -> String {
    match (is_timed(&step.action), step.duration_days) {
        (true, Some(days)) if days == 1 => " for 1 day".to_owned(),
        (true, Some(days)) => format!(" for {days} days"),
        (true, None) => " with no end date".to_owned(),
        (false, _) => String::new(),
    }
}

/// One rung read back in a sentence — the whole point of the ladder.
#[must_use]
pub fn step_line(step: &Step) -> String {
    format!("Strike {} {}", step.strikes, action_line(&step.action)) + &duration_line(step)
}

/// The whole ladder in one sentence, so the sequence is legible before it is read rung by rung.
#[must_use]
pub fn sequence_line(draft: &Draft) -> String {
    if draft.steps.is_empty() {
        return "The ladder is empty, and the instance refuses a policy with no rung.".to_owned();
    }
    let rungs = draft
        .steps
        .iter()
        .map(|step| {
            format!(
                "{} at {}",
                step.action,
                if step.strikes == 1 {
                    "1 strike".to_owned()
                } else {
                    format!("{} strikes", step.strikes)
                }
            )
        })
        .collect::<Vec<String>>()
        .join(", then ");
    format!("Inside a {}-day window: {rungs}.", draft.window_days)
}

/// The rung an instance refusal names, if it names one.
///
/// The store names a rung by position ("step 2: …"), never by strike count, because a
/// duplicate count is exactly one of the things it refuses. Reading that number back is how
/// the sentence and the row end up side by side.
fn named_step(error: &str) -> Option<usize> {
    let rest = error.split("step ").nth(1)?;
    let digits = rest
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits.parse().ok()
}

/// The escalation policy: the switch, the window, and the ladder.
#[component]
pub fn EscalationPage(
    /// The policy as the instance last answered it.
    #[prop(into)]
    policy: Signal<Option<Policy>>,
    /// Whether a read or a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal, in the instance's own words.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// What the last write did, said above the ladder.
    #[prop(optional_no_strip)]
    notice: Option<Signal<Option<String>>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
    /// Sends the ladder as it stands.
    #[prop(optional)]
    on_save: Option<Callback<Draft>>,
) -> impl IntoView {
    let write = writable(write);

    // The draft lives here because it is a form, not an instance answer: the page owns what
    // a screen *is*, and only the request leaves.
    let (draft, set_draft) = signal(Draft::new());
    // The draft is reseeded only when the instance's answer actually changes, so a save that
    // came back refused leaves the administrator's ladder on screen to be corrected.
    let seeded = StoredValue::new(String::new());
    Effect::new(move |_| {
        if let Some(policy) = policy.get() {
            let signature = format!(
                "{}|{}|{:?}|{:?}",
                policy.enabled, policy.window_days, policy.steps, policy.updated_at
            );
            if seeded.get_value() != signature {
                seeded.set_value(signature);
                set_draft.set(Draft::of(&policy));
            }
        }
    });

    let offending = Signal::derive(move || error.get().as_deref().and_then(named_step));
    // The ladder with each rung's position beside it: the store names a rung by position, so
    // the ring and the sentence have to agree on which rung that is.
    let ladder = Signal::derive(move || {
        draft
            .get()
            .steps
            .into_iter()
            .enumerate()
            .collect::<Vec<(usize, Step)>>()
    });

    let set_strikes = move |index: usize, value: String| {
        let strikes = value.trim().parse::<i64>().unwrap_or(0);
        set_draft.update(|draft| {
            if let Some(step) = draft.steps.get_mut(index) {
                step.strikes = strikes;
            }
        });
    };
    let set_action = move |index: usize, value: String| {
        set_draft.update(|draft| {
            if let Some(step) = draft.steps.get_mut(index) {
                step.action = value;
                // A warning and a flag have no length; a duration left over from the action
                // this rung used to carry would be refused for a rung that no longer has one.
                if !is_timed(&step.action) {
                    step.duration_days = None;
                }
            }
        });
    };
    let set_duration = move |index: usize, value: String| {
        let days = value.trim().parse::<i64>().ok();
        set_draft.update(|draft| {
            if let Some(step) = draft.steps.get_mut(index) {
                step.duration_days = days;
            }
        });
    };
    let add_step = move |_| {
        set_draft.update(|draft| {
            let strikes = draft.steps.last().map_or(1, |step| step.strikes + 1);
            draft.steps.push(Step {
                strikes,
                action: "flag".to_owned(),
                duration_days: None,
            });
        });
    };
    let remove_step = move |index: usize| {
        set_draft.update(|draft| {
            if index < draft.steps.len() {
                draft.steps.remove(index);
            }
        });
    };
    let save = Callback::new(move |_| {
        if let Some(on_save) = on_save {
            on_save.run(draft.get_untracked());
        }
    });

    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || error.get().is_some()>
                <p class="alert alert-error alert-soft py-1 text-sm" role="alert" data-testid="escalation-error">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || notice.and_then(|notice| notice.get()).is_some()>
                <p class="text-sm opacity-70" role="status">
                    {move || notice.and_then(|notice| notice.get()).unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || !write.get()>
                <Note class="text-sm text-warning">{NO_WRITE}</Note>
            </Show>

            <Panel
                title="Escalation"
                description="What a number of live strikes adds to an account, rung by rung."
            >
                <Show when=move || busy.get() && policy.get().is_none()>
                    <p class="text-sm opacity-60">"Reading the policy…"</p>
                </Show>

                // The rule that holds above every rung, said before the ladder is read.
                <div data-testid="escalation-never-weakens">
                    <Note class="text-sm opacity-70">
                        "An escalation never weakens a restriction already in force. A rung adds a silence or a suspension to what an account already carries; it never lifts one — a rung that reaches for less than the account already has is recorded and skipped. A strike stops counting when it leaves the window, and only a person lifts a restriction."
                    </Note>
                </div>

                <div class="flex flex-wrap items-center gap-3">
                    <label class="label cursor-pointer gap-2" for="escalation-enabled">
                        <span class="label-text text-sm">"Apply the ladder"</span>
                        <input
                            id="escalation-enabled"
                            type="checkbox"
                            class="toggle toggle-sm"
                            data-testid="escalation-enabled"
                            prop:checked=move || draft.get().enabled
                            on:change=move |event| {
                                let on = event_target_checked(&event);
                                set_draft.update(|draft| draft.enabled = on);
                            }
                        />
                    </label>
                    <label class="flex items-center gap-2 text-sm" for="escalation-window">
                        <span>"Strikes count for"</span>
                        <input
                            id="escalation-window"
                            type="number"
                            min="1"
                            class="input input-bordered input-sm w-24"
                            data-testid="escalation-window"
                            prop:value=move || draft.get().window_days.to_string()
                            on:input=move |event| {
                                let days = event_target_value(&event).trim().parse::<i64>().unwrap_or(0);
                                set_draft.update(|draft| draft.window_days = days);
                            }
                        />
                        <span>"days"</span>
                    </label>
                </div>

                <p class="text-sm font-medium" data-testid="escalation-sequence">
                    {move || sequence_line(&draft.get())}
                </p>

                <ol class="flex flex-col gap-2" data-testid="escalation-ladder">
                    <For
                        each=move || ladder.get()
                        key=|(index, step)| (*index, step.strikes, step.action.clone(), step.duration_days)
                        let:entry
                    >
                        {
                            let (index, step) = entry;
                            let here = Signal::derive(move || offending.get() == Some(index + 1));
                            let read = step_line(&step);
                            let timed = is_timed(&step.action);
                            let duration = StoredValue::new(
                                step.duration_days
                                    .map(|days| days.to_string())
                                    .unwrap_or_default(),
                            );
                            let action = step.action.clone();
                            let chosen = step.action.clone();
                            let strikes = step.strikes;
                            let options = ACTIONS
                                .into_iter()
                                .map(|name| (name, name == chosen))
                                .collect::<Vec<(&'static str, bool)>>();
                            view! {
                                <li
                                    class="flex flex-wrap items-center gap-2 rounded-box bg-base-200 p-2"
                                    class:ring-1=move || here.get()
                                    class:ring-error=move || here.get()
                                    data-testid="escalation-step"
                                >
                                    <span class="badge badge-sm badge-neutral" data-testid="escalation-ordinal">
                                        {if strikes == 1 { "1 strike".to_owned() } else { format!("{strikes} strikes") }}
                                    </span>
                                    <span class="text-sm">"→"</span>
                                    <label class="sr-only" for=format!("escalation-strikes-{index}")>"Strikes"</label>
                                    <input
                                        id=format!("escalation-strikes-{index}")
                                        type="number"
                                        min="1"
                                        class="input input-bordered input-sm w-20"
                                        data-testid="escalation-strikes"
                                        prop:value=move || strikes.to_string()
                                        on:input=move |event| set_strikes(index, event_target_value(&event))
                                    />
                                    <label class="text-sm" for=format!("escalation-action-{index}")>"strikes, apply"</label>
                                    <select
                                        id=format!("escalation-action-{index}")
                                        class="select select-bordered select-sm"
                                        data-testid="escalation-action"
                                        prop:value=move || action.clone()
                                        on:change=move |event| set_action(index, event_target_value(&event))
                                    >
                                        {options.into_iter().map(|(name, selected)| view! {
                                            <option value=name selected=selected>{name}</option>
                                        }).collect_view()}
                                    </select>
                                    <Show when=move || timed>
                                        <label class="text-sm" for=format!("escalation-duration-{index}")>"for"</label>
                                        <input
                                            id=format!("escalation-duration-{index}")
                                            type="number"
                                            min="1"
                                            placeholder="no end date"
                                            class="input input-bordered input-sm w-28"
                                            data-testid="escalation-duration"
                                            prop:value=move || duration.get_value()
                                            on:input=move |event| set_duration(index, event_target_value(&event))
                                        />
                                        <span class="text-sm">"days"</span>
                                    </Show>
                                    <span class="ml-auto text-xs opacity-60" data-testid="escalation-step-line">{read}</span>
                                    <button
                                        class="btn btn-ghost btn-xs text-error"
                                        data-testid="escalation-remove"
                                        disabled=move || busy.get() || !write.get()
                                        title=move || (!write.get()).then_some(NO_WRITE).unwrap_or_default()
                                        on:click=move |_| remove_step(index)
                                    >
                                        "Remove"
                                    </button>
                                </li>
                            }
                        }
                    </For>
                </ol>

                <div class="flex flex-wrap items-center gap-2">
                    <button
                        class="btn btn-ghost btn-xs"
                        data-testid="escalation-add"
                        disabled=move || busy.get() || !write.get()
                        on:click=add_step
                    >
                        "Add a rung"
                    </button>
                    <button
                        class="btn btn-primary btn-xs"
                        data-testid="escalation-save"
                        disabled=move || busy.get() || !write.get() || on_save.is_none()
                        title=move || (!write.get()).then_some(NO_WRITE).unwrap_or_default()
                        on:click=move |_| save.run(())
                    >
                        "Save the ladder"
                    </button>
                    <Show when=move || !busy.get() && policy.get().is_none()>
                        <p class="text-sm opacity-60">"This instance did not answer with a policy."</p>
                    </Show>
                    <Show when=move || policy.get().is_some()>
                        <p class="ml-auto text-xs opacity-60" data-testid="escalation-written">
                            {move || {
                                let policy = policy.get();
                                match policy.and_then(|policy| policy.updated_by) {
                                    Some(who) => format!("Last written by {who}."),
                                    None => "Not written since this instance was set up.".to_owned(),
                                }
                            }}
                        </p>
                    </Show>
                </div>
            </Panel>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{Draft, Policy, Step, named_step, sequence_line, step_line};

    fn step(strikes: i64, action: &str, duration_days: Option<i64>) -> Step {
        Step {
            strikes,
            action: action.to_owned(),
            duration_days,
        }
    }

    #[test]
    fn a_rung_says_what_it_does_and_how_long() {
        assert_eq!(
            step_line(&step(2, "silence", Some(7))),
            "Strike 2 silences the account for 7 days"
        );
        assert_eq!(
            step_line(&step(3, "suspend", None)),
            "Strike 3 suspends the account with no end date"
        );
        assert_eq!(
            step_line(&step(1, "flag", Some(7))),
            "Strike 1 flags the account for a moderator"
        );
    }

    #[test]
    fn the_sequence_reads_in_order() {
        let draft = Draft {
            enabled: true,
            window_days: 30,
            steps: vec![step(2, "silence", Some(7)), step(3, "suspend", None)],
        };
        assert_eq!(
            sequence_line(&draft),
            "Inside a 30-day window: silence at 2 strikes, then suspend at 3 strikes."
        );
    }

    #[test]
    fn an_empty_ladder_says_so() {
        let draft = Draft {
            enabled: true,
            window_days: 30,
            steps: Vec::new(),
        };
        assert_eq!(
            sequence_line(&draft),
            "The ladder is empty, and the instance refuses a policy with no rung."
        );
    }

    #[test]
    fn a_refusal_names_the_rung_it_is_about() {
        assert_eq!(
            named_step(
                "instance returned 422: step 3: strike counts must ascend, 2 does not follow 4"
            ),
            Some(3)
        );
        assert_eq!(
            named_step("instance returned 422: window_days must be between 1 and 3650"),
            None
        );
    }

    #[test]
    fn a_policy_seeds_the_draft_unchanged() {
        let policy = Policy {
            enabled: false,
            window_days: 90,
            steps: vec![step(1, "flag", None), step(4, "suspend", Some(30))],
            updated_by: Some("ada".to_owned()),
            updated_at: Some("2026-11-19T00:00:00Z".to_owned()),
        };
        let draft = Draft::of(&policy);
        assert_eq!(
            draft,
            Draft {
                enabled: false,
                window_days: 90,
                steps: policy.steps.clone(),
            }
        );
    }
}
