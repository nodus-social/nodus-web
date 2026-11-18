//! The dashboard: what this instance counted over the last week.
//!
//! The panel, the measure words, the graphs and the dash fallback are drawn in `nodus-view`;
//! what stays here is asking the instance and remembering the answer.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::{AdminAlert, AdminMeasure, AdminOverview, ScanningSummary};
use nodus_pages::pages::admin::overview::{MEASURES, OverviewPage};

use crate::state::alive::Alive;
use crate::state::bridge::{admin, feature, revision};

/// How many days the dashboard counts over.
const DAYS: f64 = 7.0;

/// Dashboard counts over the last seven days.
#[component]
pub fn Overview() -> impl IntoView {
    let revision = revision();
    let (overview, set_overview) = signal(None::<AdminOverview>);
    let (measures, set_measures) = signal(Vec::<AdminMeasure>::new());
    let (alerts, set_alerts) = signal(Vec::<AdminAlert>::new());
    let (scanning, set_scanning) = signal(None::<ScanningSummary>);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);
    // The scanner panel is one capability of the instance, not part of the dashboard: an
    // instance whose features do not name `scanners` has no counts to show.
    let scanners = feature("scanners");

    Effect::new(move |_| {
        revision.get();
        let alive = Alive::new();
        set_loading.set(true);
        spawn_local(async move {
            let keys: Vec<&str> = MEASURES.iter().map(|(key, _)| *key).collect();
            let meta = admin::meta().await;
            let counts = admin::overview().await;
            let alerts_read = admin::alerts(true).await;
            let measures_read = admin::measures(&keys, DAYS).await;

            let mut problems = Vec::new();
            match meta {
                Ok(meta) => alive.if_alive(|| set_scanning.set(Some(meta.scanning))),
                Err(problem) => problems.push(problem),
            }
            match counts {
                Ok(counts) => alive.if_alive(|| set_overview.set(Some(counts))),
                Err(problem) => problems.push(problem),
            }
            match alerts_read {
                Ok(list) => alive.if_alive(|| set_alerts.set(list)),
                Err(problem) => problems.push(problem),
            }
            match measures_read {
                Ok(list) => alive.if_alive(|| set_measures.set(list)),
                Err(problem) => problems.push(problem),
            }
            alive.if_alive(|| {
                set_loading.set(false);
                set_error.set((!problems.is_empty()).then(|| problems.join(" ")));
            });
        });
    });

    let acknowledge = Callback::new(move |id: String| {
        let alive = Alive::new();
        spawn_local(async move {
            if let Err(problem) = admin::acknowledge_alert(&id).await {
                alive.if_alive(|| set_error.set(Some(problem)));
                return;
            }
            match admin::alerts(true).await {
                Ok(list) => alive.if_alive(|| set_alerts.set(list)),
                Err(problem) => alive.if_alive(|| set_error.set(Some(problem))),
            }
        });
    });

    let scan_now = Callback::new(move |()| {
        let alive = Alive::new();
        spawn_local(async move {
            if let Err(problem) = admin::scan_open_queue().await {
                alive.if_alive(|| set_error.set(Some(problem)));
                return;
            }
            // The queue's size is one of the counts in the scanner panel, so read the
            // counters again rather than guessing what the re-queue did to them. The
            // dashboard's own read is cached, so this one asks past the cache.
            let refreshed = admin::meta_refreshed().await;
            alive.if_alive(|| match refreshed {
                Ok(meta) => {
                    set_error.set(None);
                    set_scanning.set(Some(meta.scanning));
                }
                Err(problem) => set_error.set(Some(problem)),
            });
        });
    });

    view! {
        <OverviewPage
            overview=overview
            measures=measures
            alerts=alerts
            scanning=Signal::derive(move || if scanners.get() { scanning.get() } else { None })
            loading=loading
            error=error
            on_acknowledge=acknowledge
            on_scan_now=scan_now
            write=Some(crate::state::bridge::admin::may_write())
        />
    }
}
