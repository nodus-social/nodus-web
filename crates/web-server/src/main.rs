//! The SSR host: a cookie session in, HTML out.
//!
//! Scope of this slice, deliberately narrow: `/` and `/home` are painted by this process;
//! every other route and every asset still comes from the static bundle, which is the same
//! app. The page it paints carries the same bundle and a JSON island, so the moment the
//! wasm arrives it takes over with the data already on screen — no spinner, and no second
//! request for a page the server has already fetched.
//!
//! Configuration, all through the environment so a unit file can set it:
//!
//! - `NODUS_LISTEN` — `host:port` to bind (default `127.0.0.1:8080`).
//! - `NODUS_ASSET_DIR` — where `trunk` wrote the bundle (default `/srv/nodus`).
//! - `NODUS_SESSIONS` — the JSON file sessions survive a restart in (default
//!   `/var/lib/nodus/sessions.json`; set it to `-` for memory only).

mod capabilities;
mod content;
mod document;
mod mastodon;
mod pages;
mod protocol;
mod public_api;
mod rows;
mod runtime;
mod session;
mod translate;
mod validate;

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use nodus_ui::session_style::SessionStyle;
use nodus_web::shared::router::Route;
use serde::Deserialize;
use serde_json::json;

use crate::document::Assets;
use crate::mastodon::Client;
use crate::pages::read_page_with_style;
use crate::session::{COOKIE, Session, Sessions};

/// How many statuses the server paints. One page, the same size the client asks for: the
/// point is to remove the first request, not to reimplement pagination.
const PAGE: u16 = 20;

#[derive(Debug)]
struct App {
    sessions: Arc<Sessions>,
    assets: std::sync::RwLock<Assets>,
    dist: PathBuf,
}

impl App {
    /// The asset names this process should serve, rediscovered when a deploy replaced them.
    fn bundle(&self) -> Assets {
        if let Ok(assets) = self.assets.read()
            && !assets.stale(&self.dist)
        {
            return assets.clone();
        }
        let fresh = Assets::discover(&self.dist);
        tracing::info!(script = %fresh.script, wasm = %fresh.wasm, "assets changed on disk");
        if let Ok(mut assets) = self.assets.write() {
            *assets = fresh.clone();
        }
        fresh
    }
}

/// A public timeline, painted with the same body the home page uses.
async fn public_feed(State(app): State<Arc<App>>, headers: HeaderMap, local: bool) -> Response {
    let Some(session) = session_of(&app, &headers) else {
        return Html(document::landing(&app.bundle())).into_response();
    };
    capabilities::resolve(&session).await;
    let staff = resolve_staff(&app, &session).await;
    let style = resolve_style(&app, &session).await;
    // The reader the chrome draws: the session's own account, which sign-in already read.
    let reader = document::Reader::from(&session);
    let client = match Client::new(&session.host, &session.token) {
        Ok(client) => client,
        Err(message) => {
            return Html(document::error_with_style(
                &app.bundle(),
                Some(&reader),
                staff,
                &message,
                Some(&style),
            ))
            .into_response();
        }
    };
    let (which, title, path) = if local {
        ("local", "Local", "/home/local")
    } else {
        ("federated", "Federated", "/home/federated")
    };
    match client.feed(local, PAGE).await {
        Ok((statuses, cursor)) => Html(document::feed_with_style(
            &app.bundle(),
            &reader,
            staff,
            which,
            title,
            path,
            &statuses,
            cursor.as_deref(),
            Some(&style),
        ))
        .into_response(),
        Err(message) => Html(document::error_with_style(
            &app.bundle(),
            Some(&reader),
            staff,
            &message,
            Some(&style),
        ))
        .into_response(),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,nodus_web=debug".into()),
        )
        .init();

    // Installed once per process: any_spawner keeps a single global set of function
    // pointers, and more than one render path reaches a component that starts a task.
    prepare_render();

    let listen = std::env::var("NODUS_LISTEN").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let listen = nodus_server_transport::parse(&listen).map_err(std::io::Error::other)?;
    let asset_dir = std::env::var("NODUS_ASSET_DIR").unwrap_or_else(|_| "/srv/nodus".to_owned());
    let sessions_path = std::env::var("NODUS_SESSIONS")
        .unwrap_or_else(|_| "/var/lib/nodus/sessions.json".to_owned());

    let assets = Assets::discover(std::path::Path::new(&asset_dir));
    let sessions = Sessions::load((sessions_path != "-").then(|| PathBuf::from(&sessions_path)));
    tracing::info!(script = %assets.script, stylesheet = ?assets.stylesheet, "serving");

    let app = Arc::new(App {
        sessions: Arc::new(sessions),
        assets: std::sync::RwLock::new(assets),
        dist: PathBuf::from(&asset_dir),
    });

    let router = router(app);

    nodus_server_transport::serve_http(&listen, router, shutdown()).await?;
    Ok(())
}

fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/session", get(whoami).post(sign_in).delete(sign_out))
        .route("/api/session/style", post(update_session_style))
        .route("/public-api/status/{host}/{id}", get(public_api::status))
        .route(
            "/public-api/status/{host}/{id}/context",
            get(public_api::context),
        )
        .route("/public-api/account/{host}/{id}", get(public_api::account))
        .route(
            "/public-api/account/{host}/{id}/statuses",
            get(public_api::account_statuses),
        )
        .route(
            "/public-api/account/{host}/{id}/followers",
            get(public_api::followers),
        )
        .route(
            "/public-api/account/{host}/{id}/following",
            get(public_api::following),
        )
        .route("/public-api/translate", post(translate::translate))
        .route("/authorize_interaction", get(authorize_interaction))
        .route("/", get(index))
        .route("/home", get(home))
        .route(
            "/home/local",
            get(|state, headers| public_feed(state, headers, true)),
        )
        .route(
            "/home/federated",
            get(|state, headers| public_feed(state, headers, false)),
        )
        .fallback(shell)
        .layer(axum::middleware::from_fn(install_executor))
        .with_state(app)
}

/// Prepares the render executor on whichever worker thread answers this request.
///
/// The executor's function pointers are process-wide, but the instance they call lives on
/// the thread that installed it, so a thread that renders without this panics instead of
/// dropping its task.
async fn install_executor(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    prepare_render();
    next.run(request).await
}

/// What `/authorize_interaction` accepts from a link somebody shared.
#[derive(Debug, Deserialize)]
struct AuthorizeQuery {
    uri: Option<String>,
}

/// `GET /authorize_interaction?uri=…` — the link Mastodon apps hand out ("open this in
/// your instance"), which arrived here as a plain 404.
///
/// Following is something only the reader's own account can do, and this process holds no
/// account, so the URL is turned into the local search for that person instead: the
/// results are the account, with the Follow button the reader already knows. A link we
/// cannot read sends the reader to search rather than to an error page — the intent was
/// to find somebody, and search is where that happens.
async fn authorize_interaction(
    axum::extract::Query(query): axum::extract::Query<AuthorizeQuery>,
) -> Response {
    match query.uri.as_deref().and_then(authorize_term) {
        Some(term) => {
            let encoded: String = url::form_urlencoded::byte_serialize(term.as_bytes()).collect();
            Redirect::to(&format!("/search?q={encoded}")).into_response()
        }
        None => Redirect::to("/search").into_response(),
    }
}

/// Reads a handle out of the shapes a shared link takes: `@user@host`, `acct:user@host`,
/// `https://host/@user`, `https://host/users/user`.
fn authorize_term(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(handle) = trimmed.strip_prefix("acct:") {
        return handle_term(handle);
    }
    if trimmed.contains("://") {
        let url = url::Url::parse(trimmed).ok()?;
        let host = url.host_str()?;
        if host.is_empty() {
            return None;
        }
        let path = url.path().trim_matches('/');
        let user = path
            .strip_prefix('@')
            .or_else(|| path.strip_prefix("users/"))
            .or_else(|| path.strip_prefix("accounts/"))?;
        let user = user.split('/').next().filter(|user| !user.is_empty())?;
        return Some(format!("@{user}@{host}"));
    }
    handle_term(trimmed)
}

fn handle_term(raw: &str) -> Option<String> {
    let handle = raw.trim_start_matches('@');
    let (user, host) = handle.rsplit_once('@')?;
    if user.is_empty() || host.is_empty() || !host.contains('.') {
        return None;
    }
    Some(format!("@{user}@{host}"))
}

#[cfg(test)]
mod authorize_tests {
    use super::authorize_term;

    #[test]
    fn shared_links_become_a_handle_search() {
        for (raw, expected) in [
            (
                "https://baraag.net/@Tomostrawberry",
                "@Tomostrawberry@baraag.net",
            ),
            (
                "https://baraag.net/@Tomostrawberry/",
                "@Tomostrawberry@baraag.net",
            ),
            (
                "https://mastodon.social/users/Gargron",
                "@Gargron@mastodon.social",
            ),
            ("@Tomostrawberry@baraag.net", "@Tomostrawberry@baraag.net"),
            ("Tomostrawberry@baraag.net", "@Tomostrawberry@baraag.net"),
            ("acct:Gargron@mastodon.social", "@Gargron@mastodon.social"),
            // A status permalink elsewhere lands on its author: the post itself cannot be
            // imported, but the person it belongs to is who the reader was looking at.
            (
                "https://baraag.net/@Tomostrawberry/115359056000000001",
                "@Tomostrawberry@baraag.net",
            ),
            ("  @Gargron@mastodon.social  ", "@Gargron@mastodon.social"),
        ] {
            assert_eq!(authorize_term(raw).as_deref(), Some(expected), "{raw}");
        }
    }

    #[test]
    fn links_we_cannot_read_are_refused() {
        for raw in [
            "",
            "   ",
            "https://baraag.net/",
            "https://baraag.net/@",
            "javascript:alert(1)",
            "just-a-word",
            "@nobody",
            "@user@nodot",
        ] {
            assert_eq!(authorize_term(raw), None, "{raw}");
        }
    }
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("stopping");
}

async fn healthz() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

/// The session cookie from a request, if it carries one.
fn cookie_of(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE)
        .map(|(_, value)| value.to_owned())
}

/// The `Set-Cookie` value. `HttpOnly` because no script needs it, `SameSite=Lax` so a
/// cross-site form cannot act as the reader, `Secure` because this host is TLS-only, and a
/// year's life because signing in again on every visit is the thing sessions exist to avoid.
fn cookie_for(id: &str) -> String {
    format!("{COOKIE}={id}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=31536000")
}

fn cleared_cookie() -> String {
    format!("{COOKIE}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0")
}

pub(crate) fn session_of(app: &App, headers: &HeaderMap) -> Option<Session> {
    app.sessions.get(&cookie_of(headers)?)
}

#[derive(Debug, Deserialize)]
struct Credentials {
    host: String,
    token: String,
}

/// Prove a token against its instance, then remember it behind an opaque id.
///
/// The token is proven before it is stored: a session that cannot read `verify_credentials`
/// would render an error page for every later request, and accepting it would make the
/// cookie a claim rather than a fact.
async fn sign_in(State(app): State<Arc<App>>, Json(body): Json<Credentials>) -> Response {
    let client = match Client::new(&body.host, &body.token) {
        Ok(client) => client,
        Err(message) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response();
        }
    };
    // Which dialect this host speaks decides how a token is proven: the compatibility
    // endpoints do not exist on Misskey, so assuming them locked its readers out entirely.
    // Detection reads the documents a server already publishes, so an unrecognised host
    // keeps the Mastodon proof it always had.
    let http = match validate::shared_public_http_client() {
        Ok(http) => http,
        Err(error) => {
            let message = error.to_string();
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response();
        }
    };
    let protocol = protocol::detect_with(&http, client.base())
        .await
        .unwrap_or_default();
    match protocol::verify_with(&http, client.base(), &body.token, protocol).await {
        Ok(verified) => {
            // The staff bit comes from the caller's own credentials document, the same
            // document the browser reads, so the first paint and hydration agree. The
            // admin-scoped account list is not asked: the session token does not carry
            // `admin:read`, so it would answer `403` for staff and plain accounts alike.
            let admin = client.is_staff().await;
            let style = client.client_metadata_style().await.ok();
            let id = app.sessions.insert(Session {
                host: client.host().to_owned(),
                token: body.token,
                account_id: verified.account_id.clone(),
                acct: verified.acct.clone(),
                protocol: verified.protocol,
                display_name: verified.display_name,
                avatar: verified.avatar,
                admin: Some(admin),
                style,
            });
            let mut response = Json(json!({
                "acct": verified.acct,
                "account_id": verified.account_id,
                "protocol": verified.protocol.as_str(),
            }))
            .into_response();
            if let Ok(value) = cookie_for(&id).parse() {
                response.headers_mut().insert(header::SET_COOKIE, value);
            }
            response
        }
        Err(message) => {
            (StatusCode::UNAUTHORIZED, Json(json!({ "error": message }))).into_response()
        }
    }
}

async fn sign_out(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if let Some(id) = cookie_of(&headers) {
        app.sessions.remove(&id);
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    if let Ok(value) = cleared_cookie().parse() {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

async fn whoami(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    match session_of(&app, &headers) {
        Some(session) => Json(session.view()).into_response(),
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalized_style_host(raw: &str) -> Option<String> {
    nodus_client_core::Instance::parse(raw)
        .ok()
        .map(|instance| validate::normalized_host(&instance.base))
}

/// Syncs updated `SessionStyle` from the browser client into SSR session state & per-account cache.
async fn update_session_style(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(style): Json<SessionStyle>,
) -> Response {
    let Some(session) = session_of(&app, &headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(raw_host) = header_value(&headers, "x-nodus-host") else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "x-nodus-host header is required" })),
        )
            .into_response();
    };
    let Some(target_account_id) = header_value(&headers, "x-nodus-account-id") else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "x-nodus-account-id header is required" })),
        )
            .into_response();
    };
    let Some(target_host) = normalized_style_host(raw_host) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid x-nodus-host header" })),
        )
            .into_response();
    };
    let session_host =
        normalized_style_host(&session.host).unwrap_or_else(|| session.host.to_ascii_lowercase());
    if target_host != session_host || target_account_id != session.account_id {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "session account mismatch" })),
        )
            .into_response();
    }

    let clean = style.sanitized();
    app.sessions
        .set_style(&session.host, &session.account_id, clean.clone());
    (StatusCode::OK, Json(clean)).into_response()
}

/// The front door. A signed-in visitor is sent to the painted home page; a stranger gets
/// the landing page, which is the same document minus the island.
async fn index(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if session_of(&app, &headers).is_some() {
        Redirect::to("/home").into_response()
    } else {
        Html(document::landing(&app.bundle())).into_response()
    }
}

/// Every route the client owns: the chrome, then the client takes over.
///
/// Where the reader holds a session, the page is painted with the data the client would
/// fetch anyway; every other path keeps the chrome, which is also the fallback when a
/// read fails. `Caddy` answers assets from disk, so anything with a file extension reaching
/// here is genuinely missing.
/// A shared link without a cookie: read the post and its thread from its own host.
async fn public_thread(app: &App, path: &str) -> Option<String> {
    let Route::Thread { host, id } = Route::from_path(path) else {
        return None;
    };
    if !validate::host_valid(&host) || !validate::id_valid(&id) {
        return None;
    }
    let client = match Client::new(&host, "") {
        Ok(client) => client,
        Err(message) => {
            tracing::warn!(%message, %host, "public post host could not be used");
            return None;
        }
    };
    let hydrated =
        capabilities::resolve_host(&host).await.is_some() && capabilities::serves_pages(&host);
    let (status, context) = if hydrated {
        match client.status_page(&id).await {
            Ok(page) => (
                page.status,
                nodus_client_core::model::Context {
                    ancestors: page.ancestors,
                    descendants: page.descendants,
                },
            ),
            Err(message) => {
                tracing::warn!(%message, %host, %id, "public post page could not be read");
                return None;
            }
        }
    } else {
        let status = match client.status(&id).await {
            Ok(status) => status,
            Err(message) => {
                tracing::warn!(%message, %host, %id, "public post could not be read");
                return None;
            }
        };
        let context = client
            .context(&id)
            .await
            .unwrap_or(nodus_client_core::model::Context {
                ancestors: Vec::new(),
                descendants: Vec::new(),
            });
        (status, context)
    };
    Some(pages::render_public_thread(
        &app.bundle(),
        &host,
        &id,
        status,
        context,
    ))
}

/// An account linked from a public post: read from its own host, without a token.
async fn public_account(app: &App, path: &str) -> Option<String> {
    let Route::Account(rest) = Route::from_path(path) else {
        return None;
    };
    let (host, segment) = rest.split_once('/')?;
    if !validate::host_valid(host) || !validate::id_valid(segment) {
        return None;
    }
    // `/accounts/{host}/{name}` is the permanent address and `/accounts/{host}/{id}` is
    // what the API hands out. A `pages` host resolves the name inside the page read, so a
    // handle costs one request; only the pre-`pages` path still needs a lookup first.
    let by_id = crate::public_api::looks_like_id(segment);
    let client = match Client::new(host, "") {
        Ok(client) => client,
        Err(message) => {
            tracing::warn!(%message, %host, "public account host could not be used");
            return None;
        }
    };
    let hydrated =
        capabilities::resolve_host(host).await.is_some() && capabilities::serves_pages(host);
    let (id, account, statuses) = if hydrated {
        let page = if by_id {
            client.profile_page(segment, PAGE).await
        } else {
            client
                .profile_page_by_handle(&format!("{segment}@{host}"), PAGE)
                .await
        };
        match page {
            Ok(page) => {
                let id = page.account.id.clone();
                (id, page.account, page.statuses)
            }
            Err(message) => {
                tracing::warn!(%message, %host, %segment, "public account page could not be read");
                return None;
            }
        }
    } else {
        let id = match crate::public_api::resolve_account_ref(host, segment, None).await {
            Some(id) => id,
            None => {
                tracing::warn!(%host, %segment, "public account name could not be resolved");
                return None;
            }
        };
        let account = match client.account(&id).await {
            Ok(account) => account,
            Err(message) => {
                tracing::warn!(%message, %host, %id, "public account could not be read");
                return None;
            }
        };
        let (statuses, _) = match client.account_statuses(&id, PAGE).await {
            Ok(page) => page,
            Err(message) => {
                tracing::warn!(%message, %host, %id, "public account posts could not be read");
                return None;
            }
        };
        (id, account, statuses)
    };
    Some(pages::render_public_account(
        &app.bundle(),
        host,
        &id,
        account,
        statuses,
    ))
}

/// The canonical page address for a handle-shaped path.
///
/// `/@name` and `/@name@host` are the addresses a reader copies from another instance, and
/// `/@name@host/{id}` is what a shared remote post looks like. The server reads pages by the
/// canonical `/accounts/{host}/{name}` and `/status/{host}/{id}` forms, so the handle form is
/// rewritten before the read rather than learned twice.
fn canonical_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/@")?;
    let mut steps = rest.split('/').filter(|step| !step.is_empty());
    let handle = steps.next()?.trim_start_matches('@');
    let id = steps.next();
    let (name, host) = match handle.split_once('@') {
        Some((name, host)) => (name.trim(), Some(host.trim())),
        None => (handle.trim(), None),
    };
    if name.is_empty() {
        return None;
    }
    let host = host.filter(|host| !host.is_empty());
    Some(match (host, id) {
        (Some(host), Some(id)) => format!("/status/{host}/{id}"),
        (Some(host), None) => format!("/accounts/{host}/{name}"),
        (None, Some(id)) => format!("/status/{id}"),
        (None, None) => {
            // A bare handle names an account here only when this deployment knows which
            // instance it is; without that the client resolves it against the session.
            let host = std::env::var("NODUS_INSTANCE_HOST").unwrap_or_default();
            let host = host.trim().to_owned();
            if host.is_empty() {
                return None;
            }
            format!("/accounts/{host}/{name}")
        }
    })
}

/// A server render is one pass over a finished tree: nothing reads a signal after it, so a
/// task started during the paint has nowhere to report and is dropped. Pages read their data
/// before the paint, through the runtime they were handed.
struct RenderExecutor;

impl any_spawner::CustomExecutor for RenderExecutor {
    fn spawn(&self, future: any_spawner::PinnedFuture<()>) {
        tokio::spawn(future);
    }

    fn spawn_local(&self, future: any_spawner::PinnedLocalFuture<()>) {
        drop(future);
    }

    fn poll_local(&self) {}
}

/// Installs the render executor on this worker thread, once per thread.
fn prepare_render() {
    let _ = any_spawner::Executor::init_local_custom_executor(RenderExecutor);
}

/// The extensions a bundle asset can carry.
///
/// A page path may contain a dot — `/@name@host` is a handle, not a file — so an extension
/// on its own does not mean "asset", only one of these does.
const ASSET_EXTENSIONS: [&str; 15] = [
    "js", "mjs", "wasm", "css", "map", "json", "png", "jpg", "jpeg", "gif", "webp", "svg", "ico",
    "woff", "woff2",
];

/// Whether `path` names an asset the bundle should have served, rather than a page.
fn is_asset(path: &str) -> bool {
    if path.ends_with('/') {
        return false;
    }
    std::path::Path::new(path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| {
            ASSET_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

/// Resolves whether `session` belongs to a moderator, caching the answer on first check.
///
/// The answer comes from the account's own credentials document and is kept on the session,
/// so a page render asks once rather than once per component. Sign-in and sign-out replace or
/// drop the session, which is what invalidates the cached bit.
async fn resolve_staff(app: &App, session: &Session) -> bool {
    if let Some(admin) = session.admin {
        return admin;
    }
    let answer = match Client::new(&session.host, &session.token) {
        Ok(client) => client.is_staff().await,
        Err(message) => {
            tracing::warn!(%message, "could not ask whether this account moderates");
            false
        }
    };
    app.sessions.set_admin(&session.host, &session.acct, answer);
    answer
}

/// Resolves the active account's `SessionStyle`, fetching fine-grained metadata once when uncached.
async fn resolve_style(app: &App, session: &Session) -> SessionStyle {
    if let Some(style) = session.style.as_ref() {
        return style.clone();
    }
    if let Some(cached) = app
        .sessions
        .cached_style(&session.host, &session.account_id)
    {
        app.sessions
            .set_style(&session.host, &session.account_id, cached.clone());
        return cached;
    }
    let fetched = match Client::new(&session.host, &session.token) {
        Ok(client) => client.client_metadata_style().await.unwrap_or_default(),
        Err(_) => SessionStyle::default(),
    };
    app.sessions
        .set_style(&session.host, &session.account_id, fetched.clone());
    fetched
}

async fn shell(State(app): State<Arc<App>>, headers: HeaderMap, uri: Uri) -> Response {
    let address = uri.path();
    if is_asset(address) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let canonical = canonical_path(address);
    let path = canonical.as_deref().unwrap_or(address);
    let Some(session) = session_of(&app, &headers) else {
        if let Some(id) = path.strip_prefix("/accounts/") {
            if !id.contains('/') {
                if let Ok(host) = std::env::var("NODUS_INSTANCE_HOST") {
                    if !host.trim().is_empty() {
                        return Redirect::to(&format!("/accounts/{}/{id}", host.trim()))
                            .into_response();
                    }
                }
            }
        }
        if let Some(html) = public_thread(&app, path).await {
            return Html(html).into_response();
        }
        if let Some(html) = public_account(&app, path).await {
            return Html(html).into_response();
        }
        return Html(document::shell(&app.bundle(), None, false, path)).into_response();
    };
    // The rail draws the moderation doors for a moderator, and it is drawn before any
    // wasm runs, so the answer has to come from the session that was established at
    // sign-in (`Session::admin`). The capability answer is what decides whether those
    // doors are painted at all: a Mastodon host does not serve them.
    capabilities::resolve(&session).await;
    let staff = resolve_staff(&app, &session).await;
    let style = resolve_style(&app, &session).await;
    let assets = app.bundle();
    let painted = match Client::new(&session.host, &session.token) {
        Ok(client) => {
            read_page_with_style(
                &client,
                &assets,
                &session.host,
                &session.acct,
                staff,
                path,
                uri.query(),
                Some(&style),
            )
            .await
        }
        Err(message) => {
            tracing::warn!(%message, "session could not be used; painting the chrome");
            None
        }
    };
    match painted {
        Some(html) => Html(html).into_response(),
        None => Html(document::shell_with_style(
            &assets,
            Some(&document::Reader::from(&session)),
            staff,
            path,
            Some(&style),
        ))
        .into_response(),
    }
}

/// The page for `path`, or `None` when this route has no server read.
///
/// A failed read is `None` as well: the chrome is a worse page than the data, but a better
/// one than an error page for a screen the client can still fill in itself.
async fn home(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    let Some(session) = session_of(&app, &headers) else {
        return Html(document::landing(&app.bundle())).into_response();
    };
    capabilities::resolve(&session).await;
    let staff = resolve_staff(&app, &session).await;
    let style = resolve_style(&app, &session).await;
    let reader = document::Reader::from(&session);
    // This process reads through Mastodon's client API. A Misskey session was proven against
    // `/api/*`, which has no `timeline/home`: painting an error as content would hide that,
    // and refusing the page would lock the reader out of their own home, so the chrome is
    // painted from the session and the client fills the timeline from the same server.
    if !session.protocol.reads_mastodon_api() {
        return Html(document::shell_with_style(
            &app.bundle(),
            Some(&reader),
            staff,
            "/home",
            Some(&style),
        ))
        .into_response();
    }
    let client = match Client::new(&session.host, &session.token) {
        Ok(client) => client,
        Err(message) => {
            return Html(document::error_with_style(
                &app.bundle(),
                Some(&reader),
                staff,
                &message,
                Some(&style),
            ))
            .into_response();
        }
    };
    paint_home(&app, &reader, staff, style, client.home(PAGE).await)
}

/// The paint half of [`home`], split out so a test can prove it paints what the read
/// returned without a network instance to read from.
fn paint_home(
    app: &App,
    reader: &document::Reader,
    staff: bool,
    style: nodus_ui::session_style::SessionStyle,
    read: Result<(Vec<nodus_client_core::model::Status>, Option<String>), String>,
) -> Response {
    match read {
        Ok((statuses, cursor)) => Html(document::home_with_style(
            &app.bundle(),
            reader,
            staff,
            &statuses,
            cursor.as_deref(),
            Some(&style),
        ))
        .into_response(),
        Err(message) => Html(document::error_with_style(
            &app.bundle(),
            Some(reader),
            staff,
            &message,
            Some(&style),
        ))
        .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::http::StatusCode;

    use super::*;

    fn test_app(host: &str) -> (Arc<App>, String, String) {
        test_app_as(host, protocol::Protocol::Nodus, true)
    }

    /// A signed-in pair for `host`; `seed` gives the Nodus staff account a published answer
    /// without making the test's closed host answer `/api/v1/meta`.
    fn test_app_as(
        host: &str,
        dialect: protocol::Protocol,
        seed: bool,
    ) -> (Arc<App>, String, String) {
        let sessions = Arc::new(Sessions::load(None));
        let staff_acct = format!("mod@{host}");
        let user_acct = format!("user@{host}");
        if seed {
            crate::capabilities::remember_for_test(
                host,
                nodus_ui::capabilities::Capabilities::from_parts(
                    "0.1.0",
                    &["admin".to_owned(), "moderation".to_owned()],
                ),
            );
        }
        let staff_id = sessions.insert(Session {
            host: host.to_owned(),
            token: "staff-token".to_owned(),
            account_id: "1".to_owned(),
            acct: staff_acct,
            protocol: dialect,
            display_name: "Mod".to_owned(),
            avatar: String::new(),
            admin: Some(true),
            style: Some(SessionStyle::default()),
        });
        let user_id = sessions.insert(Session {
            host: host.to_owned(),
            token: "user-token".to_owned(),
            account_id: "2".to_owned(),
            acct: user_acct,
            protocol: dialect,
            display_name: "User".to_owned(),
            avatar: String::new(),
            admin: Some(false),
            style: Some(SessionStyle::default()),
        });
        let app = Arc::new(App {
            sessions,
            assets: std::sync::RwLock::new(Assets {
                script: "/nodus-web.js".to_owned(),
                wasm: "/nodus-web_bg.wasm".to_owned(),
                stylesheet: None,
                stylesheet_css: None,
            }),
            dist: PathBuf::from("/nonexistent"),
        });
        (app, staff_id, user_id)
    }

    fn cookie_headers(session_id: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{COOKIE}={session_id}").parse().unwrap(),
        );
        headers
    }

    /// A status as `verify_credentials`' sibling read would hand it to the paint.
    fn status_with(content: &str) -> nodus_client_core::model::Status {
        use nodus_client_core::model::{Account, Status, Visibility};

        Status {
            id: "1".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            content: content.to_owned(),
            visibility: Visibility::Public,
            account: Account {
                id: "2".to_owned(),
                acct: "user@example.com".to_owned(),
                display_name: "User".to_owned(),
                note: String::new(),
                avatar: String::new(),
                header: String::new(),
                bot: false,
                locked: false,
                followers_count: 0,
                following_count: 0,
                statuses_count: 0,
                last_status_at: None,
                fields: Vec::new(),
                discoverable: None,
                indexable: None,
                noindex: None,
            },
            favourites_count: 0,
            reblogs_count: 0,
            replies_count: 0,
            favourited: false,
            reblogged: false,
            bookmarked: false,
            reblog: None,
            in_reply_to_id: None,
            emojis: Vec::new(),
            url: None,
            sensitive: false,
            spoiler_text: String::new(),
            language: None,
            content_format: None,
            content_source: None,
            media_attachments: Vec::new(),
            mentions: Vec::new(),
            tags: Vec::new(),
            poll: None,
            edited_at: None,
            pinned: None,
        }
    }

    #[tokio::test]
    async fn signed_in_home_paints_the_status_the_api_returned() {
        // The handler's own paint: whatever the read returned for this account is in the
        // HTML, and the chrome names the session's account rather than waiting for wasm.
        let (app, _, user_id) = test_app("example.com");
        let session = app.sessions.get(&user_id).expect("session inserted above");
        let response = paint_home(
            &app,
            &document::Reader::from(&session),
            false,
            SessionStyle::default(),
            Ok((
                vec![status_with("<p>rows read through the session</p>")],
                None,
            )),
        );
        let body = body_text(response).await;
        assert!(body.contains("rows read through the session"), "{body}");
        assert!(body.contains("@user@example.com"), "{body}");
        assert!(body.contains("User"), "{body}");
    }

    async fn body_text(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn nodus_staff_document_draws_the_administration_door_and_a_regular_one_does_not() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let (app, staff_id, user_id) = test_app(&host);

        let staff_html = body_text(
            shell(
                State(Arc::clone(&app)),
                cookie_headers(&staff_id),
                "/settings/account".parse().unwrap(),
            )
            .await,
        )
        .await;
        assert!(staff_html.contains("href=\"/admin\""), "{staff_html}");

        // The same page for an account whose own document named no role: the capability
        // gate is satisfied, so the absent door is the staff bit's decision and nothing
        // else's.
        let user_html = body_text(
            shell(
                State(app),
                cookie_headers(&user_id),
                "/settings/account".parse().unwrap(),
            )
            .await,
        )
        .await;
        assert!(!user_html.contains("href=\"/admin\""), "{user_html}");
        assert!(!user_html.contains("href=\"/moderation\""), "{user_html}");
    }

    #[tokio::test]
    async fn mastodon_staff_document_does_not_draw_nodus_doors() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let (app, staff_id, _) = test_app_as(&host, protocol::Protocol::Mastodon, false);

        let html = body_text(
            shell(
                State(app),
                cookie_headers(&staff_id),
                "/settings/account".parse().unwrap(),
            )
            .await,
        )
        .await;
        assert!(!html.contains("href=\"/admin\""), "{html}");
        assert!(!html.contains("href=\"/moderation\""), "{html}");
    }

    #[tokio::test]
    async fn anonymous_page_still_paints_its_public_chrome() {
        let (app, _, _) = test_app("example.com");
        let assets = app.bundle();
        let response = shell(State(app), HeaderMap::new(), "/sign-in".parse().unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let html = body_text(response).await;
        assert!(html.contains("Sign in · Nodus"), "{html}");
        assert!(html.contains("/nodus-web.js"), "{html}");
        assert!(!html.contains("href=\"/admin\""), "{html}");

        // A public profile is painted with no session at all; the permissive `None` must
        // keep the chrome it always drew rather than make the page look empty.
        let profile = document::page_of_public(
            &assets,
            "/@user@example.com",
            "User",
            "<p>public</p>",
            false,
        );
        assert!(profile.contains("public"), "{profile}");
        assert!(profile.contains("/nodus-web.js"), "{profile}");
        assert!(!profile.contains("href=\"/admin\""), "{profile}");
    }

    #[tokio::test]
    async fn meta_failure_paints_permissively_instead_of_an_error_page() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        // Nodus, but no seeded answer: the one meta read cannot reach this closed host, so
        // the render must keep the doors it always drew rather than turn into an error page.
        let (app, staff_id, _) = test_app_as(&host, protocol::Protocol::Nodus, false);

        let response = shell(
            State(app),
            cookie_headers(&staff_id),
            "/settings/account".parse().unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let html = body_text(response).await;
        assert!(html.contains("href=\"/admin\""), "{html}");
        assert!(!html.contains("Timeline unavailable"), "{html}");
    }

    #[tokio::test]
    async fn staff_navigation_does_not_leak_across_requests_on_same_thread() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_host = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);

        let (app, staff_id, user_id) = test_app(&closed_host);

        // Staff request hits `shell`, which awaits `read_page` and falls back to `document::shell`.
        let staff_html = body_text(
            shell(
                State(Arc::clone(&app)),
                cookie_headers(&staff_id),
                "/settings/account".parse().unwrap(),
            )
            .await,
        )
        .await;
        assert!(
            staff_html.contains("href=\"/moderation\"") && staff_html.contains("href=\"/admin\""),
            "staff response should include moderation and admin navigation"
        );

        // Subsequent non-staff and anonymous requests on the same thread must never inherit staff links.
        let home_html =
            body_text(home(State(Arc::clone(&app)), cookie_headers(&user_id)).await).await;
        assert!(
            !home_html.contains("href=\"/moderation\"") && !home_html.contains("href=\"/admin\""),
            "non-staff /home response leaked staff navigation"
        );

        let feed_html =
            body_text(public_feed(State(Arc::clone(&app)), cookie_headers(&user_id), true).await)
                .await;
        assert!(
            !feed_html.contains("href=\"/moderation\"") && !feed_html.contains("href=\"/admin\""),
            "non-staff /home/local response leaked staff navigation"
        );

        let anon_html = body_text(
            shell(
                State(Arc::clone(&app)),
                HeaderMap::new(),
                "/sign-in".parse().unwrap(),
            )
            .await,
        )
        .await;
        assert!(
            !anon_html.contains("href=\"/moderation\"") && !anon_html.contains("href=\"/admin\""),
            "anonymous /sign-in response leaked staff navigation"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn staff_navigation_is_isolated_across_concurrent_tokio_tasks() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_host = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);

        let (app, staff_id, user_id) = test_app(&closed_host);
        let mut handles = Vec::new();
        for index in 0..40 {
            let app = Arc::clone(&app);
            let is_staff = index % 2 == 0;
            let session_id = if is_staff {
                staff_id.clone()
            } else {
                user_id.clone()
            };
            handles.push(tokio::spawn(async move {
                // `/lists` triggers `read_page` to await an HTTP call (which yields across worker threads).
                let html = body_text(
                    shell(
                        State(app),
                        cookie_headers(&session_id),
                        "/lists".parse().unwrap(),
                    )
                    .await,
                )
                .await;
                (is_staff, html)
            }));
        }

        for handle in handles {
            let (is_staff, html) = handle.await.unwrap();
            let has_mod = html.contains("href=\"/moderation\"");
            let has_admin = html.contains("href=\"/admin\"");
            if is_staff {
                assert!(
                    has_mod && has_admin,
                    "staff response lost moderation/admin links across await"
                );
            } else {
                assert!(
                    !has_mod && !has_admin,
                    "non-staff response received leaked moderation/admin links"
                );
            }
        }
    }
    async fn spawn_test_server_with_app() -> (String, Arc<App>) {
        let app = Arc::new(App {
            sessions: Arc::new(Sessions::load(None)),
            assets: std::sync::RwLock::new(Assets::discover(&PathBuf::from("/nonexistent"))),
            dist: PathBuf::from("/nonexistent"),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app_for_server = Arc::clone(&app);
        tokio::spawn(async move {
            let _ = axum::serve(listener, router(app_for_server)).await;
        });
        (format!("http://{addr}"), app)
    }

    async fn spawn_test_server() -> String {
        spawn_test_server_with_app().await.0
    }

    async fn raw_http_status(addr: &str, path: &str) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let host_port = addr.strip_prefix("http://").unwrap_or(addr);
        let mut stream = tokio::net::TcpStream::connect(host_port).await.unwrap();
        let req = format!("GET {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\n\r\n");
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        let text = String::from_utf8_lossy(&buf);
        let status_line = text.lines().next().unwrap();
        status_line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap()
    }

    async fn spawn_canary_listener() -> (u16, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_clone = Arc::clone(&hits);
        tokio::spawn(async move {
            while let Ok((_stream, _)) = listener.accept().await {
                hits_clone.fetch_add(1, Ordering::SeqCst);
            }
        });
        (port, hits)
    }

    #[test]
    fn client_rejects_ssrf_targets() {
        for bad in [
            "127.0.0.1:8080",
            "http://localhost:3000",
            "10.0.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "http://127.0.0.1:8080",
            "https://[::1]:8080",
            "https://user:pass@mastodon.social",
            "https://mastodon.social/extra/path",
        ] {
            assert!(
                Client::new(bad, "").is_err(),
                "expected Client::new({bad:?}) to fail"
            );
        }
    }

    #[tokio::test]
    async fn public_api_rejects_ssrf_and_path_traversal() {
        let base = spawn_test_server().await;
        let (canary_port, canary_hits) = spawn_canary_listener().await;
        let client = reqwest::Client::new();

        // SSRF attempts against loopback/private/metadata targets.
        for path in [
            format!("/public-api/status/127.0.0.1:{canary_port}/1"),
            format!("/public-api/status/127.0.0.1:{canary_port}/1/context"),
            format!("/public-api/account/127.0.0.1:{canary_port}/1"),
            format!("/public-api/account/127.0.0.1:{canary_port}/1/statuses"),
            format!("/public-api/account/127.0.0.1:{canary_port}/1/followers"),
            format!("/public-api/account/127.0.0.1:{canary_port}/1/following"),
            "/public-api/status/169.254.169.254/latest".to_owned(),
            "/public-api/account/10.0.0.1/1".to_owned(),
            "/public-api/status/192.168.1.1/1".to_owned(),
            "/public-api/status/foo.localhost/1".to_owned(),
        ] {
            let resp = client.get(format!("{base}{path}")).send().await.unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "path={path}");
        }
        assert_eq!(canary_hits.load(Ordering::SeqCst), 0);

        // Path traversal and query-injection attempts via {id} and max_id.
        for path in [
            "/public-api/status/mastodon.social/..%2f..%2fapi%2fv1%2finstance",
            "/public-api/status/mastodon.social/..%2f..%2fapi%2fv1%2finstance/context",
            "/public-api/account/mastodon.social/..%2fadmin",
            "/public-api/account/mastodon.social/..%2fadmin/statuses",
            "/public-api/account/mastodon.social/..%2fadmin/followers",
            "/public-api/account/mastodon.social/..%2fadmin/following",
            "/public-api/account/mastodon.social/1/statuses?max_id=1%26admin=true",
            "/public-api/account/mastodon.social/1/statuses?max_id=..",
        ] {
            let resp = client.get(format!("{base}{path}")).send().await.unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "path={path}");
        }

        // Literal `.` / `..` segments sent without client-side URL normalization (`curl --path-as-is`).
        for path in [
            "/public-api/status/mastodon.social/..",
            "/public-api/status/mastodon.social/.",
            "/public-api/status/mastodon.social/%2e%2e",
            "/public-api/status/mastodon.social/%2e",
        ] {
            assert_eq!(raw_http_status(&base, path).await, 400, "path={path}");
        }
    }

    #[tokio::test]
    async fn sign_in_and_public_pages_block_ssrf_and_get_sign_in_is_inert() {
        let (base, app) = spawn_test_server_with_app().await;
        let (canary_port, canary_hits) = spawn_canary_listener().await;
        let client = reqwest::Client::new();

        // GET /sign-in must never perform outbound requests or set session cookies.
        let get_sign_in = client
            .get(format!("{base}/sign-in?host=127.0.0.1:{canary_port}"))
            .send()
            .await
            .unwrap();
        assert_eq!(get_sign_in.status(), StatusCode::OK);
        assert!(get_sign_in.headers().get("set-cookie").is_none());

        // GET /api/session must not sign in.
        let get_session = client
            .get(format!("{base}/api/session"))
            .send()
            .await
            .unwrap();
        assert_eq!(get_session.status(), StatusCode::UNAUTHORIZED);

        // POST /api/session must reject SSRF hosts without connecting.
        let post_ssrf = client
            .post(format!("{base}/api/session"))
            .json(&serde_json::json!({
                "host": format!("127.0.0.1:{canary_port}"),
                "token": "secret",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(post_ssrf.status(), StatusCode::BAD_REQUEST);

        // SSR public_thread / public_account must reject SSRF hosts and traversal IDs without connecting.
        assert!(
            public_thread(&app, &format!("/status/127.0.0.1:{canary_port}/1"))
                .await
                .is_none()
        );
        assert!(
            public_thread(&app, "/status/mastodon.social/1/../../admin")
                .await
                .is_none()
        );
        assert!(
            public_account(&app, &format!("/accounts/127.0.0.1:{canary_port}/1"))
                .await
                .is_none()
        );
        assert!(
            public_account(&app, "/accounts/mastodon.social/1/../../admin")
                .await
                .is_none()
        );

        assert_eq!(
            canary_hits.load(Ordering::SeqCst),
            0,
            "server must not connect to loopback canary"
        );
    }

    #[tokio::test]
    async fn authenticated_path_traversal_is_blocked_with_bad_request() {
        let (base, app) = spawn_test_server_with_app().await;
        let session_id = app.sessions.insert(crate::session::Session {
            host: "mastodon.social".to_owned(),
            token: "super-secret-bearer-token".to_owned(),
            account_id: "42".to_owned(),
            acct: "alice@mastodon.social".to_owned(),
            protocol: protocol::Protocol::Mastodon,
            display_name: "Alice".to_owned(),
            avatar: String::new(),
            admin: Some(false),
            style: Some(SessionStyle::default()),
        });
        let client = reqwest::Client::new();
        for path in [
            "/public-api/status/mastodon.social/..%2f..%2fapi%2fv1%2fadmin%2faccounts",
            "/public-api/account/mastodon.social/..%2f..%2fapi%2fv1%2faccounts%2fverify_credentials",
            "/public-api/account/mastodon.social/1/statuses?max_id=1%26admin=true",
        ] {
            let resp = client
                .get(format!("{base}{path}"))
                .header("cookie", format!("{}={session_id}", crate::session::COOKIE))
                .send()
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "path={path}");
        }
    }

    #[tokio::test]
    async fn session_style_sync_requires_auth_sanitizes_and_isolates_multi_account_ssr() {
        use nodus_ui::session_style::{BannerRegion, ImageSlot, NavPlacement, SplitDirection};

        let (base, app) = spawn_test_server_with_app().await;
        let client = reqwest::Client::new();

        // Unauthenticated style sync must be rejected.
        let unauth = client
            .post(format!("{base}/api/session/style"))
            .json(&SessionStyle::default())
            .send()
            .await
            .unwrap();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

        // Two distinct accounts on one host plus a same-id account on a second host.
        let alice_id = app.sessions.insert(Session {
            host: "alpha.example".to_owned(),
            token: "tok-a".to_owned(),
            account_id: "1".to_owned(),
            acct: "alice@alpha.example".to_owned(),
            protocol: protocol::Protocol::Mastodon,
            display_name: String::new(),
            avatar: String::new(),
            admin: Some(false),
            style: Some(SessionStyle::default()),
        });
        let mut bob_style = SessionStyle {
            top: BannerRegion {
                hidden: false,
                split: SplitDirection::Vertical,
                slots: vec![
                    ImageSlot::new("https://cdn.example/bob-top.png", Some("Bob"), None).unwrap(),
                ],
            },
            ..SessionStyle::default()
        };
        bob_style.nav.move_item_to("lists", NavPlacement::Right);
        let bob_id = app.sessions.insert(Session {
            host: "alpha.example".to_owned(),
            token: "tok-b".to_owned(),
            account_id: "2".to_owned(),
            acct: "bob@alpha.example".to_owned(),
            protocol: protocol::Protocol::Mastodon,
            display_name: String::new(),
            avatar: String::new(),
            admin: Some(false),
            style: Some(bob_style),
        });
        let carol_id = app.sessions.insert(Session {
            host: "beta.example".to_owned(),
            token: "tok-c".to_owned(),
            account_id: "1".to_owned(),
            acct: "carol@beta.example".to_owned(),
            protocol: protocol::Protocol::Mastodon,
            display_name: String::new(),
            avatar: String::new(),
            admin: Some(false),
            style: Some(SessionStyle::default()),
        });

        let stale_payload = serde_json::json!({
            "top": {
                "split": "horizontal",
                "slots": [{ "url": "https://cdn.example/stale-overwrite.png" }]
            }
        });

        // Missing account-binding headers must be rejected cleanly.
        let missing_headers = client
            .post(format!("{base}/api/session/style"))
            .header("cookie", format!("{COOKIE}={alice_id}"))
            .json(&stale_payload)
            .send()
            .await
            .unwrap();
        assert_eq!(missing_headers.status(), StatusCode::BAD_REQUEST);

        // Stale sync targeting Bob or Carol while Alice's session cookie is active must return 409 Conflict
        // and leave every account's cached SSR style untouched.
        let wrong_account = client
            .post(format!("{base}/api/session/style"))
            .header("cookie", format!("{COOKIE}={alice_id}"))
            .header("x-nodus-host", "alpha.example")
            .header("x-nodus-account-id", "2")
            .json(&stale_payload)
            .send()
            .await
            .unwrap();
        assert_eq!(wrong_account.status(), StatusCode::CONFLICT);

        let wrong_host = client
            .post(format!("{base}/api/session/style"))
            .header("cookie", format!("{COOKIE}={alice_id}"))
            .header("x-nodus-host", "beta.example")
            .header("x-nodus-account-id", "1")
            .json(&stale_payload)
            .send()
            .await
            .unwrap();
        assert_eq!(wrong_host.status(), StatusCode::CONFLICT);

        assert!(
            app.sessions
                .cached_style("alpha.example", "1")
                .unwrap()
                .top
                .slots
                .is_empty()
        );
        assert_eq!(
            app.sessions
                .cached_style("alpha.example", "2")
                .unwrap()
                .top
                .slots[0]
                .url,
            "https://cdn.example/bob-top.png"
        );
        assert!(
            app.sessions
                .cached_style("beta.example", "1")
                .unwrap()
                .top
                .slots
                .is_empty()
        );

        // Update Alice via POST /api/session/style with mixed valid and XSS image URLs.
        let payload = serde_json::json!({
            "top": {
                "split": "horizontal",
                "slots": [
                    { "url": "https://cdn.example/alice-1.png", "alt": "</script><img src=x>" },
                    { "url": "javascript:alert(1)" },
                    { "url": "https://cdn.example/alice-2.png" }
                ]
            },
            "left": {
                "split": "vertical",
                "slots": [{ "url": "https://cdn.example/alice-left.png" }]
            },
            "right": {
                "split": "horizontal",
                "slots": [{ "url": "https://cdn.example/alice-right.png" }]
            }
        });
        let resp = client
            .post(format!("{base}/api/session/style"))
            .header("cookie", format!("{COOKIE}={alice_id}"))
            .header("x-nodus-host", "https://ALPHA.EXAMPLE/")
            .header("x-nodus-account-id", "1")
            .json(&payload)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let returned: SessionStyle = resp.json().await.unwrap();
        assert_eq!(returned.top.slots.len(), 2);
        assert_eq!(returned.top.slots[0].url, "https://cdn.example/alice-1.png");

        // Next SSR render for Alice draws her banners + escapes the JSON island, without leaking to Bob or Carol.
        let alice_html = client
            .get(format!("{base}/settings/account"))
            .header("cookie", format!("{COOKIE}={alice_id}"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(alice_html.contains("https://cdn.example/alice-1.png"));
        assert!(alice_html.contains("https://cdn.example/alice-2.png"));
        assert!(alice_html.contains("https://cdn.example/alice-left.png"));
        assert!(alice_html.contains("https://cdn.example/alice-right.png"));
        assert!(alice_html.contains("id=\"nodus-session-style\""));
        assert!(!alice_html.contains("javascript:alert(1)"));
        assert!(!alice_html.contains("</script><img"));
        assert!(!alice_html.contains("bob-top.png"));

        // Bob on the same host retains Bob's distinct styling.
        let bob_html = client
            .get(format!("{base}/settings/styling"))
            .header("cookie", format!("{COOKIE}={bob_id}"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(bob_html.contains("https://cdn.example/bob-top.png"));
        assert!(!bob_html.contains("alice-1.png"));

        // Carol with the same account_id "1" on another host remains isolated.
        let carol_html = client
            .get(format!("{base}/settings/account"))
            .header("cookie", format!("{COOKIE}={carol_id}"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(!carol_html.contains("alice-1.png"));
        assert!(!carol_html.contains("bob-top.png"));
    }
}
