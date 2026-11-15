//! The instance's own limits, remembered per account after the first answer.

use std::cell::RefCell;

use nodus_web::shared::account_state::AccountCache;

const DEFAULT_MAX_CHARACTERS: usize = 500;

thread_local! {
    static MAX_CHARACTERS: RefCell<AccountCache<usize>> = RefCell::new(AccountCache::new());
}

/// The longest body this instance accepts, or Mastodon's default until it answers.
pub async fn max_characters() -> usize {
    let key = super::active_cache_key();
    if let Some(key) = &key {
        if let Some(limit) = MAX_CHARACTERS.with(|held| held.borrow().get(key).copied()) {
            return limit;
        }
    }
    let limit = super::actions::instance_info()
        .await
        .map_or(DEFAULT_MAX_CHARACTERS, |instance| instance.max_characters());
    if let Some(key) = key {
        MAX_CHARACTERS.with(|held| {
            held.borrow_mut().insert(key, limit);
        });
    }
    limit
}
