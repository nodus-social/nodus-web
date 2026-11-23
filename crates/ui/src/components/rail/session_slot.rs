//! One picture in a session banner, and the little editor that rides with it.
//!
//! The picture is the same node in a reader's banner and in the editor's; only the verbs
//! around it are built for the editor. Kept apart from the banner so the banner stays a
//! description of a column and this stays a description of a picture.

use leptos::prelude::*;

use crate::session_style::{
    BannerRegion, BannerRegionKind, ImageSlot, MAX_ALT_LEN, PictureTarget, StyleEditor,
};

/// The picture itself: the same node in a reader's banner and in the editor's.
pub(super) fn slot_image(slot: ImageSlot, class: &'static str) -> AnyView {
    view! {
        <img src=slot.url alt=slot.alt.unwrap_or_default() class=class loading="lazy"/>
    }
    .into_any()
}

/// One picture in a banner, with the grab, edit and remove verbs the editor adds.
///
/// Everything is built once and toggled with a class. The open panel lives in `open`, which
/// the banner owns rather than this row, because `For` keys a row on its URL and replacing
/// the picture therefore rebuilds the row: state kept down here would close itself.
#[expect(
    clippy::too_many_arguments,
    reason = "the row is a join of the banner's signals; a struct would only rename them"
)]
pub(super) fn banner_slot(
    image: AnyView,
    kind: BannerRegionKind,
    index: usize,
    editor: Option<StyleEditor>,
    region: Signal<BannerRegion>,
    dragging: RwSignal<Option<usize>>,
    open: RwSignal<Option<usize>>,
    on_pick: Option<Callback<PictureTarget>>,
) -> AnyView {
    let Some(edit) = editor else {
        return image;
    };
    let text: RwSignal<Option<String>> = RwSignal::new(None);
    let problem: RwSignal<Option<String>> = RwSignal::new(None);
    let verbs = match kind {
        BannerRegionKind::Top => "absolute right-1 top-1 flex items-center gap-1",
        // A side banner is one picture wide: three round buttons in a row would cover the
        // picture they manage, so the verbs run down the corner instead.
        BannerRegionKind::Left | BannerRegionKind::Right => {
            "absolute right-0.5 top-0.5 flex flex-col items-end gap-1"
        }
    };
    view! {
        <div
            class="relative min-w-0 flex-1"
            data-style-module="banner-slot"
            on:dragover=move |event| event.prevent_default()
            on:drop=move |event| {
                event.prevent_default();
                if let Some(from) = dragging.get_untracked() {
                    edit.move_slot(kind, from, index);
                }
                dragging.set(None);
            }
        >
            {image}
            <span class=verbs class:hidden=move || !edit.is_active()>
                <span
                    class="cursor-grab select-none rounded bg-base-100/90 px-1 text-xs opacity-70"
                    title="Drag to reorder"
                    draggable="true"
                    on:dragstart=move |_| dragging.set(Some(index))
                >
                    "\u{28ff}"
                </span>
                <button
                    type="button"
                    class="btn btn-xs btn-circle"
                    title="Edit this picture: replace it or describe it"
                    data-style-slot-edit=""
                    on:click=move |_| {
                        text.set(None);
                        problem.set(None);
                        open.update(|open| {
                            *open = if *open == Some(index) { None } else { Some(index) };
                        });
                    }
                >
                    "\u{270e}"
                </button>
                <button
                    type="button"
                    class="btn btn-xs btn-circle"
                    title="Remove this picture"
                    on:click=move |_| {
                        open.set(None);
                        edit.remove_slot(kind, index);
                    }
                >
                    "\u{2715}"
                </button>
            </span>
            {slot_editor(kind, index, edit, region, open, text, problem, on_pick)}
        </div>
    }
    .into_any()
}

/// The panel behind the edit verb: a new picture for this place, or a description for it.
///
/// Alt text is saved as it is typed, like every other verb in this editor; Cancel closes
/// the panel, and the editor's own Cancel is what throws the whole draft away. Text past
/// [`MAX_ALT_LEN`] leaves the draft alone and says so, because a description cut in half is
/// worse than a refused one.
#[expect(
    clippy::too_many_arguments,
    reason = "the panel shows the row's own state; bundling it into a struct would rename it"
)]
fn slot_editor(
    kind: BannerRegionKind,
    index: usize,
    edit: StyleEditor,
    region: Signal<BannerRegion>,
    open: RwSignal<Option<usize>>,
    text: RwSignal<Option<String>>,
    problem: RwSignal<Option<String>>,
    on_pick: Option<Callback<PictureTarget>>,
) -> AnyView {
    let anchor = if matches!(kind, BannerRegionKind::Right) {
        "right-0"
    } else {
        "left-0"
    };
    let title = format!("Alt text, up to {MAX_ALT_LEN} characters");
    view! {
        <div
            class=move || {
                if open.get() != Some(index) || !edit.is_active() {
                    return String::from("hidden");
                }
                format!(
                    "pointer-events-auto absolute top-0 z-20 flex w-60 flex-col gap-2 rounded-box border border-base-300 bg-base-100 p-2 text-xs shadow-xl {anchor}"
                )
            }
            data-style-slot-editor=""
        >
            <div class="flex items-center gap-2">
                <span class="font-semibold">{format!("Picture {}", index + 1)}</span>
                <button
                    type="button"
                    class="btn btn-ghost btn-xs ml-auto"
                    title="Close this panel; changes stay in the draft until Save"
                    on:click=move |_| {
                        text.set(None);
                        problem.set(None);
                        open.set(None);
                    }
                >
                    "Cancel"
                </button>
            </div>
            <button
                type="button"
                class="btn btn-xs w-full"
                title="Pick a different picture for this place"
                data-style-slot-replace=""
                disabled=move || on_pick.is_none()
                on:click=move |_| {
                    if let Some(pick) = on_pick {
                        pick.run(PictureTarget::Slot(kind, index));
                    }
                }
            >
                "Replace picture"
            </button>
            <input
                type="text"
                class="input input-bordered input-xs w-full"
                placeholder="Describe the picture; leave empty if it is decorative"
                title=title
                data-style-slot-alt=""
                prop:value=move || {
                    text.get().unwrap_or_else(|| {
                        region
                            .get()
                            .slots
                            .get(index)
                            .and_then(|slot| slot.alt.clone())
                            .unwrap_or_default()
                    })
                }
                on:input=move |event| {
                    let typed = event_target_value(&event);
                    text.set(Some(typed.clone()));
                    match edit.set_slot_alt(kind, index, &typed) {
                        Ok(()) => problem.set(None),
                        Err(message) => problem.set(Some(message)),
                    }
                }
            />
            <span
                role="alert"
                class="text-[10px] text-error"
                class:hidden=move || problem.get().is_none()
            >
                {move || problem.get().unwrap_or_default()}
            </span>
        </div>
    }
    .into_any()
}
