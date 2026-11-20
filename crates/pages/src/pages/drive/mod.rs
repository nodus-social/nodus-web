//! The drive: what this account keeps, in the folders it keeps it in.
//!
//! The page is drawn from four facts and nine verbs. The facts are the used bytes, the
//! folder tree, the files of the open folder, and where the open folder is; the verbs are
//! the folder's create/rename/delete, the file's rename/move/visibility/delete/download,
//! and the upload. Everything else — the picker's options, the confirmation's sentence,
//! the badge's honesty about a preview — is decided here, because it is what a screen is.
//!
//! There is no quota bar. `GET /api/v1/drive` publishes `used_bytes` and no limit, so the
//! meter says how much is used and stops there; a bar against a denominator the instance
//! never named would be a limit invented in the browser.

use leptos::prelude::*;

use crate::components::panel::Panel;
use crate::{PageHeader, Screen, Width};

/// One file as the list paints it, already formatted by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    /// The id a write names.
    pub id: String,
    /// The name the reader sees and edits.
    pub filename: String,
    /// The media type, for the type column.
    pub mime_type: String,
    /// The size, human-readable.
    pub size: String,
    /// `private`, `unlisted_link` or `public_federated`.
    pub visibility: String,
    /// `pending`, `ready` or `failed`.
    pub processing_state: String,
}

/// One folder as the sidebar paints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    /// The id a write names.
    pub id: String,
    /// The name the reader sees and edits.
    pub name: String,
    /// How far below the root it sits.
    pub depth: usize,
}

fn visibility_label(value: &str) -> &'static str {
    match value {
        "unlisted_link" => "Link only",
        "public_federated" => "Public",
        _ => "Private",
    }
}

fn type_label(mime: &str) -> String {
    if mime.is_empty() {
        "Unknown type".to_owned()
    } else {
        mime.to_owned()
    }
}

/// The drive screen.
#[component]
pub fn DrivePage(
    /// Deduplicated bytes the drive holds.
    #[prop(into)]
    used: Signal<i64>,
    /// How many files it holds.
    #[prop(into)]
    objects: Signal<i64>,
    /// The whole folder tree, parents before children.
    #[prop(into)]
    folders: Signal<Vec<FolderRow>>,
    /// The files of the open folder.
    #[prop(into)]
    files: Signal<Vec<FileRow>>,
    /// The open folder, `None` at the root.
    #[prop(into)]
    current: Signal<Option<String>>,
    /// What went wrong last, if anything.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// What is in flight, if anything.
    #[prop(into)]
    busy: Signal<Option<String>>,
    /// The upload control, which needs the browser and so is handed in.
    #[prop(optional, into)]
    upload: Option<AnyView>,
    /// Whether this session's grant may write. A reader who may only read sees the files
    /// and the download button, and no control that would answer `403`.
    #[prop(default = true)]
    write: bool,
    #[prop(optional)] on_open_folder: Option<Callback<Option<String>>>,
    #[prop(optional)] on_create_folder: Option<Callback<String>>,
    #[prop(optional)] on_rename_folder: Option<Callback<(String, String)>>,
    #[prop(optional)] on_delete_folder: Option<Callback<String>>,
    #[prop(optional)] on_rename_file: Option<Callback<(String, String)>>,
    #[prop(optional)] on_move_file: Option<Callback<(String, Option<String>)>>,
    #[prop(optional)] on_visibility: Option<Callback<(String, String)>>,
    #[prop(optional)] on_delete_file: Option<Callback<String>>,
    #[prop(optional)] on_download: Option<Callback<String>>,
) -> impl IntoView {
    let new_folder = RwSignal::new(false);
    let new_name = RwSignal::new(String::new());
    let renaming_folder = RwSignal::new(None::<(String, String)>);
    let deleting_folder = RwSignal::new(None::<(String, String)>);
    let renaming_file = RwSignal::new(None::<(String, String)>);
    let deleting_file = RwSignal::new(None::<(String, String)>);

    let open_label = move || match current.get() {
        Some(id) => folders
            .get()
            .into_iter()
            .find(|folder| folder.id == id)
            .map_or_else(|| "Folder".to_owned(), |folder| folder.name),
        None => "All files".to_owned(),
    };

    view! {
        <Screen width=Width::Wide>
            <PageHeader
                title="Drive"
                description="Your own files, in folders only you can see, unless you say otherwise."
            />

            {move || {
                error
                    .get()
                    .map(|error| {
                        view! {
                            <p class="text-sm text-error" role="alert">
                                {error}
                            </p>
                        }
                    })
            }}

            <Panel title="Storage" description="What this drive holds.">
                <p class="text-sm" data-testid="drive-used">
                    {move || {
                        format!(
                            "{} used · {} files · {} folders",
                            crate::pages::drive::bytes(used.get()),
                            objects.get(),
                            folders.get().len(),
                        )
                    }}
                </p>
                <p class="text-xs opacity-60">
                    "This instance publishes no quota, so this is the amount used, not a share of a limit."
                </p>
            </Panel>

            <div class="grid gap-3 md:grid-cols-[16rem_1fr]">
                <aside class="card h-fit border border-base-300 bg-base-100">
                    <div class="card-body gap-2 p-3">
                        <div class="flex items-center justify-between gap-2">
                            <h2 class="card-title text-base">"Folders"</h2>
                            <button
                                type="button"
                                class="btn btn-ghost btn-xs"
                                aria-label="New folder"
                                disabled=move || !write
                                on:click=move |_| {
                                    new_folder.update(|open| *open = !*open);
                                }
                            >
                                "New folder"
                            </button>
                        </div>

                        <Show when=move || new_folder.get()>
                            <div class="flex gap-1">
                                <input
                                    class="input input-xs input-bordered w-full"
                                    type="text"
                                    placeholder="Folder name"
                                    aria-label="New folder name"
                                    prop:value=move || new_name.get()
                                    on:input=move |event| {
                                        new_name.set(event_target_value(&event));
                                    }
                                />
                                <button
                                    type="button"
                                    class="btn btn-primary btn-xs"
                                    disabled=move || {
                                        on_create_folder.is_none() || new_name.get().trim().is_empty()
                                    }
                                    on:click=move |_| {
                                        let name = new_name.get_untracked();
                                        if let Some(create) = on_create_folder {
                                            create.run(name.trim().to_owned());
                                        }
                                        new_name.set(String::new());
                                        new_folder.set(false);
                                    }
                                >
                                    "Create"
                                </button>
                            </div>
                        </Show>

                        <ul class="menu menu-sm w-full p-0">
                            <li>
                                <button
                                    type="button"
                                    class=move || {
                                        if current.get().is_none() { "menu-active" } else { "" }
                                    }
                                    on:click=move |_| {
                                        if let Some(open) = on_open_folder {
                                            open.run(None);
                                        }
                                    }
                                >
                                    "All files"
                                </button>
                            </li>
                            <For
                                each=move || folders.get()
                                // A keyed list does not rebuild an item whose key is unchanged, and
                                // a row paints its own name and indent: a key of the id alone
                                // would leave a renamed folder showing its old name. The key is
                                // the row.
                                key=|folder| format!("{}|{}|{}", folder.id, folder.name, folder.depth)
                                let:folder
                            >
                                {
                                    let id = folder.id.clone();
                                    let id_open = id.clone();
                                    let id_active = id_open.clone();
                                    let id_rename = id.clone();
                                    let id_delete = id.clone();
                                    let id_show_rename = id.clone();
                                    let id_show_delete = id.clone();
                                    let name = folder.name.clone();
                                    let name_rename = name.clone();
                                    let name_delete = name.clone();
                                    let depth = folder.depth;
                                    view! {
                                        <li data-testid="folder-row">
                                            <div
                                                class="flex flex-wrap items-center gap-1"
                                                style=format!("padding-left: {}rem", depth)
                                            >
                                                <button
                                                    type="button"
                                                    class=move || {
                                                        if current.get().as_deref() == Some(id_active.as_str()) {
                                                            "menu-active flex-1 text-left"
                                                        } else {
                                                            "flex-1 text-left"
                                                        }
                                                    }
                                                    on:click=move |_| {
                                                        if let Some(open) = on_open_folder {
                                                            open.run(Some(id_open.clone()));
                                                        }
                                                    }
                                                >
                                                    {name.clone()}
                                                </button>
                                                <button
                                                    type="button"
                                                    class="btn btn-ghost btn-xs"
                                                    aria-label=format!("Rename folder {name}")
                                                    disabled=move || !write
                                                    on:click=move |_| {
                                                        renaming_folder.set(Some((id_rename.clone(), name_rename.clone())));
                                                    }
                                                >
                                                    "Rename"
                                                </button>
                                                <button
                                                    type="button"
                                                    class="btn btn-ghost btn-xs text-error"
                                                    aria-label=format!("Delete folder {name}")
                                                    disabled=move || !write
                                                    on:click=move |_| {
                                                        deleting_folder
                                                            .set(Some((id_delete.clone(), name_delete.clone())));
                                                    }
                                                >
                                                    "Delete"
                                                </button>
                                            </div>

                                            <Show when=move || {
                                                renaming_folder.get().map(|(id, _)| id) == Some(id_show_rename.clone())
                                            }>
                                                <div class="flex gap-1 py-1">
                                                    <input
                                                        class="input input-xs input-bordered w-full"
                                                        type="text"
                                                        aria-label="Folder name"
                                                        prop:value=move || {
                                                            renaming_folder
                                                                .get()
                                                                .map(|(_, draft)| draft)
                                                                .unwrap_or_default()
                                                        }
                                                        on:input=move |event| {
                                                            let text = event_target_value(&event);
                                                            renaming_folder
                                                                .update(|state| {
                                                                    if let Some((_, draft)) = state {
                                                                        *draft = text;
                                                                    }
                                                                });
                                                        }
                                                    />
                                                    <button
                                                        type="button"
                                                        class="btn btn-primary btn-xs"
                                                        disabled=move || on_rename_folder.is_none()
                                                        on:click=move |_| {
                                                            if let Some((id, draft)) = renaming_folder.get_untracked() {
                                                                if let Some(rename) = on_rename_folder {
                                                                    rename.run((id, draft));
                                                                }
                                                            }
                                                            renaming_folder.set(None);
                                                        }
                                                    >
                                                        "Save"
                                                    </button>
                                                </div>
                                            </Show>

                                            <Show when=move || {
                                                deleting_folder.get().map(|(id, _)| id) == Some(id_show_delete.clone())
                                            }>
                                                <div class="rounded-box bg-base-200 p-2 text-xs" role="alert">
                                                    <p>
                                                        "Delete “" {name.clone()}
                                                        "” and every folder and file under it? This cannot be undone."
                                                    </p>
                                                    <div class="flex gap-1 pt-1">
                                                        <button
                                                            type="button"
                                                            class="btn btn-error btn-xs"
                                                            data-testid="confirm-folder-delete"
                                                            on:click=move |_| {
                                                                if let Some((id, _)) = deleting_folder.get_untracked() {
                                                                    if let Some(delete) = on_delete_folder {
                                                                        delete.run(id);
                                                                    }
                                                                }
                                                                deleting_folder.set(None)
                                                            }
                                                        >
                                                            "Delete folder"
                                                        </button>
                                                        <button
                                                            type="button"
                                                            class="btn btn-ghost btn-xs"
                                                            on:click=move |_| deleting_folder.set(None)
                                                        >
                                                            "Cancel"
                                                        </button>
                                                    </div>
                                                </div>
                                            </Show>
                                        </li>
                                    }
                                }
                            </For>
                        </ul>

                        <Show when=move || folders.get().is_empty()>
                            <p class="text-xs opacity-60">
                                "No folders yet. Make one to group your files."
                            </p>
                        </Show>
                    </div>
                </aside>

                <section class="card border border-base-300 bg-base-100">
                    <div class="card-body gap-3 p-4">
                        <div class="flex flex-wrap items-center justify-between gap-2">
                            <h2 class="card-title text-base">{open_label}</h2>
                            {upload}
                        </div>

                        {move || {
                            busy
                                .get()
                                .map(|busy| {
                                    view! {
                                        <p class="text-sm opacity-70" role="status">
                                            {busy}
                                        </p>
                                    }
                                })
                        }}

                        <Show when=move || files.get().is_empty()>
                            <div class="rounded-box bg-base-200 p-4 text-sm" data-testid="drive-empty">
                                <p class="font-medium">"This drive is empty."</p>
                                <p class="opacity-70">
                                    "It is where you keep files for this account: upload one and it appears here, private until you change that. Folders group them; nothing leaves this instance unless you make a file public."
                                </p>
                            </div>
                        </Show>

                        <ul class="divide-y divide-base-300">
                            <For
                                each=move || files.get()
                                // Same reason as the folder list: the row paints the filename,
                                // the size, the visibility and the processing state, and a rename
                                // or a visibility change keeps the id, so a key of the id alone
                                // would never repaint what it changed.
                                key=|file| {
                                    format!(
                                        "{}|{}|{}|{}|{}|{}",
                                        file.id,
                                        file.filename,
                                        file.mime_type,
                                        file.size,
                                        file.visibility,
                                        file.processing_state,
                                    )
                                }
                                let:file
                            >
                                {
                                    let id_download = file.id.clone();
                                    let id_rename = file.id.clone();
                                    let id_move = file.id.clone();
                                    let id_visibility = file.id.clone();
                                    let id_delete = file.id.clone();
                                    let id_show_rename = file.id.clone();
                                    let id_show_delete = file.id.clone();
                                    let filename = file.filename.clone();
                                    let filename_rename = filename.clone();
                                    let filename_delete = filename.clone();
                                    let visibility = file.visibility.clone();
                                    let state = file.processing_state.clone();
                                    let badge = match state.as_str() {
                                        "pending" => Some((
                                            "badge-warning",
                                            "Preview pending — still being made",
                                        )),
                                        "failed" => Some(("badge-error", "Preview failed")),
                                        _ => None,
                                    };
                                    view! {
                                        <li class="flex flex-col gap-2 py-3" data-testid="drive-file">
                                            <div class="flex flex-wrap items-center gap-2">
                                                <span class="font-medium break-all">{file.filename.clone()}</span>
                                                {badge
                                                    .map(|(class, label)| {
                                                        view! {
                                                            <span
                                                                class=format!("badge badge-sm {class}")
                                                                title=label
                                                                data-testid="processing-badge"
                                                            >
                                                                {label}
                                                            </span>
                                                        }
                                                    })}
                                            </div>
                                            <p class="text-xs opacity-70">
                                                {format!(
                                                    "{} · {} · {}",
                                                    file.size,
                                                    type_label(&file.mime_type),
                                                    visibility_label(&visibility),
                                                )}
                                            </p>

                                            <div class="flex flex-wrap items-center gap-1">
                                                <button
                                                    type="button"
                                                    class="btn btn-ghost btn-xs"
                                                    aria-label=format!("Download {filename}")
                                                    on:click=move |_| {
                                                        if let Some(download) = on_download {
                                                            download.run(id_download.clone());
                                                        }
                                                    }
                                                >
                                                    "Download"
                                                </button>
                                                <button
                                                    type="button"
                                                    class="btn btn-ghost btn-xs"
                                                    aria-label=format!("Rename {filename}")
                                                    disabled=move || !write
                                                    on:click=move |_| {
                                                        renaming_file
                                                            .set(Some((id_rename.clone(), filename_rename.clone())));
                                                    }
                                                >
                                                    "Rename"
                                                </button>
                                                <label class="flex items-center gap-1 text-xs">
                                                    "Move to"
                                                    <select
                                                        class="select select-xs select-bordered"
                                                        aria-label=format!("Move {filename} to a folder")
                                                        disabled=!write
                                                        prop:value=move || {
                                                            current.get().unwrap_or_default()
                                                        }
                                                        on:change=move |event| {
                                                            let target = event_target_value(&event);
                                                            if let Some(move_file) = on_move_file {
                                                                move_file
                                                                    .run((
                                                                        id_move.clone(),
                                                                        if target.is_empty() { None } else { Some(target) },
                                                                    ));
                                                            }
                                                        }
                                                    >
                                                        <option value="">"Root"</option>
                                                        {move || {
                                                            folders
                                                                .get()
                                                                .into_iter()
                                                                .map(|folder| {
                                                                    let id = folder.id.clone();
                                                                    let chosen = id.clone();
                                                                    view! {
                                                                        <option value=id.clone() selected=move || {
                                                                            current.get().as_deref() == Some(chosen.as_str())
                                                                        }>
                                                                            {folder.name}
                                                                        </option>
                                                                    }
                                                                })
                                                                .collect_view()
                                                        }}
                                                    </select>
                                                </label>
                                                <label class="flex items-center gap-1 text-xs">
                                                    "Visibility"
                                                    <select
                                                        class="select select-xs select-bordered"
                                                        aria-label=format!("Visibility of {filename}")
                                                        disabled=!write
                                                        on:change=move |event| {
                                                            let value = event_target_value(&event);
                                                            if let Some(change) = on_visibility {
                                                                change.run((id_visibility.clone(), value));
                                                            }
                                                        }
                                                    >
                                                        <option value="private" selected=visibility == "private">
                                                            "Private"
                                                        </option>
                                                        <option
                                                            value="unlisted_link"
                                                            selected=visibility == "unlisted_link"
                                                        >
                                                            "Link only"
                                                        </option>
                                                        <option
                                                            value="public_federated"
                                                            selected=visibility == "public_federated"
                                                        >
                                                            "Public"
                                                        </option>
                                                    </select>
                                                </label>
                                                <button
                                                    type="button"
                                                    class="btn btn-ghost btn-xs text-error"
                                                    aria-label=format!("Delete {filename}")
                                                    disabled=move || !write
                                                    on:click=move |_| {
                                                        deleting_file
                                                            .set(Some((id_delete.clone(), filename_delete.clone())));
                                                    }
                                                >
                                                    "Delete"
                                                </button>
                                            </div>

                                            <Show when=move || {
                                                renaming_file.get().map(|(id, _)| id) == Some(id_show_rename.clone())
                                            }>
                                                <div class="flex gap-1">
                                                    <input
                                                        class="input input-xs input-bordered w-full"
                                                        type="text"
                                                        aria-label="File name"
                                                        prop:value=move || {
                                                            renaming_file
                                                                .get()
                                                                .map(|(_, draft)| draft)
                                                                .unwrap_or_default()
                                                        }
                                                        on:input=move |event| {
                                                            let text = event_target_value(&event);
                                                            renaming_file
                                                                .update(|state| {
                                                                    if let Some((_, draft)) = state {
                                                                        *draft = text;
                                                                    }
                                                                });
                                                        }
                                                    />
                                                    <button
                                                        type="button"
                                                        class="btn btn-primary btn-xs"
                                                        disabled=move || on_rename_file.is_none()
                                                        on:click=move |_| {
                                                            if let Some((id, draft)) = renaming_file.get_untracked() {
                                                                if let Some(rename) = on_rename_file {
                                                                    rename.run((id, draft));
                                                                }
                                                            }
                                                            renaming_file.set(None);
                                                        }
                                                    >
                                                        "Save"
                                                    </button>
                                                </div>
                                            </Show>

                                            <Show when=move || {
                                                deleting_file.get().map(|(id, _)| id) == Some(id_show_delete.clone())
                                            }>
                                                <div class="rounded-box bg-base-200 p-2 text-xs" role="alert">
                                                    <p>
                                                        "Delete “" {file.filename.clone()}
                                                        "”? The bytes go with it."
                                                    </p>
                                                    <div class="flex gap-1 pt-1">
                                                        <button
                                                            type="button"
                                                            class="btn btn-error btn-xs"
                                                            data-testid="confirm-file-delete"
                                                            on:click=move |_| {
                                                                if let Some((id, _)) = deleting_file.get_untracked() {
                                                                    if let Some(delete) = on_delete_file {
                                                                        delete.run(id);
                                                                    }
                                                                }
                                                                deleting_file.set(None)
                                                            }
                                                        >
                                                            "Delete file"
                                                        </button>
                                                        <button
                                                            type="button"
                                                            class="btn btn-ghost btn-xs"
                                                            on:click=move |_| deleting_file.set(None)
                                                        >
                                                            "Cancel"
                                                        </button>
                                                    </div>
                                                </div>
                                            </Show>
                                        </li>
                                    }
                                }
                            </For>
                        </ul>
                    </div>
                </section>
            </div>
        </Screen>
    }
}

/// Bytes as a reader reads them: `12.4 MB`.
#[must_use]
pub fn bytes(count: i64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    #[expect(
        clippy::cast_precision_loss,
        reason = "a byte count is displayed, not computed with"
    )]
    let mut value = count.max(0) as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", count.max(0), UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
