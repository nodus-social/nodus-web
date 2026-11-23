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
    /// What this host serves, once per document, so the first client render gates on the
    /// same answer the server painted with instead of waiting for a probe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<serde_json::Value>,
}

/// The capability answer as the island carries it, when this host has one.
///
/// The serialized answer keeps `provider` and `contract` beside `features`; only the
/// `features` array is replaced by the names the host published, in its own order. A
/// typed set iterates in declaration order, which is not the endpoint's order, and the
/// client compares this list to the endpoint's answer.
fn capabilities_island(host: &str) -> Option<serde_json::Value> {
    let at = crate::capabilities::answer_for(host)?;
    let mut answer = serde_json::to_value(&at).ok()?;
    if let Some(names) = crate::capabilities::published_names_for(host) {
        answer["features"] = serde_json::json!(names);
    }
    Some(answer)
}

/// Adds the capability answer to an island built by `pages` as a plain JSON value.
///
/// A page with no answer (anonymous, or a failed read) keeps the island it had: unknown is
/// not a refusal, and the client still has its probe.
fn with_capabilities(json: String, host: &str) -> String {
    let Some(capabilities) = capabilities_island(host) else {
        return json;
    };
    let Ok(mut island) = serde_json::from_str::<serde_json::Value>(&json) else {
        return json;
    };
    if let Some(object) = island.as_object_mut() {
        object.insert("capabilities".to_owned(), capabilities);
    }
    serde_json::to_string(&island).unwrap_or(json)
}

/// The signed-in reader, as a painted page needs them.
///
/// The server holds no SDK account for the reader: what it has is the session it proved at
/// sign-in, with the display name and avatar read in that same request. This is that,
/// shaped for the rail card the browser builds from its own account — plain data, so the
/// shared rail component draws both paints.
#[derive(Debug, Clone, Default)]
pub struct Reader {
    pub host: String,
    pub account_id: String,
    pub acct: String,
    pub display_name: String,
    pub avatar: String,
}

impl Reader {
    /// A reader known only by handle, for screens whose address names the account.
    #[must_use]
    pub fn of_handle(acct: &str) -> Self {
        let host = acct.rsplit_once('@').map_or("", |(_, host)| host);
        Self {
            host: host.to_owned(),
            account_id: String::new(),
            acct: acct.to_owned(),
            ..Self::default()
        }
    }

    /// The rail's picture of this reader, with the browser's own rules: a blank display
    /// name prints the handle, and the card links to the permanent address another
    /// instance can resolve (`handles::profile_path_for`), not to this host's numbering.
    #[must_use]
    pub fn card(&self) -> nodus_ui::components::rail::ProfileCardData {
        use nodus_web::shared::handles::{host_of_acct, name_of_acct, profile_path_for};

        // A session's `acct` is already fully qualified (`name@host`); the browser's account
        // reports a bare local name. Splitting it here keeps the painted handle identical to
        // the one the client draws instead of doubling the host.
        let host = host_of_acct(&self.acct).unwrap_or(&self.host);
        let name = name_of_acct(&self.acct);
        nodus_ui::components::rail::ProfileCardData {
            display_name: if self.display_name.trim().is_empty() {
                self.acct.clone()
            } else {
                self.display_name.clone()
            },
            handle: format!("@{name}@{host}"),
            avatar_url: self.avatar.clone(),
            avatar_shape: None,
            // The rail outlives a session switch, so it names the account the way another
            // instance understands it — the browser's own rule.
            href: profile_path_for(&self.acct, Some(&self.host)),
        }
    }
}

impl From<&crate::session::Session> for Reader {
    fn from(session: &crate::session::Session) -> Self {
        Self {
            host: session.host.clone(),
            account_id: session.account_id.clone(),
            acct: session.acct.clone(),
            display_name: session.display_name.clone(),
            avatar: session.avatar.clone(),
        }
    }
}

/// Whether a staff door should be painted for `surface`.
///
/// `None` is permissive because the render has no account to ask about or the one answer
/// failed; `Some` must be a Nodus extension answer that serves the surface. The client's
/// staff rail applies the same extension check before its role check, because Mastodon's
/// baseline says its admin route exists even though these Nodus screens do not.
fn staff_door_served(
    at: Option<&nodus_ui::capabilities::Capabilities>,
    surface: nodus_ui::capabilities::Surface,
) -> bool {
    use nodus_ui::capabilities::admits;

    at.is_none_or(|at| at.knows_extensions() && admits(Some(at), surface))
}

/// The two doors a moderator has, each already filtered by the capability answer.
///
/// The booleans are the same ones that decided whether the card is drawn at all, so the
/// heading and the rows below it cannot disagree about which doors survived the gate.
fn staff_doors(path: &str, moderation: bool, administration: bool) -> leptos::prelude::AnyView {
    imperative::rail_rows(path, moderation, administration)
}

/// Markup for the extra rail rows, kept beside the frame so both come from one place.
mod imperative {
    use leptos::prelude::*;

    /// The moderation and administration rows, each only when its surface is served.
    #[must_use]
    pub fn rail_rows(path: &str, moderation: bool, administration: bool) -> AnyView {
        let path = path.to_owned();
        view! {
            {moderation.then(|| view! {
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
            })}
            {administration.then(|| view! {
                <nodus_ui::RailEntry
                    label="Administration"
                    href="/admin"
                    hint="What this instance publishes and allows"
                    icon=nodus_ui::icons::Icon::Globe
                    active=Signal::derive(move || nodus_ui::within("/admin", &path))
                />
            })}
        }
        .into_any()
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the frame is drawn from session facts, route, style and reader"
)]
fn chrome_with_style(
    signed_in: bool,
    staff: bool,
    path: &str,
    inner: &str,
    style: Option<&nodus_ui::session_style::SessionStyle>,
    reader: Option<&Reader>,
) -> String {
    use leptos::prelude::{IntoAny, Owner, Signal};
    use nodus_ui::capabilities::Surface;

    // The frame, the rail and the icons come from `nodus-view`, the same components the
    // browser mounts. That is the whole point: two templates drift, one tree cannot.
    let path = path.to_owned();
    let door_path = path.clone();
    let inner = inner.to_owned();
    let style_prop = style.cloned().map(Signal::stored);
    // The capability answer for this reader, looked up from the value `resolve` cached before
    // the paint. `None` is not "nothing is served": it means this page has no account to ask
    // about (public chrome) or the one read failed. Both paint permissively, which is the
    // behaviour the doors had before the server knew any better.
    let at = reader.and_then(|reader| crate::capabilities::answer_for(&reader.host));
    let staff = signed_in && staff;
    let moderation_door = staff && staff_door_served(at.as_ref(), Surface::Moderation);
    let administration_door = staff && staff_door_served(at.as_ref(), Surface::Admin);
    // A staff card with no doors under it is not a section: it is an empty heading, so the
    // same two booleans that pick the rows decide whether the card is drawn.
    let staff_doors_shown = moderation_door || administration_door;
    let provided = at;
    Owner::new().with(|| {
        if let Some(at) = provided {
            nodus_ui::capabilities::provide_serves(Signal::stored(at));
        }
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
            // The same card the browser draws, from the session's own account rather than
            // from a store the server does not have.
            profile: reader.map(|reader| Signal::stored(Some(reader.card()))),
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
            // The unread count is a browser read; a painted rail offers the door as it
            // always has, and the client replaces it once it knows whether the instance
            // serves notifications at all.
            notices: None,
            extra: staff_doors_shown.then(move || nodus_ui::ExtraDoors {
                present: Signal::stored(staff_doors_shown),
                rows: std::sync::Arc::new(move || {
                    staff_doors(&door_path, moderation_door, administration_door)
                }),
            }),
        })
        .to_html()
    })
}

fn chrome(signed_in: bool, staff: bool, path: &str, inner: &str) -> String {
    chrome_with_style(signed_in, staff, path, inner, None, None)
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
    home_with_style(
        assets,
        &Reader::of_handle(acct),
        staff,
        statuses,
        cursor,
        None,
    )
}

/// The signed-in home page, painted with the active account's `SessionStyle`.
pub fn home_with_style(
    assets: &Assets,
    reader: &Reader,
    staff: bool,
    statuses: &[Status],
    cursor: Option<&str>,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    feed_with_style(
        assets, reader, staff, "home", "Home", "/home", statuses, cursor, style,
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
        assets,
        &Reader::of_handle(acct),
        staff,
        which,
        title,
        path,
        statuses,
        cursor,
        None,
    )
}

/// A timeline page painted with the active account's `SessionStyle`.
#[expect(
    clippy::too_many_arguments,
    reason = "renders a feed document from request, session, fetched timeline data, and style"
)]
pub fn feed_with_style(
    assets: &Assets,
    reader: &Reader,
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
        capabilities: capabilities_island(&reader.host),
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
            runtime: crate::runtime::StaticRuntime::page_for(
                reader,
                statuses,
                cursor.map(str::to_owned),
            ),
            route,
            card,
            header: Some(header),
            banner: None,
        })
        .to_html()
    });
    page_with_style(
        assets,
        &format!("{title} · {}", reader.acct),
        &chrome_with_style(true, staff, path, &body, style, Some(reader)),
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
    page_of_with_style(
        assets,
        &Reader::of_handle(acct),
        staff,
        path,
        title,
        inner,
        island,
        None,
    )
}

/// Paints a page component tree alongside the active account's `SessionStyle`.
#[expect(
    clippy::too_many_arguments,
    reason = "document envelope takes asset manifest, session state, route metadata, and style"
)]
pub fn page_of_with_style(
    assets: &Assets,
    reader: &Reader,
    staff: bool,
    path: &str,
    title: &str,
    inner: &str,
    island: Option<String>,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    // The island is the document's one copy of the capability answer: the screens gate on
    // it and the client adopts it, so the first render needs no probe.
    let island = island.map(|json| with_capabilities(json, &reader.host));
    page_with_style(
        assets,
        &format!("{title} · {}", reader.acct),
        &chrome_with_style(true, staff, path, inner, style, Some(reader)),
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
    shell_with_style(
        assets,
        acct.map(Reader::of_handle).as_ref(),
        staff,
        path,
        None,
    )
}

pub fn shell_with_style(
    assets: &Assets,
    reader: Option<&Reader>,
    staff: bool,
    path: &str,
    style: Option<&nodus_ui::session_style::SessionStyle>,
) -> String {
    use leptos::prelude::*;

    let Some(reader) = reader else {
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
    };

    crate::pages::render_unclaimed_with_style(assets, &reader.acct, staff, path, style)
}

#[cfg(test)]
pub fn error(assets: &Assets, staff: bool, message: &str) -> String {
    error_with_style(assets, None, staff, message, None)
}

pub fn error_with_style(
    assets: &Assets,
    reader: Option<&Reader>,
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
        &chrome_with_style(true, staff, "/home", &inner, style, reader),
        None,
        style,
        false,
    )
}

/// The shell both pages share: the app's stylesheet, its bundle, and the island.
fn page(assets: &Assets, title: &str, body: &str, island: Option<&str>, noindex: bool) -> String {
    page_with_style(assets, title, body, island, None, noindex)
}

/// Whether this document claims to be a Leptos tree the client may adopt.
///
/// `data-hydrate` is a promise: `#app`'s children are the node sequence the client's own
/// view renders, including Leptos' branch markers (`<!--bo-…-->` / `<!--bc-…-->`) and its
/// empty-view placeholders. Only `leptos::ssr::render_to_string` of that exact tree keeps
/// the promise; the hand-painted shell below does not, because it writes `inner_html` blobs
/// and plain markup. The switch exists so a spike can measure the failure; a deploy only
/// sets it once the paint comes from that render.
fn hydrate_marker() -> &'static str {
    match std::env::var("NODUS_TRY_HYDRATE") {
        Ok(value) if value.trim() == "1" => " data-hydrate",
        _ => "",
    }
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
    // The module is discovered only after the document is parsed, and the wasm only after
    // the module runs, so without these the 1.8 MB wasm fetch waits for the JS round trip.
    // The body's inline module still imports the same two hashed files; these links just
    // let the browser start both fetches while the parser is still in the head.
    let preload = format!(
        "<link rel=\"modulepreload\" href=\"{script}\">\
         <link rel=\"preload\" as=\"fetch\" crossorigin href=\"{wasm}\">",
        script = escape(&assets.script),
        wasm = escape(&assets.wasm),
    );
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
             {preload}\
             <script type=\"module\">\
               import init, * as bindings from '{script}';\
               await init({{ module_or_path: '{wasm}' }});\
               window.wasmBindings = bindings;\
             </script>\
           </head>\
           <body class=\"min-h-screen bg-base-200\">\
             <div id=\"app\"{hydrate}>{body}</div>\
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
        preload = preload,
        hydrate = hydrate_marker(),
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
    fn the_head_preloads_the_hashed_module_and_wasm() {
        // Without these the wasm request cannot start until the module script has been
        // fetched and run: the served document is the only place that knows both hashes.
        let mut assets = test_assets();
        assets.script = "/nodus-web-abc123.js".to_owned();
        assets.wasm = "/nodus-web-abc123_bg.wasm".to_owned();
        let html = home(&assets, "me@example.org", false, &[], None);
        assert!(
            html.contains("<link rel=\"modulepreload\" href=\"/nodus-web-abc123.js\">"),
            "{html}"
        );
        assert!(
            html.contains(
                "<link rel=\"preload\" as=\"fetch\" crossorigin \
                 href=\"/nodus-web-abc123_bg.wasm\">"
            ),
            "{html}"
        );
        // Both must be in the head, before the module that imports them.
        let head_end = html.find("</head>").expect("head closes");
        let module_start = html
            .find("<script type=\"module\">")
            .expect("module script");
        let preload_start = html.find("rel=\"modulepreload\"").expect("preload");
        assert!(preload_start < module_start && module_start < head_end);
    }

    #[test]
    fn the_island_carries_the_capability_list_under_capabilities() {
        // The browser gates its surfaces on the same list the server painted with, so the
        // first render needs no probe. The key is the contract: `capabilities`, holding the
        // serialized answer with `features` as the published names.
        let host = "capability-island.test";
        crate::capabilities::remember_for_test(
            host,
            nodus_ui::capabilities::Capabilities::from_parts(
                "0.1.0",
                &[
                    "admin".to_owned(),
                    "moderation".to_owned(),
                    "drive".to_owned(),
                    "feed_accounts".to_owned(),
                ],
            ),
        );
        let html = home(&test_assets(), &format!("me@{host}"), false, &[], None);
        let (_, after_open) = html
            .split_once("<script id=\"nodus-page\" type=\"application/json\">")
            .expect("island script present");
        let (island_json, _) = after_open.split_once("</script>").expect("closing script");
        let island: serde_json::Value =
            serde_json::from_str(island_json).expect("island is valid JSON");
        let carried = island["capabilities"]["features"]
            .as_array()
            .expect("the island names the capabilities under `capabilities.features`");
        let names: Vec<&str> = carried.iter().filter_map(|name| name.as_str()).collect();
        for name in ["admin", "moderation", "drive", "feed_accounts"] {
            assert!(
                names.contains(&name),
                "island is missing {name}: {island_json}"
            );
        }
        // The client parses this exact value into its typed answer; if that stops working
        // the island is decorative and the browser falls back to a probe.
        let adopted: nodus_ui::capabilities::Capabilities =
            serde_json::from_value(island["capabilities"].clone())
                .expect("the client can adopt the island's answer");
        assert!(adopted.supports(nodus_ui::capabilities::Feature::Admin));
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
                avatar_meta: None,
                header: String::new(),
                header_meta: None,
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

    fn status_by(acct: &str, display_name: &str, content: &str) -> Status {
        Status {
            id: "1".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            content: content.to_owned(),
            visibility: Visibility::Public,
            account: Account {
                id: "9".to_owned(),
                acct: acct.to_owned(),
                display_name: display_name.to_owned(),
                note: String::new(),
                avatar: "https://cdn.test/ada.png".to_owned(),
                avatar_meta: None,
                header: String::new(),
                header_meta: None,
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

    #[test]
    fn signed_in_home_paints_the_readers_timeline_and_card() {
        // One paint, the reader's own: the status the server read through the session's
        // server is in the body, and the chrome is drawn from the same session rather than
        // left blank until wasm fills it.
        let reader = Reader {
            host: "misskey.test".to_owned(),
            account_id: "9".to_owned(),
            acct: "ada@misskey.test".to_owned(),
            display_name: "Ada".to_owned(),
            avatar: "https://cdn.test/ada.png".to_owned(),
        };
        let statuses = [status_by(
            "ada@misskey.test",
            "Ada",
            "<p>first from my own server</p>",
        )];
        let html = home_with_style(&test_assets(), &reader, false, &statuses, None, None);

        assert!(html.contains("first from my own server"), "{html}");

        // Everything before `<main` is chrome: the rail card must already be there, drawn
        // from the session, rather than left empty for wasm to fill in.
        let (rail, _) = html.split_once("<main").expect("main");
        assert!(
            rail.contains("Ada"),
            "display name missing from rail: {rail}"
        );
        assert!(
            rail.contains("@ada@misskey.test"),
            "session handle missing from rail: {rail}"
        );
        assert!(
            rail.contains("https://cdn.test/ada.png"),
            "the session's avatar is not in the rail: {rail}"
        );
        assert!(
            html.contains("/accounts/misskey.test/ada"),
            "the rail card must link to the account: {html}"
        );
        assert!(
            html.contains("@ada@misskey.test"),
            "the rail handle is printed once, qualified: {html}"
        );
        assert!(
            !html.contains("@ada@misskey.test@"),
            "the painted handle doubles the host: {html}"
        );
        assert!(
            html.contains("https://cdn.test/ada.png"),
            "the session's avatar is not painted: {html}"
        );
        // The island carries the reader, so a screen that asks the runtime who is signed
        // in gets the same answer the browser's store would give.
        assert!(html.contains("\"acct\":\"ada@misskey.test\""), "{html}");
    }

    fn has_staff_links(html: &str) -> bool {
        html.contains("href=\"/moderation\"") || html.contains("href=\"/admin\"")
    }

    #[test]
    fn a_painted_page_references_nothing_but_this_origin() {
        // The server paints with the bundle this host serves; a document that named another
        // origin would either fail to boot or boot against someone else's app. The one
        // absolute URL the page may carry is the favicon's inline `data:` SVG, and status
        // bodies are absent here so their (legitimately remote) images cannot mask a leak.
        let html = home(&test_assets(), "me@example.org", false, &[], None);
        let (head, rest) = html.split_once("<body").expect("body");
        for (part, name) in [(head, "head"), (rest, "body")] {
            for attribute in ["src=", "href="] {
                assert!(
                    !part.contains(&format!("{attribute}\"http")),
                    "{name} references another origin: {part}"
                );
                assert!(
                    !part.contains(&format!("{attribute}\"//")),
                    "{name} references a protocol-relative origin: {part}"
                );
            }
        }
        assert!(html.contains("from '/nodus-web.js'"), "{html}");
        assert!(
            html.contains("module_or_path: '/nodus-web_bg.wasm'"),
            "{html}"
        );
    }

    #[test]
    fn the_painted_staff_heading_follows_the_doors_it_names() {
        // The server paint had the same defect the browser did: the staff card was drawn
        // whenever the session held a role, even when the instance served neither staff
        // surface, so the document carried a "Staff" heading with no doors under it. The
        // heading now comes from the same two doors that pick the rows.
        let assets = test_assets();

        let absent_host = "no-staff-surfaces.test";
        crate::capabilities::remember_for_test(
            absent_host,
            nodus_ui::capabilities::Capabilities::from_parts("0.1.0", &["drive".to_owned()]),
        );
        let absent = home(&assets, &format!("mod@{absent_host}"), true, &[], None);
        assert!(!absent.contains("Staff"), "{absent}");
        assert!(!absent.contains("data-nav-rail-doors"), "{absent}");
        assert!(!has_staff_links(&absent), "{absent}");

        let both_host = "staff-surfaces.test";
        crate::capabilities::remember_for_test(
            both_host,
            nodus_ui::capabilities::Capabilities::from_parts(
                "0.1.0",
                &["admin".to_owned(), "moderation".to_owned()],
            ),
        );
        let both = home(&assets, &format!("mod@{both_host}"), true, &[], None);
        assert!(both.contains("Staff"), "{both}");
        assert!(both.contains("data-nav-rail-doors"), "{both}");
        assert!(both.contains("href=\"/moderation\""), "{both}");
        assert!(both.contains("href=\"/admin\""), "{both}");
    }

    #[test]
    fn an_eligible_instance_draws_no_door_for_an_account_without_a_role() {
        // The other half of the same rule, on the paint that cannot ask a browser: the
        // instance serving both areas is one fact, and this account's role is another. The
        // flag the caller resolved is negative evidence — no staff role on the account —
        // so the door is not drawn and the heading does not survive it. Fail closed: a door
        // on a guess is a room that answers with a refusal.
        let assets = test_assets();
        let host = "role-absent.test";
        crate::capabilities::remember_for_test(
            host,
            nodus_ui::capabilities::Capabilities::from_parts(
                "0.1.0",
                &["admin".to_owned(), "moderation".to_owned()],
            ),
        );
        let html = home(&assets, &format!("reader@{host}"), false, &[], None);
        assert!(!html.contains("Staff"), "{html}");
        assert!(!html.contains("data-nav-rail-doors"), "{html}");
        assert!(!has_staff_links(&html), "{html}");
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
