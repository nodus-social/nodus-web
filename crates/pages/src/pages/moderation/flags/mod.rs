//! What the scanners found: the queue a moderator works through.
//!
//! A scanner labels content; this queue is where a person agrees or disagrees with it. Nothing
//! here is automatic — a flag says evidence, and the buttons record the judgement.

use leptos::prelude::*;
use nodus_client_core::model::{FlagState, ScanFlag, ScanSeverity};

/// What one severity is called on a flag.
#[must_use]
pub fn severity_label(severity: ScanSeverity) -> &'static str {
    match severity {
        ScanSeverity::Ignore => "Ignore",
        ScanSeverity::Flag => "Flag",
        ScanSeverity::Sensitive => "Sensitive",
        ScanSeverity::Block => "Block",
    }
}

/// What one flag state is called.
#[must_use]
pub fn state_label(state: FlagState) -> &'static str {
    match state {
        FlagState::Open => "Open",
        FlagState::Dismissed => "Dismissed",
        FlagState::Actioned => "Actioned",
    }
}

/// The scanner review queue.
#[component]
pub fn FlagsPage(
    /// The flags the server sent, newest first.
    #[prop(into)]
    flags: Signal<Vec<ScanFlag>>,
    /// The state the queue is filtered by; `None` reads every state.
    #[prop(into)]
    filter: Signal<Option<FlagState>>,
    /// Whether a read is on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// How long ago a flag was raised.
    #[prop(optional)]
    age_of: Option<Callback<String, String>>,
    /// Refilters the queue.
    #[prop(optional)]
    on_filter: Option<Callback<Option<FlagState>>>,
    /// Records the judgement on one flag.
    #[prop(optional)]
    on_resolve: Option<Callback<(String, FlagState)>>,
) -> impl IntoView {
    let button = move |word: &'static str, value: Option<FlagState>| {
        let active = Signal::derive(move || filter.get() == value);
        view! {
            <button
                class="btn btn-xs"
                class:btn-active=move || active.get()
                disabled=on_filter.is_none()
                on:click=move |_| {
                    if let Some(on_filter) = on_filter {
                        on_filter.run(value);
                    }
                }
            >
                {word}
            </button>
        }
    };

    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">{move || error.get().unwrap_or_default()}</p>
            </Show>

            <div class="flex flex-wrap items-center gap-2">
                {button("Open", Some(FlagState::Open))}
                {button("Dismissed", Some(FlagState::Dismissed))}
                {button("Actioned", Some(FlagState::Actioned))}
                {button("All", None)}
            </div>

            <Show when=move || busy.get() && flags.get().is_empty()>
                <p class="text-sm opacity-60">"Reading the queue…"</p>
            </Show>

            <ul class="flex flex-col gap-2">
                <For each=move || flags.get() key=|flag| flag.id.clone() let:flag>
                    {
                        let id = StoredValue::new(flag.id.clone());
                        let status = flag.status_id.clone().unwrap_or_default();
                        let reason = flag.reason.clone();
                        let labels = flag.labels.join(", ");
                        let has_labels = !labels.is_empty();
                        let shown_labels = labels.clone();
                        let has_status = !status.is_empty();
                        let shown_status = status.clone();
                        let score = flag
                            .score
                            .map(|score| format!("{score:.2}"))
                            .unwrap_or_else(|| "—".to_owned());
                        let severity = severity_label(flag.severity);
                        let state = state_label(flag.state);
                        let age = age_of
                            .map(|age_of| age_of.run(flag.created_at.clone()))
                            .unwrap_or_else(|| flag.created_at.clone());
                        let open = flag.state == FlagState::Open;
                        view! {
                            <li class="flex flex-col gap-2 rounded-box border border-base-300 bg-base-100 p-3">
                                <div class="flex flex-wrap items-center gap-2">
                                    <span class="badge badge-sm">{severity}</span>
                                    <span class="badge badge-ghost badge-sm">{state}</span>
                                    <span class="truncate text-sm font-medium">{reason}</span>
                                    <span class="text-xs opacity-60">{format!("score {score}")}</span>
                                    <span class="text-xs opacity-60">{age}</span>
                                </div>
                                <Show when=move || has_labels>
                                    <p class="text-xs opacity-70">{shown_labels.clone()}</p>
                                </Show>
                                <Show when=move || has_status>
                                    <a
                                        class="link text-xs"
                                        href=format!("/s/{}", shown_status)
                                    >
                                        "Open the post"
                                    </a>
                                </Show>
                                <div class="flex flex-wrap gap-2">
                                    <button
                                        class="btn btn-ghost btn-xs"
                                        disabled=move || !open || on_resolve.is_none()
                                        on:click=move |_| {
                                            if let Some(on_resolve) = on_resolve {
                                                on_resolve.run((id.get_value(), FlagState::Dismissed));
                                            }
                                        }
                                    >
                                        "Nothing wrong"
                                    </button>
                                    <button
                                        class="btn btn-ghost btn-xs"
                                        disabled=move || !open || on_resolve.is_none()
                                        on:click=move |_| {
                                            if let Some(on_resolve) = on_resolve {
                                                on_resolve.run((id.get_value(), FlagState::Actioned));
                                            }
                                        }
                                    >
                                        "Handled"
                                    </button>
                                </div>
                            </li>
                        }
                    }
                </For>
            </ul>

            <Show when=move || !busy.get() && flags.get().is_empty()>
                <p class="text-sm opacity-70">
                    "Nothing is flagged. Scanners put content here when a label crosses the threshold an administrator set."
                </p>
            </Show>
        </div>
    }
}
