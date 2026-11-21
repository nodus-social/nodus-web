//! Appeals: the queue of people who disagreed with a decision about their own account.
//!
//! The list is a read under `admin:read`; the two verdicts are writes under `admin:write`. A
//! decision changes the row and a 409 means the row the moderator was reading was already
//! stale, so the queue is read again after every answer and the sentence above the list is
//! built from what came back — the person's name is in the queue, not in the refusal.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::moderation::appeals::{Appeal, AppealDecision, AppealFilter, AppealsPage};

use crate::state::alive::Alive;
use crate::state::bridge::admin::{self, AdminAppeal, Decided};

/// The queue, refiltered and refreshed from the instance.
#[component]
pub fn Appeals() -> impl IntoView {
    let alive = Alive::new();
    let (rows, set_rows) = signal(Vec::<AdminAppeal>::new());
    let (filter, set_filter) = signal(AppealFilter::Pending);
    let (waiting, set_waiting) = signal(0_usize);
    let (waiting_more, set_waiting_more) = signal(false);
    let (has_more, set_has_more) = signal(false);
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (notice, set_notice) = signal(None::<String>);
    let write = admin::may_write();

    let reload = Callback::new(move |()| {
        let current = filter.get_untracked();
        set_busy.set(true);
        spawn_local(async move {
            let (page, pending) = queue(current).await;
            alive.if_alive(|| {
                match page {
                    Ok(list) => {
                        set_has_more.set(list.len() >= usize::from(admin::PAGE));
                        set_rows.set(list);
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                if let Some((count, capped)) = pending {
                    set_waiting.set(count);
                    set_waiting_more.set(capped);
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| reload.run(()));

    let choose = Callback::new(move |next: AppealFilter| {
        set_filter.set(next);
        set_notice.set(None);
        reload.run(());
    });

    let more = Callback::new(move |()| {
        let current = filter.get_untracked();
        let offset = u32::try_from(rows.get_untracked().len()).unwrap_or(0);
        spawn_local(async move {
            match admin::appeals(current.wire(), offset).await {
                Ok(next) => alive.if_alive(|| {
                    set_has_more.set(next.len() >= usize::from(admin::PAGE));
                    set_rows.update(|held| held.extend(next));
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let decide = Callback::new(move |decision: AppealDecision| {
        // Set before the request leaves, so the row's button is held from the first press:
        // `set_busy` lands synchronously with the click, and a second click is a no-op.
        set_busy.set(true);
        set_error.set(None);
        set_notice.set(None);
        let current = filter.get_untracked();
        spawn_local(async move {
            let outcome = admin::decide_appeal(&decision.id, decision.accept, &decision.note).await;
            // The answer may have changed the row, and a 409 says the row was stale; either
            // way the queue is what the screen should be able to trust.
            let (page, pending) = queue(current).await;
            alive.if_alive(|| {
                if let Ok(list) = &page {
                    set_has_more.set(list.len() >= usize::from(admin::PAGE));
                    set_rows.set(list.clone());
                }
                if let Some((count, capped)) = pending {
                    set_waiting.set(count);
                    set_waiting_more.set(capped);
                }
                set_busy.set(false);
                match outcome {
                    Ok(Decided::Row(row)) => set_notice.set(Some(decided_line(&decision, &row))),
                    Ok(Decided::AlreadyDecided) => {
                        // The instance's refusal carries no name; the queue does, once the
                        // row it refused comes back decided.
                        let who = page
                            .as_ref()
                            .ok()
                            .and_then(|list| list.iter().find(|row| row.id == decision.id))
                            .and_then(|row| row.decided_by.clone());
                        set_notice.set(Some(match who {
                            Some(who) => format!(
                                "That appeal was already decided by {who}. Nothing was changed."
                            ),
                            None => "That appeal was already decided by another moderator. \
                                     Nothing was changed."
                                .to_owned(),
                        }));
                    }
                    Err(message) => set_error.set(Some(message)),
                }
            });
        });
    });

    let rows = Signal::derive(move || rows.get().into_iter().map(appeal_of).collect::<Vec<_>>());

    view! {
        <AppealsPage
            appeals=rows
            filter=filter
            waiting=waiting
            waiting_more=waiting_more
            busy=busy
            error=error
            notice=Some(Signal::from(notice))
            has_more=Some(Signal::from(has_more))
            write=Some(write)
            on_filter=choose
            on_more=more
            on_decide=decide
        />
    }
}

/// The queue under `filter`, and how many appeals are waiting — the same read when the filter
/// is the work list, and one extra read otherwise.
async fn queue(filter: AppealFilter) -> (Result<Vec<AdminAppeal>, String>, Option<(usize, bool)>) {
    let page = admin::appeals(filter.wire(), 0).await;
    let pending = match filter {
        AppealFilter::Pending => page.as_ref().map(|rows| rows.len()).ok(),
        _ => admin::appeals("pending", 0)
            .await
            .map(|rows| rows.len())
            .ok(),
    };
    let waiting = pending.map(|count| (count, count >= usize::from(admin::PAGE)));
    (page, waiting)
}

fn appeal_of(row: AdminAppeal) -> Appeal {
    Appeal {
        id: row.id,
        handle: row.handle,
        action: row.action,
        reason: row.reason,
        status: row.status,
        restriction: row.restriction,
        decision_note: row.decision_note,
        decided_by: row.decided_by,
        waited: waited_words(row.age_seconds),
        decided: row
            .decided_at
            .map(|at| at.chars().take(10).collect::<String>()),
    }
}

/// How long an appeal has waited, in words. The instance measured it, so the browser does not
/// need its clock for this.
fn waited_words(seconds: i64) -> String {
    match seconds {
        ..=59 => "just arrived".to_owned(),
        60..=3_599 => format!("waited {}m", seconds / 60),
        3_600..=86_399 => format!("waited {}h", seconds / 3_600),
        _ => format!("waited {}d", seconds / 86_400),
    }
}

/// What the decision did, in the sentence above the queue.
fn decided_line(decision: &AppealDecision, row: &AdminAppeal) -> String {
    if decision.accept {
        format!("Accepted. {}", lifted(&row.action))
    } else {
        format!("Rejected. {}", stood(&row.action))
    }
}

fn lifted(action: &str) -> &'static str {
    match action {
        "suspend" => "The suspension was lifted.",
        "silence" => "The silence was lifted.",
        "reject" => "The registration was approved.",
        _ => "The restriction was lifted.",
    }
}

fn stood(action: &str) -> &'static str {
    match action {
        "suspend" => "The suspension stays in force.",
        "silence" => "The silence stays in force.",
        "reject" => "The registration stays refused.",
        _ => "The decision stands.",
    }
}
