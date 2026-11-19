//! Domain pauses: a temporary stop that is not a block, and the two verbs that move it.
//!
//! The panel and its words live in `nodus-pages`; this half reads the instance's answer into
//! the page's own shape and sends what a moderator writes.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::admin::federation::pauses::{
    DomainPauseDraft, DomainPausesPage, PausedDomain,
};

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// One row of the instance's answer, in the page's own shape.
fn row_of(row: admin::PausedDomain) -> PausedDomain {
    PausedDomain {
        domain: row.domain,
        reason: row.reason,
        expires_at: row.expires_at,
        // The instance answers whether the pause is still biting; the row does not work it
        // out from the timestamp, so the two can never disagree.
        expired: row.expired,
        created_at: row.created_at,
        created_by_handle: row.created_by_handle,
    }
}

/// Paused domains, the form that pauses one, and the lift with its confirmation.
#[component]
pub fn DomainPauses() -> impl IntoView {
    let alive = Alive::new();
    let (rows, set_rows) = signal(Vec::<PausedDomain>::new());
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (notice, set_notice) = signal(None::<String>);
    let (confirming, set_confirming) = signal(None::<String>);

    let read = Callback::new(move |()| {
        set_busy.set(true);
        spawn_local(async move {
            let outcome = admin::domain_pauses().await;
            alive.if_alive(|| {
                match outcome {
                    Ok(list) => {
                        set_rows.set(list.into_iter().map(row_of).collect());
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| read.run(()));

    let on_pause = Callback::new(move |draft: DomainPauseDraft| {
        set_busy.set(true);
        set_error.set(None);
        set_notice.set(None);
        spawn_local(async move {
            let outcome =
                admin::pause_domain(&draft.domain, &draft.reason, &draft.expires_at).await;
            alive.if_alive(|| {
                match outcome {
                    Ok(row) => {
                        // The server's row, not the draft: its `expired` and its clock are
                        // the ones the list will read a moment from now.
                        let standing = row_of(row).standing();
                        set_error.set(None);
                        set_notice.set(Some(format!(
                            "{} is paused. {standing}",
                            draft.domain.trim()
                        )));
                        read.run(());
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    let on_ask_lift = Callback::new(move |domain: String| {
        set_notice.set(None);
        set_confirming.set(Some(domain));
    });
    let on_cancel_lift = Callback::new(move |()| set_confirming.set(None));
    let on_lift = Callback::new(move |domain: String| {
        set_busy.set(true);
        set_error.set(None);
        set_notice.set(None);
        spawn_local(async move {
            let outcome = admin::lift_domain_pause(&domain).await;
            alive.if_alive(|| {
                match outcome {
                    Ok(()) => {
                        set_error.set(None);
                        set_notice.set(Some(format!(
                            "The pause on {domain} is lifted. Delivery to it and its timeline visibility resume at once."
                        )));
                        read.run(());
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_confirming.set(None);
                set_busy.set(false);
            });
        });
    });

    view! {
        <DomainPausesPage
            rows=rows
            busy=busy
            error=error
            notice=Some(Signal::from(notice))
            confirming=Some(Signal::from(confirming))
            on_pause=on_pause
            on_ask_lift=on_ask_lift
            on_cancel_lift=on_cancel_lift
            on_lift=on_lift
            write=Some(admin::may_write())
        />
    }
}
