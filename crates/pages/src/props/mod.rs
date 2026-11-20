//! One way to feed a page.
//!
//! Every page takes its data as signals and its affordances as `Option<Callback<..>>`. The
//! client hands both; the server, which can only read, hands the data and no callbacks. That
//! split used to be spelled out as a nine-field struct literal on the server side, once per
//! page — and one of those literals built the page *outside* the reactive owner, which
//! aborted the process the first time a list had a row in it.
//!
//! So the server's shape lives here instead: `MessagesPageProps::server(items, ages)` fills
//! the data and writes `None` for every verb. A page that gains a field gains it in one
//! place, and a page that forgets to is a compile error rather than a missing button.

use leptos::prelude::{AnyView, Callback, Get, Signal};
use nodus_client_core::model::{
    Account, Conversation, List, Notification, ScheduledStatus, SearchResults, Status,
};

use crate::components::card_renderer::CardRenderer;
use crate::pages::directory::DirectoryPageProps;
use crate::pages::lists::ListsPageProps;
use crate::pages::messages::MessagesPageProps;
use crate::pages::notifications::NotificationsPageProps;
use crate::pages::saved::SavedPageProps;
use crate::pages::saved::shelf::Shelf;
use crate::pages::scheduled::ScheduledPageProps;
use crate::pages::search::SearchPageProps;

/// A value the caller already has, handed to a component that expects a signal.
pub fn held<T: Clone + Send + Sync + 'static>(value: T) -> Signal<T> {
    Signal::derive(move || value.clone())
}

/// An empty option.
pub fn none<T: Clone + Send + Sync + 'static>() -> Signal<Option<T>> {
    Signal::derive(|| None)
}

/// "Nothing is loading": the server has already finished reading before it paints.
pub fn settled() -> Signal<bool> {
    Signal::derive(|| false)
}

/// The sentence a control carries when the browser's grant has no `admin:write`.
///
/// The instance decides what a token may do; the console only knows the scopes the
/// `/api/v1/admin/meta` answer named, so a control outside them explains itself before it is
/// pressed rather than after the refusal.
pub const NO_WRITE: &str =
    "This browser's staff grant does not include admin:write, so the instance would refuse this change.";

/// The sentence a control carries when the route behind it asks for the administrator role.
///
/// The account's staff role is a different question from the grant's scopes: the
/// administrator flag and media retention answer `role == admin` whatever the token holds.
pub const NO_ADMIN_ROLE: &str =
    "This needs an administrator role rather than a moderator's: the instance refuses this request whatever the grant's scopes.";

/// The sentence an administrator-only panel carries above its controls.
///
/// Said out loud rather than left to a disabled button: the grant's scopes are not the role,
/// and a moderator holding `admin:write` is still refused by these routes.
pub const ADMIN_ONLY: &str =
    "Administrators only. The instance answers this panel's routes with the admin role, not a moderator's, so nothing here can be changed from this account.";

/// The sentence a control carries when the account may not press it, or `""` when it may.
///
/// The role is asked first: a moderator whose grant carries `admin:write` is refused by an
/// administrator-only route, and naming the scope would point at the wrong reason.
#[must_use]
pub fn refusal(write: Signal<bool>, administrator: Signal<bool>) -> &'static str {
    if !administrator.get() {
        NO_ADMIN_ROLE
    } else if !write.get() {
        NO_WRITE
    } else {
        ""
    }
}

/// Whether the account's staff role is administrator.
///
/// `None` means "assume yes", for the same reason [`writable`] does: the server paints these
/// pages with every verb already absent.
#[must_use]
pub fn administrator(role: Option<Signal<bool>>) -> Signal<bool> {
    role.unwrap_or_else(|| Signal::derive(|| true))
}

/// Whether the browser's staff grant carries `admin:write`.
///
/// `None` means "assume yes": the server paints these pages with every verb already absent,
/// so each control is disabled by its missing callback and needs no second reason.
#[must_use]
pub fn writable(write: Option<Signal<bool>>) -> Signal<bool> {
    write.unwrap_or_else(|| Signal::derive(|| true))
}

/// Whether the instance serves a capability's endpoints, as `/api/v1/meta` names them.
///
/// `None` means "assume yes", for the same reason `writable` does: the page is also painted
/// where no capability list was read, and a paint that hid a control the instance serves
/// would be a lie about the screen.
#[must_use]
pub fn capable(capability: Option<Signal<bool>>) -> Signal<bool> {
    capability.unwrap_or_else(|| Signal::derive(|| true))
}

/// The ages a page prints, measured against the clock reading the caller lends it.
pub fn ages(now_millis: f64) -> Callback<String, String> {
    Callback::new(move |created: String| crate::relative_age(&created, now_millis))
}

impl MessagesPageProps {
    /// The server's shape: conversations and a clock, no verbs.
    #[must_use]
    pub fn server(
        conversations: Vec<Conversation>,
        age_of: Option<Callback<String, String>>,
    ) -> Self {
        Self {
            conversations: held(conversations),
            muted: held(Vec::new()),
            loading: settled(),
            error: none(),
            cursor: none(),
            age_of,
            on_open: None,
            on_read: None,
            on_mute: None,
            on_more: None,
        }
    }
}

impl NotificationsPageProps {
    /// The server's shape: notifications, a clock, and a way to paint the post a boost or a
    /// favourite points at.
    #[must_use]
    pub fn server(
        notifications: Vec<Notification>,
        age_of: Option<Callback<String, String>>,
        status_card: Option<Callback<Status, AnyView>>,
    ) -> Self {
        Self {
            notifications: held(notifications),
            cursor: none(),
            loading: settled(),
            error: none(),
            age_of,
            on_more: None,
            on_open: None,
            status_card,
            request_actions: None,
            banner: None,
        }
    }
}

impl ListsPageProps {
    /// The server's shape: the lists only. Members and timelines are the client's panels.
    #[must_use]
    pub fn server(lists: Vec<List>) -> Self {
        Self {
            lists: held(lists),
            selected: none(),
            title: held(String::new()),
            busy: settled(),
            error: none(),
            on_title: None,
            on_create: None,
            row: None,
            members: None,
            timeline: None,
        }
    }
}

impl ScheduledPageProps {
    /// The server's shape: what is queued, and a clock.
    #[must_use]
    pub fn server(items: Vec<ScheduledStatus>, age_of: Option<Callback<String, String>>) -> Self {
        Self {
            items: held(items),
            loading: settled(),
            error: none(),
            age_of,
            on_cancel: None,
        }
    }
}

impl DirectoryPageProps {
    /// The server's shape: the people this instance publishes.
    #[must_use]
    pub fn server(accounts: Vec<Account>) -> Self {
        Self {
            local_only: Signal::derive(|| true),
            accounts: held(accounts),
            loading: settled(),
            error: none(),
            cursor: none(),
            on_scope: None,
            on_open: None,
            on_more: None,
        }
    }
}

impl SavedPageProps {
    /// The server's shape: one shelf, its posts, and the card painter.
    #[must_use]
    pub fn server(shelf: Shelf, items: Vec<Status>, card: CardRenderer) -> Self {
        Self {
            shelf: held(shelf),
            items: held(items),
            loading: settled(),
            error: none(),
            cursor: none(),
            card,
            on_shelf: None,
            on_more: None,
        }
    }
}

impl SearchPageProps {
    /// The server's shape: the answer, the query that asked for it, and the card painter.
    #[must_use]
    pub fn server(
        query: String,
        results: SearchResults,
        age_of: Option<Callback<String, String>>,
        card: Option<Callback<Status, AnyView>>,
    ) -> Self {
        Self {
            query: held(query),
            accounts: held(results.accounts),
            statuses: held(results.statuses),
            tags: held(results.hashtags),
            searched: Signal::derive(|| true),
            loading: settled(),
            error: none(),
            on_query: None,
            on_search: None,
            age_of,
            card,
        }
    }
}
