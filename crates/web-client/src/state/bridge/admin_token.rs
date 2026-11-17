//! The admin grant: a second token, kept per account and instance, used only by
//! moderation calls.
//!
//! A token without `admin:read` answers 403 to every `/api/v1/admin/*` request, so a
//! moderator signed in normally would see an empty Administration tab that looks like a
//! bug. The grant is separate on purpose: the ordinary session stays a token that cannot
//! moderate, and only the screens that need it reach for this one.
//!
//! A grant belongs to the account that asked for it, not to the instance: the token acts
//! as that account, and a second account signed in to the same instance has not consented
//! to the wider scope. Keying by both also means switching accounts cannot silently lend
//! one moderator's authority to another.

use core::cell::RefCell;

use web_sys::Storage;

/// `localStorage` key; the version lives in the key so a new record shape can simply
/// pick a new one and this build never sees the old value.
const KEY: &str = "nodus-admin/2";

/// One account's grant on one instance.
struct Grant {
    host: String,
    account: String,
    token: String,
}

/// The stored grants, parsed whole and written back whole.
///
/// Kept as a plain value rather than a bare `Vec` so the storage format can be exercised
/// without a browser.
#[derive(Default)]
struct Grants(Vec<Grant>);

impl Grants {
    /// Reads `<host>\t<account>\t<token>` lines, dropping any it cannot parse.
    ///
    /// Version 1 wrote `<host>\t<token>`; those records are deliberately not migrated:
    /// they do not name an account, and guessing which of the instance's accounts
    /// granted them would be worse than asking for the grant again.
    fn parse(raw: &str) -> Self {
        let grants = raw
            .lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let host = fields.next()?;
                let account = fields.next()?;
                let token = fields.next()?;
                if fields.next().is_some()
                    || host.is_empty()
                    || account.is_empty()
                    || token.is_empty()
                {
                    return None;
                }
                Some(Grant {
                    host: host.to_owned(),
                    account: account.to_owned(),
                    token: token.to_owned(),
                })
            })
            .collect();
        Self(grants)
    }

    fn render(&self) -> String {
        self.0
            .iter()
            .map(|grant| format!("{}\t{}\t{}", grant.host, grant.account, grant.token))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn token(&self, host: &str, account: &str) -> Option<String> {
        self.0
            .iter()
            .find(|grant| grant.host == host && grant.account == account)
            .map(|grant| grant.token.clone())
    }

    fn put(&mut self, host: &str, account: &str, token: &str) {
        self.0
            .retain(|grant| grant.host != host || grant.account != account);
        self.0.push(Grant {
            host: host.to_owned(),
            account: account.to_owned(),
            token: token.to_owned(),
        });
    }

    fn remove(&mut self, host: &str, account: &str) {
        self.0
            .retain(|grant| grant.host != host || grant.account != account);
    }
}

thread_local! {
    /// Parsed once, written back whole: the record is a handful of lines, and rewriting
    /// it is cheaper than reasoning about a partial update.
    static GRANTS: RefCell<Option<Grants>> = const { RefCell::new(None) };
}

/// The admin token for `account` on `host`, when one was granted.
pub(super) fn load(host: &str, account: &str) -> Option<String> {
    with_grants(|grants| grants.token(host, account))
}

/// Stores `token` for `account` on `host`, replacing an earlier grant for the same pair.
pub(super) fn store(host: &str, account: &str, token: &str) {
    with_grants(|grants| grants.put(host, account, token));
    save();
}

/// Forgets `account`'s grant on `host`.
///
/// Called on sign-out of an account: the token moderates as that account, so it must not
/// outlive the session that the operator thinks they closed.
pub(super) fn clear(host: &str, account: &str) {
    with_grants(|grants| grants.remove(host, account));
    save();
}

fn with_grants<R>(f: impl FnOnce(&mut Grants) -> R) -> R {
    GRANTS.with_borrow_mut(|slot| {
        if slot.is_none() {
            *slot = Some(read().unwrap_or_default());
        }
        f(slot.as_mut().expect("just filled"))
    })
}

fn storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn read() -> Option<Grants> {
    let raw = storage()?.get_item(KEY).ok()??;
    Some(Grants::parse(&raw))
}

fn save() {
    let Some(storage) = storage() else {
        return;
    };
    let text = with_grants(|grants| grants.render());
    // The fields are tab-free by construction (a canonical host, a Mastodon account id
    // and a base64url token), so one line can be split without quoting.
    let _ = storage.set_item(KEY, &text);
}

#[cfg(test)]
mod tests {
    use super::Grants;

    #[test]
    fn two_accounts_on_one_host_keep_independent_tokens() {
        let mut grants = Grants::default();
        grants.put("mastodon.example", "account-1", "token-1");
        grants.put("mastodon.example", "account-2", "token-2");

        assert_eq!(
            grants.token("mastodon.example", "account-1").as_deref(),
            Some("token-1")
        );
        assert_eq!(
            grants.token("mastodon.example", "account-2").as_deref(),
            Some("token-2")
        );

        grants.put("mastodon.example", "account-1", "replacement");
        assert_eq!(
            grants.token("mastodon.example", "account-1").as_deref(),
            Some("replacement")
        );
        assert_eq!(
            grants.token("mastodon.example", "account-2").as_deref(),
            Some("token-2")
        );
    }

    #[test]
    fn a_grant_for_one_host_does_not_answer_for_another() {
        let mut grants = Grants::default();
        grants.put("one.example", "account-1", "token-1");
        assert_eq!(grants.token("two.example", "account-1"), None);
    }

    #[test]
    fn removing_one_account_leaves_the_other() {
        let mut grants = Grants::default();
        grants.put("mastodon.example", "account-1", "token-1");
        grants.put("mastodon.example", "account-2", "token-2");
        grants.remove("mastodon.example", "account-1");
        assert_eq!(grants.token("mastodon.example", "account-1"), None);
        assert_eq!(
            grants.token("mastodon.example", "account-2").as_deref(),
            Some("token-2")
        );
    }

    #[test]
    fn records_round_trip_through_the_storage_format() {
        let mut grants = Grants::default();
        grants.put("localhost:3000", "account-1", "token-1");
        grants.put("mastodon.example", "account-2", "token-2");
        let reparsed = Grants::parse(&grants.render());
        assert_eq!(
            reparsed.token("localhost:3000", "account-1").as_deref(),
            Some("token-1")
        );
        assert_eq!(
            reparsed.token("mastodon.example", "account-2").as_deref(),
            Some("token-2")
        );
    }

    #[test]
    fn legacy_two_field_records_are_ignored() {
        let grants = Grants::parse("mastodon.example\told-token");
        assert_eq!(grants.token("mastodon.example", "account-1"), None);
        assert_eq!(grants.render(), "");
    }

    #[test]
    fn records_with_empty_fields_are_ignored() {
        let grants = Grants::parse("mastodon.example\t\t\n\taccount-1\ttoken\n");
        assert_eq!(grants.token("mastodon.example", ""), None);
        assert_eq!(grants.token("", "account-1"), None);
    }
}
