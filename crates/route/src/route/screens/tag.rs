//! One hashtag: its header, the follow verb, and the posts carrying the tag.
//!
//! The name comes from the address, so the screen is linkable and reloadable. The state of
//! the follow button is whatever the instance answered, never a local guess: a tag that was
//! followed by another client, or a write refused for scope, must show the truth.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_client_core::model::Tag;
use nodus_web::shared::router::Route;

use super::super::{Runtime, TimelineKind, TimelineScreen};
use crate::components::CardRenderer;

/// What a 404 from the tag API means here: this instance has no such tag.
///
/// A name nobody has used is not a failure of the read, so it is a note beside the screen
/// rather than an error toast, and the follow button stays away.
pub const NO_SUCH_TAG: &str = "This instance has no such tag.";

/// The tag screen: header, follow toggle, timeline.
#[component]
pub fn TagScreen(runtime: Arc<dyn Runtime>, name: String, card: CardRenderer) -> impl IntoView {
    let alive = crate::Alive::new();
    let tag = RwSignal::new(None::<Tag>);
    let absent = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let notice = RwSignal::new(None::<String>);
    // The button is a capability of the paint (the browser can write) plus a session (whose
    // tag the write is). A stateless render has neither, so it draws no verb at all.
    let can_follow = runtime.can_follow_tags() && runtime.session().is_some();

    let asked = StoredValue::new(String::new());
    {
        let runtime = Arc::clone(&runtime);
        let name = name.clone();
        Effect::new(move |_| {
            if asked.get_value() == name {
                return;
            }
            asked.set_value(name.clone());
            let done = Callback::new(move |result: Result<Option<Tag>, String>| {
                alive.if_alive(|| match result {
                    Ok(Some(found)) => tag.set(Some(found)),
                    Ok(None) => {
                        absent.set(true);
                        notice.set(Some(NO_SUCH_TAG.to_owned()));
                    }
                    Err(message) => notice.set(Some(message)),
                });
            });
            runtime.load_tag(name.clone(), done);
        });
    }

    let toggle = Callback::new({
        let runtime = Arc::clone(&runtime);
        let name = name.clone();
        move |()| {
            if busy.get_untracked() {
                return;
            }
            let on = !tag.get_untracked().is_some_and(|found| found.following);
            busy.set(true);
            notice.set(None);
            let done = Callback::new(move |result: Result<Option<Tag>, String>| {
                alive.if_alive(|| {
                    busy.set(false);
                    match result {
                        // The instance's own answer, `following` included: the button can
                        // only ever show what the server says it did.
                        Ok(Some(found)) => tag.set(Some(found)),
                        Ok(None) => {
                            absent.set(true);
                            notice.set(Some(NO_SUCH_TAG.to_owned()));
                        }
                        Err(message) => notice.set(Some(message)),
                    }
                });
            });
            runtime.follow_tag(name.clone(), on, done);
        }
    });

    let home = Route::Home.path();
    let header: ChildrenFn = Arc::new(move || {
        view! {
            <a class="link link-hover text-sm opacity-70" href=home.clone()>
                "← Home"
            </a>
        }
        .into_any()
    });

    let tabs: ChildrenFn = Arc::new(move || {
        view! {
            {move || {
                (can_follow && !absent.get()).then(|| {
                    view! {
                        <button
                            class="btn btn-primary btn-sm"
                            disabled=move || busy.get()
                            on:click=move |_| toggle.run(())
                        >
                            {move || {
                                if tag.get().is_some_and(|found| found.following) {
                                    "Unfollow"
                                } else {
                                    "Follow"
                                }
                            }}
                        </button>
                    }
                })
            }}
        }
        .into_any()
    });

    let banner: ChildrenFn = Arc::new(move || {
        view! {
            {move || notice.get().map(|message| view! { <crate::Note>{message.clone()}</crate::Note> })}
        }
        .into_any()
    });

    view! {
        <TimelineScreen
            runtime
            kind=TimelineKind::Tag(name)
            card
            header=Some(header)
            banner=Some(banner)
            tabs=Some(tabs)
        />
    }
}
