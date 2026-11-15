//! The starting values of the composer, read from the status being edited.

use nodus_client_core::model::{MediaAttachment, Status, Visibility};

/// Everything the form needs to know before the first keystroke.
pub struct Initial {
    /// Whether this is an edit, and of what.
    pub editing: Option<String>,
    /// The body, in plain text.
    pub text: String,
    /// The visibility it was posted with.
    pub visibility: Visibility,
    /// Attachments already on the status being edited.
    pub media: Vec<MediaAttachment>,
    /// The language the status was tagged with, or empty for instance default.
    pub language: String,
    /// The content warning, if any.
    pub spoiler: String,
    /// Whether the warning field starts open.
    pub warning_open: bool,
    /// Whether the media was marked sensitive.
    pub sensitive: bool,
}

impl Initial {
    /// Reads a status, or returns the blank draft.
    pub fn of(status: Option<&Status>) -> Self {
        let Some(status) = status else {
            return Self {
                editing: None,
                text: String::new(),
                visibility: Visibility::Public,
                media: Vec::new(),
                language: String::new(),
                spoiler: String::new(),
                warning_open: false,
                sensitive: false,
            };
        };
        Self {
            editing: Some(status.id.clone()),
            text: crate::routes::feed::posts::plain_text(&status.content),
            visibility: status.visibility,
            media: status.media_attachments.clone(),
            language: status.language.clone().unwrap_or_default(),
            spoiler: status.spoiler_text.clone(),
            warning_open: !status.spoiler_text.is_empty(),
            sensitive: status.sensitive,
        }
    }
}
