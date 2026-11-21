//! The profile's metadata rows: the six (label, value) pairs and the instance's verdict on
//! which of them are verified links.
//!
//! The rows travel as a JSON member, unlike the rest of the profile: the endpoint's form
//! encoding cannot spell a nested list, so `PATCH /api/v1/accounts/update_credentials` gets
//! a JSON body for the whole update rather than a form body with one impossible key. The
//! account model in the SDK does not carry `fields` yet, so the answer is read into a local
//! struct that keeps the account for the store and the rows for the editor.

use std::cell::RefCell;

use nodus_client_core::model::Account;
use nodus_pages::pages::settings::form::{ProfileField, parse_field_error};
use serde::Deserialize;

const CREDENTIALS: &str = "/api/v1/accounts/update_credentials";
const VERIFY: &str = "/api/v1/accounts/verify_credentials";

/// What the instance answers to a credential read or write.
///
/// `flatten` because the rows sit beside the account's own members on the wire; the account
/// is still the value the store wants, and the rows are the pair this editor displays.
#[derive(Debug, Deserialize)]
struct CredentialAnswer {
    #[serde(flatten)]
    account: Account,
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
    let url = transport.url(VERIFY).map_err(|error| error.to_string())?;
    let answer: CredentialAnswer = transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())?;
    let rows = answer.rows();
    remember(&rows);
    Ok(rows)
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
    let url = transport
        .url(CREDENTIALS)
        .map_err(|error| SaveError::Form(error.to_string()))?;
    let rows: Vec<serde_json::Value> = fields
        .iter()
        .map(|field| {
            serde_json::json!({
                "name": field.name.trim(),
                "value": field.value.trim(),
            })
        })
        .collect();
    let body = serde_json::json!({
        "display_name": display_name,
        "note": note,
        "fields": rows,
    });
    let bytes = serde_json::to_vec(&body).map_err(|error| SaveError::Form(error.to_string()))?;
    let answer: CredentialAnswer = transport
        .fetch(transport.patch_body(url, "application/json", bytes))
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
        accounts: vec![answer.account.clone()],
    });
    let rows = answer.rows();
    remember(&rows);
    Ok(rows)
}
