//! One row of a list, and the status card that sits inside a notification.
//!
//! Every function here renders a component from `nodus-view` and returns the string the page
//! embeds: the markup is the client's markup, this module supplies only the data and the
//! absent verbs.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_client_core::model::Status;
use nodus_web::shared::handles;

/// One status as markup: who, what, and what can be done with it.
///
/// The whole card comes from `nodus-view`, the component the client mounts, so the first
/// paint and the one after the wasm boots cannot drift. The body is rewritten here rather
/// than in the component: the emoji shortcodes are a read of the instance's emoji list, and
/// `inner_html` already carries the HTML both paints show.
pub fn status_card(status: &Status) -> String {
    let (author_path, thread_path) = paths(status);
    painted(status, &author_path, &thread_path)
}

/// The same card as a node, for a document the client adopts.
///
/// A paint that is hydrated must serve the tree the client's own card renderer builds, and
/// the client builds a component: embedding the card's markup inside `inner_html` puts one
/// element between the renderer's answer and the card the client looks for.
///
/// `viewer` is the signed-in account the document is painted for. A hydrated card carries
/// the client's own controls — the author's menu and the share button — because the client
/// mounts them and hydration walks into them; a card painted with no viewer is the plain
/// row a document with no wasm to adopt it needs.
pub fn status_card_view(status: &Status, viewer: Option<&str>) -> AnyView {
    let (author_path, thread_path) = paths(status);
    card(status, &author_path, &thread_path, viewer).into_any()
}

/// Where a card's author and thread live, for the instance this post was read from.
fn paths(status: &Status) -> (String, String) {
    let inner = status.reblog.as_deref().unwrap_or(status);
    // The account names its instance for a remote author; a local one is named by the
    // post's own url, which is on this instance.
    let host = handles::host_of_acct(&inner.account.acct)
        .or_else(|| inner.url.as_deref().and_then(handles::host_of_url))
        .or_else(|| instance_host())
        .unwrap_or_default()
        .to_owned();
    (
        handles::profile_path_for(&inner.account.acct, Some(&host)),
        handles::status_path(Some(&host), &inner.id),
    )
}

/// The same card for a visitor: its links stay on the instance that hosts the post.
pub fn status_card_public(status: &Status, host: &str) -> String {
    let inner = status.reblog.as_deref().unwrap_or(status);
    painted(
        status,
        &handles::profile_path_for(&inner.account.acct, Some(host)),
        &format!("/status/{host}/{}", inner.id),
    )
}

/// The instance this deployment calls itself, when it knows.
///
/// A local post's url is its own address, so the host is usually in hand; a post that has
/// no url yet still links "here", which is this name.
fn instance_host() -> Option<&'static str> {
    static NAMED: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    NAMED
        .get_or_init(|| {
            std::env::var("NODUS_INSTANCE_HOST")
                .ok()
                .map(|host| host.trim().to_owned())
                .filter(|host| !host.is_empty())
        })
        .as_deref()
}

fn painted(status: &Status, author_path: &str, thread_path: &str) -> String {
    Owner::new().with(|| card(status, author_path, thread_path, None).to_html())
}

fn card(
    status: &Status,
    author_path: &str,
    thread_path: &str,
    viewer: Option<&str>,
) -> impl IntoView {
    let mut painted = status.clone();
    // A boost renders the boosted post under the booster's name, exactly as the client
    // does: the line above the card comes from the wrapper and everything below it from
    // `reblog`. `StatusBody` runs `nodus_ui::html::prepared`, and sanitising the cloned
    // status here keeps `StatusHeader` and nested models clean too.
    nodus_ui::html::sanitize_status(&mut painted);
    let inner = painted.reblog.as_deref().map_or(&painted, |inner| inner);
    let age = nodus_ui::relative_age(&inner.created_at, now_millis());
    // The client draws its menu only on the reader's own statuses, so the paint answers the
    // same question with the same answer: the account id the session already carries.
    let mine = viewer.is_some_and(|viewer| viewer == inner.account.id);
    let menu_id = StoredValue::new(inner.id.clone());
    // The client's first paint mounts these slots, and hydration walks into them: a card
    // without them is a different tree at the same place, which is what the framework
    // reports as an expected element that is not there.
    let menu: ChildrenFn = Arc::new(move || {
        view! {
            <Show when=move || mine>
                <nodus_ui::status_menu::StatusMenu
                    id=menu_id.get_value()
                    on_edit=Callback::new(|()| ())
                    on_delete=Callback::new(|()| ())
                />
            </Show>
        }
        .into_any()
    });
    let share: ChildrenFn = Arc::new(|| {
        view! {
            <nodus_ui::status_share::ShareButton
                copied=Signal::derive(|| false)
                on_copy=Callback::new(|()| ())
            />
        }
        .into_any()
    });
    let before_verbs = viewer.map(|_| menu);
    let after_verbs = viewer.map(|_| share);

    nodus_ui::status_card::StatusCard(nodus_ui::status_card::StatusCardProps {
        status: painted,
        age,
        author_path: author_path.to_owned(),
        thread_path: thread_path.to_owned(),
        on_navigate: Some(nodus_route::route::noop()),
        on_open: Some(nodus_route::route::noop()),
        on_boost: Some(nodus_route::route::noop()),
        on_favourite: Some(nodus_route::route::noop()),
        on_bookmark: Some(nodus_route::route::noop()),
        on_reply: Some(nodus_route::route::noop()),
        before_verbs,
        after_verbs,
        on_open_media: Some(nodus_route::route::noop()),
        on_vote: Some(nodus_route::route::noop()),
        interactive: true,
        body: None,
        extra: None,
        // A reaction tally is a second read of the note by id, and the instance's answer
        // to whether it serves reactions at all: neither is in the status this paint
        // was handed, and a static row has nothing to press, so the row is absent.
        reactions: None,
        on_react: None,
        on_unreact: None,
        reactions_note: None,
    })
}

/// Milliseconds since the Unix epoch, by this host's clock.
pub(crate) fn now_millis() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1_000.0)
}
