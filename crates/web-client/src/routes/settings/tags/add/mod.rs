//! The "feature a tag" form.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::pages::settings::form::FeaturedTagField;

use crate::state::alive::Alive;
use crate::state::bridge::actions;

/// Turns a typed tag into a featured one.
#[component]
pub fn AddTag(
    busy: ReadSignal<bool>,
    set_busy: WriteSignal<bool>,
    set_error: WriteSignal<Option<String>>,
    on_added: Callback<()>,
) -> impl IntoView {
    let (name, set_name) = signal(String::new());
    let (suggestions, set_suggestions) = signal(Vec::<String>::new());
    let alive = Alive::new();

    // Read once: a suggestion is an offer, and re-reading it after every addition would
    // spend a request on a list the reader is already looking at.
    Effect::new(move |_| {
        if !suggestions.get_untracked().is_empty() {
            return;
        }
        spawn_local(async move {
            let offered = actions::featured_tag_suggestions().await;
            alive.if_alive(|| set_suggestions.set(offered));
        });
    });

    let add = move || {
        let tag = name
            .get_untracked()
            .trim()
            .trim_start_matches('#')
            .to_owned();
        if tag.is_empty() {
            return;
        }
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            match actions::feature_tag(&tag).await {
                Ok(()) => {
                    alive.if_alive(|| {
                        set_name.set(String::new());
                        set_busy.set(false);
                        suggestions_forget(&set_suggestions)(&tag);
                    });
                    on_added.run(());
                }
                Err(message) => alive.if_alive(|| {
                    set_error.set(Some(message));
                    set_busy.set(false);
                }),
            }
        });
    };

    let suggest = Callback::new(move |tag: String| {
        set_busy.set(true);
        set_error.set(None);
        spawn_local(async move {
            match actions::feature_tag(&tag).await {
                Ok(()) => {
                    alive.if_alive(|| {
                        set_busy.set(false);
                        suggestions_forget(&set_suggestions)(&tag);
                    });
                    on_added.run(());
                }
                Err(message) => alive.if_alive(|| {
                    set_error.set(Some(message));
                    set_busy.set(false);
                }),
            }
        });
    });

    view! {
        <div class="flex flex-col gap-2">
            <FeaturedTagField
                name=Signal::derive(move || name.get())
                busy=Signal::derive(move || busy.get())
                on_name=Some(Callback::new(move |value: String| set_name.set(value)))
                on_add=Some(Callback::new(move |()| add()))
            />
            <Show when=move || !suggestions.get().is_empty()>
                <div class="flex flex-wrap items-center gap-1" data-tag-suggestions>
                    <span class="text-xs opacity-70">"Suggested:"</span>
                    <For
                        each=move || suggestions.get()
                        key=|tag| tag.clone()
                        children=move |tag| {
                            let label = tag.clone();
                            let pick = tag.clone();
                            view! {
                                <button
                                    type="button"
                                    class="btn btn-ghost btn-xs"
                                    data-tag-suggestion
                                    disabled=move || busy.get()
                                    on:click=move |_| suggest.run(pick.clone())
                                >
                                    "#" {label}
                                </button>
                            }
                        }
                    />
                </div>
            </Show>
        </div>
    }
}

/// Removes a tag from the offered list, so a suggestion the reader just took stops
/// offering itself while the profile list behind the form catches up.
fn suggestions_forget(set: &WriteSignal<Vec<String>>) -> impl Fn(&str) + '_ {
    move |taken: &str| {
        set.update(|offered| offered.retain(|tag| !tag.eq_ignore_ascii_case(taken)));
    }
}
