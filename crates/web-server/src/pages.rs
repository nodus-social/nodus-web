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

use crate::mastodon::{Client, ProfilePage, StatusPage};
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
    crate::prepare_render();
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
        &document::Reader::of_handle(acct),
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
        &document::Reader::of_handle(acct),
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
    session_host: &str,
    acct: &str,
    staff: bool,
    path: &str,
    query: Option<&str>,
) -> Option<String> {
    read_page_with_style(client, assets, session_host, acct, staff, path, query, None).await
}

pub(crate) async fn read_page_with_style(
    client: &Client,
    assets: &document::Assets,
    session_host: &str,
    acct: &str,
    staff: bool,
    path: &str,
    query: Option<&str>,
    style: Option<&SessionStyle>,
) -> Option<String> {
    let Some(screen) = Screen::of_path(path) else {
        return read_entity(client, assets, session_host, acct, staff, path, style).await;
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
                    &document::Reader::of_handle(acct),
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
    read_entity(client, assets, session_host, acct, staff, path, style).await
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
    render_account_with_style(assets, acct, staff, account, None, statuses, cursor, None)
}

#[must_use]
pub fn render_account_with_style(
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    account: &nodus_client_core::model::Account,
    relationship: Option<&nodus_client_core::model::Relationship>,
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
        relationship.cloned(),
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
        "relationship": relationship,
        "statuses": statuses,
        "cursor": cursor,
    });
    document::page_of_with_style(
        assets,
        &document::Reader::of_handle(acct),
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

/// The reads the entity routes make.
///
/// `Client` is the one production implementation; tests supply a fake that counts calls,
/// which is how "one request per page" is asserted instead of described.
#[allow(
    async_fn_in_trait,
    reason = "the trait is private to this module and used only as a generic bound"
)]
pub(crate) trait EntityReads {
    async fn search(&self, query: &str) -> Result<nodus_client_core::model::SearchResults, String>;
    async fn account(&self, id: &str) -> Result<nodus_client_core::model::Account, String>;
    async fn account_statuses(
        &self,
        id: &str,
        limit: u16,
    ) -> Result<(Vec<nodus_client_core::model::Status>, Option<String>), String>;
    async fn status(&self, id: &str) -> Result<nodus_client_core::model::Status, String>;
    async fn context(&self, id: &str) -> Result<nodus_client_core::model::Context, String>;
    async fn profile_page(&self, id: &str, limit: u16) -> Result<ProfilePage, String>;
    async fn profile_page_by_handle(&self, handle: &str, limit: u16)
    -> Result<ProfilePage, String>;
    async fn status_page(&self, id: &str) -> Result<StatusPage, String>;
}

impl EntityReads for Client {
    async fn search(&self, query: &str) -> Result<nodus_client_core::model::SearchResults, String> {
        Client::search(self, query).await
    }

    async fn account(&self, id: &str) -> Result<nodus_client_core::model::Account, String> {
        Client::account(self, id).await
    }

    async fn account_statuses(
        &self,
        id: &str,
        limit: u16,
    ) -> Result<(Vec<nodus_client_core::model::Status>, Option<String>), String> {
        Client::account_statuses(self, id, limit).await
    }

    async fn status(&self, id: &str) -> Result<nodus_client_core::model::Status, String> {
        Client::status(self, id).await
    }

    async fn context(&self, id: &str) -> Result<nodus_client_core::model::Context, String> {
        Client::context(self, id).await
    }

    async fn profile_page(&self, id: &str, limit: u16) -> Result<ProfilePage, String> {
        Client::profile_page(self, id, limit).await
    }

    async fn profile_page_by_handle(
        &self,
        handle: &str,
        limit: u16,
    ) -> Result<ProfilePage, String> {
        Client::profile_page_by_handle(self, handle, limit).await
    }

    async fn status_page(&self, id: &str) -> Result<StatusPage, String> {
        Client::status_page(self, id).await
    }
}

pub(crate) async fn read_entity(
    client: &Client,
    assets: &document::Assets,
    session_host: &str,
    acct: &str,
    staff: bool,
    path: &str,
    style: Option<&SessionStyle>,
) -> Option<String> {
    read_entity_with(client, assets, session_host, acct, staff, path, style).await
}

/// One account or one thread, read once when the instance hydrates pages itself.
///
/// The client is generic over [`EntityReads`] so a test can inject a fake transport and
/// count the requests a render makes. `pages` is asked once here, from the answer
/// `capabilities::resolve` already cached for this host.
async fn read_entity_with<R: EntityReads>(
    reads: &R,
    assets: &document::Assets,
    session_host: &str,
    acct: &str,
    staff: bool,
    path: &str,
    style: Option<&SessionStyle>,
) -> Option<String> {
    // The host the document is painted for is the host the session is signed into: a
    // local acct has no `@host`, and falling back to `""` would gate every local reader
    // out of the pages endpoints. A remote handle keeps its own host, because that is the
    // instance that knows the account.
    let instance_host = acct.split_once('@').map_or(session_host, |(_, host)| host);
    let pages = crate::capabilities::serves_pages(instance_host);
    if let Some(rest) = path.strip_prefix("/accounts/") {
        if rest.is_empty() {
            return None;
        }
        // `/accounts/{host}/{name|id}` is the permanent address; `/accounts/{id}` is the
        // older one. The host has to be the reader's own instance, because `reads` speaks
        // for the session: another instance's profile is a public read, not this page.
        let (host, segment) = rest
            .split_once('/')
            .map_or((None, rest), |(host, segment)| (Some(host), segment));
        if let Some(host) = host
            && !host.eq_ignore_ascii_case(instance_host)
        {
            return None;
        }
        let by_id = crate::public_api::looks_like_id(segment);
        if pages {
            // A handle is resolved by the instance inside the page read itself, so the
            // name never costs a second round trip. The id form is unchanged.
            let page = if by_id {
                reads.profile_page(segment, PAGE).await
            } else {
                reads
                    .profile_page_by_handle(&format!("{segment}@{instance_host}"), PAGE)
                    .await
            }
            .ok()?;
            return Some(render_account_with_style(
                assets,
                acct,
                staff,
                &page.account,
                page.relationship.as_ref(),
                &page.statuses,
                page.page.next.as_deref(),
                style,
            ));
        }
        // The pre-`pages` adapter still needs the id: it has no read that takes a handle.
        let id = if by_id {
            segment.to_owned()
        } else {
            let wanted = segment.to_owned();
            reads
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
        return read_entity_pre_pages(reads, assets, acct, staff, PrePages::Account(&id), style)
            .await;
    }
    let id = path.strip_prefix("/status/")?;
    if id.is_empty() || id.contains('/') {
        return None;
    }
    if pages {
        let page = reads.status_page(id).await.ok()?;
        let context = nodus_client_core::model::Context {
            ancestors: page.ancestors,
            descendants: page.descendants,
        };
        return Some(document::thread_page_with_style(
            assets,
            acct,
            staff,
            &page.status,
            &context,
            style,
        ));
    }
    read_entity_pre_pages(reads, assets, acct, staff, PrePages::Status(id), style).await
}

/// Which entity an instance without `pages` has to assemble.
enum PrePages<'a> {
    Account(&'a str),
    Status(&'a str),
}

/// The pre-`pages` path: today's reads, kept for Mastodon, Misskey and older nodus.
///
/// This is the one adapter for instances that do not advertise `pages`; it is the only
/// place that assembles a profile or a thread out of more than one call. Do not grow it
/// into a second hydrator.
async fn read_entity_pre_pages<R: EntityReads>(
    reads: &R,
    assets: &document::Assets,
    acct: &str,
    staff: bool,
    entity: PrePages<'_>,
    style: Option<&SessionStyle>,
) -> Option<String> {
    match entity {
        PrePages::Account(id) => {
            let account = reads.account(id).await.ok()?;
            let (statuses, cursor) = reads.account_statuses(id, PAGE).await.ok()?;
            Some(render_account_with_style(
                assets,
                acct,
                staff,
                &account,
                None,
                &statuses,
                cursor.as_deref(),
                style,
            ))
        }
        PrePages::Status(id) => {
            let status = reads.status(id).await.ok()?;
            let context = reads.context(id).await.ok()?;
            Some(document::thread_page_with_style(
                assets, acct, staff, &status, &context, style,
            ))
        }
    }
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
        &document::Reader::of_handle(acct),
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
        &document::Reader::of_handle(acct),
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
            fields: Vec::new(),
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

    fn test_assets() -> crate::document::Assets {
        crate::document::Assets {
            script: "/nodus-web.js".to_owned(),
            wasm: "/nodus-web_bg.wasm".to_owned(),
            stylesheet: None,
            stylesheet_css: None,
        }
    }

    fn test_account(id: &str, name: &str, bio: &str) -> nodus_client_core::model::Account {
        nodus_client_core::model::Account {
            id: id.to_owned(),
            acct: format!("{name}@pages.example"),
            display_name: name.to_owned(),
            note: format!("<p>{bio}</p>"),
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
        }
    }

    fn test_status(
        id: &str,
        body: &str,
        account: &nodus_client_core::model::Account,
    ) -> nodus_client_core::model::Status {
        nodus_client_core::model::Status {
            id: id.to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            content: format!("<p>{body}</p>"),
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
        }
    }

    /// A transport with no network: it counts the calls the render makes.
    ///
    /// The counters are split by read shape because the point of the handle route is not
    /// only "fewer calls" but "this call and not that one": a profile must never pay a
    /// lookup when the page read can resolve the handle itself.
    struct CountingReads {
        account: nodus_client_core::model::Account,
        statuses: Vec<nodus_client_core::model::Status>,
        relationship: Option<nodus_client_core::model::Relationship>,
        context: nodus_client_core::model::Context,
        calls: std::cell::Cell<usize>,
        lookups: std::cell::Cell<usize>,
        page_reads: std::cell::Cell<usize>,
        handle_reads: std::cell::Cell<usize>,
        handles: std::cell::RefCell<Vec<String>>,
        handle_error: Option<String>,
    }

    impl CountingReads {
        fn new(
            account: nodus_client_core::model::Account,
            statuses: Vec<nodus_client_core::model::Status>,
        ) -> Self {
            Self {
                account,
                statuses,
                relationship: None,
                context: nodus_client_core::model::Context::default(),
                calls: std::cell::Cell::new(0),
                lookups: std::cell::Cell::new(0),
                page_reads: std::cell::Cell::new(0),
                handle_reads: std::cell::Cell::new(0),
                handles: std::cell::RefCell::new(Vec::new()),
                handle_error: None,
            }
        }

        fn count(&self) -> usize {
            self.calls.get()
        }

        fn hit(&self) {
            self.calls.set(self.calls.get() + 1);
        }

        fn page(&self) -> super::ProfilePage {
            super::ProfilePage {
                account: self.account.clone(),
                statuses: self.statuses.clone(),
                relationship: self.relationship.clone(),
                page: crate::mastodon::PageCursors {
                    next: Some("next-page".to_owned()),
                    prev: None,
                },
            }
        }
    }

    impl super::EntityReads for CountingReads {
        async fn search(
            &self,
            _query: &str,
        ) -> Result<nodus_client_core::model::SearchResults, String> {
            self.hit();
            self.lookups.set(self.lookups.get() + 1);
            Ok(nodus_client_core::model::SearchResults {
                accounts: vec![self.account.clone()],
                ..nodus_client_core::model::SearchResults::default()
            })
        }

        async fn account(&self, _id: &str) -> Result<nodus_client_core::model::Account, String> {
            self.hit();
            Ok(self.account.clone())
        }

        async fn account_statuses(
            &self,
            _id: &str,
            _limit: u16,
        ) -> Result<(Vec<nodus_client_core::model::Status>, Option<String>), String> {
            self.hit();
            Ok((self.statuses.clone(), None))
        }

        async fn status(&self, _id: &str) -> Result<nodus_client_core::model::Status, String> {
            self.hit();
            self.statuses
                .first()
                .cloned()
                .ok_or_else(|| "none".to_owned())
        }

        async fn context(&self, _id: &str) -> Result<nodus_client_core::model::Context, String> {
            self.hit();
            Ok(self.context.clone())
        }

        async fn profile_page(&self, _id: &str, _limit: u16) -> Result<super::ProfilePage, String> {
            self.hit();
            self.page_reads.set(self.page_reads.get() + 1);
            Ok(self.page())
        }

        async fn profile_page_by_handle(
            &self,
            handle: &str,
            _limit: u16,
        ) -> Result<super::ProfilePage, String> {
            self.hit();
            self.handle_reads.set(self.handle_reads.get() + 1);
            self.handles.borrow_mut().push(handle.to_owned());
            if let Some(error) = &self.handle_error {
                return Err(error.clone());
            }
            Ok(self.page())
        }

        async fn status_page(&self, _id: &str) -> Result<super::StatusPage, String> {
            self.hit();
            let status = self
                .statuses
                .first()
                .cloned()
                .ok_or_else(|| "none".to_owned())?;
            Ok(super::StatusPage {
                status,
                ancestors: self.context.ancestors.clone(),
                descendants: self.context.descendants.clone(),
            })
        }
    }

    #[tokio::test]
    async fn a_pages_profile_is_one_call_and_paints_name_bio_and_first_post() {
        crate::capabilities::remember_for_test(
            "pages.example",
            nodus_ui::capabilities::Capabilities::from_published(&[
                nodus_ui::capabilities::Feature::Pages.as_str().to_owned(),
            ]),
        );
        let alice = test_account("1", "Alice Example", "Writer of things");
        let first = test_status("10", "First post body", &alice);
        let mut reads = CountingReads::new(alice, vec![first]);
        reads.relationship = Some(nodus_client_core::model::Relationship {
            id: nodus_client_core::model::LocalId("viewer".to_owned()),
            following: true,
            requested: false,
            followed_by: false,
            blocking: false,
            blocked_by: false,
            muting: false,
            muting_notifications: false,
            domain_blocking: false,
            endorsed: false,
        });

        let html = super::read_entity_with(
            &reads,
            &test_assets(),
            "pages.example",
            "viewer@pages.example",
            false,
            "/accounts/1",
            None,
        )
        .await
        .expect("the pages profile paints");

        assert_eq!(reads.count(), 1, "one request per profile render");
        assert_eq!(reads.page_reads.get(), 1, "the id form is one page read");
        assert_eq!(reads.handle_reads.get(), 0, "an id is not a handle");
        assert_eq!(reads.lookups.get(), 0, "an id needs no lookup");
        assert!(html.contains("Alice Example"), "{html}");
        assert!(html.contains("Writer of things"), "{html}");
        assert!(html.contains("First post body"), "{html}");
        assert!(html.contains("\"relationship\""), "{html}");
    }

    #[tokio::test]
    async fn a_pages_handle_profile_is_one_page_read_and_no_lookup() {
        crate::capabilities::remember_for_test(
            "pages.example",
            nodus_ui::capabilities::Capabilities::from_published(&[
                nodus_ui::capabilities::Feature::Pages.as_str().to_owned(),
            ]),
        );
        let alice = test_account("1", "Alice Example", "Writer of things");
        let first = test_status("10", "First post body", &alice);
        let reads = CountingReads::new(alice, vec![first]);

        let html = super::read_entity_with(
            &reads,
            &test_assets(),
            "pages.example",
            "viewer@pages.example",
            false,
            "/accounts/pages.example/alice",
            None,
        )
        .await
        .expect("the pages handle profile paints");

        assert_eq!(reads.count(), 1, "one request for a handle profile");
        assert_eq!(
            reads.lookups.get(),
            0,
            "the page read resolves the handle, so no lookup is paid"
        );
        assert_eq!(reads.page_reads.get(), 0, "a handle is not read by id");
        assert_eq!(reads.handle_reads.get(), 1, "one handle-shaped page read");
        assert_eq!(
            reads.handles.borrow().first().map(String::as_str),
            Some("alice@pages.example"),
            "the handle names the account and its own instance"
        );
        assert!(html.contains("Alice Example"), "{html}");
        assert!(html.contains("First post body"), "{html}");
    }

    #[tokio::test]
    async fn a_handle_profile_without_pages_still_pays_the_lookup() {
        let mut alice = test_account("1", "alice", "Writer of things");
        alice.acct = "alice@legacy.example".to_owned();
        let first = test_status("10", "First post body", &alice);
        let reads = CountingReads::new(alice, vec![first]);

        let html = super::read_entity_with(
            &reads,
            &test_assets(),
            "legacy.example",
            "viewer@legacy.example",
            false,
            "/accounts/legacy.example/alice",
            None,
        )
        .await
        .expect("the pre-pages handle profile paints");

        assert_eq!(reads.lookups.get(), 1, "the fallback still looks up");
        assert_eq!(
            reads.handle_reads.get(),
            0,
            "the fallback has no handle read"
        );
        assert_eq!(reads.page_reads.get(), 0, "the fallback has no page read");
        assert_eq!(reads.count(), 3, "lookup plus the account and posts reads");
        assert!(html.contains("First post body"), "{html}");
    }

    #[tokio::test]
    async fn a_missing_handle_renders_the_same_no_such_account_page_as_a_lookup_miss() {
        crate::capabilities::remember_for_test(
            "pages.example",
            nodus_ui::capabilities::Capabilities::from_published(&[
                nodus_ui::capabilities::Feature::Pages.as_str().to_owned(),
            ]),
        );
        let alice = test_account("1", "Alice Example", "Writer of things");
        let mut pages = CountingReads::new(alice.clone(), Vec::new());
        pages.handle_error = Some("the handle @nobody@pages.example is not known here".to_owned());

        let pages_miss = super::read_entity_with(
            &pages,
            &test_assets(),
            "pages.example",
            "viewer@pages.example",
            false,
            "/accounts/pages.example/nobody",
            None,
        )
        .await;

        let mut legacy = CountingReads::new(alice, Vec::new());
        legacy.account.acct = "alice@legacy.example".to_owned();
        let lookup_miss = super::read_entity_with(
            &legacy,
            &test_assets(),
            "legacy.example",
            "viewer@legacy.example",
            false,
            "/accounts/legacy.example/nobody",
            None,
        )
        .await;

        assert!(
            pages_miss.is_none(),
            "a 404 from the handle form must fall through to the shell, not paint an empty profile"
        );
        assert!(
            lookup_miss.is_none(),
            "the lookup miss is the same absence the handle form must produce"
        );
    }

    #[tokio::test]
    async fn a_pages_thread_is_one_call_and_paints_the_body_and_a_reply() {
        crate::capabilities::remember_for_test(
            "thread.example",
            nodus_ui::capabilities::Capabilities::from_published(&[
                nodus_ui::capabilities::Feature::Pages.as_str().to_owned(),
            ]),
        );
        let alice = test_account("1", "Alice Example", "Writer of things");
        let focal = test_status("10", "The post body", &alice);
        let mut reply = test_status("11", "A reply body", &alice);
        reply.in_reply_to_id = Some("10".to_owned());
        let mut reads = CountingReads::new(alice, vec![focal]);
        reads.context = nodus_client_core::model::Context {
            ancestors: Vec::new(),
            descendants: vec![reply],
        };

        let html = super::read_entity_with(
            &reads,
            &test_assets(),
            "thread.example",
            "viewer@thread.example",
            false,
            "/status/10",
            None,
        )
        .await
        .expect("the pages thread paints");

        assert_eq!(reads.count(), 1, "one request per post render");
        assert!(html.contains("The post body"), "{html}");
        assert!(html.contains("A reply body"), "{html}");
    }

    #[tokio::test]
    async fn a_host_without_pages_keeps_the_pre_pages_reads() {
        let alice = test_account("1", "Alice Example", "Writer of things");
        let first = test_status("10", "First post body", &alice);
        let reads = CountingReads::new(alice, vec![first]);

        let html = super::read_entity_with(
            &reads,
            &test_assets(),
            "legacy.example",
            "viewer@legacy.example",
            false,
            "/accounts/1",
            None,
        )
        .await
        .expect("the pre-pages profile paints");

        assert_eq!(
            reads.count(),
            2,
            "the fallback still reads account and page"
        );
        assert!(html.contains("Alice Example"), "{html}");
        assert!(html.contains("First post body"), "{html}");
    }

    #[test]
    fn a_public_profile_paints_no_island_and_no_relationship() {
        let alice = test_account("1", "Alice Example", "Writer of things");
        let first = test_status("10", "First post body", &alice);
        let html =
            super::render_public_account(&test_assets(), "pages.example", "1", alice, vec![first]);

        assert!(!html.contains("nodus-page"), "{html}");
        assert!(!html.contains("\"relationship\""), "{html}");
        assert!(html.contains("First post body"), "{html}");
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
            fields: Vec::new(),
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

    /// The host the document is painted for is the session's, so a local acct (`"ada"`,
    /// no `@host`) still reaches the pages endpoints of the instance it is signed into.
    #[tokio::test]
    async fn a_local_session_paints_with_its_own_instance_host() {
        crate::capabilities::remember_for_test(
            "local-session.test",
            nodus_ui::capabilities::Capabilities::from_published(&[
                nodus_ui::capabilities::Feature::Pages.as_str().to_owned(),
            ]),
        );
        let mut reads = CountingReads::new(test_account("1", "Ada", "Local"), Vec::new());
        reads.relationship = Some(nodus_client_core::model::Relationship {
            id: nodus_client_core::model::LocalId("viewer".to_owned()),
            following: true,
            requested: false,
            followed_by: false,
            blocking: false,
            blocked_by: false,
            muting: false,
            muting_notifications: false,
            domain_blocking: false,
            endorsed: false,
        });

        let html = super::read_entity_with(
            &reads,
            &test_assets(),
            "local-session.test",
            "ada",
            false,
            "/accounts/1",
            None,
        )
        .await
        .expect("a local reader still gets the pages profile");

        assert_eq!(reads.page_reads.get(), 1, "the session's host serves pages");
        assert_eq!(
            reads.lookups.get(),
            0,
            "no pre-pages lookup for a local acct"
        );
        assert!(html.contains("Ada"), "{html}");
        let (_, after) = html
            .split_once("<script id=\"nodus-page\" type=\"application/json\">")
            .expect("the profile document carries an island");
        let (island, _) = after.split_once("</script>").expect("closing script");
        let island: serde_json::Value = serde_json::from_str(island).expect("valid island json");
        assert_eq!(island["kind"], "account");
        assert_eq!(
            island["relationship"]["following"], true,
            "a local session's profile island carries the relationship it read"
        );
    }

    /// A remote handle names its own instance, which is the one that knows the account;
    /// the session's host must not override it.
    #[tokio::test]
    async fn a_remote_handle_paints_with_the_handle_host() {
        crate::capabilities::remember_for_test(
            "remote-handle.test",
            nodus_ui::capabilities::Capabilities::from_published(&[
                nodus_ui::capabilities::Feature::Pages.as_str().to_owned(),
            ]),
        );
        let reads = CountingReads::new(test_account("1", "Alice", "Remote"), Vec::new());

        let html = super::read_entity_with(
            &reads,
            &test_assets(),
            "local-session.test",
            "alice@remote-handle.test",
            false,
            "/accounts/1",
            None,
        )
        .await
        .expect("the remote handle's instance serves pages");

        assert_eq!(reads.page_reads.get(), 1, "the handle's host serves pages");
        assert_eq!(reads.lookups.get(), 0, "the session's host did not gate it");
        assert!(html.contains("Alice"), "{html}");
    }
}
