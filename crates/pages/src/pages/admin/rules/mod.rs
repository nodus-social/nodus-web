//! The instance's rules: the document a report cites, and the order it is cited in.
//!
//! The order is the interface. A reader says "rule 3" and a report that cites it has to keep
//! meaning the same thing, so the controls that move a rule are drawn as plainly as the ones
//! that word it: a move sends the position the rule should hold, and the instance renumbers
//! the list so the positions stay contiguous.
//!
//! This is not the moderation-rules panel. Those rows are patterns the scanning engine
//! matches; these rows are the sentences the instance publishes about itself.

use leptos::prelude::*;

use crate::components::panel::Panel;

/// One rule as this page draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleRow {
    /// The rule's identity, which is what a write names.
    pub id: String,
    /// The sentence itself.
    pub text: String,
    /// Zero-based position, as the instance holds it.
    pub ordinal: i32,
}

impl RuleRow {
    /// What a reader calls this rule: "Rule 3", counting from one.
    #[must_use]
    pub fn heading(&self) -> String {
        format!("Rule {}", self.ordinal + 1)
    }
}

/// The instance's rules, with the one being edited and the one about to be deleted.
#[component]
pub fn RulesPage(
    /// The rules, in the instance's order.
    #[prop(into)]
    rules: Signal<Vec<RuleRow>>,
    /// The sentence being typed for a new rule.
    #[prop(into)]
    draft: Signal<String>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records the new rule's sentence.
    #[prop(optional)]
    on_draft: Option<Callback<String>>,
    /// Adds the rule at the end of the list.
    #[prop(optional)]
    on_add: Option<Callback<()>>,
    /// The id of the rule being edited, when one is.
    #[prop(into)]
    #[prop(optional)]
    editing: Signal<Option<String>>,
    /// The sentence being edited.
    #[prop(into)]
    #[prop(optional)]
    edit_text: Signal<String>,
    /// Opens one rule for editing.
    #[prop(optional)]
    on_edit: Option<Callback<RuleRow>>,
    /// Records the edited sentence.
    #[prop(optional)]
    on_edit_text: Option<Callback<String>>,
    /// Saves the edit, keeping the rule's identity.
    #[prop(optional)]
    on_save_edit: Option<Callback<()>>,
    /// Abandons the edit.
    #[prop(optional)]
    on_cancel_edit: Option<Callback<()>>,
    /// The id of the rule whose deletion is waiting on a confirmation.
    #[prop(into)]
    #[prop(optional)]
    confirming: Signal<Option<String>>,
    /// Asks to delete one rule, so the question can be drawn.
    #[prop(optional)]
    on_ask_delete: Option<Callback<String>>,
    /// Takes back the question.
    #[prop(optional)]
    on_cancel_delete: Option<Callback<()>>,
    /// Deletes one rule, once confirmed.
    #[prop(optional)]
    on_delete: Option<Callback<String>>,
    /// Moves one rule to a new zero-based position.
    #[prop(optional)]
    on_move: Option<Callback<(String, i32)>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    // The last position a rule can hold, as a number the row can compare an ordinal with.
    let last = Signal::derive(move || {
        i32::try_from(rules.get().len().saturating_sub(1)).unwrap_or(i32::MAX)
    });
    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || !write.get()>
                <crate::components::chrome::Note class="text-sm text-warning">
                    {crate::props::NO_WRITE}
                </crate::components::chrome::Note>
            </Show>

            <Panel
                title="Instance rules"
                description="The rules this instance publishes, in the order a report cites them. Moving a rule renumbers the list, so a citation keeps pointing at the same sentence."
            >
                <Show when=move || rules.get().is_empty()>
                    <p class="text-sm opacity-70">"This instance has no rules yet."</p>
                </Show>

                <ol class="flex flex-col gap-2">
                    // The key carries what the row shows, not just the id: `For` reuses the view
                    // it drew for a key, and each row's sentence and heading are read once when
                    // that view is built, so a row kept under a bare id would keep drawing the
                    // words and the position the rule had before it was edited or moved.
                    <For
                        each=move || rules.get()
                        key=|rule| (rule.id.clone(), rule.ordinal, rule.text.clone())
                        let:rule
                    >
                        {
                            let id = StoredValue::new(rule.id.clone());
                            let here = rule.id.clone();
                            let ordinal = rule.ordinal;
                            let stored = StoredValue::new(rule.clone());
                            let heading = rule.heading();
                            let text = rule.text.clone();
                            let open = Signal::derive({
                                let here = here.clone();
                                move || editing.get().as_deref() == Some(here.as_str())
                            });
                            let asking = Signal::derive({
                                let here = here.clone();
                                move || confirming.get().as_deref() == Some(here.as_str())
                            });
                            // The labels are built once and read from a stored value, because
                            // the row is drawn more than once: `For` re-runs its children when
                            // the list changes, so anything a branch moves out of the closure
                            // would make that closure a `FnOnce`.
                            let ask_label = StoredValue::new(format!("Delete {heading}"));
                            let edit_label = StoredValue::new(format!("Edit {heading}"));
                            let up_label = StoredValue::new(format!("Move {heading} up"));
                            let down_label = StoredValue::new(format!("Move {heading} down"));
                            let text_label = StoredValue::new(format!("{heading} text"));
                            let shown_heading = StoredValue::new(heading);
                            let shown = StoredValue::new(text.clone());
                            view! {
                                <li class="rounded-box bg-base-200 p-2">
                                    <Show
                                        when=move || open.get()
                                        fallback=move || view! {
                                            <div class="flex items-start gap-2">
                                                <span class="shrink-0 text-xs font-medium opacity-60">
                                                    {shown_heading.get_value()}
                                                </span>
                                                <p class="min-w-0 grow whitespace-pre-line text-sm">
                                                    {shown.get_value()}
                                                </p>
                                                <div class="ml-auto flex shrink-0 gap-1">
                                                    <button
                                                        class="btn btn-ghost btn-xs"
                                                        aria-label=up_label.get_value()
                                                        disabled=move || on_move.is_none() || !write.get() || ordinal == 0
                                                        on:click=move |_| {
                                                            if let Some(on_move) = on_move {
                                                                on_move.run((id.get_value(), ordinal - 1));
                                                            }
                                                        }
                                                    >
                                                        "Move up"
                                                    </button>
                                                    <button
                                                        class="btn btn-ghost btn-xs"
                                                        aria-label=down_label.get_value()
                                                        disabled=move || on_move.is_none() || !write.get() || (ordinal >= last.get())
                                                        on:click=move |_| {
                                                            if let Some(on_move) = on_move {
                                                                on_move.run((id.get_value(), ordinal + 1));
                                                            }
                                                        }
                                                    >
                                                        "Move down"
                                                    </button>
                                                    <button
                                                        class="btn btn-ghost btn-xs"
                                                        aria-label=edit_label.get_value()
                                                        disabled=move || on_edit.is_none() || !write.get()
                                                        title=move || if write.get() { Default::default() } else { crate::props::NO_WRITE }
                                                        on:click=move |_| {
                                                            if let Some(on_edit) = on_edit {
                                                                on_edit.run(stored.get_value());
                                                            }
                                                        }
                                                    >
                                                        "Edit"
                                                    </button>
                                                    <button
                                                        class="btn btn-ghost btn-xs text-error"
                                                        aria-label=ask_label.get_value()
                                                        disabled=move || on_ask_delete.is_none() || !write.get()
                                                        title=move || if write.get() { Default::default() } else { crate::props::NO_WRITE }
                                                        on:click=move |_| {
                                                            if let Some(on_ask_delete) = on_ask_delete {
                                                                on_ask_delete.run(id.get_value());
                                                            }
                                                        }
                                                    >
                                                        "Delete"
                                                    </button>
                                                </div>
                                            </div>
                                            <Show when=move || asking.get()>
                                                <div class="mt-2 flex flex-wrap items-center gap-2 rounded-box bg-base-100 p-2">
                                                    <p class="text-xs">
                                                        "Delete this rule? Reports that cited it keep the reference."
                                                    </p>
                                                    <button
                                                        class="btn btn-error btn-xs"
                                                        disabled=move || on_delete.is_none()
                                                        on:click=move |_| {
                                                            if let Some(on_delete) = on_delete {
                                                                on_delete.run(id.get_value());
                                                            }
                                                        }
                                                    >
                                                        "Delete rule"
                                                    </button>
                                                    <button
                                                        class="btn btn-ghost btn-xs"
                                                        disabled=move || on_cancel_delete.is_none()
                                                        on:click=move |_| {
                                                            if let Some(on_cancel_delete) = on_cancel_delete {
                                                                on_cancel_delete.run(());
                                                            }
                                                        }
                                                    >
                                                        "Keep it"
                                                    </button>
                                                </div>
                                            </Show>
                                        }
                                    >
                                        <div class="flex flex-col gap-2">
                                            <textarea
                                                class="textarea textarea-bordered textarea-sm h-24"
                                                aria-label=text_label.get_value()
                                                prop:value=move || edit_text.get()
                                                disabled=on_edit_text.is_none()
                                                on:input=move |event| {
                                                    if let Some(on_edit_text) = on_edit_text {
                                                        on_edit_text.run(event_target_value(&event));
                                                    }
                                                }
                                            ></textarea>
                                            <div class="flex gap-2">
                                                <button
                                                    class="btn btn-primary btn-xs"
                                                    disabled=move || busy.get() || on_save_edit.is_none() || !write.get()
                                                    on:click=move |_| {
                                                        if let Some(on_save_edit) = on_save_edit {
                                                            on_save_edit.run(());
                                                        }
                                                    }
                                                >
                                                    "Save changes"
                                                </button>
                                                <button
                                                    class="btn btn-ghost btn-xs"
                                                    disabled=move || on_cancel_edit.is_none()
                                                    on:click=move |_| {
                                                        if let Some(on_cancel_edit) = on_cancel_edit {
                                                            on_cancel_edit.run(());
                                                        }
                                                    }
                                                >
                                                    "Cancel"
                                                </button>
                                            </div>
                                        </div>
                                    </Show>
                                </li>
                            }
                        }
                    </For>
                </ol>
            </Panel>

            <Panel
                title="Add a rule"
                description="One sentence. It is appended to the end, and the list renumbers itself."
            >
                <div class="flex flex-col gap-2">
                    <textarea
                        class="textarea textarea-bordered textarea-sm h-24"
                        placeholder="Be excellent to each other."
                        aria-label="New rule text"
                        prop:value=move || draft.get()
                        disabled=on_draft.is_none()
                        on:input=move |event| {
                            if let Some(on_draft) = on_draft {
                                on_draft.run(event_target_value(&event));
                            }
                        }
                    ></textarea>
                    <button
                        class="btn btn-sm btn-primary self-start"
                        disabled=move || busy.get() || on_add.is_none() || !write.get()
                        on:click=move |_| {
                            if let Some(on_add) = on_add {
                                on_add.run(());
                            }
                        }
                    >
                        "Add rule"
                    </button>
                </div>
            </Panel>
        </div>
    }
}
