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

use std::cell::{Cell, RefCell};
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
mod feed;
mod impressions;
pub mod limits;
pub mod notices;
pub mod pages;
mod posts;
mod runtime;
pub mod seed;
pub mod session;
pub mod sidebar;
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
/// Whether the signed-in server runs Nodus's own extensions.
///
/// A plain mirror of the negotiation's answer, kept outside the reactive graph so a caller
/// can decide before it sends anything: a Mastodon instance has no `/api/v1/nodus/*`, and
/// asking it is a guaranteed 404 in every console. An unanswered negotiation is the
/// Mastodon surface.
pub(crate) fn knows_extensions() -> bool {
    EXTENSIONS.with(Cell::get).unwrap_or(false)
}

fn record_extensions(knows: bool) {
    EXTENSIONS.with(|recorded| recorded.set(Some(knows)));
}

thread_local! {
    static EXTENSIONS: Cell<Option<bool>> = const { Cell::new(None) };
}

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
use nodus_client_core::actions::AdminActions;

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
            let capabilities = sdk.load_capabilities().await;
            record_extensions(capabilities.knows_extensions());
            // A server without the extensions cannot answer for styling, so the
            // question is only worth asking once the negotiation has said it can.
            if knows_extensions() {
                let _ = crate::state::bridge::style::refresh_active().await;
                crate::state::bridge::style::prefetch_account_thumbnails().await;
            }
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

/// Keeps an admin grant for `account` on `host`, after proving the token can moderate.
///
/// The probe is the point: a token that cannot open the report queue would leave the
/// Administration tab claiming a permission it does not have, and the operator would
/// see an empty screen instead of "the grant did not work".
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
    transport
        .reports(nodus_client_core::model::ReportState::Open, 1)
        .await
        .map_err(|error| {
            format!(
                "That account cannot moderate on {}, so the grant was not kept: {error}",
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
