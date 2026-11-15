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

mod content;
mod document;
mod mastodon;
mod pages;
mod public_api;
mod rows;
mod runtime;
mod session;
mod translate;
mod validate;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use nodus_web::shared::router::Route;
use serde::Deserialize;
use serde_json::json;

use crate::document::Assets;
use crate::mastodon::Client;
use crate::pages::read_page;
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
    let staff = resolve_staff(&app, &session).await;
    let client = match Client::new(&session.host, &session.token) {
        Ok(client) => client,
        Err(message) => {
            return Html(document::error(&app.bundle(), staff, &message)).into_response();
        }
    };
    let (which, title, path) = if local {
        ("local", "Local", "/home/local")
    } else {
        ("federated", "Federated", "/home/federated")
    };
    match client.feed(local, PAGE).await {
        Ok((statuses, cursor)) => Html(document::feed(
            &app.bundle(),
            &session.acct,
            staff,
            which,
            title,
            path,
            &statuses,
            cursor.as_deref(),
        ))
        .into_response(),
        Err(message) => Html(document::error(&app.bundle(), staff, &message)).into_response(),
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

    let listen = std::env::var("NODUS_LISTEN").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
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

    let address: SocketAddr = listen.parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/session", get(whoami).post(sign_in).delete(sign_out))
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
        .with_state(app)
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
    match client.whoami().await {
        Ok(account) => {
            let admin = client.is_staff().await;
            let id = app.sessions.insert(Session {
                host: client.host().to_owned(),
                token: body.token,
                account_id: account.id.clone(),
                acct: account.acct.clone(),
                admin: Some(admin),
            });
            let mut response = Json(json!({
                "acct": account.acct,
                "account_id": account.id,
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
/// fetch anyway; every other path keeps the skeleton, which is also the fallback when a
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
    let (host, id) = rest.split_once('/')?;
    if !validate::host_valid(host) || !validate::id_valid(id) {
        return None;
    }
    let client = Client::new(host, "").ok()?;
    let account = client.account(id).await.ok()?;
    let (statuses, _) = client.account_statuses(id, PAGE).await.ok()?;
    Some(pages::render_public_account(
        &app.bundle(),
        host,
        id,
        account,
        statuses,
    ))
}

/// Resolves whether `session` belongs to a moderator, caching the answer on first check.
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

async fn shell(State(app): State<Arc<App>>, headers: HeaderMap, uri: Uri) -> Response {
    let path = uri.path();
    if std::path::Path::new(path)
        .extension()
        .is_some_and(|_| !path.ends_with('/'))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
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
    // sign-in (`Session::admin`).
    let staff = resolve_staff(&app, &session).await;
    let assets = app.bundle();
    let painted = match Client::new(&session.host, &session.token) {
        Ok(client) => read_page(&client, &assets, &session.acct, staff, path, uri.query()).await,
        Err(message) => {
            tracing::warn!(%message, "session could not be used; painting the skeleton");
            None
        }
    };
    match painted {
        Some(html) => Html(html).into_response(),
        None => Html(document::shell(&assets, Some(&session.acct), staff, path)).into_response(),
    }
}

/// The page for `path`, or `None` when this route has no server read.
///
/// A failed read is `None` as well: the skeleton is a worse page than the data, but a better
/// one than an error page for a screen the client can still fill in itself.
async fn home(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    let Some(session) = session_of(&app, &headers) else {
        return Html(document::landing(&app.bundle())).into_response();
    };
    let staff = resolve_staff(&app, &session).await;
    let client = match Client::new(&session.host, &session.token) {
        Ok(client) => client,
        Err(message) => {
            return Html(document::error(&app.bundle(), staff, &message)).into_response();
        }
    };
    match client.home(PAGE).await {
        Ok((statuses, cursor)) => Html(document::home(
            &app.bundle(),
            &session.acct,
            staff,
            &statuses,
            cursor.as_deref(),
        ))
        .into_response(),
        Err(message) => Html(document::error(&app.bundle(), staff, &message)).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::http::StatusCode;

    use super::*;

    fn test_app(host: &str) -> (Arc<App>, String, String) {
        let sessions = Arc::new(Sessions::load(None));
        let staff_id = sessions.insert(Session {
            host: host.to_owned(),
            token: "staff-token".to_owned(),
            account_id: "1".to_owned(),
            acct: "mod@example.com".to_owned(),
            admin: Some(true),
        });
        let user_id = sessions.insert(Session {
            host: host.to_owned(),
            token: "user-token".to_owned(),
            account_id: "2".to_owned(),
            acct: "user@example.com".to_owned(),
            admin: Some(false),
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

    async fn body_text(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
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
            admin: Some(false),
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
}
