//! The profile card: the text fields, the metadata rows and the save row.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_pages::pages::settings::form::{ProfileField, ProfileFields, validate_fields};

use super::ProfileImagesCard;
use crate::state::alive::Alive;
use crate::state::bridge::{actions, profile, revision, with_sdk};

/// The bio limit the instance enforces. Shown as a counter rather than enforced on every
/// keystroke: the instance owns the rule, and a paste longer than it should be visibly
/// over budget instead of silently truncated here.
const NOTE_MAX_CHARS: usize = 500;

/// The profile card.
///
/// The fields are seeded once and then owned by the user until Save: writing them on every
/// store change would overwrite what is being typed as soon as any other screen refreshed
/// the account.
#[component]
pub(crate) fn ProfileCard() -> impl IntoView {
    let revision = revision();
    let alive = Alive::new();

    let account = move || {
        revision.get();
        with_sdk(|sdk| {
            sdk.state()
                .session
                .get()
                .and_then(|session| sdk.read_account(&session.account_id))
        })
    };

    let (display_name, set_display_name) = signal(String::new());
    let (note, set_note) = signal(String::new());
    let (fields, set_fields) = signal(Vec::<ProfileField>::new());
    let (field_errors, set_field_errors) = signal(Vec::<Option<String>>::new());
    let (busy, set_busy) = signal(false);
    let (message, set_message) = signal(None::<String>);
    let (seeded, set_seeded) = signal(false);
    let (fetching, set_fetching) = signal(false);

    Effect::new(move |_| {
        if seeded.get() {
            return;
        }
        if let Some(current) = account() {
            set_display_name.set(current.display_name);
            set_note.set(current.note);
            set_seeded.set(true);
            return;
        }
        if fetching.get_untracked() {
            return;
        }
        let Some(id) = with_sdk(|sdk| sdk.state().session.get().map(|session| session.account_id))
        else {
            return;
        };
        set_fetching.set(true);
        spawn_local(async move {
            let answer = actions::account(&id).await;
            alive.if_alive(|| {
                set_fetching.set(false);
                match answer {
                    Ok(current) => {
                        set_display_name.set(current.display_name);
                        set_note.set(current.note);
                        set_seeded.set(true);
                    }
                    Err(message) => set_message.set(Some(message)),
                }
            });
        });
    });

    // The rows are not in the store's account, so they are read once from the instance's own
    // answer, beside the text fields and for the same reason: a later refresh must not
    // overwrite a row being typed.
    let (fields_loaded, set_fields_loaded) = signal(false);
    Effect::new(move |_| {
        if fields_loaded.get() {
            return;
        }
        if with_sdk(|sdk| sdk.state().session.get()).is_none() {
            return;
        }
        spawn_local(async move {
            if let Ok(rows) = profile::fields().await {
                alive.if_alive(|| {
                    set_fields.set(rows);
                    set_fields_loaded.set(true);
                });
            }
        });
    });

    let edit_name = move |(index, value): (usize, String)| {
        set_fields.update(|rows| {
            if let Some(row) = rows.get_mut(index) {
                row.name = value;
            }
        });
    };
    let edit_value = move |(index, value): (usize, String)| {
        set_fields.update(|rows| {
            if let Some(row) = rows.get_mut(index) {
                row.value = value;
                // A mark belongs to the value the instance checked, not to the row: typing a
                // new link makes the old verdict a claim about something that is gone.
                row.verified_at = None;
            }
        });
    };
    let add_row = move |()| {
        set_fields.update(|rows| rows.push(ProfileField::default()));
    };
    let remove_row = move |index: usize| {
        set_fields.update(|rows| {
            if index < rows.len() {
                rows.remove(index);
            }
        });
    };

    let save = move || {
        if busy.get() {
            return;
        }
        let rows = fields.get_untracked();
        let errors = validate_fields(&rows);
        if errors.iter().any(Option::is_some) {
            set_field_errors.set(errors);
            set_message.set(Some("Check the metadata rows above.".to_owned()));
            return;
        }
        set_field_errors.set(Vec::new());
        set_busy.set(true);
        set_message.set(None);
        let display_name = display_name.get_untracked();
        let note = note.get_untracked();
        spawn_local(async move {
            let answer = profile::save(display_name, note, rows).await;
            alive.if_alive(|| {
                set_busy.set(false);
                match answer {
                    Ok(rows) => {
                        set_fields.set(rows);
                        set_message.set(Some("Saved.".to_owned()));
                    }
                    Err(profile::SaveError::Row { row, message }) => {
                        let mut errors = vec![None; fields.get_untracked().len()];
                        if let Some(slot) = errors.get_mut(row) {
                            *slot = Some(message);
                        }
                        set_field_errors.set(errors);
                    }
                    Err(profile::SaveError::Form(message)) => {
                        set_message.set(Some(message));
                    }
                }
            });
        });
    };

    view! {
        <div class="flex flex-col gap-4">
            <nodus_ui::components::panel::Panel title="Profile">
                <ProfileFields
                    display_name=Signal::derive(move || display_name.get())
                    note=Signal::derive(move || note.get())
                    max_note=NOTE_MAX_CHARS
                    busy=Signal::derive(move || busy.get() || fetching.get())
                    message=Signal::derive(move || message.get())
                    on_display_name=Some(Callback::new(move |value: String| set_display_name.set(value)))
                    on_note=Some(Callback::new(move |value: String| set_note.set(value)))
                    on_save=Some(Callback::new(move |()| save()))
                    fields=Some(Signal::derive(move || fields.get()))
                    field_errors=Some(Signal::derive(move || field_errors.get()))
                    on_field_name=Some(Callback::new(edit_name))
                    on_field_value=Some(Callback::new(edit_value))
                    on_add_field=Some(Callback::new(add_row))
                    on_remove_field=Some(Callback::new(remove_row))
                />
            </nodus_ui::components::panel::Panel>
            <ProfileImagesCard/>
        </div>
    }
}
