//! The account section of Settings: the session, the identities this browser holds, and
//! the links to the mute and block lists.
//!
//! Every value arrives as a signal and every change leaves as a callback, so the caller
//! keeps the SDK registry and this crate stays free of storage.

use leptos::prelude::*;

use nodus_ui::html::safe_url;
use nodus_ui::session_style::{NavPlacement, SessionStyle};

use crate::components::panel::Panel;

/// One identity the browser can switch to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountItem {
    pub key: String,
    pub label: String,
    pub host: String,
    pub current: bool,
    pub thumbnail_url: Option<String>,
}

/// Who this browser is signed in as, and the way out.
#[component]
pub fn SessionCard(
    #[prop(into)] host: Signal<String>,
    #[prop(into)] acct: Signal<String>,
    #[prop(optional_no_strip)] on_sign_out: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <Panel title="Session">
            <dl class="grid grid-cols-[8rem_minmax(0,1fr)] gap-x-3 gap-y-1 text-sm">
                <dt class="opacity-60">"Instance"</dt>
                <dd class="truncate">{move || host.get()}</dd>
                <dt class="opacity-60">"Account"</dt>
                <dd class="truncate">{move || acct.get()}</dd>
            </dl>
            <div class="card-actions mt-2 justify-end">
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=on_sign_out.is_none()
                    on:click=move |_| {
                        if let Some(on_sign_out) = on_sign_out {
                            on_sign_out.run(());
                        }
                    }
                >
                    "Sign out"
                </button>
            </div>
            <p class="text-xs opacity-60">
                "Signing out forgets the token stored in this browser."
            </p>
        </Panel>
    }
}

/// Which identities this browser holds, and which one is on screen.
///
/// `on_add` receives the host to sign the extra account in on; an empty string means "the one
/// on screen", which is the case for an alt account.
///
/// The control is painted even when the caller has no verb for it — a server-painted tree shows
/// the same card with the button disabled, exactly as it does for "Sign out of this account".
/// Hiding it instead would make the two renders tell different stories about what the page has.
#[component]
pub fn AccountsCard(
    #[prop(into)] accounts: Signal<Vec<AccountItem>>,
    #[prop(into)] busy: Signal<bool>,
    #[prop(into)] error: Signal<Option<String>>,
    #[prop(optional_no_strip)] on_switch: Option<Callback<String>>,
    #[prop(optional_no_strip)] on_sign_out: Option<Callback<()>>,
    #[prop(optional_no_strip)] on_add: Option<Callback<String>>,
) -> impl IntoView {
    view! {
        <Panel title="Accounts">
            <ul class="flex flex-col gap-2">
                {move || {
                    accounts
                        .get()
                        .into_iter()
                        .map(|account| {
                            view! {
                                <AccountRow
                                    account=account
                                    busy=busy
                                    on_switch=on_switch
                                />
                            }
                        })
                        .collect_view()
                }}
            </ul>
            <p class="text-xs opacity-60">
                "Adding an account signs in to another one without leaving this one. Switching keeps the timelines the other account already loaded."
            </p>
            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error">{move || error.get().unwrap_or_default()}</p>
            </Show>
            <div class="card-actions mt-1 flex-wrap justify-start gap-2">
                <button
                    class="btn btn-primary btn-sm"
                    disabled=move || busy.get() || on_add.is_none()
                    on:click=move |_| {
                        if let Some(on_add) = on_add {
                            on_add.run(String::new());
                        }
                    }
                >
                    "Add another account"
                </button>
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=move || busy.get() || on_sign_out.is_none()
                    on:click=move |_| {
                        if let Some(on_sign_out) = on_sign_out {
                            on_sign_out.run(());
                        }
                    }
                >
                    "Sign out of this account"
                </button>
            </div>
            <AddAccountForm busy=busy on_add=on_add />
        </Panel>
    }
}

/// One stored identity: picture, handle, instance, and the way to it.
#[component]
fn AccountRow(
    account: AccountItem,
    #[prop(into)] busy: Signal<bool>,
    #[prop(optional_no_strip)] on_switch: Option<Callback<String>>,
) -> impl IntoView {
    let AccountItem {
        key,
        label,
        host,
        current,
        thumbnail_url,
    } = account;
    let for_switch = key.clone();
    let thumb = thumbnail_url
        .as_deref()
        .filter(|url| safe_url(url))
        .map(str::to_owned);
    let initial = label
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .map_or('@', |ch| ch.to_ascii_uppercase())
        .to_string();
    let alt_text = format!("{label} session thumbnail");
    let control = if current {
        view! {
            <span class="badge badge-primary badge-sm">"on screen"</span>
        }
        .into_any()
    } else {
        view! {
            <button
                class="btn btn-ghost btn-xs"
                disabled=move || busy.get()
                on:click=move |_| {
                    if let Some(on_switch) = on_switch {
                        on_switch.run(for_switch.clone());
                    }
                }
            >
                "Switch"
            </button>
        }
        .into_any()
    };
    view! {
        <li class="flex items-center justify-between gap-3 rounded-box bg-base-200 px-3 py-2">
            <div class="flex min-w-0 items-center gap-3">
                {if let Some(url) = thumb {
                    view! {
                        <img
                            src=url
                            alt=alt_text
                            class="h-9 w-9 shrink-0 rounded-box object-cover"
                            data-account-thumbnail=""
                        />
                    }
                        .into_any()
                } else {
                    view! {
                        <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-box bg-base-300 text-xs font-semibold opacity-70">
                            {initial}
                        </div>
                    }
                        .into_any()
                }}
                <div class="flex min-w-0 flex-col">
                    <span class="truncate text-sm font-medium">{label}</span>
                    <span class="truncate text-xs opacity-60">{host}</span>
                </div>
            </div>
            {control}
        </li>
    }
}

/// The collapsed form for an account on another instance. It holds its own draft, because a
/// half-typed host is page state, not a preference worth lifting.
#[component]
fn AddAccountForm(
    #[prop(into)] busy: Signal<bool>,
    #[prop(optional_no_strip)] on_add: Option<Callback<String>>,
) -> impl IntoView {
    let (other_host, set_other_host) = signal(String::new());
    view! {
        <details class="mt-1 border-t border-base-300 pt-2">
            <summary class="cursor-pointer text-xs opacity-70">
                "Add an account from a different instance"
            </summary>
            <div class="mt-2 flex flex-wrap items-end gap-2">
                <label class="form-control w-full max-w-xs">
                    <span class="label-text mb-1 text-xs">"Instance"</span>
                    <input
                        class="input input-bordered input-sm w-full"
                        type="text"
                        placeholder="mastodon.social"
                        autocomplete="off"
                        spellcheck="false"
                        prop:value=move || other_host.get()
                        on:input=move |event| set_other_host.set(event_target_value(&event))
                    />
                </label>
                <button
                    class="btn btn-outline btn-sm"
                    disabled=move || busy.get() || on_add.is_none() || other_host.get().trim().is_empty()
                    on:click=move |_| {
                        if let Some(on_add) = on_add {
                            on_add.run(other_host.get());
                        }
                    }
                >
                    "Continue"
                </button>
            </div>
        </details>
    }
}

/// Active account session styling summary with an Edit shortcut to `/settings/styling`.
#[component]
pub fn SessionStyleCard(
    #[prop(into)] style: Signal<SessionStyle>,
    #[prop(optional_no_strip)] on_edit: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <Panel title="Session style">
            <p class="text-sm opacity-70">
                "Custom top, left, and right session images and navigation arrangement for this account."
            </p>
            {move || {
                let current = style.get();
                let thumb = current.thumbnail_url().map(str::to_owned);
                let top_count = current.top.slots.len();
                let left_count = current.left.slots.len();
                let right_count = current.right.slots.len();
                let right_nav_count = current.nav.items_in(NavPlacement::Right).len();
                let submenu_count = current.nav.items_in(NavPlacement::Submenu).len();
                view! {
                    <div class="flex flex-wrap items-center gap-3 rounded-box bg-base-200 p-3">
                        {if let Some(url) = thumb {
                            view! {
                                <img
                                    src=url
                                    alt="Active session style thumbnail"
                                    class="h-12 w-20 rounded-box object-cover"
                                />
                            }
                                .into_any()
                        } else {
                            view! {
                                <div class="flex h-12 w-20 items-center justify-center rounded-box bg-base-300 text-xs opacity-60">
                                    "Default"
                                </div>
                            }
                                .into_any()
                        }}
                        <div class="flex min-w-0 flex-1 flex-wrap gap-2 text-xs">
                            <span class="badge badge-ghost badge-sm">
                                {format!("Top: {top_count}")}
                            </span>
                            <span class="badge badge-ghost badge-sm">
                                {format!("Left: {left_count}")}
                            </span>
                            <span class="badge badge-ghost badge-sm">
                                {format!("Right: {right_count}")}
                            </span>
                            <span class="badge badge-ghost badge-sm">
                                {format!("Right nav: {right_nav_count}")}
                            </span>
                            <span class="badge badge-ghost badge-sm">
                                {format!("Submenu: {submenu_count}")}
                            </span>
                        </div>
                        <a
                            class="btn btn-primary btn-sm"
                            href="/settings/styling"
                            data-edit-session-style=""
                            on:click=move |event| {
                                if let Some(on_edit) = on_edit {
                                    event.prevent_default();
                                    on_edit.run(());
                                }
                            }
                        >
                            "Edit"
                        </a>
                    </div>
                }
            }}
        </Panel>
    }
}

/// The shortcuts to the mute and block lists.
#[component]
pub fn ModerationLinks(
    #[prop(optional_no_strip)] on_mutes: Option<Callback<()>>,
    #[prop(optional_no_strip)] on_blocks: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <Panel title="Moderation">
            <p class="text-sm opacity-70">
                "Accounts you have muted or blocked stay hidden until you undo it here."
            </p>
            <div class="card-actions justify-start gap-2">
                <a
                    class="btn btn-outline btn-sm"
                    href="/mutes"
                    on:click=move |event| {
                        if let Some(on_mutes) = on_mutes {
                            event.prevent_default();
                            on_mutes.run(());
                        }
                    }
                >
                    "Muted accounts"
                </a>
                <a
                    class="btn btn-outline btn-sm"
                    href="/blocks"
                    on:click=move |event| {
                        if let Some(on_blocks) = on_blocks {
                            event.prevent_default();
                            on_blocks.run(());
                        }
                    }
                >
                    "Blocked accounts"
                </a>
            </div>
        </Panel>
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use nodus_ui::session_style::ImageSlot;

    #[test]
    fn accounts_card_displays_safe_session_thumbnails_and_drops_unsafe_ones() {
        let accounts = Signal::stored(vec![
            AccountItem {
                key: String::from("a1"),
                label: String::from("@alice"),
                host: String::from("social.example"),
                current: true,
                thumbnail_url: Some(String::from("https://cdn.example/alice-banner.png")),
            },
            AccountItem {
                key: String::from("b2"),
                label: String::from("@bob"),
                host: String::from("other.example"),
                current: false,
                thumbnail_url: Some(String::from("javascript:alert(1)")),
            },
        ]);
        let no_error = Signal::stored(None::<String>);
        let html = view! {
            <AccountsCard
                accounts=accounts
                busy=false
                error=no_error
            />
        }
        .to_html();

        assert!(html.contains("https://cdn.example/alice-banner.png"));
        assert!(html.contains("data-account-thumbnail"));
        assert!(!html.contains("javascript:alert(1)"));
    }

    #[test]
    fn the_accounts_card_paints_the_add_control_disabled_without_a_verb() {
        let accounts = Signal::stored(vec![AccountItem {
            key: String::from("a1"),
            label: String::from("@alice"),
            host: String::from("social.example"),
            current: true,
            thumbnail_url: None,
        }]);
        let no_error = Signal::stored(None::<String>);

        // The server paints the same card; it simply has no verb to run, so the controls
        // say so instead of disappearing.
        let painted = view! {
            <AccountsCard accounts=accounts busy=false error=no_error />
        }
        .to_html();
        assert!(painted.contains("Add another account"));
        assert!(painted.contains("Add an account from a different instance"));
        assert!(painted.contains("disabled"));
        assert!(painted.contains("on screen"));

        let live = view! {
            <AccountsCard
                accounts=accounts
                busy=false
                error=no_error
                on_add=Some(Callback::new(|_: String| {}))
            />
        }
        .to_html();
        assert!(live.contains("Add another account"));
        assert!(live.contains("Sign out of this account"));
    }

    #[test]
    fn session_style_card_renders_edit_link_to_styling_page() {
        let mut style = SessionStyle::default();
        style
            .top
            .slots
            .push(ImageSlot::new("https://cdn.example/top-thumb.png", None, None).unwrap());
        let html = view! {
            <SessionStyleCard style=style/>
        }
        .to_html();

        assert!(html.contains(r#"href="/settings/styling""#));
        assert!(html.contains("Edit"));
        assert!(html.contains("https://cdn.example/top-thumb.png"));
    }
}
