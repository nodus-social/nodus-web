//! Domains this instance refuses.
//!
//! The outbound half of federation lives under Administration (`relays` and allowed
//! domains), because allowing a domain is a decision about the network, while blocking one is
//! a decision about what the people here have to see.
//!
//! The two fields of the add form are the caller's state and arrive as signals: a page may not
//! hold a draft. What it does own is the form's own layout, so the input, the severity toggle
//! and the press cannot drift apart from the list above them.

use leptos::prelude::*;
use nodus_client_core::model::{DomainBlock, DomainSeverity};

/// Blocked domains, with an add form.
#[component]
pub fn FederationPage(
    /// The blocks this instance holds, as the caller last read them.
    #[prop(into)]
    blocks: Signal<Vec<DomainBlock>>,
    /// The domain being typed.
    #[prop(into)]
    domain: Signal<String>,
    /// Whether a new block refuses the domain entirely rather than silencing it.
    #[prop(into)]
    suspend: Signal<bool>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal, when it had one.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records the typed domain. Absent where there is nowhere to keep it.
    #[prop(optional)]
    on_domain: Option<Callback<String>>,
    /// Records the severity. Absent where nothing can be chosen.
    #[prop(optional)]
    on_suspend: Option<Callback<bool>>,
    /// Blocks the typed domain. Absent where nothing can be sent.
    #[prop(optional)]
    on_block: Option<Callback<()>>,
    /// Lifts one block. Absent where nothing can be sent.
    #[prop(optional)]
    on_remove: Option<Callback<String>>,
    /// Changes how much of one domain is refused. Absent where nothing can be sent.
    #[prop(optional)]
    on_severity: Option<Callback<(String, DomainSeverity)>>,
    /// Records the list being pasted in.
    #[prop(optional)]
    on_import: Option<Callback<String>>,
    /// Imports the pasted list in one request.
    #[prop(optional)]
    on_import_now: Option<Callback<()>>,
    /// What the last import did.
    #[prop(into)]
    #[prop(optional)]
    imported: Signal<Option<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    view! {
        <Show when=move || error.get().is_some()>
            <p class="text-sm text-error">{move || error.get().unwrap_or_default()}</p>
        </Show>

        <ul class="flex flex-col gap-2">
            <For each=move || blocks.get() key=|block| block.id.clone() let:block>
                {
                    let target = StoredValue::new(block.id.clone());
                    let domain = block.domain.clone();
                    let severity = block.severity;
                    let comment = block.public_comment.clone();
                    let has_comment = !comment.is_empty();
                    let comment_text = comment.clone();
                    let select_label = format!("Severity for {domain}");
                    view! {
                        <li class="flex flex-wrap items-center justify-between gap-2 rounded-box bg-base-100 px-3 py-2 shadow-sm">
                            <div class="min-w-0">
                                <p class="truncate text-sm">{domain}</p>
                                <Show when=move || has_comment>
                                    <p class="truncate text-xs opacity-60">{comment_text.clone()}</p>
                                </Show>
                            </div>
                            <div class="flex shrink-0 items-center gap-2">
                                <select
                                    class="select select-bordered select-xs"
                                    aria-label=select_label
                                    disabled=move || on_severity.is_none() || !write.get()
                                    title=move || (!write.get()).then_some(crate::props::NO_WRITE).unwrap_or_default()
                                    on:change=move |event| {
                                        if let Some(on_severity) = on_severity {
                                            let next = match event_target_value(&event).as_str() {
                                                "silence" => DomainSeverity::Silence,
                                                "noop" => DomainSeverity::Noop,
                                                _ => DomainSeverity::Suspend,
                                            };
                                            if next != severity {
                                                on_severity.run((target.get_value(), next));
                                            }
                                        }
                                    }
                                >
                                    <option value="suspend" selected=severity == DomainSeverity::Suspend>
                                        "Suspend"
                                    </option>
                                    <option value="silence" selected=severity == DomainSeverity::Silence>
                                        "Silence"
                                    </option>
                                    <option value="noop" selected=severity == DomainSeverity::Noop>
                                        "No-op (keep on record)"
                                    </option>
                                </select>
                                <button
                                    class="btn btn-ghost btn-xs"
                                    disabled=on_remove.is_none()
                                    on:click=move |_| {
                                        // A block that could not be removed is still in force;
                                        // the list is the record of that.
                                        if let Some(on_remove) = on_remove {
                                            on_remove.run(target.get_value());
                                        }
                                    }
                                >
                                    "Remove"
                                </button>
                            </div>
                        </li>
                    }
                }
            </For>
        </ul>

        <div class="mt-4 flex flex-col gap-2">
            <input
                class="input input-bordered input-sm"
                placeholder="example.social"
                prop:value=move || domain.get()
                disabled=on_domain.is_none()
                on:input=move |event| {
                    if let Some(on_domain) = on_domain {
                        on_domain.run(event_target_value(&event));
                    }
                }
            />
            <label class="label cursor-pointer justify-start gap-2">
                <input
                    type="checkbox"
                    class="toggle toggle-sm"
                    prop:checked=move || suspend.get()
                    disabled=on_suspend.is_none()
                    on:change=move |event| {
                        if let Some(on_suspend) = on_suspend {
                            on_suspend.run(event_target_checked(&event));
                        }
                    }
                />
                <span class="label-text">"Suspend entirely (off: silence from timelines)"</span>
            </label>
            <button
                class="btn btn-error btn-sm self-start"
                disabled=move || busy.get() || on_block.is_none()
                on:click=move |_| {
                    if let Some(on_block) = on_block {
                        on_block.run(());
                    }
                }
            >
                "Block domain"
            </button>
        </div>

        <div class="mt-4 flex flex-col gap-2">
            <p class="text-sm font-medium">"Import a list"</p>
            <p class="text-xs opacity-60">
                "One domain per line, as a block list exports them. Domains already blocked are skipped."
            </p>
            <textarea
                class="textarea textarea-bordered textarea-sm h-24 font-mono text-xs"
                placeholder="one.domain"
                disabled=on_import.is_none()
                on:input=move |event| {
                    if let Some(on_import) = on_import {
                        on_import.run(event_target_value(&event));
                    }
                }
            ></textarea>
            <div class="flex items-center gap-2">
                <button
                    class="btn btn-sm"
                    disabled=move || busy.get() || on_import_now.is_none()
                    on:click=move |_| {
                        if let Some(on_import_now) = on_import_now {
                            on_import_now.run(());
                        }
                    }
                >
                    "Block these"
                </button>
                <Show when=move || imported.get().is_some()>
                    <span class="text-xs opacity-70">{move || imported.get().unwrap_or_default()}</span>
                </Show>
            </div>
        </div>
    }
}
