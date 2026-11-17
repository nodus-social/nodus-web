//! What the instance's own processes are doing, and what they just said.

use leptos::prelude::*;
use nodus_client_core::model::{NodeLog, NodeStatus, Topology, TopologyNode};

use crate::components::panel::Panel;

/// How a node's state reads in the list.
#[must_use]
pub fn status_label(status: NodeStatus) -> &'static str {
    match status {
        NodeStatus::Healthy => "Answering",
        NodeStatus::Degraded => "Degraded",
        NodeStatus::Dead => "No answer",
    }
}

/// A badge class per state, so a dead node is not just a word.
#[must_use]
pub fn status_class(status: NodeStatus) -> &'static str {
    match status {
        NodeStatus::Healthy => "badge badge-success badge-sm",
        NodeStatus::Degraded => "badge badge-warning badge-sm",
        NodeStatus::Dead => "badge badge-error badge-sm",
    }
}

/// Seconds as the shortest form that is still exact to a human.
#[must_use]
pub fn uptime(seconds: u64) -> String {
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if days > 0 {
        return format!("{days}d {}h", hours % 24);
    }
    if hours > 0 {
        return format!("{hours}h {}m", minutes % 60);
    }
    if minutes > 0 {
        return format!("{minutes}m {}s", seconds % 60);
    }
    format!("{seconds}s")
}

/// Bytes in binary units, because the numbers here are a process's heap, not a disk.
#[must_use]
pub fn bytes(count: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let count = count as f64;
    if count >= GIB {
        return format!("{:.1} GiB", count / GIB);
    }
    if count >= MIB {
        return format!("{:.0} MiB", count / MIB);
    }
    if count >= KIB {
        return format!("{:.0} KiB", count / KIB);
    }
    format!("{} B", count as u64)
}

fn fact(label: &str, value: String) -> impl IntoView {
    view! {
        <div class="flex flex-col">
            <span class="text-xs uppercase tracking-wide opacity-60">{label.to_owned()}</span>
            <span class="text-sm">{value}</span>
        </div>
    }
}

fn node_card(
    node: TopologyNode,
    on_open: Option<Callback<String>>,
) -> impl IntoView {
    let name = node.name.clone();
    let open = on_open;
    let detail = match (&node.last_error, node.last_seen_at.as_deref()) {
        (Some(error), Some(seen)) => {
            Some(format!("{error} — it last answered {}", &seen[..seen.len().min(19)]))
        }
        (Some(error), None) => Some(format!("{error} — it has never answered")),
        (None, _) => None,
    };
    let has_detail = detail.is_some();
    view! {
        <li class="flex flex-col gap-2 rounded-box border border-base-300 p-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
                <div class="flex items-center gap-2">
                    <span class="font-medium">{node.name.clone()}</span>
                    <span class=status_class(node.status)>{status_label(node.status)}</span>
                    <Show when=move || node.version_mismatch>
                        <span class="badge badge-warning badge-sm">"Another build"</span>
                    </Show>
                </div>
                <button
                    class="btn btn-ghost btn-xs"
                    on:click=move |_| {
                        if let Some(open) = open {
                            open.run(name.clone());
                        }
                    }
                >
                    "Logs"
                </button>
            </div>
            <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
                {fact("Uptime", uptime(node.uptime_seconds))}
                {fact("CPU", format!("{:.1}%", node.cpu_usage_percent))}
                {fact("Memory", bytes(node.rss_bytes))}
                {fact("Answered in", format!("{} ms", node.latency_ms))}
                {fact("Build", node.version.clone().unwrap_or_else(|| "unknown".to_owned()))}
                {fact("Reached over", node.transport.clone())}
                {fact("Queue", node.queue_depth.map_or_else(|| "—".to_owned(), |depth| depth.to_string()))}
                {fact("Endpoint", node.endpoint.clone())}
            </div>
            <Show when=move || has_detail>
                <p class="text-xs opacity-70">{detail.clone().unwrap_or_default()}</p>
            </Show>
        </li>
    }
}

fn log_line(entry: NodeLog) -> impl IntoView {
    let when = entry.timestamp.get(11..19).unwrap_or(&entry.timestamp).to_owned();
    let class = match entry.level.as_str() {
        "error" => "font-mono text-xs text-error",
        "warn" => "font-mono text-xs text-warning",
        "debug" | "trace" => "font-mono text-xs opacity-60",
        _ => "font-mono text-xs",
    };
    view! {
        <li class="flex gap-2 py-0.5">
            <span class="shrink-0 opacity-60">{when}</span>
            <span class="shrink-0 w-12 uppercase opacity-70">{entry.level.clone()}</span>
            <span class=class>{entry.message.clone()}</span>
        </li>
    }
}

/// Every node the instance knows about, and the log of the one that was opened.
#[component]
pub fn TopologyPage(
    /// The last answer, if there is one.
    #[prop(into)]
    topology: Signal<Option<Topology>>,
    /// The opened node's entries, oldest first.
    #[prop(into)]
    logs: Signal<Vec<NodeLog>>,
    /// Which node's log is open.
    #[prop(into)]
    selected: Signal<Option<String>>,
    /// The lowest level the drawer shows.
    #[prop(into)]
    min_level: Signal<String>,
    /// Whether the drawer keeps reading.
    #[prop(into)]
    live: Signal<bool>,
    /// Whether a read is in flight.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Ask again.
    #[prop(optional)]
    on_refresh: Option<Callback<()>>,
    /// Open a node's log.
    #[prop(optional)]
    on_open: Option<Callback<String>>,
    /// Close the drawer.
    #[prop(optional)]
    on_close: Option<Callback<()>>,
    /// Change the level filter.
    #[prop(optional)]
    on_level: Option<Callback<String>>,
    /// Start or stop following the log.
    #[prop(optional)]
    on_follow: Option<Callback<bool>>,
) -> impl IntoView {
    let refresh = on_refresh;
    let level_button = move |level: &'static str, label: &'static str| {
        let raise = on_level;
        view! {
            <button
                class="btn btn-xs"
                class:btn-active=move || min_level.get() == level
                on:click=move |_| {
                    if let Some(raise) = raise {
                        raise.run(level.to_owned());
                    }
                }
            >
                {label}
            </button>
        }
    };

    view! {
        <Panel
            title="System health"
            description="Each process answers about itself: its own CPU and memory, and the last log lines it kept in memory. Nothing here is stored."
        >
            <div class="flex flex-wrap items-center justify-between gap-2">
                <p class="text-xs opacity-60">
                    {move || topology.get().map_or_else(
                        || "Nothing has been asked yet.".to_owned(),
                        |topology| format!("This build is {}", topology.version),
                    )}
                </p>
                <button
                    class="btn btn-sm"
                    disabled=move || busy.get()
                    on:click=move |_| {
                        if let Some(refresh) = refresh {
                            refresh.run(());
                        }
                    }
                >
                    {move || if busy.get() { "Asking…" } else { "Ask again" }}
                </button>
            </div>
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error">{move || error.get().unwrap_or_default()}</p>
            </Show>
            <ul class="flex flex-col gap-2">
                <For each=move || topology.get().map(|topology| topology.nodes).unwrap_or_default() key=|node| node.name.clone() let:node>
                    {node_card(node, on_open)}
                </For>
            </ul>
            <Show when=move || topology.get().is_some_and(|topology| topology.nodes.is_empty())>
                <p class="text-sm opacity-60">
                    "This instance runs in one process, so there is nothing else to ask."
                </p>
            </Show>
            <Show when=move || selected.get().is_some()>
                <div class="flex flex-col gap-2 rounded-box border border-base-300 p-3">
                    <div class="flex flex-wrap items-center justify-between gap-2">
                        <span class="font-medium">
                            {move || selected.get().map(|name| format!("{name} log")).unwrap_or_default()}
                        </span>
                        <div class="flex items-center gap-1">
                            <button
                                class="btn btn-xs"
                                class:btn-active=move || live.get()
                                title="Read the drawer again every couple of seconds while it is open"
                                on:click=move |_| {
                                    if let Some(follow) = on_follow {
                                        follow.run(!live.get_untracked());
                                    }
                                }
                            >
                                {move || if live.get() { "Following" } else { "Follow" }}
                            </button>
                            {level_button("info", "Info")}
                            {level_button("warn", "Warnings")}
                            {level_button("error", "Errors")}
                            <button
                                class="btn btn-ghost btn-xs"
                                on:click=move |_| {
                                    if let Some(close) = on_close {
                                        close.run(());
                                    }
                                }
                            >
                                "Close"
                            </button>
                        </div>
                    </div>
                    <ul class="max-h-80 overflow-y-auto">
                        <For each=move || logs.get() key=|entry| (entry.timestamp.clone(), entry.message.clone()) let:entry>
                            {log_line(entry)}
                        </For>
                    </ul>
                    <Show when=move || logs.get().is_empty()>
                        <p class="text-sm opacity-60">"Nothing at this level since that node started."</p>
                    </Show>
                </div>
            </Show>
        </Panel>
    }
}
