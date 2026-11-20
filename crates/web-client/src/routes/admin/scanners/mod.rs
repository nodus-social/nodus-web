//! The scanners this instance talks to.
//!
//! Nothing here models a scanner: the console names a service, tells it a secret, and decides
//! what each of the labels it advertises means. The labels come from the service's manifest, so
//! a scanner that answers with a new label appears here with that label unruled.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_client_core::model::{
    AssetKind, AssetUpload, NewScanner, ScanSeverity, Scanner, ScannerAsset,
};
use nodus_pages::pages::admin::scanners::{ScannerDraft, ScannersPage};

use crate::platform::download::save_bytes;
use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// One scanner as the form needs it, with the entry field kept as text.
fn draft_of(draft: &ScannerDraft) -> Result<NewScanner, String> {
    let name = draft.name.trim();
    if name.is_empty() {
        return Err("Name the scanner, so the console can refer to it.".to_owned());
    }
    let endpoint = draft.endpoint.trim();
    if endpoint.is_empty() {
        return Err(
            "The scanner needs an address, such as https://scanner.example:8080.".to_owned(),
        );
    }
    let timeout_ms = draft
        .timeout_ms
        .trim()
        .parse::<i32>()
        .unwrap_or(5_000)
        .clamp(100, 60_000);
    Ok(NewScanner {
        name: name.to_owned(),
        kind: draft.kind,
        endpoint: endpoint.to_owned(),
        secret: Some(draft.secret.clone()).filter(|value| !value.is_empty()),
        enabled: true,
        asset_id: Some(draft.asset_id.clone()).filter(|value| !value.is_empty()),
        timeout_ms,
    })
}

/// Scanners, their rules, and the content sets the controller hands out.
#[component]
pub fn Scanners(
    /// Whether the account's staff role is administrator. Absent means "assume yes".
    #[prop(optional_no_strip)]
    administrator: Option<Signal<bool>>,
) -> impl IntoView {
    let administrator = nodus_pages::props::administrator(administrator);
    let alive = Alive::new();
    let (scanners, set_scanners) = signal(Vec::<Scanner>::new());
    let (assets, set_assets) = signal(Vec::<ScannerAsset>::new());
    let (draft, set_draft) = signal(ScannerDraft::default());
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (asset_name, set_asset_name) = signal(String::new());

    let reload = Callback::new(move |()| {
        spawn_local(async move {
            let outcome = admin::scanners().await;
            let held = admin::scanner_assets().await;
            alive.if_alive(|| {
                set_busy.set(true);
                match (outcome, held) {
                    (Ok(list), Ok(sets)) => {
                        set_scanners.set(list);
                        set_assets.set(sets);
                        set_error.set(None);
                    }
                    (Err(message), _) | (_, Err(message)) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    // The panel's reads are administrator routes too: a moderator who reaches this address
    // is shown the panel and its reason, and asked for nothing the instance would refuse.
    Effect::new(move |_| {
        if administrator.get() {
            reload.run(());
        } else {
            set_busy.set(false);
        }
    });

    let create = Callback::new(move |()| {
        let scanner = match draft_of(&draft.get_untracked()) {
            Ok(scanner) => scanner,
            Err(message) => {
                set_error.set(Some(message));
                return;
            }
        };
        spawn_local(async move {
            match admin::create_scanner(&scanner).await {
                Ok(_) => {
                    alive.if_alive(|| {
                        set_draft.set(ScannerDraft::default());
                        set_error.set(None);
                    });
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let toggle = Callback::new(move |(id, enabled): (String, bool)| {
        spawn_local(async move {
            let outcome = match admin::scanners().await {
                Ok(list) => match list.into_iter().find(|scanner| scanner.id == id) {
                    Some(scanner) => {
                        admin::update_scanner(
                            &id,
                            &NewScanner {
                                name: scanner.name,
                                kind: scanner.kind,
                                endpoint: scanner.endpoint,
                                secret: None,
                                enabled,
                                asset_id: scanner.asset_id,
                                timeout_ms: scanner.timeout_ms,
                            },
                        )
                        .await
                    }
                    None => Err("That scanner is gone.".to_owned()),
                },
                Err(message) => Err(message),
            };
            match outcome {
                Ok(_) => reload.run(()),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let delete = Callback::new(move |id: String| {
        spawn_local(async move {
            if admin::delete_scanner(&id).await.is_ok() {
                reload.run(());
            }
        });
    });

    let refresh = Callback::new(move |id: String| {
        spawn_local(async move {
            match admin::refresh_scanner(&id).await {
                Ok(_) => reload.run(()),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let rule = Callback::new(
        move |(scanner_id, label, threshold, severity): (String, String, f64, ScanSeverity)| {
            spawn_local(async move {
                match admin::set_scanner_rule(&scanner_id, &label, threshold, severity).await {
                    Ok(_) => reload.run(()),
                    Err(message) => alive.if_alive(|| set_error.set(Some(message))),
                }
            });
        },
    );

    let rule_delete = Callback::new(move |id: String| {
        spawn_local(async move {
            if admin::delete_scanner_rule(&id).await.is_ok() {
                reload.run(());
            }
        });
    });

    let asset_delete = Callback::new(move |id: String| {
        spawn_local(async move {
            if admin::delete_scanner_asset(&id).await.is_ok() {
                reload.run(());
            }
        });
    });

    let asset_download = Callback::new(move |(id, name): (String, String)| {
        spawn_local(async move {
            match admin::download_scanner_asset(&id).await {
                Ok(bytes) => {
                    if let Err(message) = save_bytes(&bytes, &name) {
                        alive.if_alive(|| set_error.set(Some(message)));
                    }
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let upload = {
        let alive = Alive::new();
        view! {
                <div class="flex flex-col gap-2">
                    <input
                        class="input input-bordered input-sm"
                        placeholder="What the set is called"
                        prop:value=move || asset_name.get()
                        on:input=move |event| {
                            set_asset_name.set(event_target_value(&event));
                        }
                    />
                    <input
                        class="file-input file-input-bordered file-input-sm"
                        type="file"
                        accept=".txt,.json,.csv,.bin"
                        on:change=move |event| {
                            let input = event_target::<web_sys::HtmlInputElement>(&event);
                            let Some(files) = input.files() else { return };
                            let Some(file) = files.get(0) else { return };
                            let name = asset_name.get_untracked().trim().to_owned();
                            let name = if name.is_empty() { file.name() } else { name };
                            let kind = match name.rsplit('.').next() {
                                Some("json") => AssetKind::Weights,
                                Some("csv") => AssetKind::Words,
                                _ => AssetKind::Hashes,
                            };
                            spawn_local(async move {
                                let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                                    .await
                                    .ok()
                                    .map(|buffer| js_sys::Uint8Array::new(&buffer).to_vec());
                                let Some(bytes) = buffer else {
                                    alive.if_alive(|| {
                                        set_error.set(Some("Could not read that file.".to_owned()));
                                    });
                                    return;
                                };
                                let content_type = file.type_();
                                let asset = AssetUpload {
                                    name,
                                    kind,
                                    content_type: if content_type.is_empty() {
                                        "application/octet-stream".to_owned()
                                    } else {
                                        content_type
                                    },
                                    data: base64::engine::Engine::encode(
                                        &base64::engine::general_purpose::STANDARD,
                                        &bytes,
                                    ),
                                };
                                match admin::upload_scanner_asset(&asset).await {
                                    Ok(_) => {
                                        alive.if_alive(|| set_asset_name.set(String::new()));
                                        reload.run(());
                                    }
                                    Err(message) => {
                                        alive.if_alive(|| set_error.set(Some(message)));
                                    }
                                }
                            });
                        }
                    />
                </div>
        }
        .into_any()
    };

    view! {
        <ScannersPage
            scanners=scanners
            assets=assets
            draft=draft
            busy=busy
            error=error
            on_draft=Callback::new(move |next: ScannerDraft| set_draft.set(next))
            on_create=create
            on_toggle=toggle
            on_delete=delete
            on_refresh=refresh
            on_rule=rule
            on_rule_delete=rule_delete
            on_asset_delete=asset_delete
            on_asset_download=asset_download
            upload=upload
            write=Some(crate::state::bridge::admin::may_write())
            administrator=Some(administrator)
        />
    }
}
