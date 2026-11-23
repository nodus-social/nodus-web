//! The one federation lever Mastodon exposes to clients: allowed domains.
//!
//! Relays used to live here too. They have no API — `/methods/admin/relays/` is itself a 404
//! page on the docs site and `/api/v1/admin/relays` answers the instance's HTML 404 — so a
//! client cannot offer them, and linking to the instance's own web UI instead sends the reader
//! somewhere this app cannot reach.
//!
//! The typed domain is the caller's state; the list, the withdrawal and the form are markup,
//! so the form's submit, its one input and its one press stay in one place.
//!
//! The second panel is the other direction of the same lever: an inbound `Block` names an
//! account here rather than a domain there, and this instance records it so it can stop
//! delivering to the peer that asked. It is a Nodus extension, so the caller decides whether
//! the instance knows the endpoint at all.

use leptos::prelude::*;
use nodus_client_core::model::DomainAllow;

use crate::components::panel::Panel;

/// Domains let through a general block.
#[component]
pub fn DomainsPage(
    /// The allows this instance holds, as the caller last read them.
    #[prop(into)]
    items: Signal<Vec<DomainAllow>>,
    /// The domain being typed.
    #[prop(into)]
    domain: Signal<String>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal, when it had one.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records the typed domain. Absent where there is nowhere to keep it.
    #[prop(optional)]
    on_domain: Option<Callback<String>>,
    /// Allows the typed domain. Absent where nothing can be sent.
    #[prop(optional)]
    on_allow: Option<Callback<()>>,
    /// Withdraws one allow. Absent where nothing can be sent.
    #[prop(optional)]
    on_withdraw: Option<Callback<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    view! {
        <Show when=move || !write.get()>
            <crate::components::chrome::Note class="mb-2 text-sm text-warning">
                {crate::props::NO_WRITE}
            </crate::components::chrome::Note>
        </Show>

        <Panel
            title="Allowed domains"
            description="An allow lets one domain through even when this instance blocks federation generally — for a domain you trust without wanting to federate with everyone."
        >
            <For each=move || items.get() key=|entry| entry.id.clone() let:entry>
                {
                    let target = StoredValue::new(entry.id.clone());
                    let name = entry.domain.clone();
                    view! {
                        <div class="flex items-center gap-2 rounded-box bg-base-200 p-2 text-sm">
                            <span class="truncate">{name}</span>
                            <button
                                class="btn btn-ghost btn-xs ml-auto"
                                disabled=move || on_withdraw.is_none() || !write.get()
                                title=move || if write.get() { Default::default() } else { crate::props::NO_WRITE }
                                on:click=move |_| {
                                    if let Some(on_withdraw) = on_withdraw {
                                        on_withdraw.run(target.get_value());
                                    }
                                }
                            >
                                "Withdraw"
                            </button>
                        </div>
                    }
                }
            </For>
            <Show when=move || items.get().is_empty()>
                <p class="text-sm opacity-60">"No domain is allowed through a block."</p>
            </Show>
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error">{move || error.get().unwrap_or_default()}</p>
            </Show>
            <form
                class="flex flex-wrap items-center gap-2"
                on:submit=move |event| {
                    stop_submit(&event);
                    if let Some(on_allow) = on_allow {
                        on_allow.run(());
                    }
                }
            >
                <input
                    class="input input-bordered input-sm flex-1"
                    placeholder="example.social"
                    prop:value=move || domain.get()
                    disabled=on_domain.is_none()
                    on:input=move |event| {
                        if let Some(on_domain) = on_domain {
                            on_domain.run(event_target_value(&event));
                        }
                    }
                />
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=move || busy.get() || on_allow.is_none() || !write.get()
                    title=move || if write.get() { Default::default() } else { crate::props::NO_WRITE }
                >
                    "Allow"
                </button>
            </form>
        </Panel>
    }
}

/// An inbound block, as the console reads it back: who was blocked, by whom, and when.
///
/// The wire shape carries the same three fields; this is the one the page draws, so the page
/// crate keeps its own name for it and reads no JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBlockRow {
    /// The account the peer blocked, as a URI on that peer.
    pub actor: String,
    /// The peer that sent the block, as its bare host.
    pub host: String,
    /// When the block arrived, as the instance wrote it down.
    pub created_at: String,
}

impl RemoteBlockRow {
    /// When the block arrived, with the date and the time readable side by side.
    #[must_use]
    pub fn when(&self) -> String {
        self.created_at
            .get(..19)
            .unwrap_or(&self.created_at)
            .replace('T', " ")
    }
}

/// Peers that have told this instance they block one of its accounts.
///
/// The one destructive verb is a clear, and it asks first: the row it would remove is the only
/// record that the peer asked to be skipped, and re-learning it takes the peer sending another
/// `Block`. The question sits in the row, so the table keeps its place while it is open.
#[component]
pub fn RemoteBlocksPage(
    /// The blocks the instance holds, as the caller last read them.
    #[prop(into)]
    blocks: Signal<Vec<RemoteBlockRow>>,
    /// The host whose block is waiting for its second press.
    #[prop(into)]
    confirming: Signal<Option<String>>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or clear's refusal, when it had one.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Opens the question for one host. Absent where nothing can be sent.
    #[prop(optional)]
    on_ask: Option<Callback<String>>,
    /// Puts the question away without sending anything.
    #[prop(optional)]
    on_cancel: Option<Callback<()>>,
    /// Clears one host's block. Absent where nothing can be sent.
    #[prop(optional)]
    on_clear: Option<Callback<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    view! {
        <Show when=move || !write.get()>
            <crate::components::chrome::Note class="mb-2 text-sm text-warning">
                {crate::props::NO_WRITE}
            </crate::components::chrome::Note>
        </Show>

        <Panel
            title="Remote blocks"
            description="A peer that blocks one of this instance's accounts is asking not to receive that account. Delivery to it stops while the block is held, and clearing it here is what lets delivery resume."
        >
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || !blocks.get().is_empty()>
                <div class="overflow-x-auto">
                    <table class="table table-sm">
                        <thead>
                            <tr>
                                <th>"Peer"</th>
                                <th>"Blocked actor"</th>
                                <th>"When"</th>
                                <th class="text-right">"Clear"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || blocks.get()
                                key=|block| (block.host.clone(), block.actor.clone())
                                let:block
                            >
                                {
                                    let when = block.when();
                                    let host = StoredValue::new(block.host.clone());
                                    view! {
                                        <tr>
                                            <td class="whitespace-nowrap">{block.host}</td>
                                            <td class="max-w-0 truncate">{block.actor}</td>
                                            <td class="whitespace-nowrap text-xs opacity-60">{when}</td>
                                            <td class="text-right">
                                                <Show when=move || {
                                                    confirming.get().as_deref() == Some(host.get_value().as_str())
                                                }>
                                                    <div class="flex items-center justify-end gap-1">
                                                        <button
                                                            class="btn btn-ghost btn-xs"
                                                            on:click=move |_| {
                                                                if let Some(on_cancel) = on_cancel {
                                                                    on_cancel.run(());
                                                                }
                                                            }
                                                        >
                                                            "Cancel"
                                                        </button>
                                                        <button
                                                            class="btn btn-error btn-xs"
                                                            disabled=move || busy.get()
                                                            on:click=move |_| {
                                                                if let Some(on_clear) = on_clear {
                                                                    on_clear.run(host.get_value());
                                                                }
                                                            }
                                                        >
                                                            "Clear block"
                                                        </button>
                                                    </div>
                                                </Show>
                                                <Show when=move || {
                                                    confirming.get().as_deref() != Some(host.get_value().as_str())
                                                }>
                                                    <button
                                                        class="btn btn-ghost btn-xs"
                                                        disabled=move || on_ask.is_none() || !write.get()
                                                        title=move || if write.get() { Default::default() } else { crate::props::NO_WRITE }
                                                        on:click=move |_| {
                                                            if let Some(on_ask) = on_ask {
                                                                on_ask.run(host.get_value());
                                                            }
                                                        }
                                                    >
                                                        "Clear"
                                                    </button>
                                                </Show>
                                            </td>
                                        </tr>
                                    }
                                }
                            </For>
                        </tbody>
                    </table>
                </div>
            </Show>
            <Show when=move || blocks.get().is_empty()>
                <p class="text-sm opacity-60">"No peer has blocked us."</p>
            </Show>
        </Panel>
    }
}

/// One line of a pasted or uploaded block list that could not become a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockImportProblem {
    /// The line number, counting the header as line one.
    pub line: usize,
    /// Why the line could not be read.
    pub reason: String,
}

/// What checking or applying a block list counted.
///
/// The same numbers come back from the dry run and the apply, so the report a moderator
/// approves is the report they see afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockImportReport {
    /// Every line the instance looked at, valid or not.
    pub parsed: usize,
    /// Blocks the list adds.
    pub new: usize,
    /// Blocks whose severity or flags the list changes.
    pub updated: usize,
    /// Blocks the list repeats exactly.
    pub unchanged: usize,
    /// The lines that could not be read, each with its number.
    pub invalid: Vec<BlockImportProblem>,
    /// Whether this run wrote the list rather than only counting it.
    pub applied: bool,
}

/// A block list carried in, checked, and only then written.
///
/// The apply is the one irreversible press on this screen: it can overwrite the severity of
/// blocks the list names, so the check runs first and the counts it returns are what the
/// moderator is agreeing to. A list that changed after the check clears the report, so the
/// numbers on screen always describe the text beside them.
#[component]
pub fn BlockListPage(
    /// The list as typed or pasted.
    #[prop(into)]
    text: Signal<String>,
    /// The last check or apply, when one has answered.
    #[prop(into)]
    report: Signal<Option<BlockImportReport>>,
    /// Whether a check is on its way.
    #[prop(into)]
    checking: Signal<bool>,
    /// Whether an apply is on its way.
    #[prop(into)]
    applying: Signal<bool>,
    /// The last read's, export's, check's or apply's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records the typed list. Absent where there is nowhere to keep it.
    #[prop(optional)]
    on_text: Option<Callback<String>>,
    /// Exports the held blocks in `csv` or `json`. Absent where nothing can be sent.
    #[prop(optional)]
    on_export: Option<Callback<String>>,
    /// Counts the list without writing it. Absent where nothing can be sent.
    #[prop(optional)]
    on_check: Option<Callback<()>>,
    /// Writes the checked list. Absent where nothing can be sent.
    #[prop(optional)]
    on_apply: Option<Callback<()>>,
    /// The file input the caller owns, so the bytes are read where the browser is.
    children: ChildrenFn,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    view! {
        <Show when=move || !write.get()>
            <crate::components::chrome::Note class="mb-2 text-sm text-warning">
                {crate::props::NO_WRITE}
            </crate::components::chrome::Note>
        </Show>

        <Panel
            title="Block list"
            description="Carry this instance's blocks out as a file, or bring a list in. An import is checked before anything is written, so the counts come before the commit."
        >
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>

            <div class="flex flex-wrap items-center gap-2">
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=move || on_export.is_none()
                    on:click=move |_| {
                        if let Some(on_export) = on_export {
                            on_export.run("csv".to_owned());
                        }
                    }
                >
                    "Export CSV"
                </button>
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=move || on_export.is_none()
                    on:click=move |_| {
                        if let Some(on_export) = on_export {
                            on_export.run("json".to_owned());
                        }
                    }
                >
                    "Export JSON"
                </button>
            </div>

            <textarea
                class="textarea textarea-bordered min-h-32 font-mono text-xs"
                placeholder="domain,severity,reject_media,reject_reports,reject_follows,comment"
                aria-label="Block list"
                prop:value=move || text.get()
                disabled=on_text.is_none()
                on:input=move |event| {
                    if let Some(on_text) = on_text {
                        on_text.run(event_target_value(&event));
                    }
                }
            />
            <div class="flex flex-wrap items-center gap-2">
                {children()}
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=move || checking.get() || on_check.is_none() || text.get().trim().is_empty()
                    on:click=move |_| {
                        if let Some(on_check) = on_check {
                            on_check.run(());
                        }
                    }
                >
                    {move || if checking.get() { "Checking…" } else { "Check list" }}
                </button>
            </div>

            {move || {
                report
                    .get()
                    .map(|report| {
                        let counts = [
                            ("Parsed", report.parsed),
                            ("New", report.new),
                            ("Updated", report.updated),
                            ("Unchanged", report.unchanged),
                        ];
                        let invalid = report.invalid.clone();
                        let invalid_empty = invalid.is_empty();
                        let applied = report.applied;
                        view! {
                            <div class="rounded-box border border-base-300 bg-base-200/60 p-3">
                                <p class="mb-2 text-sm font-medium">
                                    {if applied {
                                        "The list was applied."
                                    } else {
                                        "Nothing has been written yet."
                                    }}
                                </p>
                                <dl class="grid grid-cols-2 gap-2 sm:grid-cols-4">
                                    {counts
                                        .into_iter()
                                        .map(|(label, value)| {
                                            view! {
                                                <div>
                                                    <dt class="text-xs opacity-60">{label}</dt>
                                                    <dd class="text-lg font-semibold">{value}</dd>
                                                </div>
                                            }
                                        })
                                        .collect_view()}
                                </dl>
                                <Show when=move || !invalid_empty>
                                    <p class="mt-3 text-sm font-medium">
                                        "Lines that could not be read"
                                    </p>
                                    <ul class="mt-1 flex flex-col gap-1 text-sm">
                                        {invalid
                                            .iter()
                                            .map(|problem| {
                                                let line = problem.line;
                                                let reason = problem.reason.clone();
                                                view! {
                                                    <li class="text-error">
                                                        {format!("Line {line}: {reason}")}
                                                    </li>
                                                }
                                            })
                                            .collect_view()}
                                    </ul>
                                </Show>
                                <Show when=move || !applied>
                                    <button
                                        class="btn btn-primary btn-sm mt-3"
                                        disabled=move || applying.get() || on_apply.is_none() || !write.get()
                                        title=move || if write.get() { Default::default() } else { crate::props::NO_WRITE }
                                        on:click=move |_| {
                                            if let Some(on_apply) = on_apply {
                                                on_apply.run(());
                                            }
                                        }
                                    >
                                        {move || if applying.get() { "Applying…" } else { "Apply" }}
                                    </button>
                                </Show>
                            </div>
                        }
                    })
            }}
        </Panel>
    }
}

/// A block list this instance follows, as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSubscriptionRow {
    pub id: String,
    pub url: String,
    pub format: String,
    /// When the list was last read successfully, as the instance wrote it down.
    pub last_synced_at: Option<String>,
    /// Why the last attempt failed, when it did.
    pub last_error: Option<String>,
}

impl BlockSubscriptionRow {
    /// The one line that says how the last attempt went.
    #[must_use]
    pub fn last_result(&self) -> String {
        if let Some(error) = &self.last_error {
            return error.clone();
        }
        match &self.last_synced_at {
            Some(at) => format!("Synced {}", at.get(..19).unwrap_or(at).replace('T', " ")),
            None => "Not synced yet".to_owned(),
        }
    }
}

/// The published lists this instance follows.
///
/// A failed subscription stays on this list instead of disappearing: the row is the record
/// that the instance still expects the list, and its `last_error` is the only place the
/// reason survives. Deleting the row is how following stops; a failure alone is not.
#[component]
pub fn BlockSubscriptionsPage(
    /// The subscriptions as the caller last read them.
    #[prop(into)]
    items: Signal<Vec<BlockSubscriptionRow>>,
    /// The address being typed.
    #[prop(into)]
    url: Signal<String>,
    /// The format the address is published in, `csv` or `json`.
    #[prop(into)]
    format: Signal<String>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The subscription whose sync is on its way, when one is.
    #[prop(into)]
    syncing: Signal<Option<String>>,
    /// The last read's, add's, remove's or sync's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records the typed address. Absent where there is nowhere to keep it.
    #[prop(optional)]
    on_url: Option<Callback<String>>,
    /// Records the chosen format. Absent where there is nowhere to keep it.
    #[prop(optional)]
    on_format: Option<Callback<String>>,
    /// Follows the typed list. Absent where nothing can be sent.
    #[prop(optional)]
    on_add: Option<Callback<()>>,
    /// Stops following one list. Absent where nothing can be sent.
    #[prop(optional)]
    on_remove: Option<Callback<String>>,
    /// Pulls one list now. Absent where nothing can be sent.
    #[prop(optional)]
    on_sync: Option<Callback<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    view! {
        <Show when=move || !write.get()>
            <crate::components::chrome::Note class="mb-2 text-sm text-warning">
                {crate::props::NO_WRITE}
            </crate::components::chrome::Note>
        </Show>

        <Panel
            title="Subscribed lists"
            description="A published list this instance follows is re-read every few hours. A list that failed stays here with its error: the row is the record that this instance still follows it, and removing it is how following stops."
        >
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>
            <Show when=move || !items.get().is_empty()>
                <div class="overflow-x-auto">
                    <table class="table table-sm">
                        <thead>
                            <tr>
                                <th>"List"</th>
                                <th>"Format"</th>
                                <th>"Last result"</th>
                                <th class="text-right">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || items.get()
                                key=|item| item.id.clone()
                                let:item
                            >
                                {
                                    let id = StoredValue::new(item.id.clone());
                                    let result = item.last_result();
                                    let failed = item.last_error.is_some();
                                    let format = item.format.clone();
                                    view! {
                                        <tr>
                                            <td class="max-w-0 truncate">{item.url}</td>
                                            <td class="whitespace-nowrap text-xs uppercase opacity-60">
                                                {format}
                                            </td>
                                            <td class=move || {
                                                if failed {
                                                    "max-w-0 truncate text-xs text-error"
                                                } else {
                                                    "max-w-0 truncate text-xs opacity-60"
                                                }
                                            }>{result}</td>
                                            <td class="whitespace-nowrap text-right">
                                                <button
                                                    class="btn btn-ghost btn-xs"
                                                    disabled=move || {
                                                        busy.get()
                                                            || syncing.get().as_deref() == Some(id.get_value().as_str())
                                                            || on_sync.is_none()
                                                            || !write.get()
                                                    }
                                                    title=move || if write.get() { Default::default() } else { crate::props::NO_WRITE }
                                                    on:click=move |_| {
                                                        if let Some(on_sync) = on_sync {
                                                            on_sync.run(id.get_value());
                                                        }
                                                    }
                                                >
                                                    {move || {
                                                        if syncing.get().as_deref() == Some(id.get_value().as_str()) {
                                                            "Syncing…"
                                                        } else {
                                                            "Sync now"
                                                        }
                                                    }}
                                                </button>
                                                <button
                                                    class="btn btn-ghost btn-xs"
                                                    disabled=move || busy.get() || on_remove.is_none() || !write.get()
                                                    title=move || if write.get() { Default::default() } else { crate::props::NO_WRITE }
                                                    on:click=move |_| {
                                                        if let Some(on_remove) = on_remove {
                                                            on_remove.run(id.get_value());
                                                        }
                                                    }
                                                >
                                                    "Remove"
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }
                            </For>
                        </tbody>
                    </table>
                </div>
            </Show>
            <Show when=move || items.get().is_empty()>
                <p class="text-sm opacity-60">"This instance follows no published list."</p>
            </Show>
            <form
                class="flex flex-wrap items-center gap-2"
                on:submit=move |event| {
                    stop_submit(&event);
                    if let Some(on_add) = on_add {
                        on_add.run(());
                    }
                }
            >
                <input
                    class="input input-bordered input-sm flex-1"
                    placeholder="https://example.social/blocklist.csv"
                    aria-label="List address"
                    prop:value=move || url.get()
                    disabled=on_url.is_none()
                    on:input=move |event| {
                        if let Some(on_url) = on_url {
                            on_url.run(event_target_value(&event));
                        }
                    }
                />
                <select
                    class="select select-bordered select-sm"
                    aria-label="List format"
                    prop:value=move || format.get()
                    disabled=on_format.is_none()
                    on:change=move |event| {
                        if let Some(on_format) = on_format {
                            on_format.run(event_target_value(&event));
                        }
                    }
                >
                    <option value="csv">"CSV"</option>
                    <option value="json">"JSON"</option>
                </select>
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=move || busy.get() || on_add.is_none() || !write.get()
                    title=move || if write.get() { Default::default() } else { crate::props::NO_WRITE }
                >
                    "Follow list"
                </button>
            </form>
        </Panel>
    }
}

/// Keeps a submit from reloading the page, on the target that has a page to reload.
///
/// The server render never runs a listener, and its event type has no `prevent_default`; the
/// non-wasm build keeps a use of `event` so the argument is not reported as unused.
fn stop_submit(event: &leptos::ev::SubmitEvent) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = event;
    #[cfg(target_arch = "wasm32")]
    event.prevent_default();
}
