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

use super::drafts_panel::{Draft, DraftsPanel};
use super::emoji::{self, Suggestions};
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
    fn poll_ready(&self) -> Signal<bool>;
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
                language=language.with_value(std::clone::Clone::clone)
                emoji_picker=emoji_picker.with_value(std::clone::Clone::clone)
                schedule_panel=schedule.with_value(std::clone::Clone::clone)
            />

            {poll.with_value(std::clone::Clone::clone).map(|poll| poll())}

            {attachments.with_value(std::clone::Clone::clone).map(|attachments| attachments())}
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

/// The host's answers to [`ComposerRuntime`]: the composer's shape, and nothing to do.
///
/// A reader with scripting off, or one whose wasm has not arrived yet, sees the same card
/// a browser will hydrate — the counter, the visibility, the buttons — and none of them
/// answer. That is what makes the first frame adoptable: the nodes are the composer's, not
/// a stand-in's.
#[derive(Debug)]
pub struct StaticComposerRuntime {
    limit: usize,
}

impl StaticComposerRuntime {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self { limit }
    }
}

impl ComposerRuntime for StaticComposerRuntime {
    fn text(&self) -> ReadSignal<String> {
        let (text, _) = signal(String::new());
        text
    }
    fn visibility(&self) -> ReadSignal<Visibility> {
        let (visibility, _) = signal(Visibility::Public);
        visibility
    }
    fn busy(&self) -> ReadSignal<bool> {
        let (busy, _) = signal(false);
        busy
    }
    fn limit(&self) -> ReadSignal<usize> {
        let (limit, _) = signal(self.limit);
        limit
    }
    fn at(&self) -> ReadSignal<String> {
        let (at, _) = signal(String::new());
        at
    }
    fn schedule_open(&self) -> ReadSignal<bool> {
        let (open, _) = signal(false);
        open
    }
    fn poll_ready(&self) -> Signal<bool> {
        Signal::derive(|| false)
    }
    fn catalogue(&self) -> Signal<Vec<CustomEmoji>> {
        Signal::derive(Vec::new)
    }
    fn drafts(&self) -> Signal<Vec<Draft>> {
        Signal::derive(Vec::new)
    }
    fn error(&self) -> Signal<Option<String>> {
        Signal::derive(|| None)
    }
    fn notice(&self) -> Signal<Option<String>> {
        Signal::derive(|| None)
    }
    fn done(&self) -> Signal<Option<Posted>> {
        Signal::derive(|| None)
    }
}

/// The home composer as a host paints it: the live composer's tree, an inert runtime, and
/// the tabs under it. The browser hydrates this into [`LiveComposer`] with its own runtime,
/// so the first frame and every frame after it are the same nodes.
#[component]
pub fn StaticComposer(
    /// The route whose timeline is showing.
    #[prop(into)]
    active: Signal<String>,
    /// The instance's character limit.
    #[prop(default = 500)]
    limit: usize,
) -> impl IntoView {
    let runtime: Arc<dyn ComposerRuntime> = Arc::new(StaticComposerRuntime::new(limit));
    let language_host = NodeRef::<leptos::html::Div>::new();
    let editor_host = NodeRef::<leptos::html::Div>::new();
    let emoji_host = NodeRef::<leptos::html::Div>::new();

    // The writing surface is `nodus-editor`'s and cannot be rendered here, so the host draws
    // the box it boots into: same id, same class, no children. The browser adopts this node
    // by identity and ProseMirror fills it.
    let editor: ChildrenFn = Arc::new(move || {
        view! {
            <div
                id="nodus-composer-body"
                node_ref=editor_host
                class="min-h-24 w-full cursor-text text-sm leading-relaxed"
                aria-label="Write a post"
            ></div>
        }
        .into_any()
    });

    let language: ChildrenFn = Arc::new(move || {
        view! {
            <crate::LanguagePicker
                selected=Signal::derive(String::new)
                name=Signal::derive(|| "Language".to_owned())
                open=Signal::derive(|| false)
                options=Signal::derive(Vec::new)
                host=language_host
            />
        }
        .into_any()
    });

    let emoji_picker: ChildrenFn = Arc::new(move || {
        view! {
            <crate::Anchor host=emoji_host>
                <super::emoji_toggle::EmojiToggleButton
                    open=Signal::derive(|| false)
                    busy=Signal::derive(|| false)
                />
            </crate::Anchor>
        }
        .into_any()
    });

    // The attachment strip is the one sub-tree no inert runtime can supply: the file input
    // and the drive picker are the browser's own controls and live in the client crate. The
    // host draws the same nodes they draw, ids included, so the browser adopts them and the
    // client hangs the behaviour on them.
    let attachments: ChildrenFn = Arc::new(move || {
        view! {
            <crate::pages::compose::attachments::Attachments
                media=Signal::derive(Vec::new)
                uploading=Signal::derive(|| false)
                percent=Signal::derive(|| None)
                picker=Some(Arc::new(|| {
                    view! {
                        <input
                            id="nodus-attach-input"
                            class="hidden"
                            type="file"
                            accept="image/*,video/*,audio/*"
                            multiple
                        />
                        <button id="nodus-drive-open" class="hidden" type="button"></button>
                    }
                    .into_any()
                }) as ChildrenFn)
            />
        }
        .into_any()
    });

    view! {
        <div class="flex flex-col gap-2">
            <LiveComposer
                runtime=runtime
                editor=editor
                language=Some(language)
                emoji_picker=Some(emoji_picker)
                attachments=Some(attachments)
            />
            <crate::TimelineTabs active=active/>
        </div>
    }
}
