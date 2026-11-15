//! Pure multi-account registry, per-account cache, and SSR reload policy.
//!
//! Kept free of `web_sys` and `wasm_bindgen` so both the wasm client and native unit
//! tests exercise the exact same switching, token-tracking, and cache-isolation rules.

use std::collections::HashMap;

/// Key identifying a signed-in account on a specific instance: `(host, account_id)`.
pub type AccountKey = (String, String);

/// In-memory cache isolated per `(host, account_id)`.
///
/// Prevents instance metadata (supported languages, custom emojis, character limits) and
/// account defaults (posting language) from leaking across accounts when switching.
#[derive(Debug, Clone)]
pub struct AccountCache<V> {
    entries: HashMap<AccountKey, V>,
}

impl<V> Default for AccountCache<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> AccountCache<V> {
    /// Creates an empty per-account cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Reads the cached value for `key`, if one was recorded for that `(host, account_id)`.
    #[must_use]
    pub fn get(&self, key: &AccountKey) -> Option<&V> {
        self.entries.get(key)
    }

    /// Records `value` under `key`.
    pub fn insert(&mut self, key: AccountKey, value: V) {
        self.entries.insert(key, value);
    }
}

/// Decides whether `announce_to_server` should trigger a full-page reload after
/// `POST /api/session` succeeds.
///
/// When the client has already adopted a session (`has_session == true`), the signed-in
/// shell is already mounted and reloading mid-flight would cancel/duplicate boot-time
/// timeline fetches. Only an un-adopted page that has not already reloaded once needs a
/// reload so the SSR host can paint the signed-in document.
#[must_use]
pub const fn announce_should_reload(has_session: bool, already_reloaded: bool) -> bool {
    !has_session && !already_reloaded
}

/// Persisted identity metadata for one switchable account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountMeta {
    /// Instance host, as the user typed it.
    pub host: String,
    /// Bearer token for that instance.
    pub token: String,
    /// Account id on that instance.
    pub id: String,
    /// `user@host`, for display.
    pub acct: String,
}

impl AccountMeta {
    /// Stable switcher key: `"{host}|{id}"`.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}|{}", self.host, self.id)
    }
}

#[derive(Debug)]
struct LiveEntry<S> {
    state: S,
    meta: AccountMeta,
}

/// Tracks installed per-account SDKs, the staged `pending` SDK before its account id is
/// verified, and the bearer `token` for the active account.
#[derive(Debug)]
pub struct AccountRegistry<S> {
    live: HashMap<String, LiveEntry<S>>,
    pending: Option<(String, S)>,
    token: Option<String>,
}

impl<S> Default for AccountRegistry<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> AccountRegistry<S> {
    /// Creates an empty registry with no active token.
    #[must_use]
    pub fn new() -> Self {
        Self {
            live: HashMap::new(),
            pending: None,
            token: None,
        }
    }

    /// Stages a newly built SDK before its session identity is known, updating the active
    /// bearer token.
    pub fn stage(&mut self, token: String, state: S) {
        self.token = Some(token.clone());
        self.pending = Some((token, state));
    }

    /// Moves the staged SDK (when it matches `is_match`) into `live` under `meta.key()`.
    ///
    /// Pass `activate = true` when the registered account is becoming the active account on
    /// screen; pass `false` for a background restore after the user already switched away.
    pub fn register(
        &mut self,
        meta: AccountMeta,
        activate: bool,
        is_staged: impl FnOnce(&S) -> bool,
    ) {
        let key = meta.key();
        let staged = self.pending.take();
        if let Some((staged_token, state)) = staged {
            if is_staged(&state) {
                if activate {
                    self.token = Some(meta.token.clone());
                }
                self.live.insert(key, LiveEntry { state, meta });
                return;
            }
            // The pending entry belongs to a newer `stage` call; put it back.
            self.pending = Some((staged_token, state));
        }
        if activate {
            self.token = Some(meta.token.clone());
        }
        if let Some(entry) = self.live.get_mut(&key) {
            entry.meta = meta;
        }
    }

    /// Switches to `key`, reusing an existing live entry or lazily installing it from
    /// `stored` via `build`, and updates the active bearer token.
    ///
    /// Returns the active state reference and the account metadata so callers can persist
    /// `nodus.session` and announce the switched session to the SSR host.
    pub fn switch<E>(
        &mut self,
        key: &str,
        stored: &[AccountMeta],
        build: impl FnOnce(&AccountMeta) -> Result<S, E>,
        map_missing: impl FnOnce(&str) -> E,
    ) -> Result<(&S, AccountMeta), E> {
        if !self.live.contains_key(key) {
            let meta = stored
                .iter()
                .find(|account| account.key() == key)
                .cloned()
                .ok_or_else(|| map_missing("That account needs its session restored first."))?;
            let state = build(&meta)?;
            self.live.insert(key.to_owned(), LiveEntry { state, meta });
        }
        let entry = self
            .live
            .get(key)
            .expect("entry was either present or just inserted");
        self.token = Some(entry.meta.token.clone());
        Ok((&entry.state, entry.meta.clone()))
    }

    /// Removes `key` from the live registry.
    pub fn remove(&mut self, key: &str) {
        self.live.remove(key);
    }

    /// Clears the active bearer token when no signed-in account remains.
    pub fn clear_token(&mut self) {
        self.token = None;
    }

    /// The bearer token for the currently active account, if any.
    #[must_use]
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// Whether `key` currently has a live entry in memory.
    #[must_use]
    pub fn contains_live(&self, key: &str) -> bool {
        self.live.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account_a() -> AccountMeta {
        AccountMeta {
            host: "alpha.example".to_owned(),
            token: "token-a".to_owned(),
            id: "1".to_owned(),
            acct: "ada@alpha.example".to_owned(),
        }
    }

    fn account_b() -> AccountMeta {
        AccountMeta {
            host: "beta.example".to_owned(),
            token: "token-b".to_owned(),
            id: "2".to_owned(),
            acct: "grace@beta.example".to_owned(),
        }
    }

    #[test]
    fn restore_registers_pending_into_live_and_switch_lazily_installs_secondary() {
        let a = account_a();
        let b = account_b();
        let stored = vec![a.clone(), b.clone()];
        let mut reg: AccountRegistry<String> = AccountRegistry::new();

        // Reload: install() stages A, then restore() unconditionally calls register().
        reg.stage(a.token.clone(), "sdk-a".to_owned());
        reg.register(a.clone(), true, |state| state == "sdk-a");
        assert!(reg.contains_live(&a.key()));
        assert_eq!(reg.token(), Some("token-a"));

        // Secondary account B is not in `live` after reload; switch lazily installs it.
        let mut builds = 0;
        let (state_b, meta_b) = reg
            .switch(
                &b.key(),
                &stored,
                |meta| {
                    builds += 1;
                    Ok::<_, String>(format!("sdk-{}", meta.id))
                },
                str::to_owned,
            )
            .expect("switching to stored account B must succeed after reload");
        assert_eq!(builds, 1);
        assert_eq!(state_b, "sdk-2");
        assert_eq!(meta_b, b);
        assert_eq!(reg.token(), Some("token-b"));

        // Switching back to A reuses the existing live SDK without rebuilding it and restores token A.
        let (state_a, meta_a) = reg
            .switch(
                &a.key(),
                &stored,
                |_| {
                    builds += 1;
                    Ok::<_, String>("rebuilt-a".to_owned())
                },
                str::to_owned,
            )
            .expect("switching back to A must reuse live entry");
        assert_eq!(builds, 1);
        assert_eq!(state_a, "sdk-a");
        assert_eq!(meta_a, a);
        assert_eq!(reg.token(), Some("token-a"));
    }

    #[test]
    fn switching_to_unknown_account_fails_without_clobbering_active_token() {
        let a = account_a();
        let mut reg: AccountRegistry<String> = AccountRegistry::new();
        reg.stage(a.token.clone(), "sdk-a".to_owned());
        reg.register(a.clone(), true, |_| true);

        let err = reg
            .switch(
                "missing.example|99",
                &[a],
                |_| Ok::<_, String>("unused".to_owned()),
                str::to_owned,
            )
            .unwrap_err();
        assert_eq!(err, "That account needs its session restored first.");
        assert_eq!(reg.token(), Some("token-a"));
    }

    #[test]
    fn background_register_does_not_overwrite_switched_token_or_newer_pending() {
        let a = account_a();
        let b = account_b();
        let mut reg: AccountRegistry<String> = AccountRegistry::new();

        // Boot stages A, then user switches to B before A's verify_credentials returns.
        reg.stage(a.token.clone(), "sdk-a".to_owned());
        reg.switch(
            &b.key(),
            &[a.clone(), b.clone()],
            |meta| Ok::<_, String>(format!("sdk-{}", meta.id)),
            str::to_owned,
        )
        .unwrap();
        assert_eq!(reg.token(), Some("token-b"));

        // A's restore finishes in the background (`activate = false`).
        reg.register(a.clone(), false, |state| state == "sdk-a");
        assert!(reg.contains_live(&a.key()));
        assert_eq!(reg.token(), Some("token-b"));
    }

    #[test]
    fn account_cache_isolates_by_host_and_account_id() {
        let mut cache: AccountCache<Option<String>> = AccountCache::new();
        let a1 = ("alpha.example".to_owned(), "1".to_owned());
        let a2 = ("alpha.example".to_owned(), "2".to_owned());
        let b1 = ("beta.example".to_owned(), "1".to_owned());

        // Account 1 on alpha explicitly has `None` posting language.
        cache.insert(a1.clone(), None);
        // Account 2 on the same host has `"fr"`.
        cache.insert(a2.clone(), Some("fr".to_owned()));
        // Same numeric id `"1"` on a different host is still uncached (`None` outer Option).
        assert_eq!(cache.get(&a1), Some(&None));
        assert_eq!(cache.get(&a2), Some(&Some("fr".to_owned())));
        assert_eq!(cache.get(&b1), None);
    }

    #[test]
    fn announce_does_not_reload_when_session_is_already_adopted() {
        assert!(!announce_should_reload(true, false));
        assert!(!announce_should_reload(true, true));
        assert!(announce_should_reload(false, false));
        assert!(!announce_should_reload(false, true));
    }
}
