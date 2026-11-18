//! The audit log: read as the filter narrows, a page at a time.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::{AuditEntry, AuditFilter};
use nodus_pages::pages::admin::audit::AuditPage;
use nodus_ui::Note;

use crate::state::alive::Alive;
use crate::state::bridge::{admin, feature, revision};

/// How many rows one page holds: the number the request asks for, so a full page is the
/// only evidence of another one.
const PAGE: u16 = 20;

#[component]
pub fn Audit() -> impl IntoView {
    let revision = revision();
    let (entries, set_entries) = signal(Vec::<AuditEntry>::new());
    let (query, set_query) = signal(AuditFilter {
        limit: Some(PAGE),
        ..AuditFilter::default()
    });
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);

    // The filter is the read: the instance decides which rows match it.
    Effect::new(move |_| {
        revision.get();
        let filter = query.get();
        set_busy.set(true);
        let alive = Alive::new();
        spawn_local(async move {
            match admin::audit(&filter).await {
                Ok(list) => alive.if_alive(|| {
                    set_entries.set(list);
                    set_error.set(None);
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
            alive.if_alive(|| set_busy.set(false));
        });
    });

    // A second page is appended: the rows above it are still the same rows.
    let more = Callback::new(move |()| {
        let alive = Alive::new();
        spawn_local(async move {
            let mut filter = query.get_untracked();
            filter.offset = Some(u32::try_from(entries.get_untracked().len()).unwrap_or(0));
            match admin::audit(&filter).await {
                Ok(list) => alive.if_alive(|| set_entries.update(|held| held.extend(list))),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let supported = feature("admin");
    view! {
        {move || if supported.get() {
            view! {
                <AuditPage
                    entries=entries
                    query=query
                    busy=busy
                    error=error
                    has_more=Signal::derive(move || entries.get().len() >= usize::from(PAGE))
                    on_query=Callback::new(move |next: AuditFilter| set_query.set(next))
                    on_more=more
                />
            }
                .into_any()
        } else {
            view! {
                <Note class="text-sm opacity-70">
                    "This instance does not serve the audit log: its /api/v1/meta answer does not name the admin feature."
                </Note>
            }
                .into_any()
        }}
    }
}
