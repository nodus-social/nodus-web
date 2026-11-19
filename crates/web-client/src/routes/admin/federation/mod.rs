//! Federation delivery: the queue as the instance holds it, and the two retries.
//!
//! Reading is `admin:read` and retrying is `admin:write`; the page draws the levers it may
//! press from the grant, and this half is the part that sends.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::admin::federation::{Counts, DeliveriesPage, Delivery, Query, PAGE};

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// What the instance answered, translated once into the page's own shape.
fn delivery_of(row: admin::Delivery) -> Delivery {
    Delivery {
        id: row.id,
        inbox_url: row.inbox_url,
        host: row.host,
        activity_kind: row.activity_kind,
        state: row.state,
        attempts: row.attempts,
        last_status_code: row.last_status_code,
        last_error: row.last_error,
        next_attempt_at: row.next_attempt_at,
        created_at: row.created_at,
        delivered_at: row.delivered_at,
    }
}

/// The queue's counts, in the page's own shape.
fn counts_of(counts: admin::DeliveryCounts) -> Counts {
    Counts {
        pending: counts.pending,
        failed: counts.failed,
        gave_up: counts.gave_up,
        delivered: counts.delivered,
    }
}

/// The delivery queue, read once and retried on request.
#[component]
pub fn Deliveries() -> impl IntoView {
    let alive = Alive::new();
    let (rows, set_rows) = signal(Vec::<Delivery>::new());
    let (counts, set_counts) = signal(admin::DeliveryCounts::default());
    let (query, set_query) = signal(Query::default());
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (notice, set_notice) = signal(None::<String>);

    // `offset` is the only difference between the first page and the next one; the filter
    // comes from the signal so a page read after a filter change cannot use the old one.
    let read = Callback::new(move |offset: i64| {
        let asked = query.get_untracked();
        set_busy.set(true);
        spawn_local(async move {
            let host = asked.host.trim().to_owned();
            let outcome = admin::deliveries(
                asked.state.as_deref(),
                if host.is_empty() { None } else { Some(host.as_str()) },
                offset,
                PAGE,
            )
            .await;
            alive.if_alive(|| {
                match outcome {
                    Ok(answer) => {
                        set_counts.set(answer.counts);
                        set_rows.update(|rows| {
                            if offset == 0 {
                                *rows = answer.deliveries.into_iter().map(delivery_of).collect();
                            } else {
                                rows.extend(answer.deliveries.into_iter().map(delivery_of));
                            }
                        });
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| read.run(0));

    let on_query = Callback::new(move |next: Query| {
        set_notice.set(None);
        set_query.set(next);
        read.run(0);
    });
    let on_more = Callback::new(move |()| {
        let offset = i64::from(u16::try_from(rows.get_untracked().len()).unwrap_or(PAGE));
        read.run(offset);
    });
    let on_retry = Callback::new(move |id: String| {
        set_busy.set(true);
        set_notice.set(None);
        spawn_local(async move {
            let outcome = admin::retry_delivery(&id).await;
            alive.if_alive(|| {
                match outcome {
                    Ok(()) => {
                        set_error.set(None);
                        set_notice.set(Some(
                            "Retry queued. The delivery will be attempted again now.".to_owned(),
                        ));
                        read.run(0);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });
    let on_retry_failed = Callback::new(move |()| {
        let host = query.get_untracked().host.trim().to_owned();
        set_busy.set(true);
        set_notice.set(None);
        spawn_local(async move {
            let outcome = admin::retry_failed(if host.is_empty() {
                None
            } else {
                Some(host.as_str())
            })
            .await;
            alive.if_alive(|| {
                match outcome {
                    Ok(requeued) => {
                        set_error.set(None);
                        // The server's number, not "done": it says whether anything was
                        // actually waiting, and zero is a real answer.
                        set_notice.set(Some(format!(
                            "Requeued {requeued} failed {}. They will be attempted again now.",
                            if requeued == 1 { "delivery" } else { "deliveries" }
                        )));
                        read.run(0);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    view! {
        <DeliveriesPage
            rows=rows
            counts=Signal::derive(move || counts_of(counts.get()))
            query=query
            busy=busy
            error=error
            notice=Some(Signal::from(notice))
            has_more=Signal::derive(move || rows.get().len() >= usize::from(PAGE))
            write=Some(admin::may_write())
            on_query=on_query
            on_more=on_more
            on_retry=on_retry
            on_retry_failed=on_retry_failed
        />
    }
}
