//! The administrative action log: the filters that narrow it, the rows, and the next page.

use leptos::prelude::*;
use nodus_client_core::model::{AuditEntry, AuditFilter};

use crate::components::panel::Panel;

/// One page of the log, newest first, with the filter that produced it.
#[component]
pub fn AuditPage(
    /// The rows, as the caller last read them.
    #[prop(into)]
    entries: Signal<Vec<AuditEntry>>,
    /// The filter in force.
    #[prop(into)]
    query: Signal<AuditFilter>,
    /// Whether a read is already on its way: an empty list while it is is not "nothing happened".
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Whether another page may exist behind this one.
    #[prop(into)]
    has_more: Signal<bool>,
    /// Re-reads the log under a new filter.
    #[prop(optional)]
    on_query: Option<Callback<AuditFilter>>,
    /// Reads the next page.
    #[prop(optional)]
    on_more: Option<Callback<()>>,
    /// Saves every row the filter matches as one file, rather than another page.
    #[prop(optional)]
    on_export: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <Panel
            title="Audit log"
            description="Administrative and moderation actions recorded by this server, narrowed by who did what to which kind of thing."
        >
            <div class="mb-3 grid gap-2 sm:grid-cols-3">
                <input
                    class="input input-bordered input-sm"
                    placeholder="Action, e.g. resolve_report"
                    prop:value=move || query.get().action.unwrap_or_default()
                    on:input=move |event| {
                        if let Some(on_query) = on_query {
                            let mut next = query.get_untracked();
                            let value = event_target_value(&event);
                            next.action = Some(value.clone()).filter(|text| !text.is_empty());
                            // A narrower filter is a new first page: the offset counted rows
                            // the filter has just removed.
                            next.offset = None;
                            on_query.run(next);
                        }
                    }
                />
                <input
                    class="input input-bordered input-sm"
                    placeholder="Target type, e.g. report"
                    prop:value=move || query.get().target_type.unwrap_or_default()
                    on:input=move |event| {
                        if let Some(on_query) = on_query {
                            let mut next = query.get_untracked();
                            let value = event_target_value(&event);
                            next.target_type = Some(value.clone()).filter(|text| !text.is_empty());
                            next.offset = None;
                            on_query.run(next);
                        }
                    }
                />
                <input
                    class="input input-bordered input-sm"
                    placeholder="Actor account id"
                    prop:value=move || query.get().actor_id.unwrap_or_default()
                    on:input=move |event| {
                        if let Some(on_query) = on_query {
                            let mut next = query.get_untracked();
                            let value = event_target_value(&event);
                            next.actor_id = Some(value.clone()).filter(|text| !text.is_empty());
                            next.offset = None;
                            on_query.run(next);
                        }
                    }
                />
            </div>
            <div class="mb-3 flex flex-wrap items-center justify-end gap-2">
                <span class="mr-auto text-xs opacity-60">
                    "The export is every row the filter matches, not only this page."
                </span>
                <button
                    class="btn btn-sm"
                    disabled=on_export.is_none()
                    on:click=move |_| {
                        if let Some(on_export) = on_export {
                            on_export.run(());
                        }
                    }
                >
                    "Export CSV"
                </button>
            </div>
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">{move || error.get().unwrap_or_default()}</p>
            </Show>
            <ul class="divide-y divide-base-300">
                <For each=move || entries.get() key=|entry| entry.id.clone() let:entry>
                    {
                        let when = entry.created_at.get(..19).unwrap_or(&entry.created_at).replace('T', " ");
                        let actor = entry.actor_id.clone().unwrap_or_else(|| "system".to_owned());
                        let target = entry
                            .target_id
                            .as_ref()
                            .map_or_else(|| entry.target_type.clone(), |id| format!("{} {id}", entry.target_type));
                        view! {
                            <li class="flex flex-col gap-1 py-2 text-sm">
                                <div class="flex flex-wrap items-baseline justify-between gap-2">
                                    <span class="font-medium">{entry.action.clone()}</span>
                                    <span class="text-xs opacity-60">{when}</span>
                                </div>
                                <span class="text-xs opacity-70">{format!("{target} · by {actor}")}</span>
                            </li>
                        }
                    }
                </For>
            </ul>
            <Show when=move || has_more.get()>
                <button
                    class="btn btn-ghost btn-sm mt-2"
                    disabled=on_more.is_none()
                    on:click=move |_| {
                        if let Some(on_more) = on_more {
                            on_more.run(());
                        }
                    }
                >
                    "Load more"
                </button>
            </Show>
            <Show when=move || entries.get().is_empty() && !busy.get()>
                <p class="text-sm opacity-60">"No administrative action matches that filter."</p>
            </Show>
        </Panel>
    }
}
