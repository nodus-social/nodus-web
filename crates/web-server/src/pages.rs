//! What each address asks the instance for, and how that becomes a page.
//!
//! One function per family of URLs — a list page, an entity (account or thread), the saved
//! shelves, search — so `main.rs` stays a router: it decides *which* reader runs, this module
//! decides what the answer looks like.

use leptos::prelude::Owner;
use std::sync::Arc;

use nodus_state::page::{PageEntry, PageItems, PageKind};
use nodus_ui::session_style::SessionStyle;
use nodus_web::shared::router::Route;
use nodus_web::shared::screens::{Screen, ScreenKind};

use crate::mastodon::Client;
use crate::{PAGE, document, runtime::StaticRuntime};

/// The server's card painter: it has a status and needs the component's HTML.
fn card() -> nodus_ui::components::card_renderer::CardRenderer {
    std::sync::Arc::new(|status: nodus_client_core::model::Status| card_view_for(&status))
}

fn card_view_for(status: &nodus_client_core::model::Status) -> leptos::prelude::AnyView {
    use leptos::prelude::IntoAny;
    use leptos::tachys::html::element::InnerHtmlAttribute as _;

    leptos::html::div()
        .inner_html(crate::rows::status_card(status))
        .into_any()
}

/// Renders a component tree to the string the document embeds.
///
/// The closure is not a convenience: a component's body runs when it is *called*, and a
/// `For` inside that body asks `Owner::current()` while it renders. Building the props and
/// the component inside `Owner::new().with(..)` is what keeps that answer non-empty —
/// `painted(Page(props))` would build the page before the owner exists and panic with
/// "no reactive owner" the first time a list had a row in it.
fn painted<V: leptos::IntoView>(view: impl FnOnce() -> V) -> String {
    Owner::new().with(|| view().to_html())
}

#[expect(
    clippy::too_many_arguments,
    reason = "one route renderer needs the request facts, fetched page, and session style"
)]
fn render_route(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    title: &str,
    route: Route,
    kind: PageKind,
    entry: PageEntry,
    island: serde_json::Value,
    style: Option<&SessionStyle>,
) -> String {
    let runtime = StaticRuntime::pages([(kind, entry)]);
    let inner = painted(|| {
        nodus_route::route::RouteView(nodus_route::route::RouteViewProps {
            runtime,
            route,
            card: card(),
            header: None,
            banner: None,
        })
    });
    document::page_of_with_style(
        assets,
        acct,
        staff,
        path,
        title,
        &inner,
        Some(island.to_string()),
        style,
    )
}

#[allow(dead_code)]
pub(crate) fn render_thread(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    status: &nodus_client_core::model::Status,
    context: &nodus_client_core::model::Context,
) -> String {
    render_thread_with_style(assets, acct, staff, status, context, None)
}

pub(crate) fn render_thread_with_style(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    status: &nodus_client_core::model::Status,
    context: &nodus_client_core::model::Context,
    style: Option<&SessionStyle>,
) -> String {
    let mut status = status.clone();
    nodus_ui::html::sanitize_status(&mut status);
    let mut context = context.clone();
    for ancestor in &mut context.ancestors {
        nodus_ui::html::sanitize_status(ancestor);
    }
    for descendant in &mut context.descendants {
        nodus_ui::html::sanitize_status(descendant);
    }
    let path = format!("/status/{}", status.id);
    let runtime = StaticRuntime::thread(status.clone(), context.clone());
    let inner = painted(|| {
        nodus_route::route::RouteView(nodus_route::route::RouteViewProps {
            runtime,
            route: Route::Thread {
                host: String::new(),
                id: status.id.clone(),
            },
            card: card(),
            header: None,
            banner: None,
        })
    });
    let island = serde_json::json!({
        "kind": "thread",
        "status": status,
        "context": context,
    });
    document::page_of_with_style(
        assets,
        acct,
        staff,
        &path,
        "Thread",
        &inner,
        Some(island.to_string()),
        style,
    )
}

#[allow(dead_code)]
pub(crate) async fn read_page(
    client: &Client,
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    query: Option<&str>,
) -> Option<String> {
    read_page_with_style(client, assets, acct, staff, path, query, None).await
}

pub(crate) async fn read_page_with_style(
    client: &Client,
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    query: Option<&str>,
    style: Option<&SessionStyle>,
) -> Option<String> {
    let Some(screen) = Screen::of_path(path) else {
        return read_entity(client, assets, acct, staff, path, style).await;
    };

    let page = match screen.kind {
        ScreenKind::More => None,
        ScreenKind::Notifications => client
            .notifications(PAGE)
            .await
            .map(|(items, cursor)| {
                render_notifications_with_style(
                    assets,
                    acct,
                    staff,
                    path,
                    screen.title,
                    items,
                    cursor,
                    style,
                )
            })
            .ok(),
        ScreenKind::Messages => client
            .conversations(PAGE)
            .await
            .map(|(items, cursor)| {
                render_conversations_with_style(
                    assets,
                    acct,
                    staff,
                    path,
                    screen.title,
                    items,
                    cursor,
                    style,
                )
            })
            .ok(),
        ScreenKind::Saved { bookmarks } => {
            read_saved(client, assets, acct, staff, path, bookmarks, style).await
        }
        ScreenKind::Lists => client
            .lists()
            .await
            .map(|items| {
                let runtime = StaticRuntime::lists(items.clone());
                let inner = painted(|| {
                    nodus_route::route::RouteView(nodus_route::route::RouteViewProps {
                        runtime,
                        route: Route::Lists,
                        card: card(),
                        header: None,
                        banner: None,
                    })
                });
                document::page_of_with_style(
                    assets,
                    acct,
                    staff,
                    path,
                    screen.title,
                    &inner,
                    Some(
                        serde_json::json!({ "kind": "lists", "items": items, "cursor": null })
                            .to_string(),
                    ),
                    style,
                )
            })
            .ok(),
        ScreenKind::Scheduled => client
            .scheduled()
            .await
            .map(|items| {
                render_route(
                    assets,
                    acct,
                    staff,
                    path,
                    screen.title,
                    Route::Scheduled,
                    PageKind::Scheduled,
                    PageEntry {
                        items: PageItems::Scheduled(items.clone()),
                        cursor: None,
                        fetched_at: None,
                    },
                    serde_json::json!({ "kind": "scheduled", "items": items, "cursor": null }),
                    style,
                )
            })
            .ok(),
        ScreenKind::Directory => client
            .directory(true)
            .await
            .map(|mut items| {
                for account in &mut items {
                    nodus_ui::html::sanitize_account(account);
                }
                render_route(
                    assets,
                    acct,
                    staff,
                    path,
                    screen.title,
                    Route::Directory,
                    PageKind::Directory(true),
                    PageEntry {
                        items: PageItems::Accounts(items.clone()),
                        cursor: None,
                        fetched_at: None,
                    },
                    serde_json::json!({ "kind": "directory", "items": items, "cursor": null }),
                    style,
                )
            })
            .ok(),
        ScreenKind::Search => read_search(client, assets, acct, staff, path, query, style).await,
        ScreenKind::Settings => Some(render_unclaimed_with_style(
            assets, acct, staff, path, style,
        )),
        _ => None,
    };
    if page.is_some() {
        return page;
    }
    read_entity(client, assets, acct, staff, path, style).await
}

/// Before the routes with a name in them: one account, or one thread.
/// A profile page, painted by the shared renderer.
#[cfg(test)]
#[must_use]
pub fn render_account(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    account: &nodus_client_core::model::Account,
    statuses: &[nodus_client_core::model::Status],
    cursor: Option<&str>,
) -> String {
    render_account_with_style(assets, acct, staff, account, statuses, cursor, None)
}

#[must_use]
pub fn render_account_with_style(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    account: &nodus_client_core::model::Account,
    statuses: &[nodus_client_core::model::Status],
    cursor: Option<&str>,
    style: Option<&SessionStyle>,
) -> String {
    let mut account = account.clone();
    nodus_ui::html::sanitize_account(&mut account);
    let statuses: Vec<_> = statuses
        .iter()
        .cloned()
        .map(|mut status| {
            nodus_ui::html::sanitize_status(&mut status);
            status
        })
        .collect();
    let path = format!("/accounts/{}", account.id);
    let title = format!("{} · {acct}", account.acct);
    let runtime = StaticRuntime::profile(
        account.clone(),
        None,
        statuses.clone(),
        cursor.map(std::borrow::ToOwned::to_owned),
    );
    let painted = painted(|| {
        nodus_route::route::RouteView(nodus_route::route::RouteViewProps {
            runtime,
            route: Route::Account(account.id.clone()),
            card: card(),
            header: None,
            banner: None,
        })
    });
    let island = serde_json::json!({
        "kind": "account",
        "account": account,
        "statuses": statuses,
        "cursor": cursor,
    });
    document::page_of_with_style(
        assets,
        acct,
        staff,
        &path,
        &title,
        &painted,
        Some(island.to_string()),
        style,
    )
}

#[cfg(test)]
fn render_notifications(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    title: &str,
    items: Vec<nodus_client_core::model::Notification>,
    cursor: Option<String>,
) -> String {
    render_notifications_with_style(assets, acct, staff, path, title, items, cursor, None)
}

#[expect(
    clippy::too_many_arguments,
    reason = "matches render_route signature with SSR style context"
)]
fn render_notifications_with_style(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    title: &str,
    mut items: Vec<nodus_client_core::model::Notification>,
    cursor: Option<String>,
    style: Option<&SessionStyle>,
) -> String {
    for item in &mut items {
        nodus_ui::html::sanitize_notification(item);
    }
    let island = serde_json::json!({
        "kind": "notifications",
        "items": items,
        "cursor": cursor,
    });
    render_route(
        assets,
        acct,
        staff,
        path,
        title,
        Route::Notifications,
        PageKind::Notifications,
        PageEntry {
            items: PageItems::Notifications(items),
            cursor,
            fetched_at: None,
        },
        island,
        style,
    )
}

#[cfg(test)]
fn render_conversations(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    title: &str,
    items: Vec<nodus_client_core::model::Conversation>,
    cursor: Option<String>,
) -> String {
    render_conversations_with_style(assets, acct, staff, path, title, items, cursor, None)
}

#[expect(
    clippy::too_many_arguments,
    reason = "matches render_route signature with SSR style context"
)]
fn render_conversations_with_style(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    title: &str,
    mut items: Vec<nodus_client_core::model::Conversation>,
    cursor: Option<String>,
    style: Option<&SessionStyle>,
) -> String {
    for item in &mut items {
        nodus_ui::html::sanitize_conversation(item);
    }
    let island = serde_json::json!({
        "kind": "conversations",
        "items": items,
        "cursor": cursor,
    });
    render_route(
        assets,
        acct,
        staff,
        path,
        title,
        Route::Messages,
        PageKind::Conversations,
        PageEntry {
            items: PageItems::Conversations(items),
            cursor,
            fetched_at: None,
        },
        island,
        style,
    )
}

pub(crate) async fn read_entity(
    client: &Client,
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    style: Option<&SessionStyle>,
) -> Option<String> {
    if let Some(rest) = path.strip_prefix("/accounts/") {
        if rest.is_empty() {
            return None;
        }
        // `/accounts/{host}/{name|id}` is the permanent address; `/accounts/{id}` is the
        // older one. The host has to be the reader's own instance, because `client` speaks
        // for the session: another instance's profile is a public read, not this page.
        let (host, segment) = rest
            .split_once('/')
            .map_or((None, rest), |(host, segment)| (Some(host), segment));
        let instance_host = acct.split_once('@').map_or("", |(_, host)| host);
        if let Some(host) = host
            && !host.eq_ignore_ascii_case(instance_host)
        {
            return None;
        }
        let id = if segment.chars().all(|c| c.is_ascii_digit())
            || (segment.len() == 26 && segment.chars().all(|c| c.is_ascii_alphanumeric()))
        {
            segment.to_owned()
        } else {
            let wanted = segment.to_owned();
            client
                .search(&format!("@{segment}@{instance_host}"))
                .await
                .ok()?
                .accounts
                .into_iter()
                .find(|account| {
                    let (name, host) = match account.acct.split_once('@') {
                        Some((name, host)) => (name, host),
                        None => (account.acct.as_str(), instance_host),
                    };
                    name.eq_ignore_ascii_case(&wanted) && host.eq_ignore_ascii_case(instance_host)
                })?
                .id
        };
        let account = client.account(&id).await.ok()?;
        let (statuses, cursor) = client.account_statuses(&id, PAGE).await.ok()?;
        return Some(render_account_with_style(
            assets,
            acct,
            staff,
            &account,
            &statuses,
            cursor.as_deref(),
            style,
        ));
    }
    let id = path.strip_prefix("/status/")?;
    if id.is_empty() || id.contains('/') {
        return None;
    }
    let status = client.status(id).await.ok()?;
    let context = client.context(id).await.ok()?;
    Some(document::thread_page_with_style(
        assets, acct, staff, &status, &context, style,
    ))
}

/// Favourites or bookmarks: one screen, two shelves, the same shape.
pub(crate) async fn read_saved(
    client: &Client,
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    bookmarks: bool,
    style: Option<&SessionStyle>,
) -> Option<String> {
    use nodus_pages::pages::saved::shelf::Shelf;

    let (mut statuses, cursor) = client.saved(bookmarks, PAGE).await.ok()?;
    for status in &mut statuses {
        nodus_ui::html::sanitize_status(status);
    }
    let shelf = if bookmarks {
        Shelf::Bookmarks
    } else {
        Shelf::Favourites
    };
    let kind = if bookmarks {
        PageKind::Bookmarks
    } else {
        PageKind::Favourites
    };
    Some(render_route(
        assets,
        acct,
        staff,
        path,
        shelf.label(),
        Route::Saved(shelf.slug().to_owned()),
        kind,
        PageEntry {
            items: PageItems::Statuses(statuses.clone()),
            cursor: cursor.clone(),
            fetched_at: None,
        },
        serde_json::json!({
            "kind": if bookmarks { "bookmarks" } else { "favourites" },
            "items": statuses,
            "cursor": cursor,
        }),
        style,
    ))
}

/// The search page, the one route whose argument comes from the query string.
pub(crate) async fn read_search(
    client: &Client,
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    query: Option<&str>,
    style: Option<&SessionStyle>,
) -> Option<String> {
    let query = query
        .and_then(|query| {
            query.split('&').find_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                (name == "q").then(|| percent_decode(value))
            })
        })
        .unwrap_or_default();
    if query.trim().is_empty() {
        return None;
    }
    let mut results = client.search(&query).await.ok()?;
    nodus_ui::html::sanitize_search_results(&mut results);
    let runtime = StaticRuntime::pages_with_query(
        [(
            PageKind::Search(query.clone()),
            PageEntry {
                items: PageItems::Search(Box::new(results.clone())),
                cursor: None,
                fetched_at: None,
            },
        )],
        [("q".to_owned(), query.clone())],
    );
    let inner = painted(|| {
        nodus_route::route::RouteView(nodus_route::route::RouteViewProps {
            runtime,
            route: Route::Search,
            card: card(),
            header: None,
            banner: None,
        })
    });
    let island = serde_json::json!({
        "kind": "search",
        "query": query,
        "items": results,
        "cursor": null,
    })
    .to_string();
    Some(document::page_of_with_style(
        assets,
        acct,
        staff,
        path,
        "Search",
        &inner,
        Some(island),
        style,
    ))
}

/// Percent-decoding for the one query parameter this host reads. `+` is a space because a
/// search box is what sends it.
pub(crate) fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                    decoded.push(byte);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// An anonymous shared link: the post and its thread, read without a session.
pub fn render_public_thread(
    assets: &document::Assets,
    host: &str,
    id: &str,
    mut status: nodus_client_core::model::Status,
    mut context: nodus_client_core::model::Context,
) -> String {
    nodus_ui::html::sanitize_status(&mut status);
    for ancestor in &mut context.ancestors {
        nodus_ui::html::sanitize_status(ancestor);
    }
    for descendant in &mut context.descendants {
        nodus_ui::html::sanitize_status(descendant);
    }
    let path = format!("/status/{host}/{id}");
    let inner = painted(|| {
        nodus_route::route::RouteView(nodus_route::route::RouteViewProps {
            runtime: StaticRuntime::thread(status, context),
            route: Route::Thread {
                host: host.to_owned(),
                id: id.to_owned(),
            },
            card: public_card(host),
            header: None,
            banner: None,
        })
    });
    document::page_of_public(assets, &path, "Post", &inner, false)
}

fn public_card(host: &str) -> nodus_ui::components::card_renderer::CardRenderer {
    let host = host.to_owned();
    Arc::new(move |status| {
        use leptos::prelude::IntoAny;
        use leptos::tachys::html::element::InnerHtmlAttribute as _;

        leptos::html::div()
            .inner_html(crate::rows::status_card_public(&status, &host))
            .into_any()
    })
}

/// An account reached from a public post, with its public posts.
pub fn render_public_account(
    assets: &document::Assets,
    host: &str,
    id: &str,
    mut account: nodus_client_core::model::Account,
    mut statuses: Vec<nodus_client_core::model::Status>,
) -> String {
    nodus_ui::html::sanitize_account(&mut account);
    for status in &mut statuses {
        nodus_ui::html::sanitize_status(status);
    }
    let path = format!("/accounts/{host}/{id}");
    let inner = painted(|| {
        nodus_route::route::RouteView(nodus_route::route::RouteViewProps {
            runtime: StaticRuntime::profile(account.clone(), None, statuses, None),
            route: Route::Account(format!("{host}/{id}")),
            card: public_card(host),
            header: None,
            banner: None,
        })
    });
    let noindex = account.noindex == Some(true);
    document::page_of_public(assets, &path, "Account", &inner, noindex)
}

/// The last route seam: any path the data readers did not claim is drawn by the same
/// `RouteView` the browser will mount, with no prefetched answers.
#[allow(dead_code)]
pub fn render_unclaimed(assets: &document::Assets, acct: &str, staff: bool, path: &str) -> String {
    render_unclaimed_with_style(assets, acct, staff, path, None)
}

/// Same as `render_unclaimed`, framing the screen with the active account's `SessionStyle`.
pub fn render_unclaimed_with_style(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    path: &str,
    style: Option<&SessionStyle>,
) -> String {
    let route = Route::from_path(path);
    let title = Screen::of_path(path)
        .map(|screen| screen.title.to_owned())
        .unwrap_or_else(|| "Nodus".to_owned());
    let resolved_style = style.cloned().unwrap_or_default();
    let inner = painted(|| {
        nodus_route::route::RouteView(nodus_route::route::RouteViewProps {
            runtime: StaticRuntime::signed_in_with_style(acct, resolved_style),
            route,
            card: card(),
            header: None,
            banner: None,
        })
    });
    document::page_of_with_style(
        assets,
        acct,
        staff,
        path,
        title.as_str(),
        &inner,
        None,
        style,
    )
}

#[cfg(test)]
mod painting {
    use leptos::prelude::{ElementChild as _, Get as _};

    use super::painted;

    /// A page built *outside* the owner panics the moment a list has a row in it:
    /// `panicked at leptos/src/for_loop.rs:138: no reactive owner`. Building it inside
    /// `painted`'s closure is the fix, and this test is the shape of that fix.
    #[test]
    fn a_for_with_a_row_is_built_inside_the_owner() {
        let html = painted(|| {
            let items = leptos::prelude::Signal::derive(|| vec![1_u8, 2]);
            leptos::prelude::IntoView::into_view(
                leptos::html::ul().child(leptos::prelude::For(
                    leptos::prelude::ForProps::builder()
                        .each(move || items.get())
                        .key(|value: &u8| *value)
                        .children(|value: u8| leptos::html::li().child(value.to_string()))
                        .build(),
                )),
            )
        });

        assert!(html.contains("<li>1</li>"), "{html}");
    }

    #[test]
    fn render_account_and_public_pages_sanitize_xss_payloads() {
        let assets = crate::document::Assets {
            script: "/nodus-web.js".to_owned(),
            wasm: "/nodus-web_bg.wasm".to_owned(),
            stylesheet: None,
            stylesheet_css: None,
        };
        let account = nodus_client_core::model::Account {
            id: "1".to_owned(),
            acct: "alice@example.org".to_owned(),
            display_name: "Alice".to_owned(),
            note: "<p>bio</p><img src=x onerror=\"alert(1)\"><script>steal()</script>".to_owned(),
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
        };
        let status = nodus_client_core::model::Status {
            id: "10".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            content: "<p>post</p><img src=x onerror=\"steal()\"><a href=\"https://example.com/:wave:\">go</a>".to_owned(),
            visibility: nodus_client_core::model::Visibility::Public,
            account: account.clone(),
            favourites_count: 0,
            reblogs_count: 0,
            replies_count: 0,
            favourited: false,
            reblogged: false,
            bookmarked: false,
            reblog: None,
            in_reply_to_id: None,
            emojis: vec![nodus_client_core::model::CustomEmoji {
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

        let profile_html = super::render_account(
            &assets,
            "me@example.org",
            false,
            &account,
            std::slice::from_ref(&status),
            None,
        );
        assert!(!profile_html.contains("onerror"), "{profile_html}");
        assert!(!profile_html.contains("steal()"), "{profile_html}");
        assert!(
            profile_html.contains("href=\"https://example.com/:wave:\""),
            "{profile_html}"
        );

        let public_thread_html = super::render_public_thread(
            &assets,
            "example.org",
            "10",
            status,
            nodus_client_core::model::Context::default(),
        );
        assert!(
            !public_thread_html.contains("onerror"),
            "{public_thread_html}"
        );
        assert!(
            !public_thread_html.contains("steal()"),
            "{public_thread_html}"
        );
    }

    #[test]
    fn render_thread_and_render_account_emit_nodus_page_islands() {
        let assets = crate::document::Assets {
            script: "/nodus-web.js".to_owned(),
            wasm: "/nodus-web_bg.wasm".to_owned(),
            stylesheet: None,
            stylesheet_css: None,
        };
        let account = nodus_client_core::model::Account {
            id: "1".to_owned(),
            acct: "alice@example.org".to_owned(),
            display_name: "Alice".to_owned(),
            note: "<p>bio</p><img src=x onerror=\"alert(1)\">".to_owned(),
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
        };
        let status = nodus_client_core::model::Status {
            id: "10".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            content: "<p>hello</p><script>evil()</script>".to_owned(),
            visibility: nodus_client_core::model::Visibility::Public,
            account: account.clone(),
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
        };
        let context = nodus_client_core::model::Context::default();

        let thread_html = super::render_thread(&assets, "me@example.org", false, &status, &context);
        let (_, after_thread) = thread_html
            .split_once("<script id=\"nodus-page\" type=\"application/json\">")
            .expect("thread page must include #nodus-page island");
        let (thread_json, _) = after_thread
            .split_once("</script>")
            .expect("closing script");
        let thread_island: serde_json::Value =
            serde_json::from_str(thread_json).expect("valid thread island json");
        assert_eq!(thread_island["kind"], "thread");
        assert_eq!(thread_island["status"]["id"], "10");
        assert!(!thread_json.contains("<script"), "{thread_json}");
        assert!(!thread_json.contains("evil()"), "{thread_json}");

        let account_html = super::render_account(
            &assets,
            "me@example.org",
            false,
            &account,
            std::slice::from_ref(&status),
            Some("cursor-99"),
        );
        let (_, after_account) = account_html
            .split_once("<script id=\"nodus-page\" type=\"application/json\">")
            .expect("account page must include #nodus-page island");
        let (account_json, _) = after_account
            .split_once("</script>")
            .expect("closing script");
        let account_island: serde_json::Value =
            serde_json::from_str(account_json).expect("valid account island json");
        assert_eq!(account_island["kind"], "account");
        assert_eq!(account_island["account"]["id"], "1");
        assert_eq!(account_island["cursor"], "cursor-99");
        assert_eq!(account_island["statuses"][0]["id"], "10");
        assert!(!account_json.contains("onerror"), "{account_json}");

        let notifications_html = super::render_notifications(
            &assets,
            "me@example.org",
            false,
            "/notifications",
            "Notifications",
            vec![nodus_client_core::model::Notification {
                id: "n1".to_owned(),
                kind: nodus_client_core::model::NotificationKind::Favourite,
                created_at: "2026-01-01T00:00:00Z".to_owned(),
                account: account.clone(),
                status: Some(status),
            }],
            Some("notif-next-42".to_owned()),
        );
        let (_, after_notif) = notifications_html
            .split_once("<script id=\"nodus-page\" type=\"application/json\">")
            .expect("notifications page must include #nodus-page island");
        let (notif_json, _) = after_notif.split_once("</script>").expect("closing script");
        let notif_island: serde_json::Value =
            serde_json::from_str(notif_json).expect("valid notifications island json");
        assert_eq!(notif_island["kind"], "notifications");
        assert_eq!(notif_island["cursor"], "notif-next-42");

        let conversations_html = super::render_conversations(
            &assets,
            "me@example.org",
            false,
            "/messages",
            "Messages",
            vec![nodus_client_core::model::Conversation {
                id: "c1".to_owned(),
                unread: true,
                accounts: vec![account],
                last_status: None,
            }],
            Some("conv-next-77".to_owned()),
        );
        let (_, after_conv) = conversations_html
            .split_once("<script id=\"nodus-page\" type=\"application/json\">")
            .expect("conversations page must include #nodus-page island");
        let (conv_json, _) = after_conv.split_once("</script>").expect("closing script");
        let conv_island: serde_json::Value =
            serde_json::from_str(conv_json).expect("valid conversations island json");
        assert_eq!(conv_island["kind"], "conversations");
        assert_eq!(conv_island["cursor"], "conv-next-77");
    }
}
