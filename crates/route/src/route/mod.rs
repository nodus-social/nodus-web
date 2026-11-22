//! What a screen needs from the platform, and the no-op answers the server gives.
//!
//! The renderer asks for one synchronous snapshot per read. The browser answers from the
//! live SDK store; the server answers from the page it prefetched. An operation the server
//! cannot perform is reported as unavailable, so the screen draws its own failure state
//! instead of a second, server-only page.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_client_core::model::{
    Account, Context, Conversation, Filter, List, Poll, Relationship, Status, Tag,
};
use nodus_shared::{AccountItem, PostItem};
use nodus_state::page::{PageEntry, PageKind};
use nodus_web::shared::router::Route;

use crate::Alive;
use crate::components::CardRenderer;
use crate::pages::settings::nav::SettingsSection;

pub mod screens;
use screens::account::AccountScreen;
use screens::admin::{AdminScreen, ModerationScreen};
use screens::compose::ComposeScreen;
use screens::directory::DirectoryScreen;
use screens::feed::FeedScreen;
use screens::lists::ListsScreen;
use screens::messages::MessagesScreen;
use screens::notifications::NotificationsScreen;
use screens::people::PeopleList;
use screens::saved::SavedScreen;
use screens::scheduled::ScheduledScreen;
use screens::search::SearchScreen;
use screens::settings::SettingsScreen;
use screens::tag::TagScreen;
use screens::thread::ThreadScreen;

/// Which timeline a route asks for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TimelineKind {
    Home,
    Local,
    Federated,
    Account(String),
    List(String),
    Tag(String),
}

/// One page of a timeline, as the platform can answer it right now.
#[derive(Clone, Debug, Default)]
pub struct TimelineReply {
    pub items: Vec<Status>,
    pub cursor: Option<String>,
    pub held: bool,
    pub unavailable: bool,
}

impl TimelineReply {
    /// The answer when the stateless platform has no page for this timeline.
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            unavailable: true,
            ..Self::default()
        }
    }
}

/// The recommended accounts already in hand.
#[derive(Clone, Debug, Default)]
pub struct FeedPeopleView {
    pub items: Vec<AccountItem>,
    pub degraded: Option<String>,
    pub cursor: Option<String>,
    pub error: Option<String>,
    pub refreshing: bool,
}

/// The recommended posts already in hand.
#[derive(Clone, Debug, Default)]
pub struct FeedPostsView {
    pub items: Vec<PostItem>,
    pub cursor: Option<String>,
    pub error: Option<String>,
    pub refreshing: bool,
}

/// Which half of the feed a load names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeedKind {
    People,
    Posts,
}

/// The signed-in account, as much as a view needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionView {
    pub host: String,
    pub account_id: String,
    pub acct: String,
}

/// A change the reader asked for.
#[derive(Clone, Debug)]
pub enum Action {
    Favourite {
        id: String,
        on: bool,
    },
    Reblog {
        id: String,
        on: bool,
    },
    Bookmark {
        id: String,
        on: bool,
    },
    DeleteStatus(String),
    Vote {
        poll: String,
        choices: Vec<usize>,
    },
    Reply {
        parent: String,
        text: String,
    },
    Follow {
        id: String,
        on: bool,
    },
    Mute {
        id: String,
        on: bool,
    },
    Block {
        id: String,
        on: bool,
    },
    AnswerFollowRequest {
        id: String,
        accept: bool,
    },
    ReadConversation(String),
    MuteConversation {
        id: String,
        on: bool,
    },
    CreateList(String),
    RenameList {
        id: String,
        title: String,
    },
    DeleteList(String),
    SetListMember {
        list: String,
        account: String,
        member: bool,
    },
    DeleteFilter(String),
    FeatureTag(String),
    UnfeatureTag(String),
    CancelScheduled(String),
    OpenMedia {
        urls: Vec<String>,
        index: usize,
    },
    UpdateProfile {
        display_name: String,
        note: String,
    },
}

/// What a completed action answered with, when the screen needs the new value.
#[derive(Clone, Debug)]
pub enum ActionResult {
    None,
    Status(Status),
    Poll(Poll),
    Conversation(Conversation),
    Relationship(Relationship),
    List(List),
    Filter(Filter),
    Account(Account),
}

/// The platform side of a rendered route.
pub trait Runtime: Send + Sync + 'static {
    /// Bumps whenever a store the screen reads has changed.
    fn revision(&self) -> Signal<u64>;

    /// The current snapshot. Called while rendering; must not start work.
    fn timeline(&self, kind: &TimelineKind) -> TimelineReply;

    /// Starts a read and reports completion. The server answers immediately; the browser
    /// spawns it and calls `done` when the store has been updated.
    fn load_timeline(
        &self,
        kind: TimelineKind,
        cursor: Option<String>,
        done: Callback<Result<(), String>>,
    );

    /// One stored page, if the platform can answer it now.
    fn page(&self, _kind: &PageKind) -> Option<PageEntry> {
        None
    }

    /// Starts a page read. `cursor` is the page the reader is continuing from.
    fn load_page(
        &self,
        _kind: PageKind,
        _cursor: Option<String>,
        done: Callback<Result<(), String>>,
    ) {
        done.run(Err("This page is not available without the app.".to_owned()));
    }

    /// A status already in hand, if the platform keeps one.
    /// The normal composer, when this paint can write. The server has no client editor, so
    /// its thread keeps the plain reply card.
    fn reply_composer(
        &self,
        _parent: String,
        _replies: RwSignal<Vec<Status>>,
    ) -> Option<ChildrenFn> {
        None
    }

    fn status(&self, _id: &str) -> Option<Status> {
        None
    }

    /// A conversation context already in hand.
    fn context(&self, _id: &str) -> Option<Context> {
        None
    }

    /// An account already in hand.
    fn account(&self, _id: &str) -> Option<Account> {
        None
    }

    /// The viewer's relationship to an account.
    fn relationship(&self, _id: &str) -> Option<Relationship> {
        None
    }

    /// Loads one status, then one context.
    fn load_thread(
        &self,
        _id: String,
        _status: Callback<Result<Status, String>>,
        _context: Callback<Result<Context, String>>,
    ) {
    }

    /// The recommended accounts already in hand.
    fn feed_people(&self) -> FeedPeopleView {
        FeedPeopleView::default()
    }

    /// The recommended posts already in hand.
    fn feed_posts(&self) -> FeedPostsView {
        FeedPostsView::default()
    }

    /// Loads one feed page; `None` is the first page or a refresh.
    fn load_feed(
        &self,
        _kind: FeedKind,
        _cursor: Option<String>,
        done: Callback<Result<(), String>>,
    ) {
        done.run(Err("The feed needs the app.".to_owned()));
    }

    /// The client's account row; the server has none because it has no items.
    fn feed_people_row(&self) -> Option<Callback<AccountItem, AnyView>> {
        None
    }

    /// The client's post row; the server has none because it has no items.
    fn feed_posts_row(&self) -> Option<Callback<PostItem, AnyView>> {
        None
    }

    /// Opens the feed settings, a browser route.
    fn feed_settings(&self) -> Option<Callback<()>> {
        None
    }

    /// Loads a post from a host that has no session, for a shared link.
    fn load_public_thread(
        &self,
        _host: String,
        _id: String,
        status: Callback<Result<Status, String>>,
        context: Callback<Result<Context, String>>,
    ) {
        let message = "This post needs a host that can be read.".to_owned();
        status.run(Err(message.clone()));
        context.run(Err(message));
    }

    /// Loads a profile and the relationship that decides its verbs.
    fn load_profile(
        &self,
        _id: String,
        _account: Callback<Result<Account, String>>,
        _relationship: Callback<Result<Option<Relationship>, String>>,
    ) {
    }

    /// Reads an account from a host that has no session.
    fn public_account(&self, _host: &str, _id: &str) -> Option<Account> {
        None
    }

    /// Loads an account from a host that has no session.
    fn load_public_account(
        &self,
        host: String,
        id: String,
        done: Callback<Result<Account, String>>,
    ) {
        let _ = (host, id, done);
    }

    /// The public posts already read for an account with no session.
    fn public_account_statuses(&self, _host: &str, _id: &str) -> Vec<Status> {
        Vec::new()
    }

    /// Loads the public posts of an account with no session.
    fn load_public_account_statuses(
        &self,
        host: String,
        id: String,
        done: Callback<Result<Vec<Status>, String>>,
    ) {
        let _ = (host, id, done);
    }

    /// The public followers already read for an account with no session.
    fn public_account_followers(&self, _host: &str, _id: &str) -> Option<Vec<Account>> {
        None
    }

    /// Loads the public posts older than `max_id`.
    fn load_public_account_statuses_before(
        &self,
        host: String,
        id: String,
        max_id: String,
        done: Callback<Result<Vec<Status>, String>>,
    ) {
        let _ = (host, id, max_id, done);
    }

    /// Loads an account's public followers.
    fn load_public_account_followers(
        &self,
        host: String,
        id: String,
        done: Callback<Result<Vec<Account>, String>>,
    ) {
        let _ = (host, id, done);
    }

    /// The public follows already read for an account with no session.
    fn public_account_following(&self, _host: &str, _id: &str) -> Option<Vec<Account>> {
        None
    }

    /// Loads the accounts an account follows publicly.
    fn load_public_account_following(
        &self,
        host: String,
        id: String,
        done: Callback<Result<Vec<Account>, String>>,
    ) {
        let _ = (host, id, done);
    }

    /// Notes that a reader opened a public target, so the crawler can follow.
    fn note_browse(
        &self,
        _kind: &'static str,
        _host: String,
        _id: String,
        _acct: Option<String>,
        _url: Option<String>,
    ) {
    }

    /// The instance this deployment belongs to, for unsigned local links.
    fn public_host(&self) -> Option<String> {
        None
    }

    /// The signed-in account, if any.
    fn session(&self) -> Option<SessionView> {
        None
    }

    /// Follow requests waiting for an answer.
    fn follow_requests(&self) -> Option<Vec<Account>> {
        None
    }

    /// Loads the follow-request queue.
    fn load_follow_requests(&self, _done: Callback<Result<Vec<Account>, String>>) {}

    /// The lists the viewer owns.
    fn lists(&self) -> Option<Vec<List>> {
        None
    }

    /// Loads the list collection.
    fn load_lists(&self, _done: Callback<Result<Vec<List>, String>>) {}

    /// The accounts on one list, as last read.
    fn list_accounts(&self, _id: &str) -> Option<Vec<Account>> {
        None
    }

    /// Loads the accounts on one list.
    fn load_list_accounts(&self, _id: String, _done: Callback<Result<Vec<Account>, String>>) {}

    /// Performs one reader action. A stateless paint refuses it.
    /// Search accounts for a list's add form.
    fn search_accounts(&self, _query: String, done: Callback<Result<Vec<Account>, String>>) {
        done.run(Err("Search needs the app.".to_owned()));
    }

    fn action(&self, _action: Action, done: Callback<Result<ActionResult, String>>) {
        done.run(Err(
            "This action is not available without the app.".to_owned()
        ));
    }

    /// The settings sections the browser can build with its own forms.
    fn settings_body(&self, _section: SettingsSection) -> Option<ChildrenFn> {
        None
    }

    /// The instance the reader is signed in on, when they are.
    ///
    /// A stateless paint has no session, which is what decides whether a profile link from
    /// another instance can be resolved to a local account: without one there is nothing to
    /// act with, so the visitor's page is the whole answer.
    fn session_host(&self) -> Option<String> {
        None
    }

    /// Custom visual framing and navigation layout for the active session.
    fn session_style(&self) -> nodus_ui::session_style::SessionStyle {
        nodus_ui::session_style::SessionStyle::default()
    }

    /// A route body only the browser can build: the editor, the staff probe.
    fn body(&self, _route: &Route) -> Option<ChildrenFn> {
        None
    }

    /// The clock both paints use for post ages.
    fn now_millis(&self) -> f64;

    /// The query string argument a route was opened with.
    fn query(&self, _key: &str) -> Option<String> {
        None
    }

    /// The screen a More page was opened from, so its own section stays marked.
    fn last_path(&self) -> Option<String> {
        None
    }

    /// A destination click. The server has nowhere to navigate and leaves the default.
    fn drop_account(&self, _kind: &PageKind, _account: &str) {}
    fn navigate(&self, _path: String) {}

    /// One hashtag's header, read with the session.
    ///
    /// `Ok(None)` is an instance whose tag API does not know the name: a 404 there means
    /// the tag does not exist, which the screen shows as a note instead of an error.
    fn load_tag(&self, _name: String, done: Callback<Result<Option<Tag>, String>>) {
        done.run(Err("This tag can only be read in the app.".to_owned()));
    }

    /// Follows or unfollows a tag, answering with the instance's own state.
    ///
    /// The returned tag is what the screen shows: `following` is the server's answer, not
    /// the reader's click. `Ok(None)` is the same absent-tag answer as [`Self::load_tag`].
    fn follow_tag(&self, _name: String, _on: bool, done: Callback<Result<Option<Tag>, String>>) {
        done.run(Err("Following a tag needs the app.".to_owned()));
    }

    /// Whether this paint can follow tags at all, which is what draws the verb.
    fn can_follow_tags(&self) -> bool {
        false
    }

    /// A platform trigger that asks for the next page when it scrolls into view.
    fn infinite_more(&self, _active: Signal<bool>, _on_load: Callback<()>) -> Option<ChildrenFn> {
        None
    }
}

/// The callback a stateless paint supplies for one verb: present, but inert.
#[must_use]
pub fn noop<In>() -> Callback<In>
where
    In: Send + Sync + 'static,
{
    Callback::new(|_| ())
}

/// Whether an account address segment is one of our ids rather than a handle.
///
/// Mastodon's ids are digit strings and ours are 26-character ULIDs; a handle is neither,
/// which is what lets a bare name resolve as a name.
fn looks_like_id(segment: &str) -> bool {
    let digits = !segment.is_empty() && segment.chars().all(|c| c.is_ascii_digit());
    let ulid = segment.len() == 26 && segment.chars().all(|c| c.is_ascii_alphanumeric());
    digits || ulid
}

/// The route renderer both paints call.
#[component]
#[expect(
    clippy::too_many_lines,
    reason = "one arm per route: the match is the router, and splitting it hides a missing screen"
)]
pub fn RouteView(
    runtime: Arc<dyn Runtime>,
    route: Route,
    card: CardRenderer,
    #[prop(optional_no_strip)] header: Option<ChildrenFn>,
    #[prop(optional_no_strip)] banner: Option<ChildrenFn>,
) -> impl IntoView {
    // A surface this instance does not serve explains itself here, before a screen starts
    // reading something that 404s. `None` (no probe yet, or a stateless paint that carried no
    // list) draws the screen as it always did: the answer is missing, not negative.
    let at = nodus_ui::capabilities::serves();
    let answer = at.as_ref().map(|at| at.get_untracked());
    if let Some(surface) = surface_of(&route) {
        if !nodus_ui::capabilities::admits(answer.as_ref(), surface) {
            return view! { <nodus_ui::UnsupportedSurface surface=surface/> }.into_any();
        }
    }
    if let Some(body) = runtime.body(&route) {
        return body().into_any();
    }
    match route {
        // The editor's middle is the card tray: the rail and the banners are what is being
        // arranged, so the largest area is the list of what is not on the rail yet. A server
        // render knows no draft, so the tray paints the catalogue with its buttons disabled.
        Route::StyleEditor => view! {
            <nodus_ui::components::sidebar::ModuleTray
                layout=Signal::derive(Vec::new)
                nav=Signal::derive(nodus_ui::session_style::NavLayout::default)
                staff_rail=Signal::derive(|| nodus_ui::session_style::ModuleRail::Left)
                editor=None
            />
        }
        .into_any(),
        // The reader's own page with the arranging tools on: the address says which page,
        // and this says what is painted under the chrome.
        Route::Home | Route::Local | Route::Federated => {
            let kind = match route {
                Route::Local => TimelineKind::Local,
                Route::Federated => TimelineKind::Federated,
                _ => TimelineKind::Home,
            };
            view! {
                <TimelineScreen runtime kind header banner card/>
            }
            .into_any()
        }
        Route::Notifications => view! {
            <NotificationsScreen runtime card/>
        }
        .into_any(),
        Route::Tag(name) => view! {
            <TagScreen runtime name card/>
        }
        .into_any(),
        Route::Messages => view! {
            <MessagesScreen runtime/>
        }
        .into_any(),
        Route::Saved(shelf) => view! {
            <SavedScreen runtime shelf card/>
        }
        .into_any(),
        Route::Directory => view! {
            <DirectoryScreen runtime/>
        }
        .into_any(),
        Route::Scheduled => view! {
            <ScheduledScreen runtime/>
        }
        .into_any(),
        Route::Search => view! {
            <SearchScreen runtime card/>
        }
        .into_any(),
        Route::Thread { host, id } => view! {
            <ThreadScreen runtime host id card/>
        }
        .into_any(),
        Route::Account(id) => match id.split_once('/') {
            Some((host, id)) if !host.is_empty() && !id.is_empty() => {
                let signed_in = runtime.session_host();
                // A number is this instance's own numbering, not a handle: a link that
                // names this instance and an id was minted here, and looking it up by name
                // asks for `@1@host`, which no account answers to. That lookup is what sent
                // the reader's own profile to the visitor's page, which draws neither the
                // verbs nor the metadata rows.
                if signed_in
                    .as_ref()
                    .is_some_and(|mine| mine.eq_ignore_ascii_case(host))
                    && looks_like_id(id)
                {
                    view! { <AccountScreen runtime id=id.to_owned() card/> }.into_any()
                } else if signed_in.is_some() {
                    // The address names an instance and a handle, so a reader resolves it
                    // through their own instance: that keeps follows, blocks and mutes in
                    // play, and lands on the local row every other screen links to.
                    view! {
                        <crate::route::screens::profile::ProfileScreen
                            runtime
                            host=host.to_owned()
                            name=id.to_owned()
                            card
                        />
                    }
                    .into_any()
                } else {
                    view! {
                        <crate::route::screens::public_account::PublicAccountScreen
                            runtime
                            host=host.to_owned()
                            id=id.to_owned()
                            card
                        />
                    }
                    .into_any()
                }
            }
            // A bare handle means "here": with a session the reader's instance is where
            // the name resolves, so the profile is the screen, not a prompt to sign in.
            // An id-shaped segment stays with the older screen, which reads it as one.
            _ if runtime.session_host().is_some() && !looks_like_id(&id) => {
                let host = runtime.session_host().unwrap_or_default();
                view! {
                    <crate::route::screens::profile::ProfileScreen
                        runtime
                        host
                        name=id.to_owned()
                        card
                    />
                }
                .into_any()
            }
            _ => view! { <AccountScreen runtime id card/> }.into_any(),
        },
        Route::Lists => view! {
            <ListsScreen runtime card picked=None/>
        }
        .into_any(),
        Route::List(id) => view! {
            <ListsScreen runtime card picked=Some(id)/>
        }
        .into_any(),
        Route::More => {
            let path = runtime.last_path().unwrap_or_else(|| route.path());
            let navigate = Some(Callback::new(move |path: String| runtime.navigate(path)));
            view! {
                <crate::route::screens::more::MoreScreen path=path on_navigate=navigate/>
            }
            .into_any()
        }
        Route::Mutes => view! {
            <PeopleList runtime id=String::new() kind=crate::pages::people::Kind::Mutes/>
        }
        .into_any(),
        Route::Blocks => view! {
            <PeopleList runtime id=String::new() kind=crate::pages::people::Kind::Blocks/>
        }
        .into_any(),
        Route::Feed => {
            let header = header.or_else(|| {
                let header: ChildrenFn = Arc::new(|| {
                    view! {
                        <crate::pages::compose::inline::preview::ComposerPreview
                            active=Signal::derive(|| "/home/feed".to_owned())
                        />
                    }
                    .into_any()
                });
                Some(header)
            });
            view! {
                <FeedScreen runtime card header/>
            }
            .into_any()
        }
        Route::Compose | Route::ComposeEdit(_) => {
            let edit = match route {
                Route::ComposeEdit(id) => Some(id),
                _ => None,
            };
            view! {
                <ComposeScreen runtime edit/>
            }
            .into_any()
        }
        Route::Settings(section) => view! {
            <SettingsScreen runtime section=crate::pages::settings::nav::SettingsSection::of(&section)/>
        }
        .into_any(),
        Route::Admin(section) => view! {
            <AdminScreen runtime section/>
        }
        .into_any(),
        Route::Moderation(section) => view! {
            <ModerationScreen runtime section/>
        }
        .into_any(),
        // The drive is the account's own storage, read with a token the stateless render
        // does not have. The address still answers: a link to `/drive` says where it leads
        // rather than falling back to the feed.
        Route::Drive => view! {
            <crate::Screen>
                <crate::PageHeader
                    title="Drive"
                    description="Your own files, in folders only you can see."
                />
                <crate::Note>
                    "The drive is read with your session, which this render does not have. 
                     Open this address in the app to see your files."
                </crate::Note>
            </crate::Screen>
        }
        .into_any(),
        other @ Route::OauthCallback => {
            let path = other.path();
            view! {
                <crate::Screen>
                    <crate::PageHeader title="Not available" description=path.clone()/>
                    <crate::Note>
                        "This screen is not available from a stateless render."
                    </crate::Note>
                </crate::Screen>
            }
            .into_any()
        }
    }
}

/// The surface a route cannot be drawn without, when it cannot.
///
/// A route outside this list is one every server answers — a timeline, a profile, a thread,
/// the composer — so it is never gated. Gating one of those would hide a page that works.
fn surface_of(route: &Route) -> Option<nodus_ui::capabilities::Surface> {
    use nodus_ui::capabilities::Surface;
    match route {
        Route::Feed => Some(Surface::Feed),
        Route::Admin(_) => Some(Surface::Admin),
        Route::Moderation(_) => Some(Surface::Moderation),
        Route::Drive => Some(Surface::Drive),
        Route::Lists | Route::List(_) => Some(Surface::Lists),
        Route::Saved(shelf) => match shelf.as_str() {
            "bookmarks" => Some(Surface::Bookmarks),
            _ => Some(Surface::Favourites),
        },
        Route::Scheduled => Some(Surface::Scheduled),
        Route::Notifications => Some(Surface::Notifications),
        Route::Search => Some(Surface::Search),
        Route::Directory => Some(Surface::Directory),
        Route::Messages => Some(Surface::Conversations),
        _ => None,
    }
}

#[component]
pub(crate) fn TimelineScreen(
    runtime: Arc<dyn Runtime>,
    kind: TimelineKind,
    card: CardRenderer,
    #[prop(optional_no_strip)] header: Option<ChildrenFn>,
    #[prop(optional_no_strip)] banner: Option<ChildrenFn>,
    #[prop(optional_no_strip)] tabs: Option<ChildrenFn>,
) -> impl IntoView {
    let revision = runtime.revision();
    let initial = runtime.timeline(&kind);
    let loading = RwSignal::new(!initial.held && !initial.unavailable);
    let in_flight = RwSignal::new(false);
    let error = RwSignal::new(
        initial
            .unavailable
            .then(|| "This timeline is not available without the app.".to_owned()),
    );
    let alive = Alive::new();

    let data = Signal::derive({
        let runtime = Arc::clone(&runtime);
        let kind = kind.clone();
        move || {
            revision.get();
            runtime.timeline(&kind)
        }
    });
    let items = Signal::derive(move || data.get().items);
    let more = Signal::derive(move || data.get().cursor.is_some());

    let start = {
        let runtime = Arc::clone(&runtime);
        let kind = kind.clone();
        move |cursor: Option<String>| {
            if !alive.is_alive() || in_flight.get_untracked() {
                return;
            }
            in_flight.set(true);
            error.set(None);
            loading.set(true);
            let done = Callback::new(move |result: Result<(), String>| {
                in_flight.set(false);
                alive.if_alive(|| {
                    loading.set(false);
                    if let Err(message) = result {
                        error.set(Some(message));
                    }
                });
            });
            runtime.load_timeline(kind.clone(), cursor, done);
        }
    };

    {
        let start = start.clone();
        Effect::new(move |_| {
            if !data.get_untracked().held && !data.get_untracked().unavailable {
                start(None);
            }
        });
    }

    let on_more = Callback::new({
        let start = start.clone();
        move |()| start(data.get_untracked().cursor)
    });
    let auto_more = runtime.infinite_more(
        Signal::derive(move || more.get() && !loading.get()),
        on_more,
    );
    let (title, description, empty_note) = copy(&kind);
    let rows: ChildrenFn = std::sync::Arc::new(move || {
        let card = std::sync::Arc::clone(&card);
        view! {
            <crate::pages::timeline::StatusList
                items=items
                card=card
                loading=loading
                more=more
                on_more=Some(on_more)
                auto_more=auto_more.clone()
                show_empty=false
            />
        }
        .into_any()
    });

    view! {
        <crate::pages::feed::FeedPage
            title=title
            description=description
            empty_note=empty_note
            header=header
            banner=banner
            tabs=tabs
            error=Signal::derive(move || error.get())
            loading=loading
            has_items=Signal::derive(move || !items.get().is_empty())
        >
            {rows()}
        </crate::pages::feed::FeedPage>
    }
}

fn copy(kind: &TimelineKind) -> (String, String, String) {
    match kind {
        TimelineKind::Home => (
            "Home".to_owned(),
            "Posts from the people you follow.".to_owned(),
            "The people you follow have not posted yet.".to_owned(),
        ),
        TimelineKind::Local => (
            "Local".to_owned(),
            "Posts from this instance.".to_owned(),
            "No one on this instance has posted yet.".to_owned(),
        ),
        TimelineKind::Federated => (
            "Federated".to_owned(),
            "Posts from instances this one knows about.".to_owned(),
            "No federated posts have arrived yet.".to_owned(),
        ),
        TimelineKind::Account(_) => (
            "Posts".to_owned(),
            "Posts from this account.".to_owned(),
            "This account has not posted yet.".to_owned(),
        ),
        TimelineKind::List(_) => (
            "List".to_owned(),
            "Posts from the accounts on this list.".to_owned(),
            "Nobody on this list has posted yet.".to_owned(),
        ),
        TimelineKind::Tag(name) => (
            format!("#{name}"),
            format!("Posts tagged #{name}."),
            format!("Nobody has posted with #{name} yet."),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every destination the chrome offers is an address, not client state: the rail, the
    /// settings strip and the editor all have to survive a reload and a shared link.
    #[test]
    fn every_destination_is_its_own_address() {
        let mut hrefs: Vec<&str> = Vec::new();
        for group in nodus_ui::components::rail::rail() {
            hrefs.extend(
                group
                    .destinations
                    .iter()
                    .map(|destination| destination.href),
            );
        }
        for section in SettingsSection::ALL {
            let path = Route::Settings(section.slug().to_owned()).path();
            // Owned strings cannot be borrowed into the list above, so they are checked
            // where they are made: the strip builds exactly this path.
            assert_eq!(Route::from_path(&path).path(), path);
        }
        hrefs.push(Route::StyleEditor.path().leak());
        for href in hrefs {
            let route = Route::from_path(href);
            assert_eq!(route.path(), href, "{href} does not parse to itself");
        }
    }

    #[test]
    fn action_result_carries_voted_poll() {
        let poll = Poll {
            id: "poll-1".to_owned(),
            expires_at: None,
            expired: false,
            multiple: false,
            votes_count: 1,
            voters_count: Some(1),
            voted: true,
            own_votes: vec![0],
            options: vec![],
            emojis: vec![],
        };
        let result = ActionResult::Poll(poll.clone());
        assert!(matches!(result, ActionResult::Poll(returned) if returned == poll));
    }
}
