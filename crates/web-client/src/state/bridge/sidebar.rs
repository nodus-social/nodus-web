//! What the right rail's cards show.
//!
//! The cards are reads like any other: the newest page the store already holds paints
//! immediately, and [`ensure_loaded`] fills what is missing. Nothing here invents an order —
//! "trending" is the instance's ranking, and the live card is the local timeline in the order
//! it arrived, which is what a reader asking "what is happening here" wants to see.
//!
//! Every card degrades to its own sentence when it has nothing: an instance with no trends, a
//! reader with no notifications and a browser that has not asked yet are three different
//! silences, and pretending they are one is how a rail ends up looking broken.

use leptos::prelude::*;
use nodus_client_core::model::{Account, Notification, Status, Tag};
use nodus_client_sdk::Timeline;
use nodus_client_sdk::page::{PageItems, PageKind};
use nodus_ui::components::sidebar::{SidebarData, SidebarItem};

use super::{actions, pages, timeline};

/// How many rows each card holds. Enough to be a glance, few enough to stay a glance.
const ROWS: usize = 6;

thread_local! {
    /// The cards' contents, shared by every render of the rail.
    static DATA: RwSignal<SidebarData> = RwSignal::new(SidebarData::default());

    /// Whether a first load has run, so remounts do not re-ask.
    static LOADED: RwSignal<bool> = RwSignal::new(false);
}

/// Creates the shared signals with no owner.
///
/// A `thread_local` signal is built on first use, which means it belongs to whichever
/// component read it first; a rail card that remounts would then be reading a signal that
/// went away with the card. Every app-lifetime signal in this client is created before the
/// first mount for exactly that reason (see `boot::start`).
pub fn install() {
    let _ = data();
    let _ = loaded();
}

/// Whether a first load has run.
#[must_use]
pub fn loaded() -> bool {
    LOADED.with(|loaded| loaded.get())
}

/// The cards' contents right now.
#[must_use]
pub fn data() -> SidebarData {
    DATA.with(|data| data.get())
}

/// Fill the cards once per document.
pub fn ensure_loaded() {
    if LOADED.with(|loaded| loaded.get()) {
        return;
    }
    refresh();
}

/// Re-read every card, keeping what is on screen while the requests run.
pub fn refresh() {
    wasm_bindgen_futures::spawn_local(async move {
        let data = read().await;
        DATA.with(|held| held.set(data));
        LOADED.with(|loaded| loaded.set(true));
    });
}

/// Read every card from the stores and the instance.
async fn read() -> SidebarData {
    let mut data = SidebarData::default();

    let (local, _) = timeline::read(Timeline::Local, None);
    data.live_feed = local.into_iter().take(ROWS).map(status_item).collect();

    let (notifications, _) = pages::read(&PageKind::Notifications).map_or_else(
        || (Vec::new(), None),
        |entry| match entry.items {
            PageItems::Notifications(items) => (items, entry.cursor),
            _ => (Vec::new(), None),
        },
    );
    data.notifications = notifications
        .into_iter()
        .take(ROWS)
        .map(notification_item)
        .collect();

    let (accounts, _) = pages::read(&PageKind::Directory(true)).map_or_else(
        || (Vec::new(), None),
        |entry| match entry.items {
            PageItems::Accounts(items) => (items, entry.cursor),
            _ => (Vec::new(), None),
        },
    );
    data.trending_users = accounts.into_iter().take(ROWS).map(account_item).collect();

    if let Ok(tags) = actions::trending_tags(ROWS).await {
        data.trending_tags = tags.into_iter().map(tag_item).collect();
    }
    if let Ok(statuses) = actions::trending_statuses(ROWS).await {
        data.trending_posts = statuses.into_iter().map(status_item).collect();
    }

    data
}

/// One post, as the rail shows it: a preview line, the author, and a link.
fn status_item(status: Status) -> SidebarItem {
    let preview = preview_text(&status.content);
    let label = if preview.is_empty() {
        format!("@{}", status.account.acct)
    } else {
        preview
    };
    let href = match status.url.as_deref().filter(|url| !url.is_empty()) {
        Some(url) => url.to_owned(),
        // A post with no permalink of its own (remote, or not yet resolved) still has a
        // route of ours; the handle plus the id is what a local link is made of.
        None => format!("/@{}/{}", status.account.acct, status.id),
    };
    let mut item = SidebarItem::new(label, format!("@{}", status.account.acct), href);
    if !status.account.avatar.is_empty() {
        item = item.with_picture(status.account.avatar.clone());
    }
    item
}

/// One notification, phrased as what happened rather than as its type name.
fn notification_item(notification: Notification) -> SidebarItem {
    let who = format!("@{}", notification.account.acct);
    let verb = match notification.kind {
        nodus_client_core::model::NotificationKind::Follow => "followed you",
        nodus_client_core::model::NotificationKind::Favourite => "favourited your post",
        nodus_client_core::model::NotificationKind::Reblog => "boosted your post",
        nodus_client_core::model::NotificationKind::Mention => "mentioned you",
        nodus_client_core::model::NotificationKind::Poll => "a poll you voted in ended",
        nodus_client_core::model::NotificationKind::Status => "posted",
        _ => "reached you",
    };
    let label = notification
        .status
        .as_ref()
        .map(|status| preview_text(&status.content))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| verb.to_owned());
    let href = notification
        .status
        .as_ref()
        .and_then(|status| status.url.as_deref().filter(|url| !url.is_empty()))
        .map_or_else(|| "/notifications".to_owned(), ToOwned::to_owned);
    let mut item = SidebarItem::new(label, format!("{who} {verb}"), href);
    if !notification.account.avatar.is_empty() {
        item = item.with_picture(notification.account.avatar.clone());
    }
    item
}

/// One account: the name people will see, the handle that identifies it.
fn account_item(account: Account) -> SidebarItem {
    let label = if account.display_name.trim().is_empty() {
        format!("@{}", account.acct)
    } else {
        account.display_name.clone()
    };
    let mut item = SidebarItem::new(
        label,
        format!("@{}", account.acct),
        nodus_web::shared::handles::profile_path_for(
            &account.acct,
            crate::state::bridge::session_host().as_deref(),
        ),
    );
    if !account.avatar.is_empty() {
        item = item.with_picture(account.avatar.clone());
    }
    item
}

/// One tag, with the usage the instance reports when it reports any.
fn tag_item(tag: Tag) -> SidebarItem {
    let uses = tag
        .history
        .iter()
        .filter_map(|day| day.uses.parse::<u64>().ok())
        .sum::<u64>();
    let meta = if uses == 0 {
        "this week".to_owned()
    } else {
        format!("{uses} posts this week")
    };
    SidebarItem::new(
        format!("#{}", tag.name),
        meta,
        format!("/tags/{}", tag.name),
    )
}

/// The first line of a post's HTML as plain text.
///
/// The rail shows a glance, not a post: tags become their text, entities are unescaped, and
/// the cut lands on a character boundary so a multi-byte glyph is never split.
fn preview_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for character in html.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !text.ends_with(' ') {
                    text.push(' ');
                }
            }
            _ if in_tag => {}
            '&' => text.push_str("&"),
            _ => text.push(character),
        }
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&text, 90)
}

/// Cut a string at a character boundary, adding an ellipsis when it was longer.
fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::{preview_text, truncate_chars};

    #[test]
    fn a_post_becomes_one_plain_line() {
        assert_eq!(
            preview_text("<p>Hello <strong>there</strong></p><p>second</p>"),
            "Hello there second"
        );
        assert_eq!(preview_text("<p>&amp; more</p>"), "& more");
    }

    #[test]
    fn a_long_post_is_cut_without_splitting_a_character() {
        let long = "é".repeat(200);
        let cut = truncate_chars(&long, 10);
        assert_eq!(cut.chars().count(), 11);
        assert!(cut.ends_with('…'));
    }
}
