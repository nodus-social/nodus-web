//! The drive screen, wired to the instance.
//!
//! The page is handed the four facts it paints and the nine verbs it offers; this module
//! owns the signals those come from, the folder the reader has open, and the file input
//! that only the browser can provide. A write reloads the whole screen rather than patching
//! the row it touched: `used_bytes` is deduplicated, so an upload or a delete changes a
//! number no row carries, and a screen that guessed it would drift.
//!
//! The mount flag is the component's, made once in the body, not one per callback:
//! `Alive::new` registers its release with the reactive owner current when it runs, and a
//! callback invoked from inside a `<Show>` block runs under that block's owner. The
//! confirmation buttons close their block in the same tick as the write, so a flag made
//! there is released before the reply lands — every continuation after the await is then
//! dropped, and the row, the meter and `busy` all keep the values they had.

use std::rc::Rc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use nodus_pages::pages::drive::{DrivePage, FileRow, FolderRow};

use crate::state::alive::Alive;
use crate::state::bridge::drive;

#[component]
pub fn DriveScreen() -> impl IntoView {
    let (used, set_used) = signal(0_i64);
    let (objects, set_objects) = signal(0_i64);
    let (tree, set_tree) = signal(Vec::<drive::Folder>::new());
    let (files, set_files) = signal(Vec::<drive::DriveFile>::new());
    let (current, set_current) = signal(None::<String>);
    let (tick, set_tick) = signal(0_u32);
    let (busy, set_busy) = signal(None::<String>);
    let (error, set_error) = signal(None::<String>);
    let (upload_visibility, set_upload_visibility) = signal("private".to_owned());

    // One flag for the screen. See the module note: a flag made inside a callback
    // belongs to whatever block invoked it, and that block is often gone by the reply.
    let alive = Alive::new();

    let reload = Callback::new(move |()| set_tick.update(|value| *value = value.wrapping_add(1)));

    Effect::new(move |_| {
        tick.get();
        let folder = current.get();
        set_busy.set(Some("Reading the drive…".to_owned()));
        spawn_local(async move {
            match drive::load(folder.as_deref()).await {
                Ok((summary, files, tree)) => alive.if_alive(|| {
                    set_used.set(summary.used_bytes);
                    set_objects.set(summary.objects);
                    set_files.set(files);
                    set_tree.set(tree);
                    set_error.set(None);
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
            alive.if_alive(|| set_busy.set(None));
        });
    });

    let open_folder = Callback::new(move |folder: Option<String>| set_current.set(folder));

    let create_folder = Callback::new(move |name: String| {
        let parent = current.get_untracked();
        set_busy.set(Some("Making the folder…".to_owned()));
        spawn_local(async move {
            match drive::create_folder(&name, parent.as_deref()).await {
                Ok(()) => alive.if_alive(|| {
                    set_error.set(None);
                    reload.run(());
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
            alive.if_alive(|| set_busy.set(None));
        });
    });

    let rename_folder = Callback::new(move |(id, name): (String, String)| {
        spawn_local(async move {
            match drive::rename_folder(&id, &name).await {
                Ok(()) => alive.if_alive(|| {
                    set_error.set(None);
                    reload.run(());
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let delete_folder = Callback::new(move |id: String| {
        let open = current.get_untracked();
        set_busy.set(Some("Deleting the folder…".to_owned()));
        spawn_local(async move {
            match drive::delete_folder(&id).await {
                Ok(()) => alive.if_alive(|| {
                    if open.as_deref() == Some(id.as_str()) {
                        set_current.set(None);
                    }
                    set_error.set(None);
                    reload.run(());
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
            alive.if_alive(|| set_busy.set(None));
        });
    });

    let rename_file = Callback::new(move |(id, name): (String, String)| {
        spawn_local(async move {
            match drive::rename_file(&id, &name).await {
                Ok(()) => alive.if_alive(|| {
                    set_error.set(None);
                    reload.run(());
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let move_file = Callback::new(move |(id, folder): (String, Option<String>)| {
        spawn_local(async move {
            match drive::move_file(&id, folder.as_deref()).await {
                Ok(()) => alive.if_alive(|| {
                    set_error.set(None);
                    reload.run(());
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let visibility = Callback::new(move |(id, value): (String, String)| {
        spawn_local(async move {
            match drive::set_visibility(&id, &value).await {
                Ok(()) => alive.if_alive(|| {
                    set_error.set(None);
                    reload.run(());
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let delete_file = Callback::new(move |id: String| {
        set_busy.set(Some("Deleting the file…".to_owned()));
        spawn_local(async move {
            match drive::delete_file(&id).await {
                Ok(()) => alive.if_alive(|| {
                    set_error.set(None);
                    reload.run(());
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
            alive.if_alive(|| set_busy.set(None));
        });
    });

    let download = Callback::new(move |id: String| {
        let Some(file) = files
            .get_untracked()
            .into_iter()
            .find(|file| file.id == id)
        else {
            return;
        };
        spawn_local(async move {
            if let Err(message) = drive::download(&file).await {
                alive.if_alive(|| set_error.set(Some(message)));
            }
        });
    });

    let upload = {
        let on_change = move |event: web_sys::Event| {
            let Some(input) = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
            else {
                return;
            };
            let Some(list) = input.files() else {
                return;
            };
            let picked: Vec<web_sys::File> = (0..list.length())
                .filter_map(|index| list.get(index))
                .collect();
            input.set_value("");
            if picked.is_empty() {
                return;
            }
            let folder = current.get_untracked();
            let visibility = upload_visibility.get_untracked();
            spawn_local(async move {
                let total = picked.len();
                for (index, file) in picked.into_iter().enumerate() {
                    let name = file.name();
                    set_busy
                        .set(Some(format!("Uploading {name} ({} of {total})…", index + 1)));
                    let progress = {
                        let name = name.clone();
                        let ticker = set_busy;
                        Rc::new(move |percent: u8| {
                            ticker.set(Some(format!("Uploading {name} — {percent}%")));
                        })
                    };
                    if let Err(message) =
                        drive::upload(&file, folder.as_deref(), &visibility, progress).await
                    {
                        alive.if_alive(|| set_error.set(Some(message)));
                    } else {
                        alive.if_alive(|| set_error.set(None));
                    }
                }
                alive.if_alive(|| {
                    set_busy.set(None);
                    reload.run(());
                });
            });
        };
        view! {
            <div class="flex flex-wrap items-center gap-1">
                <select
                    class="select select-xs select-bordered"
                    aria-label="Visibility of new uploads"
                    prop:value=move || upload_visibility.get()
                    on:change=move |event| set_upload_visibility.set(event_target_value(&event))
                >
                    <option value="private">"Private"</option>
                    <option value="unlisted_link">"Link only"</option>
                    <option value="public_federated">"Public"</option>
                </select>
                <label class="btn btn-primary btn-sm">
                    "Upload"
                    <input
                        type="file"
                        multiple
                        class="hidden"
                        aria-label="Upload files"
                        on:change=on_change
                    />
                </label>
            </div>
        }
        .into_any()
    };

    let folder_rows = Signal::derive(move || {
        let mut flat = Vec::new();
        drive::flatten(&tree.get(), 0, &mut flat);
        flat.into_iter()
            .map(|folder| FolderRow {
                id: folder.id,
                name: folder.name,
                depth: folder.depth,
            })
            .collect::<Vec<_>>()
    });

    let file_rows = Signal::derive(move || {
        files
            .get()
            .into_iter()
            .map(|file| FileRow {
                id: file.id,
                filename: file.filename,
                mime_type: file.mime_type,
                size: drive::size(file.byte_size),
                visibility: file.visibility,
                processing_state: file.processing_state,
            })
            .collect::<Vec<_>>()
    });

    view! {
        <DrivePage
            used=Signal::derive(move || used.get())
            objects=Signal::derive(move || objects.get())
            folders=folder_rows
            files=file_rows
            current=Signal::derive(move || current.get())
            error=Signal::derive(move || error.get())
            busy=Signal::derive(move || busy.get())
            upload=upload
            write=drive::may_write()
            on_open_folder=open_folder
            on_create_folder=create_folder
            on_rename_folder=rename_folder
            on_delete_folder=delete_folder
            on_rename_file=rename_file
            on_move_file=move_file
            on_visibility=visibility
            on_delete_file=delete_file
            on_download=download
        />
    }
}
