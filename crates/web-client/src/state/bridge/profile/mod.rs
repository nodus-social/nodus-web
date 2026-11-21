//! The profile's metadata rows: the six (label, value) pairs and the instance's verdict on
//! which of them are verified links.
//!
//! The write goes through the SDK, which owns the endpoint's wire shape: rows cannot ride in
//! a form body, so an update carrying them is sent as JSON. The account model does not carry
//! `fields` yet, so the rows are read from a credential answer into a local struct — both
//! the read that opens the editor and the read after a save, since the instance is the
//! authority on what it stored and which links it verified.

use std::cell::RefCell;

use nodus_client_core::actions::AccountActions;
use nodus_client_core::model::{Account, ProfileField as SdkField, ProfileUpdate};
use nodus_client_core::rest::Transport;
use nodus_mastodon::rest::RestInstance;
use nodus_pages::pages::settings::form::{ProfileField, parse_field_error};
use serde::Deserialize;

#[cfg(test)]
mod tests;

const VERIFY: &str = "/api/v1/accounts/verify_credentials";

/// The rows inside a credential answer, the account's own members ignored.
#[derive(Debug, Deserialize)]
struct CredentialAnswer {
    #[serde(default)]
    fields: Vec<WireField>,
}

/// One metadata row as the wire spells it.
#[derive(Debug, Deserialize)]
struct WireField {
    name: String,
    value: String,
    #[serde(default)]
    verified_at: Option<String>,
}

impl From<WireField> for ProfileField {
    fn from(field: WireField) -> Self {
        Self {
            name: field.name,
            value: field.value,
            verified_at: field.verified_at,
        }
    }
}

impl CredentialAnswer {
    fn rows(self) -> Vec<ProfileField> {
        self.fields.into_iter().map(ProfileField::from).collect()
    }
}

/// Why a save did not land, in the shape the editor can show.
///
/// A `422` that names a row is placed beside that row; everything else — a revoked token, a
/// missing scope, a sentence about the whole profile — belongs at form level.
#[derive(Debug)]
pub enum SaveError {
    /// The instance blamed one row, counted from zero as the editor lists it.
    Row {
        /// Position of the offending row.
        row: usize,
        /// The instance's sentence.
        message: String,
    },
    /// The instance blamed no row, or the call never reached it.
    Form(String),
}

thread_local! {
    /// The rows the last credential read answered with. A profile is opened more often
    /// than the settings card, and the rows do not change between the two, so the read is
    /// kept here rather than repeated: a per-profile refetch would be a new request for
    /// every mount, and the account document carries no rows to read instead.
    static LAST_ROWS: RefCell<Option<Vec<ProfileField>>> = const { RefCell::new(None) };
}

/// The rows the last credential read answered with, if one has landed.
///
/// `None` means nobody has read the credentials yet in this page; the caller asks for the
/// read rather than drawing an empty profile as if the account had no rows.
#[must_use]
pub fn cached_fields() -> Option<Vec<ProfileField>> {
    LAST_ROWS.with(|rows| rows.borrow().clone())
}

fn remember(rows: &[ProfileField]) {
    LAST_ROWS.with(|held| *held.borrow_mut() = Some(rows.to_vec()));
}

/// The signed-in account's metadata rows.
pub async fn fields() -> Result<Vec<ProfileField>, String> {
    let transport = super::instance_transport().ok_or_else(|| "Not signed in.".to_owned())?;
    let rows = read(&transport).await?;
    remember(&rows);
    Ok(rows)
}

/// One credential read: the rows as the instance holds them, `verified_at` included.
async fn read<T: Transport>(transport: &RestInstance<T>) -> Result<Vec<ProfileField>, String> {
    let url = transport.url(VERIFY).map_err(|error| error.to_string())?;
    let answer: CredentialAnswer = transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())?;
    Ok(answer.rows())
}

/// Writes the name, the bio and the rows in one partial update, and returns the rows as the
/// instance stored them.
///
/// One request rather than one per field: the instance answers with the whole account, and
/// a save that sent the text and the rows separately could leave the two disagreeing if the
/// second call failed.
pub async fn save(
    display_name: String,
    note: String,
    fields: Vec<ProfileField>,
) -> Result<Vec<ProfileField>, SaveError> {
    let transport =
        super::instance_transport().ok_or_else(|| SaveError::Form("Not signed in.".to_owned()))?;
    let rows = write(&transport, display_name, note, fields).await?;
    remember(&rows);
    Ok(rows)
}

/// The save itself, against any transport so the wire body can be inspected in a test.
///
/// The rows are always sent, an empty list included: the instance reads the list as the
/// profile's whole table, so a save that omitted it after the writer removed every row
/// would leave the old rows standing. Sending the list as JSON is the SDK's choice of
/// encoding, not this crate's.
async fn write<T: Transport>(
    transport: &RestInstance<T>,
    display_name: String,
    note: String,
    fields: Vec<ProfileField>,
) -> Result<Vec<ProfileField>, SaveError> {
    let rows: Vec<SdkField> = fields
        .iter()
        .map(|field| SdkField::new(field.name.trim(), field.value.trim()))
        .collect();
    let update = ProfileUpdate::new()
        .display_name(display_name)
        .note(note)
        .fields(rows);
    let account: Account =
        transport
            .update_profile(&update)
            .await
            .map_err(|error| match error {
                nodus_client_core::Error::Status {
                    status: 422,
                    message,
                } => match parse_field_error(&message) {
                    Some((row, message)) => SaveError::Row { row, message },
                    None => SaveError::Form(message),
                },
                error => SaveError::Form(error.to_string()),
            })?;
    super::dispatch(nodus_client_sdk::Command::AccountsUpserted {
        accounts: vec![account],
    });
    // The rows the editor paints are the instance's, not the ones this module trimmed: an
    // instance stamps `verified_at` and is free to store something other than what arrived.
    // The update answer cannot carry them (`Account` has no rows), so they are read back.
    read(transport).await.map_err(SaveError::Form)
}
