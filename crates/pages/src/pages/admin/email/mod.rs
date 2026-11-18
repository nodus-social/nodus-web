//! Mail domains: which addresses may sign up, and which never may.
//!
//! A block here is about the address, not the account: an existing account keeps working, and a
//! block with "registration allowed" is advisory — it only stops new sign-ups from that domain.

use leptos::prelude::*;
use nodus_client_core::model::EmailDomainBlock;

use crate::components::panel::Panel;

/// The mail domains this instance refuses.
#[component]
pub fn EmailDomainsPage(
    /// The blocks this instance holds.
    #[prop(into)]
    items: Signal<Vec<EmailDomainBlock>>,
    /// The domain being typed.
    #[prop(into)]
    domain: Signal<String>,
    /// Whether a new block still lets existing accounts in.
    #[prop(into)]
    allow_registration: Signal<bool>,
    /// Whether a write is already on its way.
    #[prop(into)]
    busy: Signal<bool>,
    /// The last read's or write's refusal.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Records the domain.
    #[prop(optional)]
    on_domain: Option<Callback<String>>,
    /// Records the registration toggle.
    #[prop(optional)]
    on_allow_registration: Option<Callback<bool>>,
    /// Blocks the typed domain.
    #[prop(optional)]
    on_block: Option<Callback<()>>,
    /// Lifts one block.
    #[prop(optional)]
    on_withdraw: Option<Callback<String>>,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">{move || error.get().unwrap_or_default()}</p>
            </Show>

            <Show when=move || !write.get()>
                <crate::components::chrome::Note class="text-sm text-warning">
                    {crate::props::NO_WRITE}
                </crate::components::chrome::Note>
            </Show>

            <Panel
                title="Mail domains"
                description="Where a sign-up may come from. A block refuses new registrations from that mail domain; existing accounts are untouched."
            >
                <ul class="flex flex-col gap-1">
                    <For each=move || items.get() key=|item| item.id.clone() let:item>
                        {
                            let id = StoredValue::new(item.id.clone());
                            let note = if item.allow_registration {
                                "existing accounts keep working"
                            } else {
                                "no new sign-ups"
                            };
                            view! {
                                <li class="flex items-center gap-2 rounded-box bg-base-200 p-2 text-sm">
                                    <span class="truncate">{item.domain.clone()}</span>
                                    <span class="text-xs opacity-60">{note}</span>
                                    <button
                                        class="btn btn-ghost btn-xs ml-auto"
                                        disabled=move || on_withdraw.is_none() || !write.get()
                                        title=move || (!write.get()).then_some(crate::props::NO_WRITE).unwrap_or_default()
                                        on:click=move |_| {
                                            if let Some(on_withdraw) = on_withdraw {
                                                on_withdraw.run(id.get_value());
                                            }
                                        }
                                    >
                                        "Lift"
                                    </button>
                                </li>
                            }
                        }
                    </For>
                </ul>
                <Show when=move || items.get().is_empty()>
                    <p class="text-sm opacity-70">"Every mail domain may sign up."</p>
                </Show>
                <form
                    class="flex flex-wrap items-center gap-2"
                    on:submit=move |event| {
                        event.prevent_default();
                        if let Some(on_block) = on_block {
                            on_block.run(());
                        }
                    }
                >
                    <input
                        class="input input-bordered input-sm flex-1"
                        placeholder="mail.example"
                        prop:value=move || domain.get()
                        on:input=move |event| {
                            if let Some(on_domain) = on_domain {
                                on_domain.run(event_target_value(&event));
                            }
                        }
                    />
                    <label class="label cursor-pointer gap-2">
                        <span class="label-text text-xs">"Let existing accounts stay"</span>
                        <input
                            type="checkbox"
                            class="checkbox checkbox-xs"
                            prop:checked=move || allow_registration.get()
                            on:change=move |event| {
                                if let Some(on_allow_registration) = on_allow_registration {
                                    on_allow_registration.run(event_target_checked(&event));
                                }
                            }
                        />
                    </label>
                    <button
                        class="btn btn-ghost btn-sm"
                        disabled=move || busy.get() || on_block.is_none() || !write.get()
                        title=move || (!write.get()).then_some(crate::props::NO_WRITE).unwrap_or_default()
                    >
                        "Block"
                    </button>
                </form>
            </Panel>
        </div>
    }
}
