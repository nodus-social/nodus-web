//! Canned replies: the sentences a moderator sends over and over.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_client_core::model::WarningPreset;
use nodus_pages::pages::admin::replies::RepliesPage;

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// The presets this instance holds, and the one being written.
#[component]
pub fn Replies() -> impl IntoView {
    let alive = Alive::new();
    let (presets, set_presets) = signal(Vec::<WarningPreset>::new());
    let (title, set_title) = signal(String::new());
    let (text, set_text) = signal(String::new());
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);

    let reload = Callback::new(move |()| {
        spawn_local(async move {
            let outcome = admin::warning_presets().await;
            alive.if_alive(|| {
                set_busy.set(true);
                match outcome {
                    Ok(list) => {
                        set_presets.set(list);
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| reload.run(()));

    let create = Callback::new(move |()| {
        let heading = title.get_untracked().trim().to_owned();
        let body = text.get_untracked().trim().to_owned();
        if heading.is_empty() || body.is_empty() {
            set_error.set(Some(
                "A canned reply needs a title and a sentence.".to_owned(),
            ));
            return;
        }
        spawn_local(async move {
            match admin::create_warning_preset(&heading, &body).await {
                Ok(_) => {
                    alive.if_alive(|| {
                        set_title.set(String::new());
                        set_text.set(String::new());
                        set_error.set(None);
                    });
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let delete = Callback::new(move |id: String| {
        spawn_local(async move {
            if admin::delete_warning_preset(&id).await.is_ok() {
                reload.run(());
            }
        });
    });

    view! {
        <RepliesPage
            presets=presets
            title=title
            text=text
            busy=busy
            error=error
            on_title=Callback::new(move |value: String| set_title.set(value))
            on_text=Callback::new(move |value: String| set_text.set(value))
            on_create=create
            on_delete=delete
            write=Some(crate::state::bridge::admin::may_write())
        />
    }
}
