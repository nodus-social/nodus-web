//! The instance's own rules: the document a report cites, and the order it cites it in.
//!
//! This is the `instance_rules` resource (`/api/v1/admin/rules`), not the moderation-rule
//! patterns the scanning engine matches. Those are sentences about the instance itself, and
//! their order is part of the document — a report that says "rule 3" has to keep meaning the
//! same sentence — so a move sends the position the rule should hold and reads the list back
//! rather than guessing what the instance renumbered.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::admin::rules::{RuleRow, RulesPage};

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// The rules this instance holds, with the one being edited and the one being deleted.
#[component]
pub fn Rules() -> impl IntoView {
    let alive = Alive::new();
    let (rules, set_rules) = signal(Vec::<RuleRow>::new());
    let (draft, set_draft) = signal(String::new());
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (editing, set_editing) = signal(None::<String>);
    let (edit_text, set_edit_text) = signal(String::new());
    let (confirming, set_confirming) = signal(None::<String>);

    let reload = Callback::new(move |()| {
        spawn_local(async move {
            let outcome = admin::instance_rules().await;
            alive.if_alive(|| {
                set_busy.set(true);
                match outcome {
                    Ok(list) => {
                        set_rules.set(
                            list.into_iter()
                                .map(|rule| RuleRow {
                                    id: rule.id,
                                    text: rule.text,
                                    ordinal: rule.ordinal,
                                })
                                .collect::<Vec<RuleRow>>(),
                        );
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| reload.run(()));

    let add = Callback::new(move |()| {
        let text = draft.get_untracked().trim().to_owned();
        if text.is_empty() {
            set_error.set(Some("A rule needs text.".to_owned()));
            return;
        }
        set_busy.set(true);
        spawn_local(async move {
            match admin::create_instance_rule(&text, None).await {
                Ok(_) => {
                    alive.if_alive(|| {
                        set_draft.set(String::new());
                        set_error.set(None);
                    });
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| {
                    set_busy.set(false);
                    set_error.set(Some(message));
                }),
            }
        });
    });

    // Editing keeps the rule's identity: the ordinal, and any report that cited it, stay put.
    let begin_edit = Callback::new(move |rule: RuleRow| {
        set_confirming.set(None);
        set_editing.set(Some(rule.id));
        set_edit_text.set(rule.text);
    });

    let cancel_edit = Callback::new(move |()| set_editing.set(None));

    let save_edit = Callback::new(move |()| {
        let Some(id) = editing.get_untracked() else {
            return;
        };
        let text = edit_text.get_untracked().trim().to_owned();
        if text.is_empty() {
            set_error.set(Some("A rule needs text.".to_owned()));
            return;
        }
        set_busy.set(true);
        spawn_local(async move {
            match admin::update_instance_rule(&id, Some(&text), None).await {
                Ok(_) => {
                    alive.if_alive(|| {
                        set_editing.set(None);
                        set_error.set(None);
                    });
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| {
                    set_busy.set(false);
                    set_error.set(Some(message));
                }),
            }
        });
    });

    // A move sends only the position: the text is left alone, and the answer decides what
    // the list now says rather than this screen reordering its own copy.
    let move_rule = Callback::new(move |(id, ordinal): (String, i32)| {
        if ordinal < 0 {
            return;
        }
        set_busy.set(true);
        spawn_local(async move {
            match admin::update_instance_rule(&id, None, Some(ordinal)).await {
                Ok(_) => {
                    alive.if_alive(|| set_error.set(None));
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| {
                    set_busy.set(false);
                    set_error.set(Some(message));
                }),
            }
        });
    });

    let ask_delete = Callback::new(move |id: String| {
        set_editing.set(None);
        set_confirming.set(Some(id));
    });

    let cancel_delete = Callback::new(move |()| set_confirming.set(None));

    let delete = Callback::new(move |id: String| {
        set_busy.set(true);
        spawn_local(async move {
            match admin::delete_instance_rule(&id).await {
                Ok(()) => {
                    alive.if_alive(|| {
                        set_confirming.set(None);
                        set_error.set(None);
                    });
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| {
                    set_busy.set(false);
                    set_error.set(Some(message));
                }),
            }
        });
    });

    view! {
        <RulesPage
            rules=rules
            draft=draft
            busy=busy
            error=error
            editing=editing
            edit_text=edit_text
            confirming=confirming
            on_draft=Callback::new(move |value: String| set_draft.set(value))
            on_add=add
            on_edit=begin_edit
            on_edit_text=Callback::new(move |value: String| set_edit_text.set(value))
            on_save_edit=save_edit
            on_cancel_edit=cancel_edit
            on_ask_delete=ask_delete
            on_cancel_delete=cancel_delete
            on_delete=delete
            on_move=move_rule
            write=Some(admin::may_write())
        />
    }
}
