//! The sessions settings section: every browser signed in to the account.
//!
//! A session row shows what a person can recognise — who the client says it is, when it
//! was last seen, and the network the instance is willing to disclose — because the raw
//! `User-Agent` is a string only its author can love. The current session is marked and
//! its revoke control is disabled with a reason: revoking the browser you are reading in
//! would sign you out mid-sentence, so the server keeps that choice for the sign-out verb.

use leptos::prelude::*;

use crate::components::panel::Panel;

/// One session, reduced to the lines the row draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionItem {
    /// Stable identifier the revoke verb takes.
    pub id: String,
    /// Short descriptor such as "Firefox on Linux".
    pub client: String,
    /// How long ago it was created, in words ("3d ago").
    pub created: String,
    /// How long ago it last made a request, or "never".
    pub last_used: String,
    /// The network as the server disclosed it, or the fact that it did not.
    pub network: String,
    /// Whether this is the session making the request.
    pub current: bool,
}

/// A readable name for a `User-Agent`, or the fact that it is not one we know.
///
/// The order matters: every Chromium browser also claims Chrome and Safari, and every
/// iOS browser claims Safari, so the most specific token is tested first. An agent we
/// cannot name is said to be unknown rather than echoed: `curl/8.5.0` is not a browser
/// name a reader asked to see.
#[must_use]
pub fn describe_agent(agent: &str) -> String {
    if agent.trim().is_empty() {
        return "Unknown client".to_owned();
    }
    let browser = if agent.contains("Edg/") || agent.contains("Edge/") {
        "Edge"
    } else if agent.contains("OPR/") || agent.contains("Opera") {
        "Opera"
    } else if agent.contains("Firefox/") || agent.contains("FxiOS") {
        "Firefox"
    } else if agent.contains("CriOS") || agent.contains("Chrome/") {
        "Chrome"
    } else if agent.contains("Safari/") {
        "Safari"
    } else {
        return "Unknown client".to_owned();
    };
    let system = if agent.contains("Android") {
        "Android"
    } else if agent.contains("iPhone") || agent.contains("iPad") || agent.contains("iPod") {
        "iOS"
    } else if agent.contains("Windows") {
        "Windows"
    } else if agent.contains("Mac OS X") || agent.contains("Macintosh") {
        "macOS"
    } else if agent.contains("CrOS") {
        "ChromeOS"
    } else if agent.contains("Linux") || agent.contains("X11") {
        "Linux"
    } else {
        "an unknown system"
    };
    format!("{browser} on {system}")
}

/// The sentence a current row carries, if it is the current one.
#[must_use]
pub fn current_marker(current: bool) -> Option<&'static str> {
    current.then_some("Current session")
}

/// Why the revoke control on a row is disabled, if it is.
///
/// A reason rather than a bare `disabled`: the reader can see the control, so the page
/// owes them the sentence that says why it is not the one to use.
#[must_use]
pub fn revoke_blocked_reason(current: bool) -> Option<&'static str> {
    current.then_some("You cannot revoke the session you are using.")
}

/// Every signed-in session, and the two ways to end them.
// A `must_use` on the component cannot survive the macro rewrite, so the lint is
// answered here rather than by an attribute that disappears.
#[allow(clippy::must_use_candidate)]
#[component]
pub fn SessionsCard(
    #[prop(into)] sessions: Signal<Vec<SessionItem>>,
    #[prop(into)] busy: Signal<bool>,
    #[prop(into)] error: Signal<Option<String>>,
    #[prop(optional_no_strip)] on_revoke: Option<Callback<String>>,
    #[prop(optional_no_strip)] on_revoke_others: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <Panel title="Sessions">
            <p class="text-sm opacity-70">
                "Each browser or app signed in to your account. Revoking one ends it there; this browser stays signed in."
            </p>

            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error">{move || error.get().unwrap_or_default()}</p>
            </Show>

            <ul class="flex flex-col gap-2">
                {move || {
                    sessions
                        .get()
                        .into_iter()
                        .map(|session| {
                            let id = session.id;
                            let reason = revoke_blocked_reason(session.current);
                            let marker = current_marker(session.current);
                            view! {
                                <li
                                    class="flex items-start justify-between gap-2 rounded-box bg-base-200 px-3 py-2"
                                    data-session-row
                                    data-current=session.current.to_string()
                                >
                                    <div class="min-w-0">
                                        <p class="flex flex-wrap items-center gap-2 text-sm font-medium">
                                            <span class="truncate">{session.client}</span>
                                            {marker.map(|marker| view! {
                                                <span class="badge badge-sm badge-primary">{marker}</span>
                                            })}
                                        </p>
                                        <p class="text-xs opacity-70">
                                            "Created "{session.created}" · Last used "{session.last_used}
                                        </p>
                                        <p class="truncate text-xs opacity-60">{session.network}</p>
                                    </div>
                                    <button
                                        class="btn btn-ghost btn-xs"
                                        disabled=move || busy.get() || reason.is_some() || on_revoke.is_none()
                                        title=reason.unwrap_or_default()
                                        on:click=move |_| {
                                            if let Some(on_revoke) = on_revoke {
                                                on_revoke.run(id.clone());
                                            }
                                        }
                                    >
                                        "Revoke"
                                    </button>
                                </li>
                            }
                        })
                        .collect_view()
                }}
            </ul>

            <Show when=move || sessions.get().is_empty() && !busy.get()>
                <p class="text-sm opacity-70">"No sessions to show."</p>
            </Show>

            <div class="card-actions mt-1 justify-end">
                <button
                    class="btn btn-ghost btn-sm"
                    disabled=move || {
                        busy.get()
                            || on_revoke_others.is_none()
                            || sessions.get().iter().all(|session| session.current)
                    }
                    on:click=move |_| {
                        if let Some(on_revoke_others) = on_revoke_others {
                            on_revoke_others.run(());
                        }
                    }
                >
                    "Revoke all other sessions"
                </button>
            </div>
        </Panel>
    }
}

#[cfg(test)]
mod tests {
    use super::{current_marker, describe_agent, revoke_blocked_reason};

    #[test]
    fn a_firefox_agent_reads_as_firefox_on_linux() {
        assert_eq!(
            describe_agent(
                "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0"
            ),
            "Firefox on Linux"
        );
    }

    #[test]
    fn chromium_browsers_do_not_get_named_safari() {
        assert_eq!(
            describe_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"
            ),
            "Chrome on macOS"
        );
        assert_eq!(
            describe_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 Edg/126.0.0.0"
            ),
            "Edge on Windows"
        );
    }

    #[test]
    fn an_iphone_browser_is_safari_on_ios() {
        assert_eq!(
            describe_agent(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1"
            ),
            "Safari on iOS"
        );
    }

    #[test]
    fn an_unknown_or_garbage_agent_is_not_echoed() {
        assert_eq!(describe_agent(""), "Unknown client");
        assert_eq!(describe_agent("curl/8.5.0"), "Unknown client");
        assert_eq!(
            describe_agent("Mozilla/5.0 (compatible; SomeBot/2.1)"),
            "Unknown client"
        );
    }

    #[test]
    fn only_the_current_session_is_marked() {
        assert_eq!(current_marker(true), Some("Current session"));
        assert_eq!(current_marker(false), None);
    }

    #[test]
    fn the_current_session_explains_why_revoke_is_disabled() {
        assert_eq!(
            revoke_blocked_reason(true),
            Some("You cannot revoke the session you are using.")
        );
        assert_eq!(revoke_blocked_reason(false), None);
    }
}
