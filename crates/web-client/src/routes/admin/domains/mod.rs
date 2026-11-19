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
//! is drawn only where the `/api/v1/meta` features name `remote_blocks`.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::DomainAllow;
use nodus_pages::pages::admin::domains::{
    BlockImportProblem, BlockImportReport, BlockListPage, BlockSubscriptionRow,
    BlockSubscriptionsPage, DomainsPage, RemoteBlockRow, RemoteBlocksPage,
};

use crate::platform::download::save_bytes;
use crate::state::alive::Alive;
use crate::state::bridge::{admin, feature, revision};

/// The bytes of a chosen file as text, or the sentence to show when they cannot be read.
async fn read_list_file(file: web_sys::File) -> Result<String, String> {
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| format!("Could not read that file: {error:?}"))?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    String::from_utf8(bytes).map_err(|_| "That file is not UTF-8 text.".to_owned())
}

/// Domains let through a general block.
#[component]
pub fn Domains() -> impl IntoView {
    let revision = revision();
    // The block-list screen and its subscriptions are the `block_lists` capability; the
    // inbound-block panel below is the separate `remote_blocks` one. Asking one name for
    // the other hides a screen the instance serves.
    let block_lists = feature("block_lists");
    let remote_blocks = feature("remote_blocks");
    let (items, set_items) = signal(Vec::<DomainAllow>::new());
    let (domain, set_domain) = signal(String::new());
    let (busy, set_busy) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let (blocks, set_blocks) = signal(Vec::<RemoteBlockRow>::new());
    let (confirming, set_confirming) = signal(None::<String>);
    let (block_busy, set_block_busy) = signal(false);
    let (block_error, set_block_error) = signal(None::<String>);
    let (list_text, set_list_text) = signal(String::new());
    let (import_report, set_import_report) = signal(None::<BlockImportReport>);
    let (checking, set_checking) = signal(false);
    let (applying, set_applying) = signal(false);
    let (subscriptions, set_subscriptions) = signal(Vec::<BlockSubscriptionRow>::new());
    let (subscription_url, set_subscription_url) = signal(String::new());
    let (subscription_format, set_subscription_format) = signal("csv".to_owned());
    let (subscription_busy, set_subscription_busy) = signal(false);
    let (syncing, set_syncing) = signal(None::<String>);
    let (subscription_error, set_subscription_error) = signal(None::<String>);

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
        if !remote_blocks.get() {
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

    // The block list is carried out as the instance streamed it: the bytes are the file, so
    // nothing here re-encodes them and a CSV export stays a CSV.
    let export = Callback::new(move |format: String| {
        set_block_error.set(None);
        spawn_local(async move {
            match admin::export_block_list(&format).await {
                Ok(bytes) => {
                    let name = format!("domain-blocks.{format}");
                    if let Err(message) = save_bytes(&bytes, &name) {
                        set_block_error.set(Some(message));
                    }
                }
                Err(message) => set_block_error.set(Some(message)),
            }
        });
    });

    // Typing or choosing a different list invalidates the counts beside it: they describe the
    // text that was checked, and a moderator must not apply a list they never saw counted.
    let record_list = Callback::new(move |text: String| {
        set_import_report.set(None);
        set_list_text.set(text);
    });

    let check = Callback::new(move |()| {
        let text = list_text.get_untracked();
        if text.trim().is_empty() {
            return;
        }
        set_checking.set(true);
        set_block_error.set(None);
        spawn_local(async move {
            match admin::import_block_list(&text, true).await {
                Ok(report) => set_import_report.set(Some(BlockImportReport {
                    parsed: report.parsed,
                    new: report.new,
                    updated: report.updated,
                    unchanged: report.unchanged,
                    invalid: report
                        .invalid
                        .into_iter()
                        .map(|problem| BlockImportProblem {
                            line: problem.line,
                            reason: problem.reason,
                        })
                        .collect(),
                    applied: false,
                })),
                Err(message) => set_block_error.set(Some(message)),
            }
            set_checking.set(false);
        });
    });

    let apply = Callback::new(move |()| {
        let text = list_text.get_untracked();
        set_applying.set(true);
        set_block_error.set(None);
        spawn_local(async move {
            match admin::import_block_list(&text, false).await {
                Ok(report) => set_import_report.set(Some(BlockImportReport {
                    parsed: report.parsed,
                    new: report.new,
                    updated: report.updated,
                    unchanged: report.unchanged,
                    invalid: report
                        .invalid
                        .into_iter()
                        .map(|problem| BlockImportProblem {
                            line: problem.line,
                            reason: problem.reason,
                        })
                        .collect(),
                    applied: true,
                })),
                Err(message) => set_block_error.set(Some(message)),
            }
            set_applying.set(false);
        });
    });

    let refresh_subscriptions = Callback::new(move |()| {
        spawn_local(async move {
            match admin::block_subscriptions().await {
                Ok(list) => set_subscriptions.set(
                    list.into_iter()
                        .map(|item| BlockSubscriptionRow {
                            id: item.id,
                            url: item.url,
                            format: item.format,
                            last_synced_at: item.last_synced_at,
                            last_error: item.last_error,
                        })
                        .collect(),
                ),
                Err(message) => set_subscription_error.set(Some(message)),
            }
        });
    });

    Effect::new(move |_| {
        if !block_lists.get() {
            return;
        }
        revision.get();
        refresh_subscriptions.run(());
    });

    let follow = Callback::new(move |()| {
        let url = subscription_url.get_untracked().trim().to_owned();
        if url.is_empty() {
            return;
        }
        let format = subscription_format.get_untracked();
        set_subscription_busy.set(true);
        set_subscription_error.set(None);
        spawn_local(async move {
            match admin::subscribe_block_list(&url, &format).await {
                Ok(entry) => {
                    set_subscription_url.set(String::new());
                    set_subscriptions.update(|list| {
                        list.push(BlockSubscriptionRow {
                            id: entry.id,
                            url: entry.url,
                            format: entry.format,
                            last_synced_at: entry.last_synced_at,
                            last_error: entry.last_error,
                        });
                    });
                }
                Err(message) => set_subscription_error.set(Some(message)),
            }
            set_subscription_busy.set(false);
        });
    });

    let unfollow = Callback::new(move |id: String| {
        set_subscription_busy.set(true);
        set_subscription_error.set(None);
        spawn_local(async move {
            match admin::unsubscribe_block_list(&id).await {
                Ok(()) => set_subscriptions.update(|list| list.retain(|item| item.id != id)),
                Err(message) => set_subscription_error.set(Some(message)),
            }
            set_subscription_busy.set(false);
        });
    });

    // A sync changes the row's last result either way, so the list is read again rather than
    // patched from the sync report: the instance is the one that records the error.
    let sync = Callback::new(move |id: String| {
        set_syncing.set(Some(id.clone()));
        set_subscription_error.set(None);
        spawn_local(async move {
            match admin::sync_block_subscription(&id).await {
                Ok(report) => {
                    // A sync that could not reach or read the list never answers here: the
                    // instance records the reason on the row and returns an error, so the
                    // `Err` arm below is where a failed pull is reported. What a successful
                    // pull can still carry is lines it could not read.
                    if report.invalid > 0 {
                        set_subscription_error.set(Some(format!(
                            "{} line{} in that list could not be read; the rest was applied.",
                            report.invalid,
                            if report.invalid == 1 { "" } else { "s" }
                        )));
                    }
                    refresh_subscriptions.run(());
                }
                Err(message) => set_subscription_error.set(Some(message)),
            }
            set_syncing.set(None);
        });
    });

    let list_file = move || {
        view! {
            <input
                class="file-input file-input-bordered file-input-sm"
                type="file"
                accept=".csv,.json,text/csv,application/json"
                aria-label="Block list file"
                on:change=move |event| {
                    let input = event_target::<web_sys::HtmlInputElement>(&event);
                    let Some(files) = input.files() else { return };
                    let Some(file) = files.get(0) else { return };
                    spawn_local(async move {
                        match read_list_file(file).await {
                            Ok(text) => {
                                record_list.run(text);
                                set_block_error.set(None);
                            }
                            Err(message) => set_block_error.set(Some(message)),
                        }
                    });
                }
            />
        }
    };

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
                write=Some(crate::state::bridge::admin::may_write())
            />
            <Show when=move || block_lists.get()>
                <BlockListPage
                    text=list_text
                    report=import_report
                    checking=checking
                    applying=applying
                    error=block_error
                    on_text=record_list
                    on_export=export
                    on_check=check
                    on_apply=apply
                    write=Some(crate::state::bridge::admin::may_write())
                >
                    {list_file}
                </BlockListPage>
                <BlockSubscriptionsPage
                    items=subscriptions
                    url=subscription_url
                    format=subscription_format
                    busy=subscription_busy
                    syncing=syncing
                    error=subscription_error
                    on_url=Callback::new(move |value: String| set_subscription_url.set(value))
                    on_format=Callback::new(move |value: String| set_subscription_format.set(value))
                    on_add=follow
                    on_remove=unfollow
                    on_sync=sync
                    write=Some(crate::state::bridge::admin::may_write())
                />
            </Show>
            <Show when=move || remote_blocks.get()>
                <RemoteBlocksPage
                    blocks=blocks
                    confirming=confirming
                    busy=block_busy
                    error=block_error
                    on_ask=ask
                    on_cancel=cancel
                    on_clear=clear
                    write=Some(crate::state::bridge::admin::may_write())
                />
            </Show>
        </div>
    }
}
