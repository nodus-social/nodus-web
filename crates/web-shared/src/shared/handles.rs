//! Where an account lives, and the address that says so.
//!
//! An id is only meaningful on the instance that minted it. Mastodon numbers its accounts
//! and nodus uses ULIDs, so `/accounts/1234` and `/accounts/01M7…` are not addresses: they
//! are "some instance's account 1234" with the instance left out. Every link this site
//! draws therefore carries the host, exactly as the ActivityPub forms do
//! (`/accounts/{host}/{id}`, `/status/{host}/{id}`), and a reader can paste one to another
//! instance and be understood.
//!
//! The helpers here keep that rule in one place: the host comes from the `acct` when the
//! account names one, and from the caller's own instance when it does not.

/// The instance an `acct` names: `user@example.social` → `example.social`.
///
/// `None` for a local account, because a bare `user` says "here" and only the caller knows
/// where "here" is.
pub fn host_of_acct(acct: &str) -> Option<&str> {
    acct.split_once('@')
        .map(|(_, host)| host.trim())
        .filter(|host| !host.is_empty())
}

/// The name part of an `acct`: `user@host` → `user`.
///
/// Profiles are addressed by name, not by id: an id is the instance's private numbering,
/// while the name is what the reader can type, paste, and carry to another instance.
pub fn name_of_acct(acct: &str) -> &str {
    acct.split_once('@').map_or(acct, |(name, _)| name).trim()
}

/// The instance an absolute url names: `https://example.social/notes/1` → `example.social`.
///
/// Used where an account does not name its instance but its post does: a status fetched
/// from another instance carries that instance's url, and the id beside it belongs to the
/// same place.
pub fn host_of_url(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim();
    (!host.is_empty()).then_some(host)
}

/// `/accounts/{host}/{id}`, or the bare `/accounts/{id}` when no host is known.
///
/// The fallback exists for the one case that cannot be resolved, not as a preference: a
/// link without a host is whoever happens to be reading it.
pub fn account_path(host: Option<&str>, id: &str) -> String {
    match host.map(str::trim).filter(|host| !host.is_empty()) {
        Some(host) => format!("/accounts/{host}/{}", id.trim()),
        None => format!("/accounts/{}", id.trim()),
    }
}

/// `/status/{host}/{id}`, or `/status/{id}` when no host is known.
pub fn status_path(host: Option<&str>, id: &str) -> String {
    match host.map(str::trim).filter(|host| !host.is_empty()) {
        Some(host) => format!("/status/{host}/{}", id.trim()),
        None => format!("/status/{}", id.trim()),
    }
}

/// The permanent address of the account an `acct` names: `/accounts/{host}/{name}`.
///
/// `local_host` is what this instance calls itself, used only when the `acct` does not
/// already name an instance. The name is taken from the `acct` rather than from an id, so
/// the address survives a database move and means the same thing to every instance.
pub fn profile_path_for(acct: &str, local_host: Option<&str>) -> String {
    account_path(host_of_acct(acct).or(local_host), name_of_acct(acct))
}

#[cfg(test)]
mod tests {
    use super::{account_path, host_of_acct, host_of_url, profile_path_for, status_path};

    #[test]
    fn an_acct_names_its_instance_only_when_it_has_one() {
        assert_eq!(host_of_acct("lux@mastodon.social"), Some("mastodon.social"));
        assert_eq!(host_of_acct("luxuride"), None);
        assert_eq!(host_of_acct("lux@"), None);
        assert_eq!(host_of_acct("lux@  "), None);
    }

    #[test]
    fn a_url_names_its_instance() {
        assert_eq!(
            host_of_url("https://mastodon.social/@lux/115"),
            Some("mastodon.social")
        );
        assert_eq!(host_of_url("https://baraag.net"), Some("baraag.net"));
        assert_eq!(
            host_of_url("https://example.social?x=1"),
            Some("example.social")
        );
        assert_eq!(host_of_url("not a url"), None);
        assert_eq!(host_of_url(""), None);
    }

    #[test]
    fn links_carry_the_instance_that_minted_the_id() {
        assert_eq!(
            account_path(Some("nodus.luxuride.eu"), "01M7GK5K"),
            "/accounts/nodus.luxuride.eu/01M7GK5K"
        );
        assert_eq!(
            status_path(Some("nodus.luxuride.eu"), "01M7GK5K"),
            "/status/nodus.luxuride.eu/01M7GK5K"
        );
        // Without a known host the id keeps the old shape rather than inventing one.
        assert_eq!(account_path(None, "01M7GK5K"), "/accounts/01M7GK5K");
    }

    #[test]
    fn a_profile_address_names_the_instance_and_the_handle() {
        // A local account carries no host, so the reading instance supplies it.
        assert_eq!(
            profile_path_for("luxuride", Some("nodus.luxuride.eu")),
            "/accounts/nodus.luxuride.eu/luxuride"
        );
        // A remote account names its own, whichever instance paints the link.
        assert_eq!(
            profile_path_for("gargron@mastodon.social", Some("nodus.luxuride.eu")),
            "/accounts/mastodon.social/gargron"
        );
        // A name is portable; an id is not, which is why links never carry one.
        assert_eq!(profile_path_for("luxuride", None), "/accounts/luxuride");
    }
}
