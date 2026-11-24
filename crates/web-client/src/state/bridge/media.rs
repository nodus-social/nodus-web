//! The media library: every picture this account uploaded, and the one delete.
//!
//! `GET /api/v1/media/library` answers rows naming what still uses each picture. The
//! server decides whether a delete is allowed, and says why in a 422 when it is not: that
//! sentence is returned as-is rather than rephrased, because "still used by a post" is the
//! server's own answer about the row the reader just clicked.

use serde::Deserialize;

/// One row, as `GET /api/v1/media/library` answers it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Row {
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub preview_url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created_at: Option<String>,
    /// `status`, `drive`, `style` or `none`.
    #[serde(default)]
    pub usage: String,
    #[serde(default)]
    pub usage_id: Option<String>,
    #[serde(default)]
    pub duplicate_of: Option<String>,
}

impl Row {
    /// The picture to draw: the preview when the server sent one.
    #[must_use]
    pub fn picture(&self) -> &str {
        if self.preview_url.is_empty() {
            &self.url
        } else {
            &self.preview_url
        }
    }
}

/// The media library transport, or the sentence a signed-out reader gets.
fn transport() -> Result<nodus_mastodon::BrowserInstance, String> {
    super::instance_transport()
        .ok_or_else(|| "Sign in to see the pictures you uploaded.".to_owned())
}

fn library_url(id: Option<&str>) -> Result<url::Url, String> {
    let mut segments = vec!["api", "v1", "media", "library"];
    if let Some(id) = id {
        segments.push(id);
    }
    transport()?
        .url_segments(&segments)
        .map_err(|error| error.to_string())
}

/// Reads the whole library.
pub async fn load() -> Result<Vec<Row>, String> {
    let transport = transport()?;
    transport
        .fetch(transport.get(library_url(None)?))
        .await
        .map_err(|error| error.to_string())
}

/// Deletes one picture.
///
/// A 422 is the server naming what still holds it, so the message is handed back exactly as
/// the server wrote it: it is the explanation the row shows, not a decode failure.
pub async fn delete(id: &str) -> Result<(), String> {
    let transport = transport()?;
    match transport
        .send_empty(transport.delete(library_url(Some(id))?))
        .await
    {
        Ok(()) => Ok(()),
        Err(nodus_mastodon::Error::Status {
            status: 422,
            message,
        }) if !message.trim().is_empty() => Err(message),
        Err(error) => Err(error.to_string()),
    }
}
