//! Glue between the SDK (source of truth) and Leptos (renderer).
//!
//! The SDK is deliberately framework-agnostic: its state is `Rc<RefCell<_>>`
//! handles, which are single-threaded and therefore not usable directly as Leptos
//! signals (Leptos 0.8 signals must be `Send + Sync`). This module is the one
//! place that bridges the two, and it does so with a single revision counter:
//!
//! * the SDK owns the data and notifies subscribers on change;
//! * each subscription bumps a `RwSignal<u64>` revision;
//! * a view renders by reading the revision (which subscribes the view to it) and
//!   then reading the data straight out of the SDK.
//!
//! Views therefore never copy state and never keep their own cache — they read SDK
//! state and dispatch commands, which is exactly the split we want.
//!
//! This module also owns the two things the SDK deliberately does not: the
//! browser-local session record (milestone 1, see [`save_session`]) and the
//! transient request flags of the Feed tab ([`feed_ui`]) — "refreshing" and "the
//! last request failed" describe a request, not the data.
//!
//! See `web/docs/02-architecture.md` ("SDK owns state") and `sdk/README.md`.

use std::cell::RefCell;
use std::rc::Rc;

use leptos::prelude::*;
use nodus_client_sdk::instance_api::Instance;
use nodus_client_sdk::state::Session;
use nodus_client_sdk::{Command, MemoryPersistence, Sdk, Subscription, SystemClock};
use nodus_discovery::BrowserDiscovery;
use nodus_mastodon::BrowserInstance;
use nodus_web::shared::account_state::{AccountKey, AccountMeta, AccountRegistry};
use url::Url;

use crate::state::oauth;

pub mod accounts;
pub mod actions;
pub mod capabilities;
pub mod misskey;
pub(crate) mod profile_banners;
pub(crate) mod profile_media;
pub mod provider;

// Whether the account's instance serves the Nodus extension surface, as the answer for one
// host. A session can live on any server, so "this deployment has a recommendation engine" is
// not the question a screen needs answered: an account on a Mastodon instance can read its
// own server's timelines and none of the surfaces built on top of ours. The answer is
// therefore asked of the account's instance, once per host for this browser, and recorded
// here so a view can read it synchronously — a render must not wait on a request.
thread_local! {
    static EXTENSIONS: RefCell<Option<(String, bool)>> = const { RefCell::new(None) };
    /// The feature names the same answer advertised, for the same host.
    ///
    /// Beside the yes/no rather than inside it because a view asks about one name at a
    /// time. Deliberately **not** remembered in storage: a list kept for a month would hide
    /// a panel the instance started serving, and a panel that never comes back is worse
    /// than one request per visit.
    static FEATURES: RefCell<Option<(String, Vec<String>)>> = const { RefCell::new(None) };
    /// Whether a request for the answer is already in flight.
    static ASKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn extensions_revision() -> RwSignal<u64> {
    thread_local! {
        static REVISION: RwSignal<u64> = RwSignal::new(0);
    }
    REVISION.with(|revision| *revision)
}

/// What the extension question answered, as a signal a view can depend on.
///
/// `false` until an answer arrives: showing a surface the instance cannot serve is a worse
/// first frame than showing it a moment late.
#[must_use]
pub fn supports_extensions() -> Signal<bool> {
    let revision = extensions_revision();
    Signal::derive(move || {
        revision.get();
        EXTENSIONS.with_borrow(|known| known.as_ref().is_some_and(|(_, known)| *known))
    })
}

/// Whether the account's instance advertises one Nodus feature.
///
/// The body of `GET /api/v1/meta` is the answer, and an **absent name means unsupported**:
/// the server publishes exactly the surfaces it serves, so a client that guessed would paint
/// a panel whose every request can only 404. The vocabulary the server publishes today, and
/// what each name gates in this console:
///
/// * `feed_accounts` — the recommendation feed;
/// * `events` — interaction feedback is accepted;
/// * `sync_token` — the server keeps an encrypted token and syncs follows;
/// * `admin` — the `/admin` area;
/// * `moderation` — the `/moderation` area;
/// * `scanners` — the scanners section and the dashboard's scanner panel;
/// * `topology` — the System section;
/// * `protocol` — the Protocol section;
/// * `remote_blocks` — the inbound-block panel of the domains screen;
/// * `polls` — voting in a status.
///
/// The list is read here rather than through the SDK's `Capabilities`, which decodes four
/// names into a closed enum and drops the rest; the SDK is not this console's to widen.
#[must_use]
pub fn feature(name: &'static str) -> Signal<bool> {
    let revision = extensions_revision();
    Signal::derive(move || {
        revision.get();
        FEATURES.with_borrow(|held| {
            held.as_ref()
                .is_some_and(|(_, names)| names.iter().any(|held| held == name))
        })
    })
}

/// Where the answer for a host is remembered between visits.
///
/// One browser meets a Mastodon instance once, not once per page: keeping the answer beside
/// the session costs a line of storage and saves a refused request on every load.
const SUPPORT_KEY: &str = "nodus-extensions/1";

fn remembered_support(host: &str) -> Option<bool> {
    let stored = crate::platform::storage::get(SUPPORT_KEY)?;
    stored.lines().find_map(|line| {
        let (known_host, answer) = line.split_once('\t')?;
        (known_host == host).then(|| answer.trim() == "yes")
    })
}

fn remember_support(host: &str, answer: bool) {
    let mut lines: Vec<String> = crate::platform::storage::get(SUPPORT_KEY)
        .map(|stored| {
            stored
                .lines()
                .filter(|line| !line.starts_with(&format!("{host}\t")))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    lines.push(format!("{host}\t{}", if answer { "yes" } else { "no" }));
    crate::platform::storage::set(SUPPORT_KEY, &lines.join("\n"));
}

/// Drops one host's remembered answer, so a later sign-in asks the instance again.
fn forget_support(host: &str) {
    let Some(stored) = crate::platform::storage::get(SUPPORT_KEY) else {
        return;
    };
    let prefix = format!("{host}\t");
    let kept = stored
        .lines()
        .filter(|line| !line.starts_with(&prefix))
        .collect::<Vec<_>>();
    if kept.is_empty() {
        crate::platform::storage::remove(SUPPORT_KEY);
    } else {
        crate::platform::storage::set(SUPPORT_KEY, &kept.join("\n"));
    }
}

fn settle(host: &str, answer: bool, features: Vec<String>) -> bool {
    EXTENSIONS.with_borrow_mut(|known| *known = Some((host.to_owned(), answer)));
    FEATURES.with_borrow_mut(|held| *held = Some((host.to_owned(), features)));
    remember_support(host, answer);
    extensions_revision().update(|revision| *revision = revision.wrapping_add(1));
    answer
}

/// The `features` list of a `/api/v1/meta` body.
///
/// `None` means the body is not the API's answer: it did not parse as JSON, or it carried
/// no `features` list. That matters because a proxy with a fallthrough route answers every
/// path with the page itself — a `200 text/html` — and believing that would pin the panels
/// on the host until sign-out.
fn features_of(body: &[u8]) -> Option<Vec<String>> {
    #[derive(serde::Deserialize)]
    struct Meta {
        #[serde(default)]
        features: Vec<String>,
    }
    let features = serde_json::from_slice::<Meta>(body).ok()?.features;
    (!features.is_empty()).then_some(features)
}

/// Asks the account's instance what it serves, and which Nodus surfaces it advertises.
///
/// Called from adoption and from a sign-in, not from a render: the answer arrives later and
/// repaints whatever read [`supports_extensions`] or [`feature`]. A question already in
/// flight is not asked twice.
///
/// The body is asked for, not just the status: the status says only that something
/// answered, and the `features` list says what of Nodus is here. A 200 whose body is not
/// the API — the page itself, from a proxy's fallthrough — is not support, and neither is a
/// featureless body: both settle "no" and are remembered that way. Both answers are
/// settled together, so a panel and the rail behind it can never disagree.
///
/// The remembered answer is a fallback for a host that does not answer at all — a transport
/// failure or a timeout — and it carries no feature list, so a Nodus browser that meets a
/// dead network this visit sees the rail but not the panels rather than a blank page.
pub async fn probe_extensions() -> bool {
    let Some(host) = session_host() else {
        return false;
    };
    if let Some((known_host, known)) = EXTENSIONS.with_borrow(|known| known.clone()) {
        if known_host == host {
            return known;
        }
    }
    // A host that has already said no is not asked again: the answer is a fact about the
    // instance, not about this page, and a 404 repeated on every navigation is a console
    // error the reader did not cause. A remembered "yes" is still asked, because the
    // feature list behind it is what the panels are built from and only the response
    // carries it.
    if remembered_support(&host) == Some(false) {
        return settle(&host, false, Vec::new());
    }
    if ASKING.with(std::cell::Cell::get) {
        return remembered_support(&host).unwrap_or(false);
    }
    let Some(transport) = instance_transport() else {
        return settle(
            &host,
            remembered_support(&host).unwrap_or(false),
            Vec::new(),
        );
    };
    let Ok(url) = transport.url("/api/v1/meta") else {
        return settle(&host, false, Vec::new());
    };
    ASKING.with(|asking| asking.set(true));
    // A host that answers slowly must not keep the question "in flight" for the rest of the
    // visit: an abandoned future has to release the claim too.
    let _claim = Claim;
    let answering = transport.fetch_bytes(transport.get(url));
    let (answer, features) = match patience(answering, PATIENCE).await {
        Some(Ok(body)) => match features_of(&body) {
            Some(features) => (true, features),
            None => (false, Vec::new()),
        },
        Some(Err(_)) | None => (remembered_support(&host).unwrap_or(false), Vec::new()),
    };
    settle(&host, answer, features)
}

/// How long a question waits for an answer before the surface is assumed absent.
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(6);

/// Holds the claim on the extension question and gives it back however the future ends.
struct Claim;

impl Drop for Claim {
    fn drop(&mut self) {
        ASKING.with(|asking| asking.set(false));
    }
}

/// Waits for `future`, or for `limit`, whichever comes first.
///
/// `None` means the instance did not answer in time; the caller treats that as "no".
pub(super) async fn patience<F: std::future::Future>(
    future: F,
    limit: std::time::Duration,
) -> Option<F::Output> {
    use std::cell::RefCell;
    use std::rc::Rc;
    let slot: Rc<RefCell<Option<F::Output>>> = Rc::new(RefCell::new(None));
    let filled = Rc::clone(&slot);
    let answering = async move {
        *filled.borrow_mut() = Some(future.await);
    };
    let clock = async move {
        gloo_timers::future::TimeoutFuture::new(
            u32::try_from(limit.as_millis()).unwrap_or(u32::MAX),
        )
        .await;
    };
    let answering = Box::pin(answering);
    let clock = Box::pin(clock);
    match futures_util::future::select(answering, clock).await {
        futures_util::future::Either::Left((_, _)) => slot.borrow_mut().take(),
        futures_util::future::Either::Right(_) => None,
    }
}

/// Forgets what was asked, for when the session changes under it.
pub fn forget_extensions() {
    // The remembered answer belongs to the account that is leaving, not to the browser: a
    // sign-out that kept it would make the next sign-in on this host inherit this one's
    // extensions (or lack of them).
    if let Some(host) = session_host() {
        forget_support(&host);
    }
    EXTENSIONS.with_borrow_mut(|known| *known = None);
    FEATURES.with_borrow_mut(|held| *held = None);
    extensions_revision().update(|revision| *revision = revision.wrapping_add(1));
}

#[cfg_attr(
    not(feature = "admin"),
    allow(
        dead_code,
        reason = "the moderation screens are compiled out of this build"
    )
)]
pub mod admin;
mod admin_token;
pub mod backfill;
pub mod drive;
mod feed;
mod impressions;
pub mod limits;
pub mod notices;
pub mod notifications;
pub mod pages;
mod posts;
pub mod profile;
mod runtime;
pub mod seed;
pub mod session;
pub mod settings;
pub mod sidebar;
pub mod stream;
pub mod style;
pub mod timeline;

// Re-exported so every call site keeps saying `bridge::<verb>`: the split into
// files is a reading aid, not a new API surface.
pub use feed::{follow_item, not_interested, reload_feed, unfollow_item};
pub use impressions::{
    begin as begin_impression, finish as finish_impression, install as install_impression_flush,
};
pub use posts::{load_posts_once, posts_ui, reload_posts};
pub use runtime::start as start_background_tick;
pub use session::{authorize_sync, load_sync_consent, set_sync, sign_in, sign_out};

/// The SDK as this app wires it: browser transports, memory cache, system clock.
///
/// The transports carry the token, so a session change rebuilds this value
/// instead of swapping a token underneath it.
pub type WebSdk = Sdk<BrowserInstance, BrowserDiscovery, MemoryPersistence, SystemClock>;

/// The error type [`WebSdk`] produces, for the one branch that inspects a variant.
type WebSdkError =
    nodus_client_sdk::SdkError<nodus_client_sdk::instance_api::Error, nodus_discovery::Error>;

/// Where the API lives when the page has no `window` (native tests, prerender).
const FALLBACK_API_BASE: &str = "http://127.0.0.1:3000";

/// One account's SDK, transport and subscriptions.
struct Live {
    sdk: Rc<WebSdk>,
    transport: BrowserInstance,
    /// Kept alive so this account keeps receiving its own events; dropping the SDK
    /// while these lived on would call back into a store nothing renders. Never read,
    /// which is the point of an owning field.
    _subscriptions: Vec<Subscription>,
}

thread_local! {
    /// The one SDK instance for this app.
    static SDK: RefCell<Option<Rc<WebSdk>>> = const { RefCell::new(None) };
    /// The instance transport, kept separately: the SDK exposes no way to call
    /// `follow` (it is a state machine, not an action surface), and a follow is a
    /// plain instance write.
    static INSTANCE: RefCell<Option<BrowserInstance>> = const { RefCell::new(None) };
    /// The feed snapshot id of the last loaded page: `EventBatch` needs it and
    /// `FeedState` does not keep it.
    static FEED_ID: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Request flags for the Feed tab; created with [`init`].
    static FEED_UI: RefCell<Option<FeedUi>> = const { RefCell::new(None) };
    /// Bumped by every SDK subscription; views `.get()` it to track changes.
    static REVISION: RefCell<Option<RwSignal<u64>>> = const { RefCell::new(None) };
    /// Subscriptions must outlive `init`; dropping them stops the bumps.
    static SUBSCRIPTIONS: RefCell<Vec<Subscription>> = const { RefCell::new(Vec::new()) };
    /// Whether [`init`] has wired the SDK, so a remount does not subscribe twice.
    static INSTALLED: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
    /// Per-account live SDKs, staged pending SDK, and active bearer token.
    static REGISTRY: RefCell<AccountRegistry<Live>> = RefCell::new(AccountRegistry::new());
}

/// The Feed tab's request state, which belongs to the view and not to the SDK.
#[derive(Debug, Clone, Copy)]
pub struct FeedUi {
    /// A feed request is in flight; the list stays visible, dimmed.
    pub refreshing: RwSignal<bool>,
    /// The last request's error, rendered inline with a retry.
    pub error: RwSignal<Option<String>>,
}

impl FeedUi {
    fn new() -> Self {
        Self {
            refreshing: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }
}

/// The Feed tab's request flags. Falls back to fresh signals if a view is built
/// before [`init`], so rendering order can never panic.
pub fn feed_ui() -> FeedUi {
    FEED_UI.with_borrow_mut(|slot| *slot.get_or_insert_with(FeedUi::new))
}

/// Wires the SDK into the reactive graph, then restores a stored session in the
/// background. Call once, from the root component.
///
/// Idempotent: a second call is a no-op, so remounting the app does not
/// double-subscribe.
pub fn init() {
    if INSTALLED.get() {
        return;
    }
    INSTALLED.set(true);
    install_reactive_root();

    // Until a session exists the views only read state, but `with_sdk` must still
    // answer, so install an unauthenticated SDK for the API origin.
    if let Some(base) = api_base() {
        let _ = install(&Instance { base }, "");
    }

    // Same-origin shell may carry an SSR session style island; adopt or restore cache first.
    style::install();

    // Reading, validating and re-installing the stored record is the session
    // module's job; `init` only asks for it once the unauthenticated SDK exists.
    session::restore_saved();
    style::adopt_ssr_style_for_active();

    // The server-painted page was read before this SDK existed (the mount replaces the
    // node the island lives in), so it waited; adopt it now that there is a store.
    seed::adopt_pending();
}

thread_local! {
    /// Whether a repaint is already scheduled for this microtask.
    static REPAINT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Schedules one repaint for the current microtask, however many writes arrive.
///
/// Every store notifies its own subscribers, so one action (a favourite, a page landing,
/// a socket frame) used to bump the same revision several times and rebuild every list
/// several times in a row. Coalescing turns a burst into one render; the flag resets
/// inside the microtask, so a later write schedules the next one.
fn schedule_repaint() {
    if REPAINT.with(std::cell::Cell::get) {
        return;
    }
    REPAINT.with(|scheduled| scheduled.set(true));
    let callback = wasm_bindgen::closure::Closure::once_into_js(move || {
        REPAINT.with(|scheduled| scheduled.set(false));
        let _ = revision().try_update(|generation| *generation += 1);
    });
    let callback: js_sys::Function = wasm_bindgen::JsCast::unchecked_into(callback);
    if let Some(window) = web_sys::window() {
        window.queue_microtask(&callback);
    }
}

/// Creates the app-lifetime signals while no owner exists.
///
/// Must run before the first mount. A signal created inside a component is owned by it,
/// so a later write from a socket frame or a tick — both outlive the screen that
/// happened to be mounted first — would raise
/// `reactive_graph … Tried to access a reactive value that has already been disposed`,
/// panic inside a task poll, and leave the executor unable to resume anything.
pub fn install_reactive_root() {
    if REVISION.with_borrow(Option::is_none) {
        let revision = RwSignal::new(0_u64);
        REVISION.with_borrow_mut(|slot| *slot = Some(revision));
    }
    feed_ui();
    posts::install();
    profile_media::install();
}

/// The revision signal: read it inside a view to re-render on SDK changes.
///
/// # Panics
///
/// Panics if [`init`] has not run, which would silently produce a view that never
/// updates — a loud failure is the safer one.
#[must_use]
pub fn revision() -> RwSignal<u64> {
    REVISION
        .with_borrow(|slot| *slot)
        .expect("state::bridge::init() must run before rendering views")
}

/// Reads SDK state. Read [`revision`] first, in the same reactive scope.
///
/// # Panics
///
/// Panics if [`init`] has not run.
pub fn with_sdk<R>(f: impl FnOnce(&WebSdk) -> R) -> R {
    SDK.with_borrow(|slot| {
        let sdk = slot
            .as_ref()
            .expect("state::bridge::init() must run before rendering views");
        f(sdk)
    })
}

/// Applies a command, triggering re-renders through [`revision`].
pub fn dispatch(command: Command) {
    if let Some(sdk) = handle() {
        sdk.dispatch(command);
    }
}

/// The snapshot id of the list currently on screen, if any.
///
/// A refresh passes it to the server so the request is understood as superseding that
/// snapshot; a "load more" never does, because it is a read of the same list.
pub(super) fn existing_feed_id() -> Option<String> {
    FEED_ID.with_borrow(std::clone::Clone::clone)
}

/// The bearer behind the installed SDK, for the few calls that need the home API directly.
pub(crate) fn session_token() -> Option<String> {
    REGISTRY.with_borrow(|reg| reg.token().map(str::to_owned))
}

/// The `(host, account_id)` key for per-account in-memory caches, when signed in.
pub(super) fn active_cache_key() -> Option<AccountKey> {
    handle().and_then(|sdk| {
        sdk.state()
            .session
            .get()
            .map(|session| (session.host, session.account_id))
    })
}

/// Whether `sdk` is still the active SDK installed in this tab.
pub(super) fn is_current_sdk(sdk: &Rc<WebSdk>) -> bool {
    handle().is_some_and(|current| Rc::ptr_eq(&current, sdk))
}

/// Called when the session ends, so a late batch cannot carry the previous
/// account's snapshot id into the next one.
pub(super) fn forget_feed_id() {
    FEED_ID.with_borrow_mut(|slot| *slot = None);
}

/// Builds a [`WebSdk`] for `instance` and swaps it in. Replaces any previous one.
fn build_live(instance: &Instance, token: &str) -> Result<Live, String> {
    let base =
        api_base().ok_or_else(|| "Could not determine the API origin for this page.".to_owned())?;
    let transport = BrowserInstance::new(instance, token);
    let sdk = Rc::new(Sdk::new(
        transport.clone(),
        BrowserDiscovery::new(base, instance.base.host_str().unwrap_or_default(), token),
        MemoryPersistence::new(),
        SystemClock,
    ));

    let bump = schedule_repaint;
    let mut subscriptions = Vec::new();
    {
        let state = sdk.state();
        subscriptions.push(state.capabilities.subscribe(bump));
        subscriptions.push(state.session.subscribe(bump));
        subscriptions.push(state.stream.subscribe(bump));
        subscriptions.push(state.feed.subscribe(bump));
        subscriptions.push(state.timelines.subscribe(bump));
        // The page store backs notifications, messages, saved, scheduled, people and the
        // directory. Without this line the first visit to any of them loaded the data and
        // then never re-rendered: the screen showed its empty state until it was remounted.
        subscriptions.push(state.pages.subscribe(bump));
        // Entities, for the same reason as pages: a status that arrives on the socket,
        // or is favourited in another tab, must repaint every screen showing it.
        subscriptions.push(state.statuses.subscribe(bump));
        subscriptions.push(state.accounts.subscribe(bump));
        // `sync` is a consent the *server* holds and the card's button is the only
        // writer, so without this subscription the toggle kept rendering the value it
        // read on mount until the operator reloaded the page.
        subscriptions.push(state.sync.subscribe(bump));
    }

    // Dropping the previous vector unsubscribes from the previous SDK, before that
    // SDK is replaced, so no callback can fire against state nothing renders.
    // Every socket frame is applied by the SDK before a view sees it: an event that
    // arrived on the wire has to land in the same store a page reads, or the screen
    // holding the status cannot repaint from it.
    let sink = wasm_bindgen::closure::Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new({
        let sdk = Rc::clone(&sdk);
        move |frame: wasm_bindgen::JsValue| {
            if let Some(text) = frame.as_string() {
                sdk.apply_frame(&text);
            }
        }
    });
    let transport =
        transport.with_stream_sink(wasm_bindgen::JsCast::unchecked_into(sink.into_js_value()));

    // Negotiate the protocol once per session. This is a question about the server,
    // not about a screen, so it must not be asked from a render: every view reads the
    // recorded answer and picks its path synchronously. A server that does not answer
    // keeps the Mastodon surface, which is the answer when there is no answer.
    wasm_bindgen_futures::spawn_local({
        let sdk = Rc::clone(&sdk);
        async move {
            // A painted page carries the answer in its island; only a document without
            // one (a static deploy, a client-side route) has to ask.
            if crate::state::bridge::capabilities::seeded().is_none() {
                sdk.load_capabilities().await;
            }
            probe_extensions().await;
        }
    });

    Ok(Live {
        sdk,
        transport,
        _subscriptions: subscriptions,
    })
}

fn install(instance: &Instance, token: &str) -> Result<(), String> {
    let live = build_live(instance, token)?;
    let sdk = Rc::clone(&live.sdk);
    let transport = live.transport.clone();

    SUBSCRIPTIONS.with_borrow_mut(|slot| *slot = Vec::new());
    INSTANCE.with_borrow_mut(|slot| *slot = Some(transport));
    REGISTRY.with_borrow_mut(|reg| reg.stage(token.to_owned(), live));
    SDK.with_borrow_mut(|slot| *slot = Some(sdk));
    // Which API this server speaks is a property of the session, so it is asked for
    // once here and read from the record by every screen after that.
    provider::watch(instance);
    misskey::install(instance, token);
    revision().update(|generation| *generation += 1);
    Ok(())
}

fn meta_of(account: &accounts::Account) -> AccountMeta {
    AccountMeta {
        host: account.host.clone(),
        token: account.token.clone(),
        id: account.id.clone(),
        acct: account.acct.clone(),
    }
}

/// Names the SDK built by [`install`] and keeps it for switching.
///
/// Called once the session is known: the host and token arrive first, the account id
/// only when the instance answers, and an account the switcher cannot name is an
/// account the user cannot return to.
pub fn register(account: accounts::Account) {
    register_account(account, None, true);
}

/// Registers an account restored in the background after the user already switched away,
/// without hijacking the active token or session.
pub(super) fn register_background(account: accounts::Account, expected_sdk: &Rc<WebSdk>) {
    register_account(account, Some(expected_sdk), false);
}

fn register_account(account: accounts::Account, expected_sdk: Option<&Rc<WebSdk>>, activate: bool) {
    let key = account.key();
    let meta = meta_of(&account);
    REGISTRY.with_borrow_mut(|reg| {
        reg.register(meta, activate, |live| {
            expected_sdk.is_none_or(|sdk| Rc::ptr_eq(&live.sdk, sdk))
        });
    });
    let mut list = accounts::load();
    accounts::insert(&mut list, account);
    accounts::store(&list);
    if activate {
        accounts::set_active(Some(&key));
        revision().update(|generation| *generation += 1);
    }
}

/// The accounts this browser holds, oldest first.
#[must_use]
pub fn account_list() -> Vec<accounts::Account> {
    accounts::load()
}

/// The key of the account on screen.
#[must_use]
pub fn active_key() -> Option<String> {
    accounts::active()
}

/// Show another account without signing out of this one.
///
/// Reuses an already-installed SDK when present, or lazily installs the stored account
/// from `accounts::load()`, then updates `TOKEN`, `nodus.session`, and the SSR cookie.
pub fn switch(key: &str) -> Result<(), String> {
    let stored: Vec<AccountMeta> = accounts::load().iter().map(meta_of).collect();
    let (sdk, transport, meta) = REGISTRY.with_borrow_mut(|reg| {
        reg.switch(
            key,
            &stored,
            |meta| {
                let instance = Instance::parse(&meta.host).map_err(|error| error.to_string())?;
                let live = build_live(&instance, &meta.token)?;
                live.sdk.adopt_session(Session {
                    host: meta.host.clone(),
                    account_id: meta.id.clone(),
                    acct: meta.acct.clone(),
                    scopes: session::scopes(),
                });
                Ok(live)
            },
            str::to_owned,
        )
        .map(|(live, meta)| (Rc::clone(&live.sdk), live.transport.clone(), meta))
    })?;

    impressions::finish_all();
    // The new account may be on a different server than the one on screen, so the protocol
    // is asked again for its host before anything is drawn with it.
    let instance = Instance::parse(&meta.host).map_err(|error| error.to_string())?;
    provider::watch(&instance);
    misskey::install(&instance, &meta.token);
    INSTANCE.with_borrow_mut(|slot| *slot = Some(transport));
    SDK.with_borrow_mut(|slot| *slot = Some(sdk));
    accounts::set_active(Some(key));
    forget_feed_id();
    feed_ui().error.set(None);
    posts_ui().error.set(None);
    session::activate(&meta.host, &meta.token, &meta.id, &meta.acct);
    style::on_account_activated(&meta.host, &meta.id);
    revision().update(|generation| *generation += 1);
    Ok(())
}

/// Forget one account, and fall back to another if it was the one on screen.
pub fn forget(key: &str) {
    REGISTRY.with_borrow_mut(|reg| reg.remove(key));
    let mut list = accounts::load();
    list.retain(|account| account.key() != key);
    accounts::store(&list);
    if active_key().as_deref() == Some(key) {
        match list.first() {
            Some(next) if switch(&next.key()).is_ok() => {}
            _ => {
                accounts::set_active(None);
                REGISTRY.with_borrow_mut(AccountRegistry::clear_token);
                SDK.with_borrow_mut(|slot| *slot = None);
                INSTANCE.with_borrow_mut(|slot| *slot = None);
                provider::forget();
                misskey::forget();
                style::on_signed_out();
                revision().update(|generation| *generation += 1);
            }
        }
    }
}

/// The current SDK, if [`init`] has installed one.
fn handle() -> Option<Rc<WebSdk>> {
    SDK.with_borrow(std::clone::Clone::clone)
}

/// Keeps an admin grant for `account` on `host`, after proving the token can open the
/// console.
///
/// The probe is the point: a token the instance refuses for `admin/meta` would leave the
/// Administration tab claiming a permission it does not have, and the operator would see an
/// empty screen instead of "the grant did not work". It is the same document every panel
/// asks for first, so a grant that survives it is a grant the panels can use.
///
/// The canonical host is stored, not the callback's spelling, so the grant is found by
/// the same key the sign-in record and the request builder use.
pub async fn keep_admin_token(host: &str, account: &str, token: &str) -> Result<(), String> {
    let account = account.trim();
    if account.is_empty() {
        return Err(
            "This browser does not know which account the grant belongs to. Sign in again \
             and retry."
                .to_owned(),
        );
    }
    let endpoint = oauth::normalise_host(host)?;
    let instance = instance_for(&endpoint)?;
    let transport = BrowserInstance::new(&instance, token);
    let url = transport
        .url("/api/v1/admin/meta")
        .map_err(|error| error.to_string())?;
    transport
        .fetch_bytes(transport.get(url))
        .await
        .map_err(|error| {
            format!(
                "That account cannot open the console on {}, so the grant was not kept: {error}",
                endpoint.host
            )
        })?;
    admin_token::store(&endpoint.host, account, token);
    admin::forget_standing();
    Ok(())
}

/// The admin transport for the signed-in account, when a grant was kept.
///
/// Moderation calls go through this one; everything else keeps the ordinary session
/// transport, which is what makes a leaked grant a narrower loss than a leaked session.
pub fn admin_transport() -> Option<BrowserInstance> {
    let (host, account) = admin_scope()?;
    let endpoint = oauth::normalise_host(&host).ok()?;
    let token = admin_token::load(&endpoint.host, &account)?;
    let instance = instance_for(&endpoint).ok()?;
    Some(BrowserInstance::new(&instance, &token))
}

/// The instance the active session was signed in on, when there is one.
///
/// A profile link from another instance carries that instance's id, so the router needs to
/// know whether the reader holds a session at all before it can decide to resolve the link
/// to the account this instance knows.
#[must_use]
pub fn session_host() -> Option<String> {
    with_sdk(|sdk| sdk.state().session.get().map(|session| session.host))
}

/// The `(host, account_id)` an admin grant would be used for, if signed in.
///
/// Granting needs both: the instance address decides where the token is sent, and the
/// account id decides whose grant a later transport may read.
pub fn admin_scope() -> Option<(String, String)> {
    with_sdk(|sdk| {
        sdk.state()
            .session
            .get()
            .map(|session| (session.host, session.account_id))
    })
}

/// Forgets the signed-in account's admin grant.
pub fn forget_admin_token() {
    if let Some((host, account)) = admin_scope() {
        if let Ok(endpoint) = oauth::normalise_host(&host) {
            admin_token::clear(&endpoint.host, &account);
        }
    }
    admin::forget_standing();
}

/// Builds an [`Instance`] from a canonical endpoint.
fn instance_for(endpoint: &oauth::Endpoint) -> Result<Instance, String> {
    Instance::parse(&endpoint.url("")).map_err(|error| error.to_string())
}

/// The instance transport, when a session has installed one.
///
/// Follow and unfollow are actions, not state, so they cannot go through
/// `dispatch`, and the SDK exposes no action surface: the transport is kept beside
/// it and handed to the [`feed`] verbs instead.
fn instance_transport() -> Option<BrowserInstance> {
    INSTANCE.with_borrow(Clone::clone)
}

/// The API base for this page: this origin plus `/api`.
///
/// The origin is the whole answer: every discovery path already begins with `/api/v1`,
/// so a base that carried `/api` would produce `/api/api/v1/...`.
///
/// The `/api` prefix belongs to the deployment, not to the client: Trunk proxies it to
/// the feeds service in development (`web/Trunk.toml`), and a same-origin deployment
/// lets Caddy serve `/api/*` from that service directly. Built with `web_sys::Url`
/// rather than string formatting so a port-bearing origin cannot produce a wrong slash.
fn api_base() -> Option<Url> {
    let from_window = web_sys::window().and_then(|window| {
        let origin = window.location().origin().ok()?;
        Url::parse(&origin).ok()
    });
    from_window.or_else(|| Url::parse(FALLBACK_API_BASE).ok())
}
