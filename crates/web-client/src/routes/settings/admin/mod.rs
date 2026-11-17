//! The admin-scope grant, shared by the moderation and administration gates.
//!
//! This is not a settings section: moderation is a place, reached from the rail, and the
//! permission it needs is asked for at the door rather than at sign-in.

use crate::state::bridge::admin_scope;
use crate::state::oauth;

/// Starts the authorization that gives this browser an admin-scoped token for the
/// signed-in account.
///
/// The moderation screens call this from their own gate: asking for `admin:read` at sign-in
/// would hand every reader far more access than a reader needs, so the scope is requested
/// the moment it is actually needed and can be forgotten again afterwards. `return_to` is
/// the in-app path the callback should come back to.
pub async fn request_grant(return_to: &str) -> Result<(), String> {
    let (host, account) = admin_scope().ok_or("There is no instance to ask yet.")?;
    if account.is_empty() {
        return Err(
            "This browser has not finished signing in, so it cannot ask for a moderation \
             grant yet."
                .to_owned(),
        );
    }
    oauth::begin_admin(&host, &account, return_to).await
}
