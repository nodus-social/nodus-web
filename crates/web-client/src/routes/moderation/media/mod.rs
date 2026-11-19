//! Quarantined media: the list is a read under `admin:read`, the two outcomes are writes
//! under `admin:write`.
//!
//! A decision changes the row — releasing leaves it in the instance's answer, deleting
//! removes it — so the list is read again after every answer and the screen shows only what
//! is still waiting. The sentence above the list is built from what the instance answered,
//! because the verdict names what the decision did more exactly than the press did.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::moderation::media::{MediaDecision, MediaPage, Quarantined};

use crate::state::alive::Alive;
use crate::state::bridge::admin::{self, Cleared, MediaVerdict, QuarantinedMedia};

/// The quarantine work list.
#[component]
pub fn QuarantinedMediaList() -> impl IntoView {
    let alive = Alive::new();
    let (rows, set_rows) = signal(Vec::<QuarantinedMedia>::new());
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (notice, set_notice) = signal(None::<String>);
    let write = admin::may_write();

    let reload = Callback::new(move |()| {
        set_busy.set(true);
        spawn_local(async move {
            let outcome = admin::quarantined_media().await;
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

    let decide = Callback::new(move |decision: MediaDecision| {
        // Set before the request leaves, so every button is held from the first press.
        set_busy.set(true);
        set_error.set(None);
        set_notice.set(None);
        spawn_local(async move {
            let outcome =
                admin::decide_media(&decision.id, decision.release, &decision.reason).await;
            // The answer may have changed the row; reading the list again is what makes the
            // row leave the work list, and what makes a stale press say so.
            let page = admin::quarantined_media().await;
            alive.if_alive(|| {
                if let Ok(list) = &page {
                    set_rows.set(list.clone());
                }
                set_busy.set(false);
                match outcome {
                    Ok(Cleared::Row(verdict)) => {
                        set_notice.set(Some(cleared_line(&decision, &verdict)));
                    }
                    Ok(Cleared::AlreadyCleared) => set_notice.set(Some(
                        "That attachment was already cleared, or deleted, by another moderator. \
                         Nothing was changed."
                            .to_owned(),
                    )),
                    Err(message) => set_error.set(Some(message)),
                }
            });
        });
    });

    // Only what is still waiting is the work: the instance also answers with attachments it
    // released, and a released picture is neither a decision to make nor a thing to destroy.
    let media = Signal::derive(move || {
        rows.get()
            .into_iter()
            .filter(|row| row.state == "quarantined")
            .map(quarantined_of)
            .collect::<Vec<_>>()
    });

    view! {
        <MediaPage
            media=media
            busy=busy
            error=error
            notice=Some(Signal::from(notice))
            write=Some(write)
            on_decide=decide
        />
    }
}

fn quarantined_of(row: QuarantinedMedia) -> Quarantined {
    Quarantined {
        id: row.id,
        status_id: row.status_id,
        content_type: row.content_type,
        url: row.url,
        set: row.set,
        content_hash: row.content_hash,
        perceptual_hash: row.perceptual_hash,
        state: row.state,
        created_at: row.created_at.chars().take(10).collect(),
    }
}

/// What the decision did, in the sentence above the list.
fn cleared_line(decision: &MediaDecision, verdict: &MediaVerdict) -> String {
    let tail = match verdict.reports_resolved {
        0 => String::new(),
        1 => " One report was closed with it.".to_owned(),
        count => format!(" {count} reports were closed with it."),
    };
    if decision.release {
        format!(
            "Released. The picture is back on its post and the post is back in the timelines.{tail}"
        )
    } else {
        format!("Deleted. The attachment is destroyed and cannot be recovered.{tail}")
    }
}
