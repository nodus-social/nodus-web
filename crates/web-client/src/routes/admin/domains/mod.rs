//! The one federation lever Mastodon exposes to clients: allowed domains.
//!
//! Relays used to live here too. They have no API — `/methods/admin/relays/` is itself a
//! 404 page on the docs site and `/api/v1/admin/relays` answers the instance's HTML 404 —
//! so a client cannot offer them, and linking to the instance's own web UI instead sends
//! the reader somewhere this app cannot reach.
//!
//! The panel, the list and the form are drawn in `nodus-view`; what stays here is the read,
//! the write and the list the two keep in step.
//!
//! The second panel is a Nodus extension — an inbound `Block` recorded against a host — so it
//! is drawn only where `supports_extensions` says the instance answers for it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::DomainAllow;
use nodus_pages::pages::admin::domains::{DomainsPage, RemoteBlockRow, RemoteBlocksPage};

use crate::state::alive::Alive;
use crate::state::bridge::{admin, revision, supports_extensions};

/// Domains let through a general block.
#[component]
pub fn Domains() -> impl IntoView {
    let revision = revision();
    let supported = supports_extensions();
    let (items, set_items) = signal(Vec::<DomainAllow>::new());
    let (domain, set_domain) = signal(String::new());
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let (blocks, set_blocks) = signal(Vec::<RemoteBlockRow>::new());
    let (confirming, set_confirming) = signal(None::<String>);
    let (block_busy, set_block_busy) = signal(false);
    let (block_error, set_block_error) = signal(None::<String>);

    Effect::new(move |_| {
        revision.get();
        let alive = Alive::new();
        spawn_local(async move {
            match admin::domain_allows().await {
                Ok(list) => alive.if_alive(|| set_items.set(list)),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    Effect::new(move |_| {
        if !supported.get() {
            return;
        }
        revision.get();
        let alive = Alive::new();
        spawn_local(async move {
            match admin::remote_blocks().await {
                Ok(list) => alive.if_alive(|| {
                    set_blocks.set(
                        list.into_iter()
                            .map(|block| RemoteBlockRow {
                                actor: block.actor,
                                host: block.host,
                                created_at: block.created_at,
                            })
                            .collect(),
                    )
                }),
                Err(message) => alive.if_alive(|| set_block_error.set(Some(message))),
            }
        });
    });

    let allow = Callback::new(move |()| {
        let name = domain.get_untracked().trim().to_owned();
        if name.is_empty() {
            return;
        }
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            match admin::create_domain_allow(&name).await {
                Ok(entry) => {
                    set_domain.set(String::new());
                    set_items.update(|list| list.push(entry));
                }
                Err(message) => set_error.set(Some(message)),
            }
            set_busy.set(false);
        });
    });

    let withdraw = Callback::new(move |id: String| {
        spawn_local(async move {
            match admin::delete_domain_allow(&id).await {
                Ok(()) => set_items.update(|list| list.retain(|item| item.id != id)),
                Err(message) => set_error.set(Some(message)),
            }
        });
    });

    let ask = Callback::new(move |host: String| {
        set_block_error.set(None);
        set_confirming.set(Some(host));
    });

    let cancel = Callback::new(move |()| set_confirming.set(None));

    let clear = Callback::new(move |host: String| {
        set_block_busy.set(true);
        set_block_error.set(None);
        spawn_local(async move {
            match admin::delete_remote_block(&host).await {
                Ok(()) => {
                    set_confirming.set(None);
                    set_blocks.update(|list| list.retain(|block| block.host != host));
                }
                Err(message) => set_block_error.set(Some(message)),
            }
            set_block_busy.set(false);
        });
    });

    view! {
        <div class="flex flex-col gap-3">
            <DomainsPage
                items=items
                domain=domain
                busy=busy
                error=error
                on_domain=Callback::new(move |value: String| set_domain.set(value))
                on_allow=allow
                on_withdraw=withdraw
            />
            <Show when=move || supported.get()>
                <RemoteBlocksPage
                    blocks=blocks
                    confirming=confirming
                    busy=block_busy
                    error=block_error
                    on_ask=ask
                    on_cancel=cancel
                    on_clear=clear
                />
            </Show>
        </div>
    }
}
