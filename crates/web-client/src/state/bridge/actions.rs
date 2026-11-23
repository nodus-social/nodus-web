//! User actions against the instance: the verbs that are not state.
//!
//! The SDK deliberately owns *state* — session, cursors, the feed snapshot — and not
//! these. A favourite is a thing the user did, and what it returns is the instance's own
//! answer, so the view calls one of these, keeps the returned status, and re-renders from
//! it. Nothing here writes into `AppState`: an optimistic toggle that the server later
//! refused would be a lie the state machine cannot retract.

use core::cell::RefCell;
use core::future::Future;

use nodus_client_core::InstanceApi;
use nodus_client_core::actions::{
    AccountActions, Browse, FilterActions, HighlightActions, InstanceMeta, MediaActions,
    ScheduleActions, Search, SessionActions, SettingsActions, StatusActions, TagActions,
};
use nodus_client_core::model::{
    Account, AuthSession, Context, Conversation, CustomEmoji, Draft, Filter, FilterDraft,
    HistoryEntry, Language, List, LocalId, MediaAttachment, Poll, PollDraft, PreferenceRefusal,
    Preferences, PreferencesPatch, ProfileUpdate, Relationship, SearchResults, Status, Tag,
};
use nodus_client_sdk::protocol::Protocol;
use nodus_mastodon::BrowserInstance;
use nodus_web::shared::account_state::AccountCache;

pub use nodus_client_core::model::Visibility;

use super::misskey::BrowserMisskey;
use super::{active_cache_key, instance_transport, misskey, provider};

/// The Misskey bridge, when the session's server is a Misskey one.
///
/// The protocol is awaited rather than read as a signal: a read that picked a transport
/// from a probe still in flight would send a Misskey verb to a Mastodon server and report
/// the `404` as a failure, when all that happened is that the answer had not landed yet.
async fn misskey_bridge() -> Option<BrowserMisskey> {
    if provider::ensure().await != Protocol::Misskey {
        return None;
    }
    misskey::current()
}

async fn cached_list<T: Clone>(
    cache: &'static std::thread::LocalKey<RefCell<AccountCache<Vec<T>>>>,
    label: &str,
    fetch: impl Future<Output = Result<Vec<T>, String>>,
) -> Vec<T> {
    let key = active_cache_key();
    if let Some(key) = &key {
        if let Some(held) = cache.with(|held| held.borrow().get(key).cloned()) {
            return held;
        }
    }
    match fetch.await {
        Ok(values) => {
            if let Some(key) = key {
                cache.with(|held| {
                    held.borrow_mut().insert(key, values.clone());
                });
            }
            values
        }
        Err(message) => {
            web_sys::console::warn_1(&format!("nodus: {label}: {message}").into());
            Vec::new()
        }
    }
}

/// The languages this instance publishes in.
///
/// Cached per account like the emoji catalogue: the list changes when an admin edits
/// the server's languages, not between two posts, and a composer that refetched it on every
/// open would spend a request per keystroke-sized decision.
#[expect(
    clippy::future_not_send,
    reason = "the browser transport is single-threaded; the SDK is used from one task at a time"
)]
pub async fn languages() -> Vec<Language> {
    thread_local! {
        static HELD: RefCell<AccountCache<Vec<Language>>> = RefCell::new(AccountCache::new());
    }
    cached_list(&HELD, "languages unavailable", async {
        match instance_transport() {
            Some(transport) => call(transport.languages()).await,
            None => Err("the client is not installed".to_owned()),
        }
    })
    .await
}

/// Tags this instance is promoting, for the rail's card.
#[expect(
    clippy::future_not_send,
    reason = "the browser transport is single-threaded; the SDK is used from one task at a time"
)]
pub async fn trending_tags(limit: usize) -> Result<Vec<nodus_client_core::model::Tag>, String> {
    match instance_transport() {
        Some(transport) => call(transport.trending_tags(limit)).await,
        None => Err("the client is not installed".to_owned()),
    }
}

/// Posts this instance is promoting, for the rail's card.
#[expect(
    clippy::future_not_send,
    reason = "the browser transport is single-threaded; the SDK is used from one task at a time"
)]
pub async fn trending_statuses(limit: usize) -> Result<Vec<Status>, String> {
    match instance_transport() {
        Some(transport) => call(transport.trending_statuses(limit)).await,
        None => Err("the client is not installed".to_owned()),
    }
}

/// The signed-in account's own posting language, when it set one.
///
/// This is the only truthful "default language" Mastodon exposes: an instance publishes the
/// languages it supports, not the one a writer posts in. `None` means "ask the instance",
/// which in practice means English or the instance's first published language.
pub async fn my_language() -> Option<String> {
    thread_local! {
        static HELD: RefCell<AccountCache<Option<String>>> = RefCell::new(AccountCache::new());
    }

    let key = active_cache_key();
    if let Some(key) = &key {
        if let Some(answer) = HELD.with(|held| held.borrow().get(key).cloned()) {
            return answer;
        }
    }
    let fresh = match instance_transport() {
        Some(transport) => call(transport.credential_account())
            .await
            .ok()
            .and_then(|account| account.source.language),
        None => None,
    };
    if let Some(key) = key {
        HELD.with(|held| {
            held.borrow_mut().insert(key, fresh.clone());
        });
    }
    fresh
}

/// The emoji this instance offers, asked for once per account.
///
/// The set is large (hundreds of entries on a busy instance) and changes only when the
/// instance's cache does, so it is fetched on demand and kept for the life of the document
/// rather than on every keystroke that opens a suggestion list.
pub async fn custom_emojis() -> Vec<CustomEmoji> {
    thread_local! {
        static HELD: RefCell<AccountCache<Vec<CustomEmoji>>> = RefCell::new(AccountCache::new());
    }
    cached_list(&HELD, "could not read custom emojis", async {
        match instance_transport() {
            Some(transport) => call(transport.custom_emojis()).await,
            None => Err("the client is not installed".to_owned()),
        }
    })
    .await
}

/// The transport for the current session, or a message fit for inline display.
fn transport() -> Result<BrowserInstance, String> {
    instance_transport().ok_or_else(|| "Not signed in.".to_owned())
}

/// Runs one transport call, flattening its error into a display string.
async fn call<T>(
    future: impl Future<Output = Result<T, nodus_client_core::Error>>,
) -> Result<T, String> {
    future.await.map_err(|error| error.to_string())
}

/// The page size every list view asks for.
///
/// One constant, because a mismatch between "what was loaded" and "what the cursor
/// assumed" is how a timeline skips or repeats an entry.
pub const PAGE: u16 = 20;

/// One status.
pub async fn status(id: &str) -> Result<Status, String> {
    if let Some(bridge) = misskey_bridge().await {
        let note = call(bridge.note(id)).await?;
        misskey::remember(id, &note);
        return Ok(note.status);
    }
    call(transport()?.status(id)).await
}

/// The ancestors and descendants of a status.
pub async fn thread(id: &str) -> Result<Context, String> {
    if let Some(bridge) = misskey_bridge().await {
        return call(bridge.status_context(id)).await;
    }
    call(transport()?.status_context(id)).await
}

/// One account.
pub async fn account(id: &str) -> Result<Account, String> {
    let id = LocalId(id.to_owned());
    call(transport()?.account(&id)).await
}

/// The relationship between the signed-in account and `id`.
pub async fn relationship(id: &str) -> Result<Option<Relationship>, String> {
    let ids = [LocalId(id.to_owned())];
    let found = call(transport()?.relationships(&ids)).await?;
    Ok(found.into_iter().next())
}

/// Favourites or un-favourites a status.
pub async fn favourite(id: &str, on: bool) -> Result<Status, String> {
    if let Some(bridge) = misskey_bridge().await {
        // The star is Misskey's private favourite, not its public reaction: the two are
        // separate verbs on that API, and the star has always meant "keep this".
        return if on {
            call(bridge.favourite(id)).await
        } else {
            call(bridge.unfavourite(id)).await
        };
    }
    let transport = transport()?;
    if on {
        call(transport.favourite(id)).await
    } else {
        call(transport.unfavourite(id)).await
    }
}

/// Adds the reader's reaction to a note, replacing any reaction already there.
pub async fn react(id: &str, emoji: &str) -> Result<(), String> {
    misskey::reactions::react(id, emoji).await
}

/// Removes the reader's reaction, whichever key it was.
pub async fn unreact(id: &str) -> Result<(), String> {
    misskey::reactions::unreact(id).await
}

/// Boosts or un-boosts a status.
pub async fn reblog(id: &str, on: bool) -> Result<Status, String> {
    let transport = transport()?;
    if on {
        call(transport.reblog(id)).await
    } else {
        call(transport.unreblog(id)).await
    }
}

/// Bookmarks or un-bookmarks a status.
pub async fn bookmark(id: &str, on: bool) -> Result<Status, String> {
    let transport = transport()?;
    if on {
        call(transport.bookmark(id)).await
    } else {
        call(transport.unbookmark(id)).await
    }
}

/// Deletes a status the signed-in account wrote.
pub async fn delete_status(id: &str) -> Result<(), String> {
    call(transport()?.delete_status(id)).await
}

/// Everything a compose box can say beyond the text.
#[derive(Debug, Clone, PartialEq)]
pub struct NewStatus {
    /// The body, as typed.
    pub text: String,
    /// Requested visibility; the instance may still narrow it.
    pub visibility: Visibility,
    /// Parent status when replying.
    pub reply_to: Option<String>,
    /// Attachments already uploaded, in display order.
    pub media_ids: Vec<String>,
    /// Content warning; empty means none.
    pub spoiler_text: String,
    /// Whether the attachment sits behind the warning.
    pub sensitive: bool,
    /// A poll to attach; `None` for a plain post.
    pub poll: Option<PollDraft>,
    /// The language the post is written in; `None` lets the instance guess.
    pub language: Option<String>,
}

impl NewStatus {
    /// A reply or a post with nothing but text.
    pub fn text(text: impl Into<String>, visibility: Visibility) -> Self {
        Self {
            text: text.into(),
            visibility,
            reply_to: None,
            media_ids: Vec::new(),
            spoiler_text: String::new(),
            sensitive: false,
            poll: None,
            language: None,
        }
    }

    /// The same, as a reply to `parent`.
    pub fn reply(text: impl Into<String>, parent: &str) -> Self {
        Self {
            reply_to: Some(parent.to_owned()),
            ..Self::text(text, Visibility::Public)
        }
    }

    /// The wire draft, with a fresh idempotency key per attempt.
    fn draft(&self) -> Draft {
        Draft {
            text: self.text.clone(),
            visibility: self.visibility,
            in_reply_to_id: self.reply_to.clone(),
            // A new key per attempt: the point is that a retried *request* cannot post
            // twice, not that the same text may never be posted twice.
            idempotency_key: Some(format!("nodus-{}", js_sys::Date::now())),
            media_ids: self.media_ids.clone(),
            spoiler_text: self.spoiler_text.clone(),
            sensitive: self.sensitive,
            poll: self.poll.clone(),
            language: self.language.clone(),
        }
    }
}

/// Publishes a status.
pub async fn publish(status: &NewStatus) -> Result<Status, String> {
    call(transport()?.post_status(&status.draft())).await
}

/// What a thread publish managed to send, and why it stopped when it did.
pub struct ThreadOutcome {
    /// The posts the instance accepted, oldest first.
    pub statuses: Vec<Status>,
    /// The refusal that stopped the chain, if it stopped before the last post.
    pub error: Option<String>,
}

/// Publishes posts in order, each answering the one before.
///
/// A failure does not pretend the earlier posts were rolled back: the instance has them,
/// so the outcome carries them home and the composer can show the thread that exists.
pub async fn publish_thread(parts: &[NewStatus]) -> ThreadOutcome {
    let mut statuses = Vec::new();
    let mut parent = parts.first().and_then(|part| part.reply_to.clone());
    let transport = match transport() {
        Ok(transport) => transport,
        Err(message) => {
            return ThreadOutcome {
                statuses,
                error: Some(message),
            };
        }
    };
    for part in parts {
        let mut status = part.clone();
        status.reply_to = parent.clone();
        match call(transport.post_status(&status.draft())).await {
            Ok(posted) => {
                parent = Some(posted.id.clone());
                statuses.push(posted);
            }
            Err(message) => {
                let error = if statuses.is_empty() {
                    message
                } else {
                    format!(
                        "Posted {} of {} posts, then stopped: {message}",
                        statuses.len(),
                        parts.len()
                    )
                };
                return ThreadOutcome {
                    statuses,
                    error: Some(error),
                };
            }
        }
    }
    ThreadOutcome {
        statuses,
        error: None,
    }
}

/// Sets or replaces one attachment's alt text.
///
/// The instance answers with the media as it now stands, so the caller can replace its
/// copy rather than patch a field and hope the rest did not move.
pub async fn update_media(id: &str, description: &str) -> Result<MediaAttachment, String> {
    call(transport()?.update_media(id, description)).await
}

/// Rewrites a status the signed-in account wrote.
pub async fn edit(id: &str, status: &NewStatus) -> Result<Status, String> {
    let edited = call(transport()?.edit_status(id, &status.draft())).await?;
    super::timeline::upsert(vec![edited.clone()]);
    Ok(edited)
}

/// Reads one attachment's server-side processing state.
pub async fn media_status(id: &str) -> Result<nodus_client_core::model::MediaAttachment, String> {
    call(transport()?.media_status(id)).await
}

pub async fn set_media_sensitive(
    id: &str,
    sensitive: bool,
) -> Result<nodus_client_core::model::MediaAttachment, String> {
    call(transport()?.set_media_sensitive(id, sensitive)).await
}

#[expect(
    dead_code,
    reason = "the composer uploads through the progress-aware bridge"
)]
pub async fn upload_media(
    bytes: Vec<u8>,
    mime: String,
    description: Option<String>,
) -> Result<MediaAttachment, String> {
    call(transport()?.upload_media(&bytes, &mime, description.as_deref())).await
}

/// Attaches a file the drive already holds, without re-uploading its bytes.
///
/// `POST /api/v1/media` reads a multipart body, and a single text field
/// `stored_object_id` names the drive object: the server mints the attachment over the
/// bytes it already has, so the body carries no `file` part at all. That absence is the
/// point — it is what proves nothing was copied or sent again.
pub async fn attach_from_drive(object_id: &str) -> Result<MediaAttachment, String> {
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen::prelude::Closure;

    let transport = instance_transport().ok_or("This session has no instance to attach to.")?;
    let url = transport
        .url_segments(&["api", "v1", "media"])
        .map_err(|error| error.to_string())?;
    let refused = || "This browser refused the attachment.".to_owned();
    let form = web_sys::FormData::new().map_err(|_| refused())?;
    form.append_with_str("stored_object_id", object_id)
        .map_err(|_| refused())?;
    let xhr = web_sys::XmlHttpRequest::new().map_err(|_| refused())?;
    xhr.open_with_async("POST", url.as_str(), true)
        .map_err(|_| refused())?;
    let token = transport.token().expose();
    if !token.is_empty() {
        xhr.set_request_header("Authorization", &format!("Bearer {token}"))
            .map_err(|_| refused())?;
    }
    xhr.set_response_type(web_sys::XmlHttpRequestResponseType::Text);

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let loaded = {
            let resolve = resolve.clone();
            let xhr = xhr.clone();
            Closure::<dyn FnMut()>::new(move || {
                resolve.call1(&wasm_bindgen::JsValue::NULL, &xhr).ok();
            })
        };
        let failed = {
            let reject = reject.clone();
            Closure::<dyn FnMut()>::new(move || {
                reject.call0(&wasm_bindgen::JsValue::NULL).ok();
            })
        };
        xhr.set_onload(Some(loaded.as_ref().unchecked_ref()));
        xhr.set_onerror(Some(failed.as_ref().unchecked_ref()));
        loaded.forget();
        failed.forget();
    });
    xhr.send_with_opt_form_data(Some(&form))
        .map_err(|_| refused())?;
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|_| refused())?;

    let text = xhr
        .response_text()
        .map_err(|_| refused())?
        .unwrap_or_default();
    if xhr.status().map_err(|_| refused())? >= 400 {
        return Err(format!("The instance refused the drive file: {text}"));
    }
    serde_json::from_str::<MediaAttachment>(&text).map_err(|error| error.to_string())
}

/// The account id the session belongs to, for "may I edit this" questions.
pub fn viewer_id() -> Option<String> {
    super::with_sdk(|sdk| sdk.state().session.get().map(|session| session.account_id))
}

/// Pending follow requests, newest first.
pub async fn follow_requests() -> Result<Vec<Account>, String> {
    // Named through the action trait: `InstanceApi` carries this read too, and both reach
    // the same request, so an unqualified call is ambiguous rather than different.
    let transport = transport()?;
    call(AccountActions::follow_requests(&transport)).await
}

/// Accepts a follow request.
pub async fn authorize_follow_request(id: &str) -> Result<Relationship, String> {
    call(transport()?.authorize_follow_request(&LocalId(id.to_owned()))).await
}

/// Refuses a follow request.
pub async fn reject_follow_request(id: &str) -> Result<(), String> {
    call(transport()?.reject_follow_request(&LocalId(id.to_owned()))).await
}

/// Votes on a poll and returns the updated poll.
pub async fn vote(poll_id: &str, choices: &[usize]) -> Result<Poll, String> {
    call(transport()?.vote(poll_id, choices)).await
}

/// The edit history of a status, oldest first.
#[expect(
    dead_code,
    reason = "kept for the screen that will call it: the composer uploads through the editor path, the edit history panel and filter editing are still to come"
)]
pub async fn history(id: &str) -> Result<Vec<HistoryEntry>, String> {
    call(transport()?.history(id)).await
}

/// Follows or unfollows an account.
pub async fn follow(id: &str, on: bool) -> Result<Relationship, String> {
    let id = LocalId(id.to_owned());
    let transport = transport()?;
    // Follow lives on `InstanceApi` because the recommender's sync worker uses it, so
    // it needs no action trait; the branch is here because the two futures differ.
    if on {
        call(transport.follow(&id)).await
    } else {
        call(transport.unfollow(&id)).await
    }
}

/// Blocks or unblocks an account.
pub async fn block(id: &str, on: bool) -> Result<Relationship, String> {
    let id = LocalId(id.to_owned());
    let transport = transport()?;
    if on {
        call(transport.block(&id)).await
    } else {
        call(transport.unblock(&id)).await
    }
}

/// Mutes or unmutes an account.
pub async fn mute(id: &str, on: bool) -> Result<Relationship, String> {
    let id = LocalId(id.to_owned());
    let transport = transport()?;
    if on {
        call(transport.mute(&id)).await
    } else {
        call(transport.unmute(&id)).await
    }
}

/// The browsers and apps signed in to the account.
///
/// The list is a Nodus surface: the settings tab is only drawn when the instance
/// publishes `sessions`, and the server marks which row is the request that asked.
pub async fn sessions() -> Result<Vec<AuthSession>, String> {
    let transport = transport()?;
    call(SessionActions::sessions(&transport)).await
}

/// Ends one signed-in session.
pub async fn revoke_session(id: &str) -> Result<(), String> {
    let transport = transport()?;
    call(SessionActions::revoke_session(&transport, id)).await
}

/// Ends every session except the one reading this page.
pub async fn revoke_other_sessions() -> Result<(), String> {
    let transport = transport()?;
    call(SessionActions::revoke_other_sessions(&transport)).await
}

/// The defaults the instance reports for this account.
///
/// Mastodon answers with values the account has stored; a server that does not store
/// them still has to answer, so the caller must not read this as "what the account
/// chose". The settings screen says which server it is talking to.
pub async fn preferences() -> Result<Preferences, String> {
    let transport = transport()?;
    call(SettingsActions::preferences(&transport)).await
}

/// Why a preferences write did not land.
///
/// A refusal keeps its key and its allowed set: the page paints the instance's sentence
/// under the row that asked, so flattening the refusal to a string at the bridge would
/// throw away the only part the row needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferenceSaveError {
    Refused(PreferenceRefusal),
    Failed(String),
}

impl PreferenceSaveError {
    #[must_use]
    pub fn sentence(&self) -> String {
        match self {
            Self::Refused(refusal) => refusal.to_string(),
            Self::Failed(message) => message.clone(),
        }
    }
}

/// Saves a partial preferences body and answers with the whole map the instance now holds.
///
pub async fn update_preferences(
    patch: &PreferencesPatch,
) -> Result<Preferences, PreferenceSaveError> {
    let transport = transport().map_err(PreferenceSaveError::Failed)?;
    SettingsActions::update_preferences(&transport, patch)
        .await
        .map_err(|error| match error {
            nodus_client_core::Error::PreferenceRefused(refusal) => {
                PreferenceSaveError::Refused(refusal)
            }
            other => PreferenceSaveError::Failed(other.to_string()),
        })
}

/// Searches accounts, statuses and hashtags; `resolve` lets the instance fetch remote
/// matches it has never seen.
pub async fn search(query: &str, resolve: bool) -> Result<SearchResults, String> {
    let transport = transport()?;
    call(Search::search(&transport, query, resolve)).await
}

/// The account's lists.
pub async fn lists() -> Result<Vec<List>, String> {
    let transport = transport()?;
    call(Browse::lists(&transport)).await
}

/// Creates a list and returns it.
pub async fn create_list(title: &str) -> Result<List, String> {
    call(transport()?.create_list(title)).await
}

/// Renames a list.
pub async fn rename_list(id: &str, title: &str) -> Result<List, String> {
    call(transport()?.rename_list(id, title)).await
}

/// Deletes a list; the people on it stay followed.
pub async fn delete_list(id: &str) -> Result<(), String> {
    call(transport()?.delete_list(id)).await
}

/// The accounts on a list.
pub async fn list_accounts(id: &str) -> Result<Vec<Account>, String> {
    call(transport()?.list_accounts(id)).await
}

/// Adds to or removes from a list, which differ only in the verb.
pub async fn set_list_member(list_id: &str, account_id: &str, member: bool) -> Result<(), String> {
    let transport = transport()?;
    let accounts = [LocalId(account_id.to_owned())];
    if member {
        call(transport.add_to_list(list_id, &accounts)).await
    } else {
        call(transport.remove_from_list(list_id, &accounts)).await
    }
}

/// Saves the signed-in account's profile.
///
/// The answer is dispatched into the entity store rather than handed back to one screen:
/// the display name shows up on the profile, the composer and every card that mentions
/// the account, and only the store can reach all of them.
pub async fn update_profile(display_name: String, note: String) -> Result<Account, String> {
    let transport = transport()?;
    let update = ProfileUpdate::new().display_name(display_name).note(note);
    let account = call(transport.update_profile(&update)).await?;
    super::dispatch(nodus_client_sdk::Command::AccountsUpserted {
        accounts: vec![account.clone()],
    });
    Ok(account)
}

/// Saves one of the signed-in account's profile pictures.
///
/// Separate from [`update_profile`] on purpose: the picture is uploaded the moment it is
/// picked, and sending it through the same call as the text fields would save whatever was
/// being typed at that moment along with it.
pub async fn update_profile_images(
    avatar: Option<String>,
    header: Option<String>,
) -> Result<Account, String> {
    let mut update = ProfileUpdate::new();
    if let Some(id) = avatar {
        update = update.avatar(id);
    }
    if let Some(id) = header {
        update = update.header(id);
    }
    if update.is_empty() {
        return Err("Choose a picture first.".to_owned());
    }
    let account = call(transport()?.update_profile(&update)).await?;
    super::dispatch(nodus_client_sdk::Command::AccountsUpserted {
        accounts: vec![account.clone()],
    });
    Ok(account)
}

/// Creates a filter from a draft.
pub async fn create_filter(draft: &FilterDraft) -> Result<Filter, String> {
    call(transport()?.create_filter(draft)).await
}

/// Edits a filter.
///
/// Unused until the card grows an editor; the SDK keeps it because a filter whose
/// phrases cannot be corrected is a filter the reader must delete and retype.
#[expect(
    dead_code,
    reason = "kept for the screen that will call it: the composer uploads through the editor path, the edit history panel and filter editing are still to come"
)]
pub async fn update_filter(id: &str, draft: &FilterDraft) -> Result<Filter, String> {
    call(transport()?.update_filter(id, draft)).await
}

/// Deletes a filter, and with it its phrases.
pub async fn delete_filter(id: &str) -> Result<(), String> {
    call(transport()?.delete_filter(id)).await
}

/// Features a tag on the signed-in account's profile.
pub async fn feature_tag(name: &str) -> Result<(), String> {
    call(transport()?.feature_tag(name)).await.map(|_tag| ())
}

/// Stops featuring a tag.
pub async fn unfeature_tag(id: &str) -> Result<(), String> {
    call(transport()?.unfeature_tag(id)).await
}

/// Tags the instance suggests featuring, busiest first.
///
/// A failure is answered as an empty list on purpose: a suggestion is an offer, and an
/// instance that cannot make one (an old Mastodon without the endpoint, a network blip)
/// should leave the form as it was rather than put an error under a reader who never
/// asked for suggestions.
pub async fn featured_tag_suggestions() -> Vec<String> {
    match transport() {
        Ok(instance) => call(instance.featured_tag_suggestions())
            .await
            .map(|tags| tags.into_iter().map(|tag| tag.hashtag()).collect())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// One tag's header, with the reader's subscription state.
///
/// A 404 is answered as `Ok(None)` rather than an error: on this API a name nobody has
/// used is a tag that does not exist, which the screen says in a note and not in a toast.
pub async fn tag(name: &str) -> Result<Option<Tag>, String> {
    match transport()?.tag(name).await {
        Ok(found) => Ok(Some(found)),
        Err(nodus_client_core::Error::Status { status: 404, .. }) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Follows or unfollows a tag, answering with the instance's own state.
///
/// Nothing is written locally: the returned tag carries the `following` the server
/// recorded, so the button reflects what happened rather than what was clicked.
pub async fn follow_tag(name: &str, on: bool) -> Result<Option<Tag>, String> {
    let instance = transport()?;
    let answer = if on {
        instance.follow_tag(name).await
    } else {
        instance.unfollow_tag(name).await
    };
    match answer {
        Ok(found) => Ok(Some(found)),
        Err(nodus_client_core::Error::Status { status: 404, .. }) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// Marks a conversation as read.
pub async fn mark_conversation_read(id: &str) -> Result<(), String> {
    call(transport()?.mark_conversation_read(id)).await
}

/// Mutes or unmutes a conversation.
pub async fn mute_conversation(id: &str, muted: bool) -> Result<Conversation, String> {
    call(transport()?.mute_conversation(id, muted)).await
}

/// Queues a draft for `at`, in RFC 3339.
pub async fn schedule(status: &NewStatus, at: &str) -> Result<(), String> {
    call(transport()?.schedule(&status.draft(), at))
        .await
        .map(|_queued| ())
}

/// Cancels a queued post.
pub async fn cancel_scheduled(id: &str) -> Result<(), String> {
    call(transport()?.cancel_scheduled(id)).await
}

/// What the instance says about itself: name, version, contact, rules.
pub async fn instance_info() -> Result<nodus_client_core::model::InstanceInfo, String> {
    let transport = instance_transport().ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .instance_info()
        .await
        .map_err(|error| error.to_string())
}

/// Uploads one file and reports how much of it has left the browser.
///
/// `upload_media` in the SDK goes through `fetch`, which reports nothing until the
/// whole body is sent. This builds the same request (`media_upload_request`) and sends
/// it with `XMLHttpRequest`, whose `upload` progress events are the only real signal a
/// page can get.
pub async fn upload_media_with_progress(
    bytes: Vec<u8>,
    mime: &str,
    description: Option<&str>,
    on_progress: impl Fn(u8) + 'static,
) -> Result<MediaAttachment, String> {
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen::prelude::Closure;

    let transport = instance_transport().ok_or("This session has no instance to upload to.")?;
    let request = transport
        .media_upload_request(&bytes, mime, description)
        .map_err(|error| error.to_string())?;
    let unsupported = || "This browser refused the upload.".to_owned();
    let xhr = web_sys::XmlHttpRequest::new().map_err(|_| unsupported())?;
    xhr.open_with_async(request.method.as_str(), request.url.as_str(), true)
        .map_err(|_| unsupported())?;
    for (name, value) in &request.headers {
        xhr.set_request_header(name, value)
            .map_err(|_| unsupported())?;
    }
    xhr.set_response_type(web_sys::XmlHttpRequestResponseType::Text);
    let upload = xhr.upload().map_err(|_| unsupported())?;
    let ticker =
        Closure::<dyn FnMut(web_sys::ProgressEvent)>::new(move |event: web_sys::ProgressEvent| {
            if event.length_computable() && event.total() > 0.0 {
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a share of the body is a whole percentage"
                )]
                let percent = ((event.loaded() / event.total()) * 100.0) as u8;
                on_progress(percent);
            }
        });
    upload.set_onprogress(Some(ticker.as_ref().unchecked_ref()));

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let loaded = {
            let resolve = resolve.clone();
            let xhr = xhr.clone();
            Closure::<dyn FnMut()>::new(move || {
                resolve.call1(&wasm_bindgen::JsValue::NULL, &xhr).ok();
            })
        };
        let failed = {
            let reject = reject.clone();
            Closure::<dyn FnMut()>::new(move || {
                reject.call0(&wasm_bindgen::JsValue::NULL).ok();
            })
        };
        xhr.set_onload(Some(loaded.as_ref().unchecked_ref()));
        xhr.set_onerror(Some(failed.as_ref().unchecked_ref()));
        loaded.forget();
        failed.forget();
    });
    let sent = xhr
        .send_with_opt_u8_array(request.body.as_deref())
        .map_err(|_| unsupported());
    sent?;
    ticker.forget();
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|_| unsupported())?;

    let text = xhr
        .response_text()
        .map_err(|_| unsupported())?
        .unwrap_or_default();
    if xhr.status().map_err(|_| unsupported())? >= 400 {
        return Err(format!("The instance refused the file: {text}"));
    }
    serde_json::from_str::<MediaAttachment>(&text).map_err(|error| error.to_string())
}
