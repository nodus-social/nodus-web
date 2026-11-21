//! The profile's metadata rows: the six (label, value) pairs and the instance's verdict on
//! which of them are verified links.
//!
//! The write goes through the SDK, which owns the endpoint's wire shape: rows cannot ride in
//! a form body, so an update carrying them is sent as JSON. The SDK's account model carries
//! those rows on both answers — the credential read that opens the editor, and the update's
//! own answer — so `verified_at` is read from the account instead of decoded a second time.

use nodus_client_core::InstanceApi;
use nodus_client_core::actions::AccountActions;
use nodus_client_core::model::{Account, ProfileField as SdkField, ProfileUpdate};
use nodus_client_core::rest::Transport;
use nodus_mastodon::rest::RestInstance;
use nodus_pages::pages::settings::form::{ProfileField, parse_field_error};

#[cfg(test)]
mod tests;

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

/// The rows an account carries, in row order.
fn account_rows(account: &Account) -> Vec<ProfileField> {
    account
        .fields
        .iter()
        .map(|field| ProfileField {
            name: field.name.clone(),
            value: field.value.clone(),
            verified_at: field.verified_at.clone(),
        })
        .collect()
}

/// The signed-in account's metadata rows, as the instance holds them.
pub async fn fields() -> Result<Vec<ProfileField>, String> {
    let transport = super::instance_transport().ok_or_else(|| "Not signed in.".to_owned())?;
    let account = transport
        .verify_credentials()
        .await
        .map_err(|error| error.to_string())?;
    Ok(account_rows(&account))
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
    write(&transport, display_name, note, fields).await
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
    // The rows the editor paints are the instance's, not the ones this module trimmed: an
    // instance stamps `verified_at` and is free to store something other than what arrived.
    let stored = account_rows(&account);
    super::dispatch(nodus_client_sdk::Command::AccountsUpserted {
        accounts: vec![account],
    });
    Ok(stored)
}
