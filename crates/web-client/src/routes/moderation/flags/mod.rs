//! What the scanners found: the queue a moderator works through.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_client_core::model::{FlagState, ScanFlag};
use nodus_pages::pages::moderation::flags::FlagsPage;

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// One page of the scanner review queue.
#[component]
pub fn Scans() -> impl IntoView {
    let alive = Alive::new();
    let (flags, set_flags) = signal(Vec::<ScanFlag>::new());
    let (filter, set_filter) = signal(Some(FlagState::Open));
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (revision, set_revision) = signal(0_u32);

    Effect::new(move |_| {
        let state = filter.get();
        let _ = revision.get();
        spawn_local(async move {
            let outcome = admin::scan_flags(state).await;
            alive.if_alive(|| {
                set_busy.set(true);
                match outcome {
                    Ok(list) => {
                        set_flags.set(list);
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    let resolve = Callback::new(move |(id, state): (String, FlagState)| {
        spawn_local(async move {
            match admin::resolve_scan_flag(&id, state).await {
                Ok(_) => set_revision.update(|count| *count += 1),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    view! {
        <FlagsPage
            flags=flags
            filter=filter
            busy=busy
            error=error
            on_filter=Callback::new(move |next: Option<FlagState>| set_filter.set(next))
            on_resolve=resolve
        />
    }
}
