//! The browser push card: whether this browser is subscribed, and what it is told about.
//!
//! Two facts decide what the card can offer, and both come from outside it: whether the
//! browser has a `PushManager` and a service worker at all, and whether the instance
//! advertises `push`. A browser that cannot subscribe is told so in one sentence instead of
//! being shown a toggle that would fail; when the instance does not advertise push the card
//! is not painted (the route decides that), because a missing capability name is the
//! instance's answer, not a browser the reader can fix.
//!
//! Only the four kinds the sender filters on are offered. An alert kind the card cannot name
//! is a switch whose label nobody can check against what arrives.

use leptos::prelude::*;

use crate::components::panel::Panel;

/// The sentence a browser without push support is shown.
pub const UNSUPPORTED: &str =
    "This browser cannot receive push notifications: it has no PushManager or no service worker.";

/// The sentence a client the instance gave no push key is shown. Without a key the browser
/// cannot subscribe, so the card offers no toggle. Spelled the same in
/// `web_client::state::bridge::push::NO_KEY`, which is what a subscribe that raced the card
/// answers with — one sentence reaches the reader either way.
pub const NO_KEY: &str =
    "This instance did not publish a push key to this client, so this browser cannot subscribe.";

/// The kinds of push notification this instance delivers, in the order the card lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushKind {
    Mention,
    Favourite,
    Reblog,
    Follow,
}

impl PushKind {
    /// Every kind, in the order the card lists them.
    pub const ALL: [Self; 4] = [Self::Mention, Self::Favourite, Self::Reblog, Self::Follow];

    /// What the row calls this kind.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Mention => "Mentions",
            Self::Favourite => "Favourites",
            Self::Reblog => "Boosts",
            Self::Follow => "Follows",
        }
    }

    /// The name the wire uses, which is also the name the card's checkbox carries.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Mention => "mention",
            Self::Favourite => "favourite",
            Self::Reblog => "reblog",
            Self::Follow => "follow",
        }
    }
}

/// The push card.
#[component]
pub fn BrowserPushCard(
    /// Whether this browser has a `PushManager` and a service worker.
    #[prop(into)]
    supported: Signal<bool>,
    /// Whether the instance gave this client a push key at all.
    #[prop(into)]
    keyed: Signal<bool>,
    /// Whether the browser currently holds a subscription.
    #[prop(into)]
    subscribed: Signal<bool>,
    /// Whether a subscribe, unsubscribe or alert update is in flight.
    #[prop(into)]
    working: Signal<bool>,
    /// The alert flags, one per kind, in [`PushKind::ALL`] order.
    #[prop(into)]
    alerts: Signal<Vec<(PushKind, bool)>>,
    /// The last refusal, printed in place: a permission the reader denied, or the server's
    /// own sentence when it refused the subscription.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Called with the new subscribed state when the toggle changes.
    #[prop(optional_no_strip)]
    on_toggle: Option<Callback<bool>>,
    /// Called with the kind and its new value when a checkbox changes.
    #[prop(optional_no_strip)]
    on_alert_change: Option<Callback<(PushKind, bool)>>,
) -> impl IntoView {
    view! {
        <Panel
            title="Notifications in this browser"
            description="Receive notifications for new activity even when this tab is not open."
        >
            <Show when=move || !supported.get()>
                <p class="text-sm opacity-70">{UNSUPPORTED}</p>
            </Show>

            <Show when=move || supported.get() && !keyed.get()>
                <p class="text-sm opacity-70">{NO_KEY}</p>
            </Show>

            // Nothing at all when either fact is missing: a disabled toggle invites a click
            // that cannot do anything, and the sentence above already said why.
            <Show when=move || supported.get() && keyed.get()>
            <div class="flex flex-col gap-4">
                <div class="flex items-center justify-between">
                    <span class="text-sm font-medium">"Push notifications in this browser"</span>
                    <input
                        type="checkbox"
                        class="toggle"
                        aria-label="Push notifications in this browser"
                        // A PROPERTY, not an attribute: a refusal puts the value back, and an
                        // attribute write does not move the DOM property the reader sees.
                        prop:checked=move || subscribed.get()
                        disabled=move || working.get()
                        on:change=move |event| {
                            if let Some(on_toggle) = on_toggle {
                                on_toggle.run(event_target_checked(&event));
                            }
                        }
                    />
                </div>

                <Show when=move || working.get()>
                    <p class="text-xs opacity-60" role="status">
                        {move || if subscribed.get() { "Turning off…" } else { "Turning on…" }}
                    </p>
                </Show>

                <Show when=move || error.get().is_some()>
                    <p class="text-sm text-error" role="alert">
                        {move || error.get().unwrap_or_default()}
                    </p>
                </Show>

                <Show when=move || subscribed.get() && !working.get()>
                    <div class="flex flex-col gap-2 rounded-box bg-base-200 p-3">
                        <p class="text-xs font-semibold">"Notify me about"</p>
                        <For
                            each=move || alerts.get()
                            key=|(kind, _)| kind.wire()
                            children=move |(kind, enabled)| {
                                view! {
                                    <label class="flex cursor-pointer items-center gap-2">
                                        <input
                                            type="checkbox"
                                            class="checkbox checkbox-sm"
                                            name=kind.wire()
                                            checked=enabled
                                            on:change=move |event| {
                                                if let Some(on_alert_change) = on_alert_change {
                                                    on_alert_change
                                                        .run((kind, event_target_checked(&event)));
                                                }
                                            }
                                        />
                                        <span class="text-sm">{kind.label()}</span>
                                    </label>
                                }
                            }
                        />
                    </div>
                </Show>
            </div>
            </Show>
        </Panel>
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn painted(supported: bool, keyed: bool, subscribed: bool, error: Option<&str>) -> String {
        view! {
            <BrowserPushCard
                supported=Signal::stored(supported)
                keyed=Signal::stored(keyed)
                subscribed=Signal::stored(subscribed)
                working=Signal::stored(false)
                alerts=Signal::stored(
                    PushKind::ALL.iter().map(|kind| (*kind, true)).collect::<Vec<_>>(),
                )
                error=Signal::stored(error.map(str::to_owned))
                on_toggle=Some(Callback::new(|_| {}))
                on_alert_change=Some(Callback::new(|_| {}))
            />
        }
        .to_html()
    }

    #[test]
    fn a_browser_without_push_is_told_so_in_one_sentence() {
        let html = painted(false, false, false, None);
        assert!(html.contains("cannot receive push notifications"), "{html}");
        assert!(!html.contains("Notify me about"), "{html}");
    }

    #[test]
    fn a_supported_browser_is_offered_the_toggle() {
        let html = painted(true, true, false, None);
        assert!(!html.contains("cannot receive push notifications"), "{html}");
        assert!(html.contains("Push notifications in this browser"), "{html}");
        assert!(!html.contains("Notify me about"), "{html}");
    }

    #[test]
    fn a_client_with_no_push_key_is_told_so_and_offered_no_toggle() {
        let html = painted(true, false, false, None);
        assert!(html.contains("did not publish a push key"), "{html}");
        assert!(!html.contains("Push notifications in this browser"), "{html}");
        assert!(!html.contains("Notify me about"), "{html}");
    }

    #[test]
    fn a_subscription_offers_the_four_kinds() {
        let html = painted(true, true, true, None);
        assert!(html.contains("Notify me about"), "{html}");
        assert!(html.contains("Mentions"), "{html}");
        assert!(html.contains("Favourites"), "{html}");
        assert!(html.contains("Boosts"), "{html}");
        assert!(html.contains("Follows"), "{html}");
    }

    #[test]
    fn a_refusal_is_printed_where_the_card_is() {
        let html = painted(true, true, false, Some("permission was denied"));
        assert!(html.contains("permission was denied"), "{html}");
    }

    #[test]
    fn wire_names_are_the_ones_the_sender_filters_on() {
        assert_eq!(PushKind::Mention.wire(), "mention");
        assert_eq!(PushKind::Favourite.wire(), "favourite");
        assert_eq!(PushKind::Reblog.wire(), "reblog");
        assert_eq!(PushKind::Follow.wire(), "follow");
    }
}
