//! The dashboard: what this instance counted over the last week.
//!
//! The panel, the measure words, the graphs and the dash fallback are drawn in `nodus-view`;
//! what stays here is asking the instance and remembering the answer.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::{AdminAlert, AdminMeasure, AdminOverview, ScanningSummary};
use nodus_pages::pages::admin::overview::{MEASURES, OverviewPage};

use crate::state::alive::Alive;
use crate::state::bridge::{admin, revision};

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

    view! {
        <OverviewPage
            overview=overview
            measures=measures
            alerts=alerts
            scanning=scanning
            loading=loading
            error=error
            on_acknowledge=acknowledge
        />
    }
}
