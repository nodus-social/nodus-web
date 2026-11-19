//! Moderation rules: the patterns the instance applies without a person.
//!
//! A rule is a pattern, the kind of pattern it is, where it looks and what it does when it
//! finds a match. The screen exists for one reason above the others: a pattern that cannot
//! match is worse than no rule at all, because an administrator believes it is guarding the
//! instance. So every row can be tried against a sample body before it is trusted, and the
//! dry run says plainly whether it matched, which bytes it matched and what text they were.
//!
//! The dry run is the instance's, not this screen's: the same scanner that runs on delivery
//! answers, so a rule that looks right here behaves the same way when a post arrives.

use leptos::prelude::*;

use crate::components::panel::Panel;
use crate::props::{writable, NO_WRITE};
use crate::Note;

/// One rule as the screen draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The instance's id.
    pub id: String,
    /// The name an administrator gave it.
    pub name: String,
    /// The pattern itself, shown verbatim so it can be compared with what was written.
    pub pattern: String,
    /// `keyword`, `regex` or `domain`.
    pub pattern_kind: String,
    /// `inbound`, `local` or `both`.
    pub scope: String,
    /// `flag`, `silence` or `suspend`.
    pub action: String,
    /// Whether ingest applies it right now.
    pub enabled: bool,
    /// How many posts it has caught.
    pub hits: i64,
}

/// What an administrator typed, before the instance has validated it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleDraft {
    /// The name an administrator gave it.
    pub name: String,
    /// The pattern itself.
    pub pattern: String,
    /// `keyword`, `regex` or `domain`.
    pub pattern_kind: String,
    /// `inbound`, `local` or `both`.
    pub scope: String,
    /// `flag`, `silence` or `suspend`.
    pub action: String,
    /// Whether ingest should apply it.
    pub enabled: bool,
}

impl RuleDraft {
    /// An empty draft: a keyword rule watching arrivals, which flags.
    #[must_use]
    pub fn new() -> Self {
        Self {
            name: String::new(),
            pattern: String::new(),
            pattern_kind: "keyword".to_owned(),
            scope: "inbound".to_owned(),
            action: "flag".to_owned(),
            enabled: true,
        }
    }

    /// The draft an existing rule starts from when it is opened for editing.
    #[must_use]
    pub fn of(rule: &Rule) -> Self {
        Self {
            name: rule.name.clone(),
            pattern: rule.pattern.clone(),
            pattern_kind: rule.pattern_kind.clone(),
            scope: rule.scope.clone(),
            action: rule.action.clone(),
            enabled: rule.enabled,
        }
    }
}

impl Default for RuleDraft {
    fn default() -> Self {
        Self::new()
    }
}

/// An edit: the rule it replaces, and what it becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleEdit {
    /// The rule being rewritten.
    pub id: String,
    /// Every field after the edit.
    pub draft: RuleDraft,
}

/// What the instance answered when a rule was tried against a sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRun {
    /// The rule that was tried.
    pub rule_id: String,
    /// Whether the pattern matched.
    pub matched: bool,
    /// The matched bytes, when something matched.
    pub span: Option<(usize, usize, String)>,
    /// The instance's own sentence: which rule, which bytes, or that there was none.
    pub reason: String,
    /// How much of the sample was scanned.
    pub scanned_bytes: usize,
    /// Whether the sample was longer than the scan cap.
    pub truncated: bool,
}

/// The words a kind is shown as, and the ones the instance accepts.
const KINDS: [&str; 3] = ["keyword", "regex", "domain"];
/// The scopes the instance accepts.
const SCOPES: [&str; 3] = ["inbound", "local", "both"];
/// The actions the instance accepts.
const ACTIONS: [&str; 3] = ["flag", "silence", "suspend"];

/// A scope in a sentence.
fn scope_line(scope: &str) -> &'static str {
    match scope {
        "inbound" => "posts arriving from other instances",
        "local" => "posts this instance's members write",
        "both" => "every post either way",
        _ => "an unknown scope",
    }
}

/// An action in a sentence.
fn action_line(action: &str) -> &'static str {
    match action {
        "flag" => "flags it for a moderator",
        "silence" => "silences the author",
        "suspend" => "suspends the author",
        _ => "does something unrecognised",
    }
}

/// The rules list, the form that writes one, and the dry run that tests one.
#[component]
pub fn RulesPage(
    /// Every rule the instance holds.
    #[prop(into)]
    rules: Signal<Vec<Rule>>,
    /// The last dry run's answer, whichever rule it was for.
    #[prop(into)]
    dry: Signal<Option<DryRun>>,
    /// The rule whose dry run is on its way, so its button can be held.
    #[prop(into)]
    testing: Signal<Option<String>>,
    /// Whether a read or write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal, in the instance's own words.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// What the last write did, said above the list.
    #[prop(optional_no_strip)]
    notice: Option<Signal<Option<String>>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
    /// Creates the rule a form describes.
    #[prop(optional)]
    on_create: Option<Callback<RuleDraft>>,
    /// Rewrites a rule in place.
    #[prop(optional)]
    on_update: Option<Callback<RuleEdit>>,
    /// Deletes a rule; the caller asks first.
    #[prop(optional)]
    on_delete: Option<Callback<String>>,
    /// Tries a rule against a sample body, without recording a hit.
    #[prop(optional)]
    on_test: Option<Callback<(String, String)>>,
) -> impl IntoView {
    let write = writable(write);

    // The new-rule form. It lives here because it is a form, not an instance answer: the
    // page owns what a screen *is*, and only the request leaves.
    let (draft, set_draft) = signal(RuleDraft::new());
    let (editing, set_editing) = signal(None::<String>);
    let (edit_draft, set_edit_draft) = signal(RuleDraft::new());
    // Which rule's dry-run panel is open, and what was pasted into it. One at a time: two
    // open boxes with one answer between them is exactly the confusion this screen avoids.
    let (sampling, set_sampling) = signal(None::<String>);
    let (sample, set_sample) = signal(String::new());
    // A delete asks first, in the row, so a mis-click cannot remove a guard.
    let (confirming, set_confirming) = signal(None::<String>);

    let field = move |setter: WriteSignal<RuleDraft>, key: fn(&mut RuleDraft, String)| {
        Callback::new(move |value: String| {
            setter.update(|draft| key(draft, value));
        })
    };

    let submit_new = move |_| {
        let draft = draft.get_untracked();
        if draft.name.trim().is_empty() || draft.pattern.trim().is_empty() {
            // The instance says the same thing, but saying it here saves a round trip and
            // keeps the form's own emptiness from looking like a refusal.
            return;
        }
        if let Some(on_create) = on_create {
            on_create.run(draft);
        }
    };

    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || error.get().is_some()>
                <p class="alert alert-error alert-soft py-1 text-sm" role="alert">
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
                title="Moderation rules"
                description="Patterns the instance applies without a person. A rule that cannot match is worse than no rule."
            >
                <Show when=move || busy.get() && rules.get().is_empty()>
                    <p class="text-sm opacity-60">"Reading the rules…"</p>
                </Show>
                <Show when=move || !busy.get() && rules.get().is_empty()>
                    <p class="text-sm opacity-60" data-testid="rules-empty">"No rule is active."</p>
                </Show>
                <ul class="divide-y divide-base-300">
                    <For each=move || rules.get() key=|rule| rule.id.clone() let:rule>
                        {
                            let id = StoredValue::new(rule.id.clone());
                            let here = id.get_value();
                            let open = Signal::derive(move || editing.get().as_deref() == Some(here.as_str()));
                            let testing_here = {
                                let id = id.get_value();
                                Signal::derive(move || testing.get().as_deref() == Some(id.as_str()))
                            };
                            let sampling_here = {
                                let id = id.get_value();
                                Signal::derive(move || sampling.get().as_deref() == Some(id.as_str()))
                            };
                            let confirming_here = {
                                let id = id.get_value();
                                Signal::derive(move || confirming.get().as_deref() == Some(id.as_str()))
                            };
                            let stored = StoredValue::new(rule.clone());
                            let hits = if rule.hits == 1 {
                                "1 hit".to_owned()
                            } else {
                                format!("{} hits", rule.hits)
                            };
                            let state = if rule.enabled { "enabled" } else { "disabled" };
                            let summary = StoredValue::new(format!(
                                "{} · {} · {} · {hits} · {state}",
                                rule.pattern_kind, rule.scope, rule.action
                            ));
                            let sentence = StoredValue::new(format!(
                                "Looks at {} and {}.",
                                scope_line(&rule.scope),
                                action_line(&rule.action)
                            ));
                            view! {
                                <li class="py-2" data-testid="rule-row">
                                    <Show
                                        when=move || open.get()
                                        fallback={
                                            let held = rule.clone();
                                            move || {
                                            let rule = held.clone();
                                            let delete_name = rule.name.clone();
                                            let id = id.get_value();
                                            view! {
                                                <div class="flex items-start justify-between gap-3">
                                                    <div class="min-w-0">
                                                        <p class="text-sm font-medium">{rule.name.clone()}</p>
                                                        <code class="block truncate text-xs" data-testid="rule-pattern">{rule.pattern.clone()}</code>
                                                        <p class="mt-1 text-xs opacity-60" data-testid="rule-summary">{summary.get_value()}</p>
                                                        <p class="text-xs opacity-50">{sentence.get_value()}</p>
                                                    </div>
                                                    <div class="flex shrink-0 gap-1">
                                                        <button
                                                            class="btn btn-ghost btn-xs"
                                                            disabled=move || busy.get() || on_test.is_none()
                                                            on:click={
                                                                let id = id.clone();
                                                                move |_| {
                                                                    let id = id.clone();
                                                                    let already = sampling.get_untracked().as_deref() == Some(id.as_str());
                                                                    set_sampling.set(if already { None } else { Some(id) });
                                                                }
                                                            }
                                                        >
                                                            "Test"
                                                        </button>
                                                        <button
                                                            class="btn btn-ghost btn-xs"
                                                            disabled=move || busy.get() || !write.get()
                                                            title=move || (!write.get()).then_some(NO_WRITE).unwrap_or_default()
                                                            on:click={
                                                                let stored = stored;
                                                                move |_| {
                                                                    let rule = stored.get_value();
                                                                    set_edit_draft.set(RuleDraft::of(&rule));
                                                                    set_editing.set(Some(rule.id.clone()));
                                                                }
                                                            }
                                                        >
                                                            "Edit"
                                                        </button>
                                                        <button
                                                            class="btn btn-ghost btn-xs text-error"
                                                            disabled=move || busy.get() || !write.get()
                                                            title=move || (!write.get()).then_some(NO_WRITE).unwrap_or_default()
                                                            on:click={
                                                                let id = id.clone();
                                                                move |_| set_confirming.set(Some(id.clone()))
                                                            }
                                                        >
                                                            "Delete"
                                                        </button>
                                                    </div>
                                                </div>
                                                <Show when=move || confirming_here.get()>
                                                    <div class="mt-2 rounded-box bg-base-200 p-2" data-testid="delete-confirm">
                                                        <p class="text-sm">
                                                            {format!("Delete “{}”? What it caught stays on record, but the rule stops guarding the instance.", delete_name)}
                                                        </p>
                                                        <div class="mt-1 flex gap-2">
                                                            <button
                                                                class="btn btn-error btn-xs"
                                                                disabled=move || busy.get() || on_delete.is_none()
                                                                on:click={
                                                                    let id = id.clone();
                                                                    move |_| {
                                                                        set_confirming.set(None);
                                                                        if let Some(on_delete) = on_delete {
                                                                            on_delete.run(id.clone());
                                                                        }
                                                                    }
                                                                }
                                                            >
                                                                "Yes, delete"
                                                            </button>
                                                            <button
                                                                class="btn btn-ghost btn-xs"
                                                                on:click=move |_| set_confirming.set(None)
                                                            >
                                                                "Cancel"
                                                            </button>
                                                        </div>
                                                    </div>
                                                </Show>
                                            }
                                            }
                                        }
                                    >
                                        <RuleForm
                                            draft=edit_draft
                                            busy=busy
                                            write=write
                                            submit_label="Save changes"
                                            on_name=field(set_edit_draft, |draft, value| draft.name = value)
                                            on_pattern=field(set_edit_draft, |draft, value| draft.pattern = value)
                                            on_kind=field(set_edit_draft, |draft, value| draft.pattern_kind = value)
                                            on_scope=field(set_edit_draft, |draft, value| draft.scope = value)
                                            on_action=field(set_edit_draft, |draft, value| draft.action = value)
                                            on_enabled=Callback::new(move |value: bool| {
                                                set_edit_draft.update(|draft| draft.enabled = value);
                                            })
                                            on_submit={
                                                let id = id.get_value();
                                                Callback::new(move |_| {
                                                    let draft = edit_draft.get_untracked();
                                                    if draft.name.trim().is_empty() || draft.pattern.trim().is_empty() {
                                                        return;
                                                    }
                                                    if let Some(on_update) = on_update {
                                                        on_update.run(RuleEdit { id: id.clone(), draft });
                                                    }
                                                    // Close the form: a refusal arrives on the
                                                    // panel above, and keeping a saved rule
                                                    // open would look like it was not saved.
                                                    set_editing.set(None);
                                                })
                                            }
                                            on_cancel=Callback::new(move |_| set_editing.set(None))
                                        />
                                    </Show>
                                    <Show when=move || sampling_here.get()>
                                        <div class="mt-2 flex flex-col gap-2 rounded-box bg-base-200 p-2" data-testid="dry-run">
                                            <p class="text-xs opacity-60">
                                                "Paste a sample body. The dry run writes nothing: no hit is counted and no audit row is written."
                                            </p>
                                            <textarea
                                                class="textarea textarea-bordered min-h-20 text-sm"
                                                aria-label="Sample body"
                                                placeholder="A post body to test this rule against"
                                                prop:value=move || sample.get()
                                                on:input=move |event| set_sample.set(event_target_value(&event))
                                            ></textarea>
                                            <div class="flex items-center gap-2">
                                                <button
                                                    class="btn btn-primary btn-xs"
                                                    disabled=move || busy.get() || testing_here.get() || on_test.is_none()
                                                    on:click={
                                                        let id = id.get_value();
                                                        move |_| {
                                                            if let Some(on_test) = on_test {
                                                                on_test.run((id.clone(), sample.get_untracked()));
                                                            }
                                                        }
                                                    }
                                                >
                                                    "Dry run"
                                                </button>
                                                <Show when=move || testing_here.get()>
                                                    <span class="text-xs opacity-60">"Trying it…"</span>
                                                </Show>
                                            </div>
                                            <DryRunAnswer dry=dry rule_id=id.get_value() />
                                        </div>
                                    </Show>
                                </li>
                            }
                        }
                    </For>
                </ul>
            </Panel>

            <Panel title="New rule" description="Every field is read by the instance, and a pattern that cannot compile is refused with the reason and where.">
                <RuleForm
                    draft=draft
                    busy=busy
                    write=write
                    submit_label="Create rule"
                    on_name=field(set_draft, |draft, value| draft.name = value)
                    on_pattern=field(set_draft, |draft, value| draft.pattern = value)
                    on_kind=field(set_draft, |draft, value| draft.pattern_kind = value)
                    on_scope=field(set_draft, |draft, value| draft.scope = value)
                    on_action=field(set_draft, |draft, value| draft.action = value)
                    on_enabled=Callback::new(move |value: bool| {
                        set_draft.update(|draft| draft.enabled = value);
                    })
                    on_submit=Callback::new(move |_| submit_new(()))
                />
            </Panel>
        </div>
    }
}

/// The fields one rule is written with, shared by the create and edit forms.
#[component]
fn RuleForm(
    /// The fields as they stand.
    #[prop(into)]
    draft: Signal<RuleDraft>,
    /// Whether a request is on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// Whether this browser's grant carries `admin:write`.
    #[prop(into)]
    write: Signal<bool>,
    /// The word on the submit button.
    #[prop(into)]
    submit_label: String,
    /// Records the name.
    on_name: Callback<String>,
    /// Records the pattern.
    on_pattern: Callback<String>,
    /// Records the kind.
    on_kind: Callback<String>,
    /// Records the scope.
    on_scope: Callback<String>,
    /// Records the action.
    on_action: Callback<String>,
    /// Records the enabled toggle.
    on_enabled: Callback<bool>,
    /// Saves the form.
    on_submit: Callback<()>,
    /// Abandons the edit; absent on the create form.
    #[prop(optional)]
    on_cancel: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <form
            class="flex flex-col gap-2"
            on:submit=move |event| {
                stop_submit(&event);
                on_submit.run(());
            }
        >
            <input
                class="input input-bordered input-sm"
                placeholder="Name"
                aria-label="Rule name"
                prop:value=move || draft.get().name
                disabled=!write.get_untracked()
                on:input=move |event| on_name.run(event_target_value(&event))
            />
            <input
                class="input input-bordered input-sm font-mono"
                placeholder="Pattern"
                aria-label="Rule pattern"
                prop:value=move || draft.get().pattern
                disabled=!write.get_untracked()
                on:input=move |event| on_pattern.run(event_target_value(&event))
            />
            <div class="flex flex-wrap gap-2">
                <select
                    class="select select-bordered select-sm"
                    aria-label="Pattern kind"
                    disabled=!write.get_untracked()
                    on:change=move |event| on_kind.run(event_target_value(&event))
                >
                    {KINDS.into_iter().map(|kind| view! {
                        <option value=kind selected=move || draft.get().pattern_kind == kind>{kind}</option>
                    }).collect_view()}
                </select>
                <select
                    class="select select-bordered select-sm"
                    aria-label="Scope"
                    disabled=!write.get_untracked()
                    on:change=move |event| on_scope.run(event_target_value(&event))
                >
                    {SCOPES.into_iter().map(|scope| view! {
                        <option value=scope selected=move || draft.get().scope == scope>{scope}</option>
                    }).collect_view()}
                </select>
                <select
                    class="select select-bordered select-sm"
                    aria-label="Action"
                    disabled=!write.get_untracked()
                    on:change=move |event| on_action.run(event_target_value(&event))
                >
                    {ACTIONS.into_iter().map(|action| view! {
                        <option value=action selected=move || draft.get().action == action>{action}</option>
                    }).collect_view()}
                </select>
                <label class="label cursor-pointer gap-2">
                    <input
                        type="checkbox"
                        class="toggle toggle-sm"
                        prop:checked=move || draft.get().enabled
                        disabled=!write.get_untracked()
                        on:change=move |event| on_enabled.run(event_target_checked(&event))
                    />
                    <span class="label-text">"Enabled"</span>
                </label>
            </div>
            <div class="flex gap-2">
                <button
                    class="btn btn-primary btn-sm self-start"
                    disabled=move || busy.get() || !write.get()
                >
                    {submit_label.clone()}
                </button>
                {on_cancel.map(|on_cancel| view! {
                    <button
                        type="button"
                        class="btn btn-ghost btn-sm"
                        on:click=move |_| on_cancel.run(())
                    >
                        "Cancel"
                    </button>
                })}
            </div>
        </form>
    }
}

/// What the last dry run said, when it was this rule's.
#[component]
fn DryRunAnswer(
    /// The last answer, whichever rule asked.
    #[prop(into)]
    dry: Signal<Option<DryRun>>,
    /// The rule this panel belongs to.
    #[prop(into)]
    rule_id: String,
) -> impl IntoView {
    let mine = {
        let rule_id = rule_id.clone();
        Signal::derive(move || dry.get().filter(|dry| dry.rule_id == rule_id))
    };
    view! {
        <Show when=move || mine.get().is_some()>
            {move || mine.get().map(|dry| {
                let verdict = if dry.matched { "Matched" } else { "No match" };
                let class = if dry.matched { "badge badge-success badge-sm" } else { "badge badge-ghost badge-sm" };
                let scanned = if dry.truncated {
                    format!("Scanned the first {} bytes; the sample was cut.", dry.scanned_bytes)
                } else {
                    format!("Scanned {} bytes.", dry.scanned_bytes)
                };
                view! {
                    <div class="flex flex-col gap-1" data-testid="dry-run-answer">
                        <span class=class data-testid="dry-run-verdict">{verdict}</span>
                        <p class="text-sm">{dry.reason.clone()}</p>
                        {dry.span.clone().map(|(start, end, text)| view! {
                            <code class="rounded bg-base-300 px-1 text-xs">
                                {format!("bytes {start}..{end}: {text}")}
                            </code>
                        })}
                        <p class="text-xs opacity-60">{scanned}</p>
                    </div>
                }
            })}
        </Show>
    }
}

/// Keeps a submit from reloading the page on wasm, and keeps the native build's event used.
fn stop_submit(event: &leptos::ev::SubmitEvent) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = event;
    #[cfg(target_arch = "wasm32")]
    event.prevent_default();
}
