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
    let (editing, set_editing) = signal(None::<String>);
    let (edit_title, set_edit_title) = signal(String::new());
    let (edit_text, set_edit_text) = signal(String::new());
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

    // Editing reuses the preset's identity instead of replacing it: a warning already sent
    // refers to it, and deleting would break that reference.
    let begin_edit = Callback::new(move |preset: WarningPreset| {
        set_editing.set(Some(preset.id));
        set_edit_title.set(preset.title);
        set_edit_text.set(preset.text);
    });

    let cancel_edit = Callback::new(move |()| set_editing.set(None));

    let save_edit = Callback::new(move |()| {
        let Some(id) = editing.get_untracked() else {
            return;
        };
        let heading = edit_title.get_untracked().trim().to_owned();
        let body = edit_text.get_untracked().trim().to_owned();
        if heading.is_empty() || body.is_empty() {
            set_error.set(Some(
                "A canned reply needs a title and a sentence.".to_owned(),
            ));
            return;
        }
        spawn_local(async move {
            match admin::update_warning_preset(&id, &heading, &body).await {
                Ok(_) => {
                    alive.if_alive(|| {
                        set_editing.set(None);
                        set_error.set(None);
                    });
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
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
            editing=editing
            edit_title=edit_title
            edit_text=edit_text
            on_edit=begin_edit
            on_cancel_edit=cancel_edit
            on_save_edit=save_edit
            on_edit_title=Callback::new(move |value: String| set_edit_title.set(value))
            on_edit_text=Callback::new(move |value: String| set_edit_text.set(value))
            write=Some(crate::state::bridge::admin::may_write())
        />
    }
}
