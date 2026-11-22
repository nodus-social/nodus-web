//! What the signed-in account's own credentials document says about its staff role.
//!
//! Every Mastodon-shaped server answers the authenticated account's own document at
//! `GET /api/v1/accounts/verify_credentials`, and it is the document both paints should
//! read: it is the one account read a session token is allowed to make, so it answers the
//! same way for an administrator, a moderator and a plain account. The admin-scoped
//! account list is not that question — it refuses every token without `admin:read`,
//! including the SPA's own `read write follow` grant, so it reads "not staff" about the
//! people who run the instance.
//!
//! The SDK's `Account` model names only the portable Mastodon fields and drops `role`, so
//! the raw document is read here. Nodus sends both spellings on the own document (`role`
//! for compatibility, `staff_role` as its own); Mastodon sends `role` alone and only for a
//! staff account. Misskey has no such document — its `/api/i` carries `isAdmin`/
//! `isModerator`, which this console does not read yet — so a Misskey session reports no
//! role and draws no staff doors.

use super::super::instance_transport;

/// The role an own credentials document names, and what it takes to use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::state::bridge) struct OwnRole {
    /// The wider of the two doors; `false` is the moderator's.
    pub(in crate::state::bridge) administrator: bool,
    /// Whether Nodus asks for a separate admin grant before the panels work.
    ///
    /// Nodus carries its admin scopes in an OAuth grant apart from the session token, so a
    /// role in its own `staff_role` spelling still has to walk the consent screen. A
    /// Mastodon role arrives on the session token itself, so it opens the door directly.
    pub(in crate::state::bridge) needs_grant: bool,
}

impl OwnRole {
    /// Whether this role lets the rail draw its door without asking for a grant first.
    ///
    /// The rail draws a door only for a working role; a role that still needs a grant is
    /// the gate's consent prompt, not a door.
    pub(in crate::state::bridge) const fn opens_door(self) -> bool {
        !self.needs_grant
    }
}

/// The fields of `verify_credentials` this console reads.
#[derive(Debug, Default, serde::Deserialize)]
struct Credentials {
    #[serde(default)]
    role: Option<RoleName>,
    #[serde(default)]
    staff_role: Option<String>,
}

/// Mastodon's `role` entity (`{id, name, color, permissions, highlighted}`).
#[derive(Debug, serde::Deserialize)]
struct RoleName {
    #[serde(default)]
    name: String,
}

impl Credentials {
    /// The role this document names, if any.
    fn own_role(&self) -> Option<OwnRole> {
        if let Some(spelling) = self.staff_role.as_deref() {
            return match spelling {
                "admin" => Some(OwnRole {
                    administrator: true,
                    needs_grant: true,
                }),
                "moderator" => Some(OwnRole {
                    administrator: false,
                    needs_grant: true,
                }),
                _ => None,
            };
        }
        let name = self.role.as_ref()?.name.trim();
        // Mastodon's role name is the administrator's own label, so a custom word is still
        // a staff account: the presence of the entity is the signal, and the wider door is
        // only claimed when the name spells it.
        (!name.is_empty()).then(|| OwnRole {
            administrator: name.eq_ignore_ascii_case("admin"),
            needs_grant: false,
        })
    }
}

/// Reads the signed-in account's role from its own credentials document.
///
/// The ordinary session transport, not the admin grant: the question is what this account
/// is, and a grant is the consequence of the answer rather than the way to ask it.
pub(in crate::state::bridge) async fn own_role() -> Result<Option<OwnRole>, String> {
    let transport = instance_transport().ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/accounts/verify_credentials")
        .map_err(|error| error.to_string())?;
    let bytes = transport
        .fetch_bytes(transport.get(url))
        .await
        .map_err(|error| format!("The instance did not answer with this account: {error}"))?;
    let credentials: Credentials = serde_json::from_slice(&bytes)
        .map_err(|error| format!("The account document could not be read: {error}"))?;
    Ok(credentials.own_role())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mastodon_role_opens_the_door_without_a_grant() {
        // The shape Mastodon returns on an own document; the name is custom on purpose,
        // because an instance's administrator may name its roles whatever it likes.
        let document = serde_json::json!({
            "id": "1",
            "acct": "owner",
            "role": {
                "id": "3",
                "name": "Owner",
                "color": "",
                "permissions": "1",
                "highlighted": true
            }
        });
        let credentials: Credentials = serde_json::from_value(document).unwrap();
        let role = credentials.own_role().expect("a role entity is staff");
        assert!(
            !role.administrator,
            "a custom name claims the narrower door"
        );
        assert!(role.opens_door());
    }

    #[test]
    fn a_plain_document_draws_no_door() {
        let document = serde_json::json!({
            "id": "2",
            "acct": "user",
            "role": null,
            "staff_role": null
        });
        let credentials: Credentials = serde_json::from_value(document).unwrap();
        assert_eq!(credentials.own_role(), None);
    }

    #[test]
    fn the_nodus_spelling_keeps_the_grant_prompt() {
        let document = serde_json::json!({
            "id": "3",
            "acct": "mod",
            "role": { "name": "Moderator" },
            "staff_role": "moderator"
        });
        let credentials: Credentials = serde_json::from_value(document).unwrap();
        let role = credentials.own_role().expect("a staff spelling is a role");
        assert!(role.needs_grant);
        assert!(!role.opens_door());
    }
}
