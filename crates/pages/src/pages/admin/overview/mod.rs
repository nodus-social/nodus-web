//! Where the numbers land: what the instance counted, what it did, and what is asking for a
//! decision.
//!
//! Every block here draws only what the server sent. A measure with no points is an empty
//! chart with its own sentence, not a flat zero line; an alert with no rows says so.

use leptos::prelude::*;
use nodus_client_core::model::{AdminAlert, AdminMeasure, AdminOverview, ScanningSummary};

use crate::components::charts::{Chart, Kind, Series, Spec};

/// The windows the performance panel offers, as the API spells them and as the buttons read.
pub const STATS_RANGES: [(&str, &str); 3] = [
    ("7d", "Last 7 days"),
    ("30d", "Last 30 days"),
    ("90d", "Last 90 days"),
];

/// How moderation performed in one window, as the page draws it.
///
/// The counts are kept as labelled pairs rather than as a struct of fields because every one
/// of them is painted the same way: a number under its own label, readable without a hover.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PerformanceStats {
    pub signups: i64,
    pub active_accounts: i64,
    pub posts: i64,
    /// Report counts, labelled in the order the instance lists them.
    pub reports: Vec<(&'static str, i64)>,
    /// Decisions taken, by the action they applied.
    pub decisions: Vec<(&'static str, i64)>,
    /// `None` when the window holds no decision, which is not the same as zero seconds.
    pub median_seconds: Option<f64>,
    /// The accounts that filed the most reports, already named for reading.
    pub top_reporters: Vec<(String, i64)>,
    /// The domains the window's reports named most.
    pub top_domains: Vec<(String, i64)>,
    /// What the scanners did.
    pub scanners: Vec<(&'static str, i64)>,
}

/// The measures the dashboard asks for, in the order the summary shows them.
pub const MEASURES: [(&str, &str); 5] = [
    ("new_users", "New accounts"),
    ("active_user_count", "Active accounts"),
    ("interactions", "Interactions"),
    ("opened_reports", "Reports opened"),
    ("resolved_reports", "Reports resolved"),
];

/// The dashboard.
#[component]
pub fn OverviewPage(
    /// The last read of the instance's counts.
    #[prop(into)]
    overview: Signal<Option<AdminOverview>>,
    /// The measure series, newest point last.
    #[prop(into)]
    measures: Signal<Vec<AdminMeasure>>,
    /// The alerts nobody has acknowledged.
    #[prop(into)]
    alerts: Signal<Vec<AdminAlert>>,
    /// What the scanners are doing, when the server reports it.
    #[prop(into)]
    scanning: Signal<Option<ScanningSummary>>,
    /// Whether a read is on its way.
    #[prop(into)]
    loading: Signal<bool>,
    /// The last read's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// The moderation performance document for the chosen window.
    #[prop(into)]
    stats: Signal<Option<PerformanceStats>>,
    /// Which window the panel is counting over: `7d`, `30d` or `90d`.
    #[prop(into)]
    stats_range: Signal<String>,
    /// Whether the performance read is on its way.
    #[prop(into)]
    stats_loading: Signal<bool>,
    /// The performance read's refusal, when it had one.
    #[prop(into)]
    stats_error: Signal<Option<String>>,
    /// Asks for another window. Absent where nothing can be sent.
    #[prop(optional)]
    on_range: Option<Callback<String>>,
    /// Acknowledges one alert. Absent where this token may not write.
    #[prop(optional)]
    on_acknowledge: Option<Callback<String>>,
    /// Hands the posts on the review queue to the scanners again. Absent where this token
    /// may not write, and absent on an instance that does not serve `scanners`.
    #[prop(optional)]
    on_scan_now: Option<Callback<()>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    let account_growth = Signal::derive(move || {
        chart_spec(
            &measures.get(),
            "new_users",
            Kind::Bar,
            "New accounts",
            "No accounts joined during these days.",
        )
    });
    let interactions = Signal::derive(move || {
        chart_spec(
            &measures.get(),
            "interactions",
            Kind::Line,
            "Interactions",
            "No interactions were recorded during these days.",
        )
    });
    let reports = Signal::derive(move || {
        let all = measures.get();
        let opened = series(&all, "opened_reports", "Opened");
        let resolved = series(&all, "resolved_reports", "Resolved");
        let labels = labels(&all, "opened_reports");
        if opened.is_none() && resolved.is_none() {
            return None;
        }
        Some(Spec::new(
            Kind::Bar,
            labels,
            opened.into_iter().chain(resolved).collect(),
        ))
    });
    let backlog = Signal::derive(move || {
        chart_spec(
            &measures.get(),
            "open_reports",
            Kind::Line,
            "Open reports",
            "No reports are open.",
        )
    });
    let summaries = Signal::derive(move || measures.get().iter().map(row_of).collect::<Vec<_>>());

    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || loading.get() && overview.get().is_none()>
                <p class="text-sm opacity-60">"Reading this instance…"</p>
            </Show>
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">{move || error.get().unwrap_or_default()}</p>
            </Show>
            <Show when=move || !write.get()>
                <crate::components::chrome::Note class="text-sm text-warning">
                    {crate::props::NO_WRITE}
                </crate::components::chrome::Note>
            </Show>

            <PerformancePanel
                stats=stats
                range=stats_range
                loading=stats_loading
                error=stats_error
                on_range=on_range
            />

            {move || {
                overview
                    .get()
                    .map(|overview| {
                        view! {
                            <div class="grid grid-cols-2 gap-2 sm:grid-cols-3">
                                {count("Accounts", overview.accounts)}
                                {count("Local accounts", overview.local_accounts)}
                                {count("Active accounts", overview.active_accounts)}
                                {count("Open reports", overview.open_reports)}
                                {count("Resolved reports", overview.resolved_reports)}
                                {count("Unassigned reports", overview.unassigned_reports)}
                                {count("Waiting approval", overview.pending_accounts)}
                                {count("Silences", overview.silences)}
                                {count("Suspensions", overview.suspensions)}
                                {count("Blocked domains", overview.domain_blocks)}
                                {count("Blocked email domains", overview.email_domain_blocks)}
                                {count("Staff", overview.staff)}
                            </div>
                        }
                    })
            }}

            <div class="rounded-box border border-base-300 bg-base-100 p-3">
                <div class="mb-2 flex items-baseline justify-between gap-2">
                    <h2 class="text-sm font-semibold">"Last seven days"</h2>
                    <p class="text-xs opacity-60">"Totals, with the seven days before them"</p>
                </div>
                <dl class="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-3">
                    {move || {
                        summaries
                            .get()
                            .into_iter()
                            .map(|(label, total, previous)| {
                                view! {
                                    <div class="min-w-0">
                                        <dt class="truncate text-xs opacity-60">{label}</dt>
                                        <dd class="text-lg font-semibold">{total}</dd>
                                        <dd class="text-xs opacity-60">{format!("was {previous}")}</dd>
                                    </div>
                                }
                            })
                            .collect_view()
                    }}
                </dl>
                {move || {
                    summaries
                        .get()
                        .is_empty()
                        .then(|| {
                            view! {
                                <p class="mt-2 text-sm opacity-70">
                                    "This server did not return any measures."
                                </p>
                            }
                        })
                }}
            </div>

            <div class="grid gap-3 lg:grid-cols-2">
                <div class="rounded-box border border-base-300 bg-base-100 p-3">
                    <Chart spec=reports empty="No reports were filed or resolved in these days."/>
                </div>
                <div class="rounded-box border border-base-300 bg-base-100 p-3">
                    <Chart spec=backlog empty="No open reports in these days."/>
                </div>
                <div class="rounded-box border border-base-300 bg-base-100 p-3">
                    <Chart spec=account_growth empty="No accounts joined in these days."/>
                </div>
                <div class="rounded-box border border-base-300 bg-base-100 p-3">
                    <Chart spec=interactions empty="No interactions in these days."/>
                </div>
            </div>
            <Show when=move || !loading.get() && measures.get().is_empty() && error.get().is_none()>
                <p class="text-sm opacity-70">
                    "This server does not publish per-day measures, so the graphs stay empty."
                </p>
            </Show>

            {move || {
                scanning
                    .get()
                    .map(|scanning| {
                        view! {
                            <div class="rounded-box border border-base-300 bg-base-100 p-3">
                                <div class="mb-2 flex items-center justify-between gap-2">
                                    <h2 class="text-sm font-semibold">"Scanners"</h2>
                                    <button
                                        class="btn btn-ghost btn-xs"
                                        disabled=move || on_scan_now.is_none() || !write.get()
                                        title=move || if !write.get() { crate::props::NO_WRITE } else { Default::default() }
                                        on:click=move |_| {
                                            if let Some(on_scan_now) = on_scan_now {
                                                on_scan_now.run(());
                                            }
                                        }
                                    >
                                        "Scan now"
                                    </button>
                                </div>
                                <dl class="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-5">
                                    {count("Scanners", scanning.scanners)}
                                    {count("Enabled", scanning.enabled)}
                                    {count("Rules", scanning.rules)}
                                    {count("Assets", scanning.assets)}
                                    {count("Queued scans", scanning.queued)}
                                </dl>
                            </div>
                        }
                    })
            }}

            <div class="rounded-box border border-base-300 bg-base-100 p-3">
                <div class="mb-2 flex items-baseline justify-between gap-2">
                    <h2 class="text-sm font-semibold">"Alerts"</h2>
                    <p class="text-xs opacity-60">"Unacknowledged failures this server recorded"</p>
                </div>
                <ul class="flex flex-col gap-2">
                    <For each=move || alerts.get() key=|alert| alert.id.clone() let:alert>
                        {
                            let id = alert.id.clone();
                            let title = format!(
                                "{} · {}",
                                alert.severity,
                                alert.kind,
                            );
                            let detail = format!(
                                "{} {}{}",
                                alert.method.clone().unwrap_or_default(),
                                alert.path.clone().unwrap_or_default(),
                                alert.status.map(|status| format!(" · {status}")).unwrap_or_default(),
                            );
                            view! {
                                <li class="flex items-start justify-between gap-3 rounded-box bg-base-200/60 px-3 py-2">
                                    <div class="min-w-0">
                                        <p class="truncate text-sm font-medium">{title}</p>
                                        <p class="truncate text-xs opacity-70">{detail}</p>
                                        <p class="truncate text-xs opacity-50">{alert.created_at.clone()}</p>
                                    </div>
                                    <button
                                        class="btn btn-ghost btn-xs shrink-0"
                                        disabled=move || on_acknowledge.is_none() || !write.get()
                                        title=move || if !write.get() { crate::props::NO_WRITE } else { Default::default() }
                                        on:click=move |_| {
                                            if let Some(on_acknowledge) = on_acknowledge {
                                                on_acknowledge.run(id.clone());
                                            }
                                        }
                                    >
                                        "Acknowledge"
                                    </button>
                                </li>
                            }
                        }
                    </For>
                </ul>
                <Show when=move || alerts.get().is_empty()>
                    <p class="text-sm opacity-70">"Nothing needs attention."</p>
                </Show>
            </div>
        </div>
    }
}

/// The moderation performance document, with its window switch.
///
/// Every number is painted with its label beside it, so the panel is readable without
/// hovering and the two bar charts are a second reading of the same counts rather than the
/// only place the counts exist. `NULL` median is shown as a sentence, because a window with
/// no decision has no median to round to zero.
#[component]
fn PerformancePanel(
    /// The document for the chosen window, or `None` while it is out.
    #[prop(into)]
    stats: Signal<Option<PerformanceStats>>,
    /// Which window is chosen.
    #[prop(into)]
    range: Signal<String>,
    /// Whether a read is on its way.
    #[prop(into)]
    loading: Signal<bool>,
    /// The last read's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Asks for another window. `None` where nothing can be sent.
    on_range: Option<Callback<String>>,
) -> impl IntoView {
    let reports_chart = Signal::derive(move || {
        stats.get().map(|stats| {
            Spec::new(
                Kind::Bar,
                stats
                    .reports
                    .iter()
                    .map(|(label, _)| (*label).to_owned())
                    .collect(),
                vec![Series::new(
                    "Reports",
                    stats
                        .reports
                        .iter()
                        .map(|(_, value)| *value as f64)
                        .collect(),
                )],
            )
        })
    });
    let decisions_chart = Signal::derive(move || {
        stats.get().map(|stats| {
            Spec::new(
                Kind::Bar,
                stats
                    .decisions
                    .iter()
                    .map(|(label, _)| (*label).to_owned())
                    .collect(),
                vec![Series::new(
                    "Decisions",
                    stats
                        .decisions
                        .iter()
                        .map(|(_, value)| *value as f64)
                        .collect(),
                )],
            )
        })
    });

    view! {
        <div class="rounded-box border border-base-300 bg-base-100 p-3">
            <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
                <h2 class="text-sm font-semibold">"Moderation performance"</h2>
                <div class="flex items-center gap-1" role="group" aria-label="Report range">
                    {STATS_RANGES
                        .iter()
                        .map(|(value, label)| {
                            let value = *value;
                            let active = move || range.get() == value;
                            view! {
                                <button
                                    class="btn btn-xs"
                                    class:btn-active=active
                                    aria-pressed=move || {
                                        if range.get() == value { "true" } else { "false" }
                                    }
                                    disabled=move || on_range.is_none()
                                    on:click=move |_| {
                                        if let Some(on_range) = on_range {
                                            on_range.run(value.to_owned());
                                        }
                                    }
                                >
                                    {*label}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
            <Show when=move || error.get().is_some()>
                <p class="mb-2 text-sm text-error" role="alert">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || loading.get() && stats.get().is_none()>
                <p class="text-sm opacity-60">"Reading this window…"</p>
            </Show>

            {move || {
                stats
                    .get()
                    .map(|stats| {
                        let median = match stats.median_seconds {
                            Some(seconds) => duration(seconds),
                            None => "No decision in this window".to_owned(),
                        };
                        let reporters = stats.top_reporters.clone();
                        let domains = stats.top_domains.clone();
                        view! {
                            <dl class="grid grid-cols-2 gap-2 sm:grid-cols-3">
                                {metric("Signups", stats.signups)}
                                {metric("Active accounts", stats.active_accounts)}
                                {metric("Posts", stats.posts)}
                            </dl>

                            <p class="mt-3 text-xs font-medium uppercase opacity-60">"Reports"</p>
                            <dl class="grid grid-cols-2 gap-2 sm:grid-cols-4">
                                {stats
                                    .reports
                                    .iter()
                                    .map(|(label, value)| metric(label, *value))
                                    .collect_view()}
                            </dl>

                            <p class="mt-3 text-xs font-medium uppercase opacity-60">"Decisions"</p>
                            <dl class="grid grid-cols-2 gap-2 sm:grid-cols-5">
                                {stats
                                    .decisions
                                    .iter()
                                    .map(|(label, value)| metric(label, *value))
                                    .collect_view()}
                            </dl>

                            <dl class="mt-3 grid grid-cols-1 gap-2 sm:grid-cols-2">
                                <div class="rounded-box bg-base-200/60 px-3 py-2">
                                    <dt class="text-xs opacity-60">"Median time to decision"</dt>
                                    <dd class="text-lg font-semibold">{median}</dd>
                                </div>
                                <div class="rounded-box bg-base-200/60 px-3 py-2">
                                    <dt class="text-xs opacity-60">"Scanner queue"</dt>
                                    <dd class="text-sm">
                                        {stats
                                            .scanners
                                            .iter()
                                            .map(|(label, value)| format!("{label} {value}"))
                                            .collect::<Vec<_>>()
                                            .join(" · ")}
                                    </dd>
                                </div>
                            </dl>

                            <div class="mt-3 grid gap-3 lg:grid-cols-2">
                                <div>
                                    <Chart
                                        spec=reports_chart
                                        empty="No reports in this window."
                                        caption="Reports".to_owned()
                                    />
                                </div>
                                <div>
                                    <Chart
                                        spec=decisions_chart
                                        empty="No decisions in this window."
                                        caption="Decisions".to_owned()
                                    />
                                </div>
                            </div>

                            <div class="mt-3 grid gap-3 lg:grid-cols-2">
                                <div>
                                    <p class="text-xs font-medium uppercase opacity-60">
                                        "Top reporters"
                                    </p>
                                    <ul class="mt-1 flex flex-col gap-1 text-sm">
                                        {reporters
                                            .into_iter()
                                            .map(|(label, count)| {
                                                view! {
                                                    <li class="flex items-baseline justify-between gap-2">
                                                        <span class="truncate">{label}</span>
                                                        <span class="font-semibold">{count}</span>
                                                    </li>
                                                }
                                            })
                                            .collect_view()}
                                    </ul>
                                    <Show when=move || stats.top_reporters.is_empty()>
                                        <p class="text-sm opacity-60">"No reports were filed."</p>
                                    </Show>
                                </div>
                                <div>
                                    <p class="text-xs font-medium uppercase opacity-60">
                                        "Top reported domains"
                                    </p>
                                    <ul class="mt-1 flex flex-col gap-1 text-sm">
                                        {domains
                                            .into_iter()
                                            .map(|(label, count)| {
                                                view! {
                                                    <li class="flex items-baseline justify-between gap-2">
                                                        <span class="truncate">{label}</span>
                                                        <span class="font-semibold">{count}</span>
                                                    </li>
                                                }
                                            })
                                            .collect_view()}
                                    </ul>
                                    <Show when=move || stats.top_domains.is_empty()>
                                        <p class="text-sm opacity-60">"No reported domain."</p>
                                    </Show>
                                </div>
                            </div>
                        }
                    })
            }}
        </div>
    }
}

/// A count under its own label, which is the whole point of the panel.
fn metric(label: &str, value: i64) -> impl IntoView {
    view! {
        <div class="rounded-box bg-base-200/60 px-3 py-2">
            <dt class="truncate text-xs opacity-60">{label.to_owned()}</dt>
            <dd class="text-lg font-semibold">{value}</dd>
        </div>
    }
}

/// A duration in the largest whole unit that keeps it readable.
fn duration(seconds: f64) -> String {
    let seconds = seconds.round().max(0.0) as i64;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60)
    } else {
        format!("{}d {}h", seconds / 86_400, (seconds % 86_400) / 3_600)
    }
}

/// A labelled count, or an honest dash when the server did not send the block.
fn count(label: &'static str, value: i64) -> impl IntoView {
    view! {
        <div class="rounded-box bg-base-200/60 px-3 py-2">
            <p class="truncate text-xs opacity-60">{label}</p>
            <p class="text-lg font-semibold">{value}</p>
        </div>
    }
}

/// The summary row of one measure: its label and the two totals.
fn row_of(measure: &AdminMeasure) -> (&'static str, String, String) {
    let label = MEASURES
        .iter()
        .find(|(key, _)| *key == measure.key)
        .map_or("—", |(_, label)| *label);
    (
        label,
        measure.total.clone().unwrap_or_else(|| "—".to_owned()),
        measure
            .previous_total
            .clone()
            .unwrap_or_else(|| "—".to_owned()),
    )
}

/// The days the measure has, oldest first.
fn labels(measures: &[AdminMeasure], key: &str) -> Vec<String> {
    measures
        .iter()
        .find(|measure| measure.key == key)
        .map(|measure| {
            measure
                .data
                .iter()
                .map(|point| point.date.get(5..).unwrap_or(&point.date).to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// One measure as a series, when the server sent it.
fn series(measures: &[AdminMeasure], key: &str, label: &str) -> Option<Series> {
    let measure = measures.iter().find(|measure| measure.key == key)?;
    Some(Series::new(
        label,
        measure
            .data
            .iter()
            .map(|point| point.value.parse::<f64>().unwrap_or(0.0))
            .collect(),
    ))
}

/// One measure as a whole chart, or `None` when the server did not send that series.
fn chart_spec(
    measures: &[AdminMeasure],
    key: &str,
    kind: Kind,
    label: &str,
    _empty: &str,
) -> Option<Spec> {
    let series = series(measures, key, label)?;
    let labels = labels(measures, key);
    let spec = Spec::new(kind, labels, vec![series]);
    (!spec.is_empty()).then_some(spec)
}
