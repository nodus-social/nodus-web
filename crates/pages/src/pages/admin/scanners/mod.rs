//! Scanners: the services that read content, and what their labels mean here.
//!
//! The scanner is a service, so this screen configures the address and the secret rather than
//! any model; the labels arrive from the service's manifest, and each one gets a threshold and
//! a severity, because what counts as worth blocking is the instance's decision.
//!
//! A scanner with no rule for a label does nothing with it, and a scanner with no content set
//! cannot check anything it was asked to — both are shown here rather than guessed at.

use leptos::prelude::*;
use nodus_client_core::model::{AssetKind, ScanSeverity, Scanner, ScannerAsset, ScannerKind};

use crate::components::panel::Panel;

/// The words severity carries, in the order the select offers them.
const SEVERITIES: [ScanSeverity; 4] = [
    ScanSeverity::Ignore,
    ScanSeverity::Flag,
    ScanSeverity::Sensitive,
    ScanSeverity::Block,
];

/// What one severity is called in the console.
#[must_use]
pub fn severity_label(severity: ScanSeverity) -> &'static str {
    match severity {
        ScanSeverity::Ignore => "Ignore",
        ScanSeverity::Flag => "Flag",
        ScanSeverity::Sensitive => "Mark sensitive",
        ScanSeverity::Block => "Block",
    }
}

/// What one severity does, in the sentence under it.
#[must_use]
pub fn severity_hint(severity: ScanSeverity) -> &'static str {
    match severity {
        ScanSeverity::Ignore => "Record the label and do nothing.",
        ScanSeverity::Flag => "Put the subject in the review queue.",
        ScanSeverity::Sensitive => "Hide the media behind a warning.",
        ScanSeverity::Block => "Remove the post outright.",
    }
}

/// What one scanner kind reads.
#[must_use]
pub fn kind_label(kind: ScannerKind) -> &'static str {
    match kind {
        ScannerKind::Image => "Images",
        ScannerKind::Video => "Videos",
        ScannerKind::Text => "Text",
    }
}

/// What one content set holds.
#[must_use]
pub fn asset_label(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Hashes => "Hashes",
        AssetKind::Weights => "Weights",
        AssetKind::Words => "Words",
    }
}

/// The scanner being typed, before it is sent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScannerDraft {
    /// What the console calls it.
    pub name: String,
    /// What it reads.
    pub kind: ScannerKind,
    /// Its base URL, with scheme.
    pub endpoint: String,
    /// The secret it proves itself with.
    pub secret: String,
    /// The content set it checks against, when it needs one.
    pub asset_id: String,
    /// How long one answer may take, in milliseconds.
    pub timeout_ms: String,
}

/// The scanners this instance talks to.
#[component]
pub fn ScannersPage(
    /// The scanners, as the console last read them.
    #[prop(into)]
    scanners: Signal<Vec<Scanner>>,
    /// The content sets this instance holds.
    #[prop(into)]
    assets: Signal<Vec<ScannerAsset>>,
    /// The scanner being typed.
    #[prop(into)]
    draft: Signal<ScannerDraft>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records one field of the draft.
    #[prop(optional)]
    on_draft: Option<Callback<ScannerDraft>>,
    /// Saves the draft as a new scanner.
    #[prop(optional)]
    on_create: Option<Callback<()>>,
    /// Switches one scanner on or off.
    #[prop(optional)]
    on_toggle: Option<Callback<(String, bool)>>,
    /// Forgets one scanner.
    #[prop(optional)]
    on_delete: Option<Callback<String>>,
    /// Asks one service for its manifest again.
    #[prop(optional)]
    on_refresh: Option<Callback<String>>,
    /// Records a threshold and a severity for one label.
    #[prop(optional)]
    on_rule: Option<Callback<(String, String, f64, ScanSeverity)>>,
    /// Forgets one rule.
    #[prop(optional)]
    on_rule_delete: Option<Callback<String>>,
    /// Forgets one content set.
    #[prop(optional)]
    on_asset_delete: Option<Callback<String>>,
    /// Fetches one content set so it can be saved and looked at.
    #[prop(optional)]
    on_asset_download: Option<Callback<(String, String)>>,
    /// The form that uploads a content set, drawn by the caller because it reads a file.
    #[prop(optional, into)]
    upload: Option<AnyView>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
    /// Whether the account's staff role is administrator. Absent means "assume yes".
    #[prop(optional_no_strip)]
    administrator: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    let administrator = crate::props::administrator(administrator);
    // The instance's scanner routes answer `role == admin`; the grant's scope is not enough.
    let may = Signal::derive(move || write.get() && administrator.get());
    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">{move || error.get().unwrap_or_default()}</p>
            </Show>
            <Show when=move || !administrator.get()>
                <crate::components::chrome::Note class="text-sm text-warning">
                    {crate::props::ADMIN_ONLY}
                </crate::components::chrome::Note>
            </Show>
            <Show when=move || !write.get()>
                <crate::components::chrome::Note class="text-sm text-warning">
                    {crate::props::NO_WRITE}
                </crate::components::chrome::Note>
            </Show>

            <Panel
                title="Scanners"
                description="A scanner is a service this instance calls. It answers with labels; what each label means is decided here, not there."
            >
                <For each=move || scanners.get() key=|scanner| scanner.id.clone() let:scanner>
                    {
                        let id = StoredValue::new(scanner.id.clone());
                        let enabled = scanner.enabled;
                        let name = scanner.name.clone();
                        let endpoint = scanner.endpoint.clone();
                        let meta = format!(
                            "{} · {} · {} ms · contract {}",
                            kind_label(scanner.kind),
                            scanner.endpoint,
                            scanner.timeout_ms,
                            scanner.contract,
                        );
                        let manifest = scanner
                            .manifest_at
                            .as_deref()
                            .map_or_else(|| "no manifest read yet".to_owned(), |at| format!("manifest read {at}"));
                        let labels = StoredValue::new(scanner.labels.clone());
                        let rules = StoredValue::new(scanner.rules.clone());
                        let has_rules = !scanner.rules.is_empty();
                        let has_labels = !scanner.labels.is_empty();
                        let unruled: Vec<String> = scanner
                            .labels
                            .iter()
                            .filter(|label| !scanner.rules.iter().any(|rule| &rule.label == *label))
                            .cloned()
                            .collect();
                        let has_unruled = !unruled.is_empty();
                        let unruled_text = format!(
                            "Unruled labels on this scanner: {}",
                            unruled.join(", ")
                        );
                        view! {
                            <div class="flex flex-col gap-2 rounded-box bg-base-200 p-2">
                                <div class="flex flex-wrap items-center gap-2">
                                    <span class="truncate text-sm font-medium">{name.clone()}</span>
                                    <span class="truncate text-xs opacity-60">{meta.clone()}</span>
                                    <span class="text-xs opacity-60">{manifest.clone()}</span>
                                    <button
                                        class="btn btn-ghost btn-xs ml-auto"
                                        disabled=move || on_toggle.is_none() || !may.get()
                                        title=move || crate::props::refusal(write, administrator)
                                        on:click=move |_| {
                                            if let Some(on_toggle) = on_toggle {
                                                on_toggle.run((id.get_value(), !enabled));
                                            }
                                        }
                                    >
                                        {if enabled { "Disable" } else { "Enable" }}
                                    </button>
                                    <button
                                        class="btn btn-ghost btn-xs"
                                        disabled=move || on_refresh.is_none() || !may.get()
                                        title=move || crate::props::refusal(write, administrator)
                                        on:click=move |_| {
                                            if let Some(on_refresh) = on_refresh {
                                                on_refresh.run(id.get_value());
                                            }
                                        }
                                    >
                                        "Read manifest"
                                    </button>
                                    <button
                                        class="btn btn-ghost btn-xs text-error"
                                        disabled=move || on_delete.is_none() || !may.get()
                                        title=move || crate::props::refusal(write, administrator)
                                        on:click=move |_| {
                                            if let Some(on_delete) = on_delete {
                                                on_delete.run(id.get_value());
                                            }
                                        }
                                    >
                                        "Forget"
                                    </button>
                                </div>
                                <p class="text-xs opacity-50">{endpoint.clone()}</p>

                                <Show when=move || has_rules || has_labels>
                                    <ul class="flex flex-col gap-1">
                                        <For each=move || rules.get_value() key=|rule| rule.id.clone() let:rule>
                                            {
                                                let rule_id = StoredValue::new(rule.id.clone());
                                                let text = format!(
                                                    "{} · ≥ {:.2} · {}",
                                                    rule.label,
                                                    rule.threshold,
                                                    severity_label(rule.severity),
                                                );
                                                view! {
                                                    <li class="flex items-center gap-2 text-xs">
                                                        <span class="truncate">{text}</span>
                                                        <button
                                                            class="btn btn-ghost btn-xs ml-auto"
                                                            disabled=move || on_rule_delete.is_none() || !may.get()
                                                            title=move || crate::props::refusal(write, administrator)
                                                            on:click=move |_| {
                                                                if let Some(on_rule_delete) = on_rule_delete {
                                                                    on_rule_delete.run(rule_id.get_value());
                                                                }
                                                            }
                                                        >
                                                            "Remove"
                                                        </button>
                                                    </li>
                                                }
                                            }
                                        </For>
                                    </ul>
                                </Show>

                                <Show when=move || has_unruled>
                                    <p class="text-xs opacity-60">
                                        {unruled_text.clone()}
                                    </p>
                                </Show>

                                {rule_form(id.get_value(), labels.get_value(), on_rule)}
                            </div>
                        }
                    }
                </For>
                <Show when=move || scanners.get().is_empty()>
                    <p class="text-sm opacity-70">
                        "No scanner is configured, so nothing is scanned. Add one below and the queue fills up on its own."
                    </p>
                </Show>
            </Panel>

            <Panel title="Add a scanner" description="The service must answer the Nodus scanner contract; its manifest supplies the labels.">
                <div class="grid gap-2 sm:grid-cols-2">
                    <input
                        class="input input-bordered input-sm"
                        placeholder="Name"
                        disabled=move || !may.get()
                        prop:value=move || draft.get().name
                        on:input=move |event| {
                            let value = event_target_value(&event);
                            if let Some(on_draft) = on_draft {
                                let mut next = draft.get_untracked();
                                next.name = value;
                                on_draft.run(next);
                            }
                        }
                    />
                    <select
                        class="select select-bordered select-sm"
                        prop:value=move || format!("{:?}", draft.get().kind)
                        disabled=move || !may.get()
                        on:change=move |event| {
                            let kind = match event_target_value(&event).as_str() {
                                "Video" => ScannerKind::Video,
                                "Text" => ScannerKind::Text,
                                _ => ScannerKind::Image,
                            };
                            if let Some(on_draft) = on_draft {
                                let mut next = draft.get_untracked();
                                next.kind = kind;
                                on_draft.run(next);
                            }
                        }
                    >
                        <option value="Image">"Images"</option>
                        <option value="Video">"Videos"</option>
                        <option value="Text">"Text"</option>
                    </select>
                    <input
                        class="input input-bordered input-sm sm:col-span-2"
                        placeholder="https://scanner.example:8080"
                        disabled=move || !may.get()
                        prop:value=move || draft.get().endpoint
                        on:input=move |event| {
                            let value = event_target_value(&event);
                            if let Some(on_draft) = on_draft {
                                let mut next = draft.get_untracked();
                                next.endpoint = value;
                                on_draft.run(next);
                            }
                        }
                    />
                    <input
                        class="input input-bordered input-sm"
                        placeholder="Shared secret"
                        disabled=move || !may.get()
                        prop:value=move || draft.get().secret
                        on:input=move |event| {
                            let value = event_target_value(&event);
                            if let Some(on_draft) = on_draft {
                                let mut next = draft.get_untracked();
                                next.secret = value;
                                on_draft.run(next);
                            }
                        }
                    />
                    <input
                        class="input input-bordered input-sm"
                        placeholder="Timeout in milliseconds (5000)"
                        disabled=move || !may.get()
                        prop:value=move || draft.get().timeout_ms
                        on:input=move |event| {
                            let value = event_target_value(&event);
                            if let Some(on_draft) = on_draft {
                                let mut next = draft.get_untracked();
                                next.timeout_ms = value;
                                on_draft.run(next);
                            }
                        }
                    />
                    <select
                        class="select select-bordered select-sm"
                        prop:value=move || draft.get().asset_id
                        disabled=move || !may.get()
                        on:change=move |event| {
                            let value = event_target_value(&event);
                            if let Some(on_draft) = on_draft {
                                let mut next = draft.get_untracked();
                                next.asset_id = value;
                                on_draft.run(next);
                            }
                        }
                    >
                        <option value="">"No content set"</option>
                        <For each=move || assets.get() key=|asset| asset.id.clone() let:asset>
                            {
                                let id = asset.id.clone();
                                let label = format!("{} ({})", asset.name, asset_label(asset.kind));
                                view! { <option value=id.clone()>{label}</option> }
                            }
                        </For>
                    </select>
                    <button
                        class="btn btn-sm btn-primary"
                        disabled=move || busy.get() || on_create.is_none() || !may.get()
                        title=move || crate::props::refusal(write, administrator)
                        on:click=move |_| {
                            if let Some(on_create) = on_create {
                                on_create.run(());
                            }
                        }
                    >
                        "Add scanner"
                    </button>
                </div>
            </Panel>

            <Panel
                title="Content sets"
                description="Hashes, weights or word lists this instance distributes to its scanners. A scanner without one checks nothing, because it has nothing to compare against."
            >
                <ul class="flex flex-col gap-1">
                    <For each=move || assets.get() key=|asset| asset.id.clone() let:asset>
                        {
                            let id = StoredValue::new(asset.id.clone());
                            let name = if asset.name.is_empty() {
                                format!("asset-{}", asset.id)
                            } else {
                                asset.name.clone()
                            };
                            let meta = format!(
                                "{} · {} · {} bytes · {}",
                                asset_label(asset.kind),
                                asset.name,
                                asset.size,
                                asset.digest.chars().take(12).collect::<String>(),
                            );
                            view! {
                                <li class="flex items-center gap-2 rounded-box bg-base-200 px-2 py-1 text-xs">
                                    <span class="truncate">{meta}</span>
                                    <button
                                        class="btn btn-ghost btn-xs ml-auto"
                                        disabled=move || on_asset_download.is_none()
                                        title="Save the bytes the scanner was given"
                                        on:click=move |_| {
                                            if let Some(on_asset_download) = on_asset_download {
                                                on_asset_download.run((id.get_value(), name.clone()));
                                            }
                                        }
                                    >
                                        "Download"
                                    </button>
                                    <button
                                        class="btn btn-ghost btn-xs"
                                        disabled=move || on_asset_delete.is_none() || !may.get()
                                        title=move || crate::props::refusal(write, administrator)
                                        on:click=move |_| {
                                            if let Some(on_asset_delete) = on_asset_delete {
                                                on_asset_delete.run(id.get_value());
                                            }
                                        }
                                    >
                                        "Forget"
                                    </button>
                                </li>
                            }
                        }
                    </For>
                </ul>
                <Show when=move || assets.get().is_empty()>
                    <p class="text-sm opacity-70">"No content set was uploaded yet."</p>
                </Show>
                {upload}
            </Panel>
        </div>
    }
}

/// One scanner's rule editor: a label, a threshold and what the label means.
fn rule_form(
    scanner_id: String,
    labels: Vec<String>,
    on_rule: Option<Callback<(String, String, f64, ScanSeverity)>>,
) -> impl IntoView {
    let label = StoredValue::new(labels.first().cloned().unwrap_or_default());
    let threshold = StoredValue::new(String::from("0.5"));
    let severity = StoredValue::new(ScanSeverity::default());
    let hint = Signal::derive(move || severity_hint(severity.get_value()));
    view! {
        <div class="flex flex-wrap items-center gap-2">
            <input
                class="input input-bordered input-xs w-40"
                placeholder="Label"
                list="nodus-scanner-labels"
                prop:value=move || label.get_value()
                on:input=move |event| label.set_value(event_target_value(&event))
            />
            <datalist id="nodus-scanner-labels">
                <For each=move || labels.clone() key=|label| label.clone() let:label>
                    {view! { <option value=label.clone()>{label.clone()}</option> }}
                </For>
            </datalist>
            <input
                class="input input-bordered input-xs w-20"
                placeholder="0.5"
                prop:value=move || threshold.get_value()
                on:input=move |event| threshold.set_value(event_target_value(&event))
            />
            <select
                class="select select-bordered select-xs"
                on:change=move |event| {
                    let value = event_target_value(&event);
                    severity.set_value(
                        SEVERITIES
                            .into_iter()
                            .find(|severity| severity_label(*severity) == value)
                            .unwrap_or_default(),
                    );
                }
            >
                <For each=move || SEVERITIES key=|severity| format!("{severity:?}") let:option>
                    {
                        let word = severity_label(option);
                        view! { <option value=word>{word}</option> }
                    }
                </For>
            </select>
            <button
                class="btn btn-ghost btn-xs"
                disabled=move || on_rule.is_none()
                on:click=move |_| {
                    let Some(on_rule) = on_rule else { return };
                    let label = label.get_value().trim().to_owned();
                    if label.is_empty() {
                        return;
                    }
                    let threshold = threshold.get_value().parse::<f64>().unwrap_or(0.5);
                    on_rule.run((scanner_id.clone(), label, threshold, severity.get_value()));
                }
            >
                "Add rule"
            </button>
            <span class="text-xs opacity-50">{move || hint.get()}</span>
        </div>
    }
}
