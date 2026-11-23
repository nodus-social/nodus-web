//! The preferences editor: the nine values the API stores, in three groups.
//!
//! The page owns no request. It paints an [`PreferencesEditor`] the caller holds and reports
//! one change at a time through `on_change`; the caller sends the partial patch and hands
//! the answer back. A refused change is a sentence filed under the row that asked, never a
//! toast at the edge of the screen.

use leptos::prelude::*;
use nodus_client_core::model::{
    ExpandMedia, Preferences, PreferencesPatch, QuotePolicy, Visibility,
};

use crate::components::panel::Panel;

/// The three groups the nine values are filed under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceGroup {
    Posting,
    Reading,
    Notifications,
}

impl PreferenceGroup {
    pub const ALL: [Self; 3] = [Self::Posting, Self::Reading, Self::Notifications];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Posting => "Posting defaults",
            Self::Reading => "Reading",
            Self::Notifications => "Notifications",
        }
    }

    /// The one sentence the notification group carries.
    ///
    /// Both keys are accepted and remembered today, so they stay editable: what the note
    /// says is that nothing on this instance acts on them yet.
    #[must_use]
    pub const fn note(self) -> Option<&'static str> {
        match self {
            Self::Posting | Self::Reading => None,
            Self::Notifications => Some(
                "These two settings are stored, but they have no effect on this instance yet: it sends no notification email, and report notifications have no policy here.",
            ),
        }
    }
}

/// One of the nine preference keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceField {
    PostingVisibility,
    PostingSensitive,
    PostingLanguage,
    PostingQuotePolicy,
    ReadingExpandMedia,
    ReadingExpandSpoilers,
    ReadingAutoplayGifs,
    NotificationEmail,
    NotificationReport,
}

impl PreferenceField {
    pub const ALL: [Self; 9] = [
        Self::PostingVisibility,
        Self::PostingSensitive,
        Self::PostingLanguage,
        Self::PostingQuotePolicy,
        Self::ReadingExpandMedia,
        Self::ReadingExpandSpoilers,
        Self::ReadingAutoplayGifs,
        Self::NotificationEmail,
        Self::NotificationReport,
    ];

    #[must_use]
    pub const fn group(self) -> PreferenceGroup {
        match self {
            Self::PostingVisibility
            | Self::PostingSensitive
            | Self::PostingLanguage
            | Self::PostingQuotePolicy => PreferenceGroup::Posting,
            Self::ReadingExpandMedia | Self::ReadingExpandSpoilers | Self::ReadingAutoplayGifs => {
                PreferenceGroup::Reading
            }
            Self::NotificationEmail | Self::NotificationReport => PreferenceGroup::Notifications,
        }
    }

    /// The key this field has on the wire.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::PostingVisibility => "posting:default:visibility",
            Self::PostingSensitive => "posting:default:sensitive",
            Self::PostingLanguage => "posting:default:language",
            Self::PostingQuotePolicy => "posting:default:quote_policy",
            Self::ReadingExpandMedia => "reading:expand:media",
            Self::ReadingExpandSpoilers => "reading:expand:spoilers",
            Self::ReadingAutoplayGifs => "reading:autoplay:gifs",
            Self::NotificationEmail => "notification:email",
            Self::NotificationReport => "notification:report",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PostingVisibility => "Default visibility",
            Self::PostingSensitive => "New posts start sensitive",
            Self::PostingLanguage => "Default language",
            Self::PostingQuotePolicy => "Who may quote new posts",
            Self::ReadingExpandMedia => "Media in timelines",
            Self::ReadingExpandSpoilers => "Content warnings start open",
            Self::ReadingAutoplayGifs => "Autoplay GIFs",
            Self::NotificationEmail => "Email notifications",
            Self::NotificationReport => "Report notifications",
        }
    }
}

/// One value, tagged by the control that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferenceValue {
    Visibility(Visibility),
    Sensitive(bool),
    Language(Option<String>),
    QuotePolicy(QuotePolicy),
    ExpandMedia(ExpandMedia),
    Spoilers(bool),
    Autoplay(bool),
    Email(bool),
    Report(bool),
}

impl PreferenceValue {
    /// Writes this value onto the preference it belongs to.
    pub fn apply(self, preferences: &mut Preferences) {
        match self {
            Self::Visibility(value) => preferences.posting_default_visibility = value,
            Self::Sensitive(value) => preferences.posting_default_sensitive = value,
            Self::Language(value) => preferences.posting_default_language = value,
            Self::QuotePolicy(value) => preferences.posting_default_quote_policy = value,
            Self::ExpandMedia(value) => preferences.reading_expand_media = value,
            Self::Spoilers(value) => preferences.reading_expand_spoilers = value,
            Self::Autoplay(value) => preferences.reading_autoplay_gifs = value,
            Self::Email(value) => preferences.notification_email = value,
            Self::Report(value) => preferences.notification_report = value,
        }
    }

    /// The value `preferences` currently holds for `field`.
    #[must_use]
    pub fn of(field: PreferenceField, preferences: &Preferences) -> Self {
        match field {
            PreferenceField::PostingVisibility => {
                Self::Visibility(preferences.posting_default_visibility)
            }
            PreferenceField::PostingSensitive => {
                Self::Sensitive(preferences.posting_default_sensitive)
            }
            PreferenceField::PostingLanguage => {
                Self::Language(preferences.posting_default_language.clone())
            }
            PreferenceField::PostingQuotePolicy => {
                Self::QuotePolicy(preferences.posting_default_quote_policy)
            }
            PreferenceField::ReadingExpandMedia => {
                Self::ExpandMedia(preferences.reading_expand_media)
            }
            PreferenceField::ReadingExpandSpoilers => {
                Self::Spoilers(preferences.reading_expand_spoilers)
            }
            PreferenceField::ReadingAutoplayGifs => {
                Self::Autoplay(preferences.reading_autoplay_gifs)
            }
            PreferenceField::NotificationEmail => Self::Email(preferences.notification_email),
            PreferenceField::NotificationReport => Self::Report(preferences.notification_report),
        }
    }
}

/// The last confirmed map, the optimistic map, and the one save in flight.
///
/// `Preferences` is the whole object because that is what the endpoint answers with, but
/// [`begin`](Self::begin) turns each edit into a patch against the confirmed copy, so the
/// body carries one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferencesEditor {
    confirmed: Preferences,
    edited: Preferences,
    saving: Option<PreferenceField>,
    errors: Vec<(PreferenceField, String)>,
    saved: bool,
}

impl PreferencesEditor {
    #[must_use]
    pub fn new(confirmed: Preferences) -> Self {
        Self {
            edited: confirmed.clone(),
            confirmed,
            saving: None,
            errors: Vec::new(),
            saved: false,
        }
    }

    #[must_use]
    pub fn edited(&self) -> &Preferences {
        &self.edited
    }

    #[must_use]
    pub fn saving(&self) -> Option<PreferenceField> {
        self.saving
    }

    #[must_use]
    pub fn error(&self, field: PreferenceField) -> Option<&str> {
        self.errors
            .iter()
            .find(|(held, _)| *held == field)
            .map(|(_, message)| message.as_str())
    }

    #[must_use]
    pub fn saved(&self) -> bool {
        self.saved
    }

    /// Applies one edit and returns the partial body to send.
    ///
    /// `None` when a save is already in flight or the edit changes nothing: an empty patch
    /// would ask the server to accept a request with no question in it.
    pub fn begin(
        &mut self,
        field: PreferenceField,
        value: PreferenceValue,
    ) -> Option<PreferencesPatch> {
        if self.saving.is_some() {
            return None;
        }
        let previous = self.edited.clone();
        value.apply(&mut self.edited);
        let patch = PreferencesPatch::between(&self.confirmed, &self.edited);
        if patch.is_empty() {
            self.edited = previous;
            return None;
        }
        self.saving = Some(field);
        self.errors.retain(|(held, _)| *held != field);
        self.saved = false;
        Some(patch)
    }

    /// The instance accepted the change and answered with the whole map.
    pub fn accepted(&mut self, server: Preferences) {
        self.confirmed.clone_from(&server);
        self.edited = server;
        self.saving = None;
        self.saved = true;
    }

    /// The instance refused, or the request never reached it.
    ///
    /// The row goes back to the confirmed value, the control stays editable, and the
    /// sentence stays under the row it is about.
    pub fn refused(&mut self, field: PreferenceField, sentence: String) {
        PreferenceValue::of(field, &self.confirmed).apply(&mut self.edited);
        self.saving = None;
        self.saved = false;
        self.errors.retain(|(held, _)| *held != field);
        self.errors.push((field, sentence));
    }
}

/// The editor, painted from the caller's state.
#[component]
pub fn PreferencesCard(
    #[prop(into)] editor: Signal<PreferencesEditor>,
    #[prop(optional_no_strip)] on_change: Option<Callback<(PreferenceField, PreferenceValue)>>,
) -> impl IntoView {
    let groups = PreferenceGroup::ALL
        .into_iter()
        .map(|group| {
            view! {
                <section class="flex flex-col gap-2" data-preference-group=group.label()>
                    <h3 class="text-sm font-semibold">{group.label()}</h3>
                    {move || {
                        let current = editor.get();
                        PreferenceField::ALL
                            .into_iter()
                            .filter(|field| field.group() == group)
                            .map(|field| row_view(field, &current, on_change).into_any())
                            .collect_view()
                    }}
                    {group
                        .note()
                        .map(|note| {
                            view! {
                                <p class="text-xs opacity-70" data-preference-note="">
                                    {note}
                                </p>
                            }
                        })}
                </section>
            }
        })
        .collect_view();

    view! {
        <Panel
            title="Preferences"
            description="Each change is saved on its own. A refused value goes back, and the instance's reason appears under the row."
        >
            {groups}
            <Show when=move || editor.get().saved()>
                <p class="text-sm text-success" role="status">
                    "Saved."
                </p>
            </Show>
        </Panel>
    }
}

fn row_view(
    field: PreferenceField,
    editor: &PreferencesEditor,
    on_change: Option<Callback<(PreferenceField, PreferenceValue)>>,
) -> impl IntoView {
    let saving = editor.saving() == Some(field);
    let error = editor.error(field).map(str::to_owned);
    view! {
        <div class="rounded-box bg-base-200 px-3 py-2" data-preference=field.key()>
            <div class="flex flex-wrap items-center justify-between gap-2">
                <span class="text-sm">{field.label()}</span>
                {control(field, editor, on_change)}
            </div>
            <Show when=move || saving>
                <p class="text-xs opacity-70" role="status">
                    "Saving…"
                </p>
            </Show>
            {error
                .map(|message| {
                    view! {
                        <p class="text-xs text-error" role="alert">
                            {message}
                        </p>
                    }
                })}
        </div>
    }
}

/// The control the value deserves: a select for the three closed enums, a checkbox for a
/// boolean, a text input for the language (`empty` is the server's `null`).
#[expect(
    clippy::too_many_lines,
    reason = "one arm per field keeps each value beside the control that edits it"
)]
fn control(
    field: PreferenceField,
    editor: &PreferencesEditor,
    on_change: Option<Callback<(PreferenceField, PreferenceValue)>>,
) -> AnyView {
    let disabled = on_change.is_none() || editor.saving().is_some();
    let preferences = editor.edited();
    match field {
        PreferenceField::PostingVisibility => {
            let current = preferences.posting_default_visibility;
            view! {
                <select
                    class="select select-sm"
                    aria-label="Default visibility"
                    prop:value=current.as_str()
                    disabled=disabled
                    on:change=move |event| {
                        if let (Some(on_change), Some(value)) =
                            (on_change, visibility_value(&event_target_value(&event)))
                        {
                            on_change.run((field, PreferenceValue::Visibility(value)));
                        }
                    }
                >
                    {VISIBILITIES
                        .into_iter()
                        .map(|value| {
                            view! {
                                <option value=value.as_str() selected=value == current>
                                    {visibility_label(value)}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            }
            .into_any()
        }
        PreferenceField::PostingQuotePolicy => {
            let current = preferences.posting_default_quote_policy;
            view! {
                <select
                    class="select select-sm"
                    aria-label="Who may quote new posts"
                    prop:value=current.to_string()
                    disabled=disabled
                    on:change=move |event| {
                        if let (Some(on_change), Ok(value)) =
                            (on_change, event_target_value(&event).parse::<QuotePolicy>())
                        {
                            on_change.run((field, PreferenceValue::QuotePolicy(value)));
                        }
                    }
                >
                    {QUOTE_POLICIES
                        .into_iter()
                        .map(|value| {
                            view! {
                                <option value=value.to_string() selected=value == current>
                                    {quote_policy_label(value)}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            }
            .into_any()
        }
        PreferenceField::ReadingExpandMedia => {
            let current = preferences.reading_expand_media;
            view! {
                <select
                    class="select select-sm"
                    aria-label="Media in timelines"
                    prop:value=current.to_string()
                    disabled=disabled
                    on:change=move |event| {
                        if let (Some(on_change), Ok(value)) =
                            (on_change, event_target_value(&event).parse::<ExpandMedia>())
                        {
                            on_change.run((field, PreferenceValue::ExpandMedia(value)));
                        }
                    }
                >
                    {EXPAND_MEDIA
                        .into_iter()
                        .map(|value| {
                            view! {
                                <option value=value.to_string() selected=value == current>
                                    {expand_media_label(value)}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
            }
            .into_any()
        }
        PreferenceField::PostingSensitive => checkbox(
            field,
            preferences.posting_default_sensitive,
            disabled,
            on_change,
            PreferenceValue::Sensitive,
        ),
        PreferenceField::ReadingExpandSpoilers => checkbox(
            field,
            preferences.reading_expand_spoilers,
            disabled,
            on_change,
            PreferenceValue::Spoilers,
        ),
        PreferenceField::ReadingAutoplayGifs => checkbox(
            field,
            preferences.reading_autoplay_gifs,
            disabled,
            on_change,
            PreferenceValue::Autoplay,
        ),
        PreferenceField::NotificationEmail => checkbox(
            field,
            preferences.notification_email,
            disabled,
            on_change,
            PreferenceValue::Email,
        ),
        PreferenceField::NotificationReport => checkbox(
            field,
            preferences.notification_report,
            disabled,
            on_change,
            PreferenceValue::Report,
        ),
        PreferenceField::PostingLanguage => {
            let current = preferences
                .posting_default_language
                .clone()
                .unwrap_or_default();
            view! {
                <input
                    type="text"
                    class="input input-sm input-bordered w-40"
                    aria-label="Default language"
                    placeholder="Server default"
                    prop:value=current
                    disabled=disabled
                    on:change=move |event| {
                        if let Some(on_change) = on_change {
                            let typed = event_target_value(&event);
                            let typed = typed.trim();
                            let value = (!typed.is_empty()).then(|| typed.to_owned());
                            on_change.run((field, PreferenceValue::Language(value)));
                        }
                    }
                />
            }
            .into_any()
        }
    }
}

fn checkbox(
    field: PreferenceField,
    current: bool,
    disabled: bool,
    on_change: Option<Callback<(PreferenceField, PreferenceValue)>>,
    wrap: fn(bool) -> PreferenceValue,
) -> AnyView {
    view! {
        <input
            type="checkbox"
            class="toggle toggle-sm"
            aria-label=field.label()
            prop:checked=current
            disabled=disabled
            on:change=move |event| {
                if let Some(on_change) = on_change {
                    on_change.run((field, wrap(event_target_checked(&event))));
                }
            }
        />
    }
    .into_any()
}

const VISIBILITIES: [Visibility; 4] = [
    Visibility::Public,
    Visibility::Unlisted,
    Visibility::Private,
    Visibility::Direct,
];

const QUOTE_POLICIES: [QuotePolicy; 3] = [
    QuotePolicy::Public,
    QuotePolicy::Followers,
    QuotePolicy::Nobody,
];

const EXPAND_MEDIA: [ExpandMedia; 3] = [
    ExpandMedia::Default,
    ExpandMedia::ShowAll,
    ExpandMedia::HideAll,
];

fn visibility_value(value: &str) -> Option<Visibility> {
    VISIBILITIES
        .into_iter()
        .find(|known| known.as_str() == value)
}

fn visibility_label(value: Visibility) -> &'static str {
    match value {
        Visibility::Public => "Public",
        Visibility::Unlisted => "Unlisted",
        Visibility::Private => "Followers only",
        Visibility::Direct => "Mentioned only",
    }
}

fn quote_policy_label(value: QuotePolicy) -> &'static str {
    match value {
        QuotePolicy::Public => "Anyone",
        QuotePolicy::Followers => "Followers",
        QuotePolicy::Nobody => "Nobody",
    }
}

fn expand_media_label(value: ExpandMedia) -> &'static str {
    match value {
        ExpandMedia::Default => "The server default",
        ExpandMedia::ShowAll => "Show all media",
        ExpandMedia::HideAll => "Hide all media",
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use nodus_client_core::model::PreferenceRefusal;

    fn painted(editor: PreferencesEditor) -> String {
        view! {
            <PreferencesCard
                editor=Signal::stored(editor)
                on_change=Some(Callback::new(|_| {}))
            />
        }
        .to_html()
    }

    /// One change is one key: the endpoint takes a partial body, and the editor builds it
    /// from the confirmed map rather than sending what it happens to hold.
    #[test]
    fn one_change_sends_only_the_changed_key() {
        let mut editor = PreferencesEditor::new(Preferences::default());
        let patch = editor
            .begin(
                PreferenceField::PostingVisibility,
                PreferenceValue::Visibility(Visibility::Unlisted),
            )
            .expect("a changed value is a patch");
        assert_eq!(
            serde_json::to_string(&patch).expect("serialises"),
            r#"{"posting:default:visibility":"unlisted"}"#
        );
    }

    /// An empty language field clears the key with an explicit JSON `null`, not by
    /// omitting it.
    #[test]
    fn clearing_the_language_sends_an_explicit_null() {
        let confirmed = Preferences {
            posting_default_language: Some("en".to_owned()),
            ..Preferences::default()
        };
        let mut editor = PreferencesEditor::new(confirmed);
        let patch = editor
            .begin(
                PreferenceField::PostingLanguage,
                PreferenceValue::Language(None),
            )
            .expect("a changed language is a patch");
        assert_eq!(
            serde_json::to_string(&patch).expect("serialises"),
            r#"{"posting:default:language":null}"#
        );
    }

    /// A refused key is the row's own sentence, and the control goes back to what the
    /// instance last confirmed.
    #[test]
    fn an_unknown_key_is_shown_in_its_row_and_the_value_reverts() {
        let mut editor = PreferencesEditor::new(Preferences::default());
        editor
            .begin(
                PreferenceField::PostingVisibility,
                PreferenceValue::Visibility(Visibility::Unlisted),
            )
            .expect("a changed value is a patch");
        let refusal = PreferenceRefusal::UnknownKey {
            key: "posting:default:visibility".to_owned(),
        };
        editor.refused(PreferenceField::PostingVisibility, refusal.to_string());

        assert_eq!(
            editor.edited().posting_default_visibility,
            Visibility::Public
        );
        assert_eq!(editor.saving(), None);
        let html = painted(editor);
        assert!(html.contains("role=\"alert\""), "{html}");
        assert!(
            html.contains("unknown preference: posting:default:visibility"),
            "{html}"
        );
        assert!(
            html.contains(r#"<option value="public" selected"#),
            "the row must show the confirmed value: {html}"
        );
    }

    /// The allowed set is the server's, kept verbatim inside the sentence.
    #[test]
    fn an_invalid_value_shows_the_allowed_set() {
        let mut editor = PreferencesEditor::new(Preferences::default());
        editor
            .begin(
                PreferenceField::PostingQuotePolicy,
                PreferenceValue::QuotePolicy(QuotePolicy::Nobody),
            )
            .expect("a changed value is a patch");
        let refusal = PreferenceRefusal::InvalidValue {
            key: "posting:default:quote_policy".to_owned(),
            value: "sometimes".to_owned(),
            allowed: "public, followers, nobody".to_owned(),
        };
        editor.refused(PreferenceField::PostingQuotePolicy, refusal.to_string());

        let html = painted(editor);
        assert!(
            html.contains("allowed values are public, followers, nobody"),
            "{html}"
        );
        assert!(html.contains("role=\"alert\""), "{html}");
    }

    /// The two notification switches say once, together, that nothing acts on them yet.
    #[test]
    fn the_notification_note_is_painted_exactly_once() {
        let html = painted(PreferencesEditor::new(Preferences::default()));
        let note = PreferenceGroup::Notifications
            .note()
            .expect("the notification group carries the note");
        assert_eq!(html.matches(note).count(), 1, "{html}");
        assert_eq!(html.matches("data-preference-note").count(), 1, "{html}");
    }

    /// Every key has one row, under its group, so a value cannot be silently dropped.
    #[test]
    fn every_key_is_painted_once() {
        let html = painted(PreferencesEditor::new(Preferences::default()));
        for field in PreferenceField::ALL {
            assert_eq!(
                html.matches(&format!("data-preference=\"{}\"", field.key()))
                    .count(),
                1,
                "missing row for {field:?}: {html}"
            );
        }
        for group in PreferenceGroup::ALL {
            assert!(
                html.contains(&format!("data-preference-group=\"{}\"", group.label())),
                "missing group {group:?}: {html}"
            );
        }
    }
}
