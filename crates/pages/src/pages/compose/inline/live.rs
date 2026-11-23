//! The composer, painted by both sides from one tree.
//!
//! The card, the writing slot, the suggestions, the drafts popup and the bar used to be
//! assembled twice: a preview on the host and a live composer in the browser, with the
//! browser replacing one with the other a task turn after hydration. That replacement is a
//! tree that changes shape right after the document is adopted, which is exactly what
//! hydration cannot do without a visible swap.
//!
//! So the composition lives here and neither side owns it. What each side owns is
//! behaviour, behind [`ComposerRuntime`]: the draft signals, the submit path, the catalogue,
//! the uploads and the clock. The host answers with the inert implementation — same signals,
//! no callbacks — and the browser with the live one. Both draw the same nodes, because the
//! only difference between them is *what the controls do*, not that they exist.
//!
//! Two slots stay outside: the writing surface and the pickers. The editor is
//! `nodus-editor`, which is wasm-only and depends on this crate, so it cannot be rendered
//! here; the emoji picker, the language list and the schedule panel each own a document
//! listener or the reader's clock. They arrive as children, and each side hands over the
//! same markup with a different amount of behaviour attached.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_client_core::model::{CustomEmoji, Visibility};

use super::emoji::{self, Suggestions};
use super::drafts_panel::{Draft, DraftsPanel};
use super::{ComposerBar, InlineComposer, Posted};

/// What the composer does, as the platform can answer it.
///
/// Every method is a read or an action; none of them is state. The host's implementation
/// returns empty signals and no callbacks, which draws the composer's first frame; the
/// browser's returns the draft it is editing.
pub trait ComposerRuntime: Send + Sync + 'static {
    /// The text being written.
    fn text(&self) -> ReadSignal<String>;
    /// Who may see the post.
    fn visibility(&self) -> ReadSignal<Visibility>;
    /// Whether a send is in flight.
    fn busy(&self) -> ReadSignal<bool>;
    /// The instance's character limit.
    fn limit(&self) -> ReadSignal<usize>;
    /// The instant to send at (`YYYY-MM-DDTHH:mm`), or empty for now.
    fn at(&self) -> ReadSignal<String>;
    /// Whether the schedule panel is showing.
    fn schedule_open(&self) -> ReadSignal<bool>;
    /// Whether the poll being edited would be accepted.
    fn poll_ready(&self) -> ReadSignal<bool>;
    /// The instance's emoji catalogue.
    fn catalogue(&self) -> Signal<Vec<CustomEmoji>>;
    /// The unsent drafts, already read and aged.
    fn drafts(&self) -> Signal<Vec<Draft>>;
    /// The last send's refusal.
    fn error(&self) -> Signal<Option<String>>;
    /// The line a queued post leaves behind.
    fn notice(&self) -> Signal<Option<String>>;
    /// What a published post went out as.
    fn done(&self) -> Signal<Option<Posted>>;

    fn on_visibility(&self) -> Option<Callback<Visibility>> {
        None
    }
    fn on_add_media(&self) -> Option<Callback<()>> {
        None
    }
    fn on_add_drive(&self) -> Option<Callback<()>> {
        None
    }
    fn on_add_poll(&self) -> Option<Callback<()>> {
        None
    }
    fn on_toggle_schedule(&self) -> Option<Callback<()>> {
        None
    }
    fn on_thread(&self) -> Option<Callback<()>> {
        None
    }
    fn on_submit(&self) -> Option<Callback<()>> {
        None
    }
    /// Resumes the draft at `index` (the `:draft` popup).
    fn on_resume(&self) -> Option<Callback<usize>> {
        None
    }
    /// Inserts a picked emoji's shortcode.
    fn on_insert_emoji(&self) -> Option<Callback<String>> {
        None
    }
}

/// The composer both paints share: the browser hydrates the host's nodes into it.
#[component]
pub fn LiveComposer(
    runtime: Arc<dyn ComposerRuntime>,
    /// The writing surface. ProseMirror in the browser; the same box, empty, on the host.
    editor: ChildrenFn,
    /// The line above the box; empty means "New post".
    #[prop(optional, into)]
    heading: String,
    /// The language list, which is the instance's.
    #[prop(optional_no_strip)]
    language: Option<ChildrenFn>,
    /// The emoticon and the picker it opens.
    #[prop(optional_no_strip)]
    emoji_picker: Option<ChildrenFn>,
    /// The schedule panel, which reads the reader's clock.
    #[prop(optional_no_strip)]
    schedule: Option<ChildrenFn>,
    /// The poll editor.
    #[prop(optional_no_strip)]
    poll: Option<ChildrenFn>,
    /// The attachment strip.
    #[prop(optional_no_strip)]
    attachments: Option<ChildrenFn>,
) -> impl IntoView {
    let text = runtime.text();
    let runtime_body = Arc::clone(&runtime);
    let editor = StoredValue::new(editor);
    let body: ChildrenFn = Arc::new(move || {
        let runtime = Arc::clone(&runtime_body);
        view! {
            // The suggestions sit above the box, where the writer is already looking:
            // a dialog would steal focus and swallow the next keystroke.
            <Suggestions
                query=Signal::derive(move || {
                    emoji::pending(&text.get())
                        .map(|(_, query)| query)
                        .unwrap_or_default()
                })
                open=Signal::derive(move || emoji::pending(&text.get()).is_some())
                catalogue=runtime.catalogue()
                insert=runtime.on_insert_emoji()
            />
            <DraftsPanel
                open=Signal::derive(move || {
                    emoji::pending(&text.get())
                        .is_some_and(|(_, code)| code.eq_ignore_ascii_case("draft"))
                })
                drafts=runtime.drafts()
                insert=runtime.on_resume()
            />
            {editor.with_value(|editor| editor())}
        }
        .into_any()
    });

    let runtime_controls = Arc::clone(&runtime);
    let at = runtime.at();
    let scheduling = Signal::derive(move || !at.get().trim().is_empty());
    let when = {
        let runtime = Arc::clone(&runtime);
        Signal::derive(move || runtime.at().get())
    };
    // The slots are read more than once (a re-render asks again), and a `ChildrenFn` is not
    // `Copy`, so they are held where the closure can hand out a clone.
    let language = StoredValue::new(language);
    let emoji_picker = StoredValue::new(emoji_picker);
    let schedule = StoredValue::new(schedule);
    let poll = StoredValue::new(poll);
    let attachments = StoredValue::new(attachments);
    let controls: ChildrenFn = Arc::new(move || {
        let runtime = Arc::clone(&runtime_controls);
        view! {
            <ComposerBar
                text=runtime.text()
                visibility=runtime.visibility()
                busy=runtime.busy()
                scheduling=scheduling
                when=when
                limit=runtime.limit()
                poll_ready=runtime.poll_ready()
                schedule_open=runtime.schedule_open()
                on_visibility=runtime.on_visibility()
                on_add_media=runtime.on_add_media()
                on_add_drive=runtime.on_add_drive()
                on_add_poll=runtime.on_add_poll()
                on_toggle_schedule=runtime.on_toggle_schedule()
                on_thread=runtime.on_thread()
                on_submit=runtime.on_submit()
                language=language.with_value(|slot| slot.clone())
                emoji_picker=emoji_picker.with_value(|slot| slot.clone())
                schedule_panel=schedule.with_value(|slot| slot.clone())
            />

            {poll.with_value(|slot| slot.clone()).map(|poll| poll())}

            {attachments.with_value(|slot| slot.clone()).map(|attachments| attachments())}
        }
        .into_any()
    });

    // The props struct, not the builder: `error`, `notice` and `done` are `Option`s whose
    // absent state is part of the tree.
    InlineComposer(super::InlineComposerProps {
        heading,
        body,
        controls,
        error: Some(runtime.error()),
        notice: Some(runtime.notice()),
        done: Some(runtime.done()),
    })
}
