//! The report queue: the filter, the pages, and the decision panel.
//!
//! The page draws; what stays here is asking the instance, remembering what it said, and
//! refilling the list after a decision.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::{Report, ReportFilter};
use nodus_pages::pages::admin::federation::forwarded::Forward;
use nodus_pages::pages::moderation::reports::{Forwarding, ReportQuery, ReportsPage};
use nodus_pages::pages::moderation::{BulkOutcome, ReportBatch};

use crate::routes::format::relative_time;
use crate::state::bridge::{admin, revision};

/// How many reports one page holds, which is also how a full page reads as "maybe more".
const PAGE: u16 = 20;

/// How many forwards the queue reads at once, so a row can show its report's state.
///
/// The queue cannot ask per row: an absent forward answers 404, which reads the same as a
/// broken request. One list of the newest forwards covers every row a moderator is likely
/// to act on, and a row that is missing from it simply offers the press — which the
/// idempotent endpoint answers with the row that already exists anyway.
const FORWARDED_PAGE: u16 = 50;

/// The instance's sentence, with the transport's lead-in and error envelope peeled off.
///
/// A refusal names why it refused — a local account, a blocked or paused domain — and that
/// sentence is the only explanation a moderator gets, so it is shown as it stands. The
/// transport hands the raw body back as `instance returned 409: {"error":{"message":…}}`,
/// so the envelope has to be opened before the sentence is there.
fn reason_of(problem: String) -> String {
    let body = match problem.split_once(": ") {
        Some((lead, rest))
            if lead.starts_with("discovery returned ") || lead.starts_with("instance returned ") =>
        {
            rest
        }
        _ => return problem,
    };
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|envelope| envelope.get("error")?.get("message")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| body.to_owned())
}

/// The page's shape of one forwarded report.
fn forward(row: admin::Forwarded) -> Forward {
    Forward {
        id: row.id,
        report_id: row.report_id,
        target_domain: row.target_domain,
        activity_uri: row.activity_uri,
        state: row.state,
        attempts: row.attempts,
        last_error: row.last_error,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

/// Open reports, each with the one button a moderator needs.
#[component]
pub fn ReportQueue() -> impl IntoView {
    let revision = revision();
    let (reports, set_reports) = signal(Vec::<Report>::new());
    let (query, set_query) = signal(ReportQuery::default());
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (has_more, set_has_more) = signal(false);
    let (selected, set_selected) = signal(None::<Report>);
    let (bulk, set_bulk) = signal(BulkOutcome::default());
    let (forwarded, set_forwarded) = signal(Vec::<Forward>::new());
    let (forwarding, set_forwarding) = signal(Forwarding::default());
    let forwardable = crate::state::bridge::feature("report_forwarding");

    // What was forwarded, read once per revision. An instance whose capability list omits
    // forwarding is not asked at all: there, a report about a remote account could only
    // ever be refused.
    Effect::new(move |_| {
        revision.get();
        if !forwardable.get() {
            return;
        }
        spawn_local(async move {
            match admin::forwarded(None, None, FORWARDED_PAGE).await {
                Ok(rows) => set_forwarded.set(rows.into_iter().map(forward).collect()),
                Err(problem) => set_error.set(Some(problem)),
            }
        });
    });

    let filter = move |query: &ReportQuery| ReportFilter {
        state: query.state_value(),
        category: Some(query.category.clone()).filter(|word| !word.is_empty()),
        assigned_account_id: Some(query.assignee.clone()).filter(|word| !word.is_empty()),
        limit: Some(PAGE),
        ..ReportFilter::default()
    };

    Effect::new(move |_| {
        revision.get();
        let query = query.get();
        spawn_local(async move {
            set_loading.set(true);
            match admin::reports_page(&filter(&query)).await {
                Ok(list) => {
                    set_has_more.set(list.len() >= usize::from(PAGE));
                    set_reports.set(list);
                    set_error.set(None);
                }
                Err(problem) => set_error.set(Some(problem)),
            }
            set_loading.set(false);
        });
    });

    let more = Callback::new(move |()| {
        let query = query.get_untracked();
        let offset = u32::try_from(reports.get_untracked().len()).unwrap_or(0);
        spawn_local(async move {
            let page = ReportFilter {
                offset: Some(offset),
                ..filter(&query)
            };
            match admin::reports_page(&page).await {
                Ok(list) => {
                    set_has_more.set(list.len() >= usize::from(PAGE));
                    set_reports.update(|current| current.extend(list));
                }
                Err(problem) => set_error.set(Some(problem)),
            }
        });
    });

    let open = Callback::new(move |id: String| {
        spawn_local(async move {
            match admin::report(&id).await {
                Ok(report) => set_selected.set(Some(report)),
                Err(problem) => set_error.set(Some(problem)),
            }
        });
    });

    let close = Callback::new(move |()| set_selected.set(None));

    let refresh = move || {
        let query = query.get_untracked();
        spawn_local(async move {
            match admin::reports_page(&filter(&query)).await {
                Ok(list) => {
                    set_has_more.set(list.len() >= usize::from(PAGE));
                    set_reports.set(list);
                    set_error.set(None);
                }
                Err(problem) => set_error.set(Some(problem)),
            }
            set_selected.set(None);
        });
    };

    let resolve = Callback::new(move |(id, note): (String, String)| {
        spawn_local(async move {
            match admin::resolve_report_with_note(&id, &note).await {
                Ok(_) => refresh(),
                Err(problem) => set_error.set(Some(problem)),
            }
        });
    });

    let reopen = Callback::new(move |id: String| {
        spawn_local(async move {
            match admin::reopen_report(&id).await {
                Ok(_) => refresh(),
                Err(problem) => set_error.set(Some(problem)),
            }
        });
    });

    let assign = Callback::new(move |(id, account): (String, Option<String>)| {
        spawn_local(async move {
            match admin::assign_report(&id, account.as_deref()).await {
                Ok(report) => {
                    set_reports.update(|current| {
                        if let Some(slot) = current.iter_mut().find(|row| row.id == report.id) {
                            *slot = report.clone();
                        }
                    });
                    set_selected.set(Some(report));
                    set_error.set(None);
                }
                Err(problem) => set_error.set(Some(problem)),
            }
        });
    });

    // One action over every ticked report. The instance answers per id, so a report it
    // refused keeps its sentence on its own row instead of becoming one toast.
    let batch = Callback::new(move |request: ReportBatch| {
        spawn_local(async move {
            let outcome = admin::bulk_reports(
                &request.ids,
                request.action.wire(),
                request.assignee.as_deref(),
                &request.note,
            )
            .await;
            match outcome {
                Ok(answer) => {
                    set_bulk.set(BulkOutcome {
                        applied: answer.applied,
                        refusals: answer.refusals,
                    });
                    set_error.set(None);
                    refresh();
                }
                Err(problem) => set_error.set(Some(problem)),
            }
        });
    });

    // The instance is idempotent: a second press answers the row the first one created, so
    // the queue only has to record that row and the action becomes the state. A refusal
    // is the instance's own sentence and stays on the row that caused it.
    let forward = Callback::new(move |(id, note): (String, String)| {
        set_forwarding.update(|held| held.in_flight.push(id.clone()));
        spawn_local(async move {
            let answer = admin::forward_report(&id, &note).await;
            set_forwarding.update(|held| {
                held.in_flight.retain(|held_id| held_id != &id);
                held.refusals.retain(|(held_id, _)| held_id != &id);
            });
            match answer {
                Ok(row) => {
                    set_forwarded.update(|rows| {
                        rows.retain(|held| held.report_id != row.report_id);
                        rows.insert(0, forward(row));
                    });
                }
                Err(problem) => {
                    set_forwarding.update(|held| held.refusals.push((id, reason_of(problem))));
                }
            }
        });
    });

    // The clock is the browser's; the page only asks how long ago a report was filed.
    let age_of = Callback::new(|created_at: String| relative_time(&created_at));

    view! {
        <ReportsPage
            reports=reports
            query=query
            loading=loading
            error=error
            has_more=has_more
            selected=selected
            forwarded=forwarded
            forwarding=forwarding
            age_of=age_of
            on_query=Callback::new(move |next: ReportQuery| set_query.set(next))
            on_more=more
            on_open=open
            on_close=close
            on_resolve=resolve
            on_reopen=reopen
            on_assign=assign
            bulk=bulk
            on_bulk=batch
            write=Some(crate::state::bridge::admin::may_write())
            forwardable=Some(crate::state::bridge::feature("report_forwarding"))
            on_forward=forward
        />
    }
}
