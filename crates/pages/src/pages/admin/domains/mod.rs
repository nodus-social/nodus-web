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
) -> impl IntoView {
    view! {
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
                                disabled=on_withdraw.is_none()
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
                    disabled=move || busy.get() || on_allow.is_none()
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
) -> impl IntoView {
    view! {
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
                                                        disabled=on_ask.is_none()
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
