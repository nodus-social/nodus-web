//! Report forwarding: what this instance sent to a peer, read once and grouped by state.
//!
//! The panel and its words live in `nodus-pages`; this half asks the instance and
//! translates its answer.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::admin::federation::forwarded::{Forward, ForwardedPage};

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// How many forwards the panel reads. A page of them, not the whole history: what matters is
/// what a peer is still owed, what just failed, and which gave up — older rows stay in the
/// audit log.
const LIMIT: u16 = 50;

/// What the instance answered, in the page's own shape.
fn forward_row(row: admin::Forwarded) -> Forward {
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

/// The forwarded reports, grouped by what became of each delivery.
#[component]
pub fn Forwarded() -> impl IntoView {
    let alive = Alive::new();
    let (rows, set_rows) = signal(Vec::<Forward>::new());
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);

    Effect::new(move |_| {
        spawn_local(async move {
            let outcome = admin::forwarded(None, None, LIMIT).await;
            alive.if_alive(move || {
                match outcome {
                    Ok(list) => set_rows.set(list.into_iter().map(forward_row).collect()),
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    view! { <ForwardedPage rows=rows busy=busy error=error/> }
}
