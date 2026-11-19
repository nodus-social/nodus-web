//! Moderation rules: the list is a read under `admin:read`, the writes under `admin:write`,
//! and the dry run a read that writes nothing.
//!
//! The screen keeps the instance's answers as they came: a rule that was refused is shown
//! with the instance's sentence (a regex names the byte it failed on), and a dry run is
//! shown as the scanner answered it, matched or not.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::moderation::rules::{DryRun, Rule, RuleDraft, RulesPage};

use crate::state::alive::Alive;
use crate::state::bridge::admin::{self, ModerationRule};

/// The moderation rules panel.
#[component]
pub fn Rules() -> impl IntoView {
    let alive = Alive::new();
    let (rows, set_rows) = signal(Vec::<ModerationRule>::new());
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (notice, set_notice) = signal(None::<String>);
    let (testing, set_testing) = signal(None::<String>);
    let (dry, set_dry) = signal(None::<DryRun>);
    let write = admin::may_write();

    let reload = Callback::new(move |()| {
        set_busy.set(true);
        spawn_local(async move {
            let outcome = admin::moderation_rules().await;
            alive.if_alive(|| {
                match outcome {
                    Ok(list) => {
                        set_rows.set(list);
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| reload.run(()));

    // Every write follows the same shape: say what is happening, send it, then read the list
    // again — the hit count and the enabled flag are the instance's, not the form's.
    let finish = move |outcome: Result<(), String>, done: &'static str| {
        match outcome {
            Ok(()) => {
                set_error.set(None);
                set_notice.set(Some(done.to_owned()));
                reload.run(());
            }
            Err(message) => {
                set_notice.set(None);
                set_error.set(Some(message));
            }
        }
    };

    let create = Callback::new(move |draft: RuleDraft| {
        set_busy.set(true);
        set_error.set(None);
        set_notice.set(None);
        spawn_local(async move {
            let outcome = admin::create_moderation_rule(
                &draft.name,
                &draft.pattern,
                &draft.pattern_kind,
                &draft.scope,
                &draft.action,
                draft.enabled,
            )
            .await;
            alive.if_alive(|| {
                finish(outcome.map(|_| ()), "Rule created.");
                set_busy.set(false);
            });
        });
    });

    let update = Callback::new(
        move |edit: nodus_pages::pages::moderation::rules::RuleEdit| {
            set_busy.set(true);
            set_error.set(None);
            set_notice.set(None);
            spawn_local(async move {
                let draft = edit.draft;
                let outcome = admin::update_moderation_rule(
                    &edit.id,
                    &draft.name,
                    &draft.pattern,
                    &draft.pattern_kind,
                    &draft.scope,
                    &draft.action,
                    draft.enabled,
                )
                .await;
                alive.if_alive(|| {
                    finish(outcome.map(|_| ()), "Rule saved.");
                    set_busy.set(false);
                });
            });
        },
    );

    let delete = Callback::new(move |id: String| {
        set_busy.set(true);
        set_error.set(None);
        set_notice.set(None);
        spawn_local(async move {
            let outcome = admin::delete_moderation_rule(&id).await;
            alive.if_alive(|| {
                finish(outcome, "Rule deleted.");
                set_busy.set(false);
            });
        });
    });

    let test = Callback::new(move |(id, sample): (String, String)| {
        set_testing.set(Some(id.clone()));
        set_error.set(None);
        spawn_local(async move {
            let outcome = admin::dry_run_moderation_rule(&id, &sample).await;
            alive.if_alive(|| {
                set_testing.set(None);
                match outcome {
                    Ok(answer) => set_dry.set(Some(DryRun {
                        rule_id: answer
                            .rule
                            .map(|rule| rule.id)
                            .filter(|held| !held.is_empty())
                            .unwrap_or(id),
                        matched: answer.matched,
                        span: answer
                            .span
                            .map(|span| (span.start, span.end, span.text)),
                        reason: answer.reason,
                        scanned_bytes: answer.scanned_bytes,
                        truncated: answer.truncated,
                    })),
                    // A regex that cannot compile comes back here with the instance's
                    // sentence; the panel above shows it rather than a silent no-answer.
                    Err(message) => set_error.set(Some(message)),
                }
            });
        });
    });

    let rules = Signal::derive(move || rows.get().into_iter().map(rule_of).collect::<Vec<_>>());

    view! {
        <RulesPage
            rules=rules
            dry=dry
            testing=testing
            busy=busy
            error=error
            notice=Some(Signal::from(notice))
            write=Some(write)
            on_create=create
            on_update=update
            on_delete=delete
            on_test=test
        />
    }
}

fn rule_of(row: ModerationRule) -> Rule {
    Rule {
        id: row.id,
        name: row.name,
        pattern: row.pattern,
        pattern_kind: row.pattern_kind,
        scope: row.scope,
        action: row.action,
        enabled: row.enabled,
        hits: row.hits,
    }
}
