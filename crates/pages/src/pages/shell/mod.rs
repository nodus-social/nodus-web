//! The envelope a route gets when the server has the chrome but not the data.
//!
//! The heading and the section tabs are the same pieces the client mounts. No placeholder is
//! painted under them: a card cannot know the height of the rows it fronts, and a screen that
//! paints an empty box teaches the reader to expect one.

use leptos::prelude::*;

use crate::components::chrome::tab_list::Tab;
use crate::{PageHeader, TabList, TabListProps};

/// A heading and its section tabs; the screen's own body arrives as the caller's content.
#[component]
pub fn ShellPage(
    #[prop(into)] title: String,
    #[prop(into)] description: String,
    tabs: Vec<Tab>,
    #[prop(into)] active: String,
    #[prop(optional_no_strip)] on_navigate: Option<Callback<String>>,
    /// The screen itself, once its data is there.
    children: ChildrenFn,
) -> impl IntoView {
    let tabs = StoredValue::new(tabs);
    let children = StoredValue::new(children);
    view! {
        <div class="mx-auto flex w-full flex-col gap-3 px-2 max-w-2xl">
            <PageHeader title=title description=description/>
            <Show when=move || tabs.with_value(|tabs| !tabs.is_empty())>
                {TabList(TabListProps {
                    tabs: tabs.get_value(),
                    active: active.clone(),
                    on_navigate,
                })}
            </Show>
            {children.with_value(|children| children())}
        </div>
    }
}

/// The timeline's refusal, with the heading the client uses for Home.
#[component]
pub fn TimelineError(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <div class="mx-auto flex w-full flex-col gap-3 px-2 max-w-2xl">
            <h1 class="text-xl font-semibold tracking-tight">"Home"</h1>
            <p class="alert alert-error mt-2 text-sm" role="alert">
                "Could not read the timeline: "
                {message}
            </p>
        </div>
    }
}
