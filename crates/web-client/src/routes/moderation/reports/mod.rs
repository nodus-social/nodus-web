//! The report queue: the filter, the pages, and the decision panel.
//!
//! The page draws; what stays here is asking the instance, remembering what it said, and
//! refilling the list after a decision.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::{Report, ReportFilter};
use nodus_pages::pages::moderation::reports::{ReportQuery, ReportsPage};

use crate::routes::format::relative_time;
use crate::state::bridge::{admin, revision};

/// How many reports one page holds, which is also how a full page reads as "maybe more".
const PAGE: u16 = 20;

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
            age_of=age_of
            on_query=Callback::new(move |next: ReportQuery| set_query.set(next))
            on_more=more
            on_open=open
            on_close=close
            on_resolve=resolve
            on_reopen=reopen
            on_assign=assign
        />
    }
}
