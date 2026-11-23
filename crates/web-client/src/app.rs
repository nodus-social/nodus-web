//! The application shell: the rail, the screen the address bar asks for, and the chrome
//! around both.
//!
//! Composition only: every screen comes from `crate::routes`, the chrome from `nodus-view`
//! or `crate::shell`.

use crate::router::{self, Route};
use crate::routes::callback::Callback;
use crate::routes::home::{FederatedTimeline, HomeTimeline, LocalTimeline};
use crate::routes::notifications::NotificationsView;
use crate::routes::sign_in::SignIn;
use crate::shell;
use crate::state::bridge::{init, revision, with_sdk};
use nodus_web::shared::screens::Screen;

use leptos::prelude::*;
use nodus_ui::components::CardRenderer;
#[cfg(feature = "admin")]
use nodus_ui::icons::Icon;

/// Application shell: the rail, and the screen the address bar asks for.
#[component]
#[expect(
    clippy::too_many_lines,
    reason = "one arm per route: the match is the router, and splitting it hides a missing screen"
)]
pub(crate) fn App() -> impl IntoView {
    init();
    // Refreshes arrive on the SDK's own schedule; the views only render them.
    // Keeps the address bar and the back button in step with the rail.
    router::install();

    let revision = revision();
    // The server paints the title for the page it thinks it is serving, which — for any
    // screen behind a session — is the signed-out one, so a signed-in reader would see
    // "Sign in" in the tab for the whole session. The page half comes from the route table
    // the two paints share; the instance half is whatever the server wrote, kept as it is.
    Effect::new(move |_| {
        let path = router::route().get().path();
        let Some(screen) = Screen::of_path(&path) else {
            return;
        };
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let instance = document
            .title()
            .rsplit_once('\u{b7}')
            .map_or_else(|| "Nodus".to_owned(), |(_, name)| name.trim().to_owned());
        document.set_title(&screen.document_title(&instance));
    });
    let route = router::route();
    Effect::new(move |_| crate::state::runtime::remember_path(&route.get().path()));
    // A `Memo`, not a closure: the screen match below reads this, and a plain closure
    // would re-run it on every state notification — rebuilding the current screen
    // (disposing its owner, dropping its state, re-firing its effects) each time the
    // background tick or any SDK write bumped `revision`. A memo only notifies when the
    // *value* changes, which is exactly when the screen has to change too.
    let signed_in = Memo::new(move |_| {
        revision.get();
        with_sdk(|sdk| sdk.state().session.get().is_some())
    });
    // The recommendation, styling and staff surfaces are ours; a Mastodon account has no
    // server behind them. The route arms below and the rail read the same answer.
    let extensions = crate::state::bridge::supports_extensions();
    // The staff card is not painted until a door survives its gate, and a component that is
    // not painted cannot run the probe that would open one. Asking here, in the shell that
    // every screen paints, is what keeps the question from depending on its own answer.
    #[cfg(feature = "admin")]
    crate::state::bridge::admin::ensure_standing();
    // The negotiated answer, put in scope for the shared screens: the rail, the screens and
    // their explanations all read this one value instead of a per-call argument, so the
    // browser and the server paint from the same rule (the server simply has none to read).
    let serves = crate::state::bridge::capabilities::serves();
    nodus_ui::capabilities::provide_serves(serves);
    // Each surface this build draws, answered by the SDK rather than by the raw list: a
    // Mastodon or Misskey server publishes no `features` at all, and its baseline is what
    // says it serves administration, moderation and the drive.
    use nodus_ui::capabilities::Surface;
    let feed_accounts = crate::state::bridge::capabilities::served(Surface::Feed);
    let admin_on = crate::state::bridge::capabilities::served(Surface::Admin);
    let moderation_on = crate::state::bridge::capabilities::served(Surface::Moderation);
    // `/api/v1/meta` does publish a `drive` feature (`drive`, and beside it `drive_share` and
    // `drive_attach`), so the door follows that name and falls back to the grant the session
    // already carries — an older instance, or one whose answer has not arrived yet, still has
    // the user's scopes to go on. The stored session is `read write follow` (see
    // `session::scopes`), so `read` is the wider grant a drive-readable token carries; a token
    // without `read:drive` still gets the server's 403, which the screen shows rather than hides.
    let drive_feature = crate::state::bridge::capabilities::served(Surface::Drive);
    let drive_on =
        Memo::new(move |_| drive_feature.get() || crate::state::bridge::drive::may_read());

    // Who is signed in, as the rail card shows them. Held here rather than read from the
    // store, because the store only holds accounts some page has already drawn: the reader's
    // own profile is otherwise fetched by the screen that shows it.
    let profile = RwSignal::new(None::<nodus_ui::components::rail::ProfileCardData>);

    // One fetch per session: `asked` remembers which account the rail has already asked
    // about, so a revision that changes nothing does not become another request.
    let asked = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        revision.get();
        let Some(session) = with_sdk(|sdk| sdk.state().session.get()) else {
            profile.set(None);
            return;
        };
        let stored = with_sdk(|sdk| sdk.read_account(&session.account_id));
        if let Some(account) = stored {
            profile.set(Some(profile_card(&account, &session.host)));
            return;
        }
        if asked.get_untracked().as_deref() == Some(session.account_id.as_str()) {
            return;
        }
        asked.set(Some(session.account_id.clone()));
        let host = session.host.clone();
        let account_id = session.account_id.clone();
        leptos::task::spawn_local(async move {
            use crate::state::bridge::{actions, dispatch};
            let Ok(account) = actions::account(&account_id).await else {
                return;
            };
            // Uppercase the account so every screen that shows this reader sees one copy
            // of them; the rail card is simply the first reader of that fact.
            dispatch(nodus_client_sdk::Command::AccountsUpserted {
                accounts: vec![account.clone()],
            });
            profile.set(Some(profile_card(&account, &host)));
        });
    });

    let card: CardRenderer = std::sync::Arc::new(|status| {
        view! { <crate::routes::status::StatusCard status/> }.into_any()
    });

    // The cards are a read, so they fill once per document and then only on refresh: a rail
    // that re-asked on every navigation would be a request per click for something that
    // changes slower than a conversation does.
    Effect::new(move |_| {
        if signed_in.get() {
            crate::state::bridge::sidebar::ensure_loaded();
        }
    });

    // Notifications are the rail's badge and a page of their own, so they are read once per
    // document, like the rail's cards: a badge that re-asked on every click would cost a
    // request per navigation to keep a number that changes when someone else acts.
    Effect::new(move |_| {
        if signed_in.get() {
            crate::state::bridge::notifications::ensure_loaded();
        }
    });

    // The live stream: the instance can push what happens, and without this the rail's badge
    // and the notifications page learned about it only on their next fetch.
    //
    // Gated on the *account's* instance being ours: a Mastodon server streams over a
    // WebSocket on the same path, and an `EventSource` pointed at it would sit in silence
    // while claiming to be live. Such an account keeps polling, which works.
    //
    // The route is read so this re-runs on every navigation: entering the shell starts the
    // stream, leaving it (a signed-out screen) stops it, and a navigation while it is open
    // must reuse it rather than open a second one — `stream::start` is what guarantees that.
    Effect::new(move |_| {
        let _ = route.get();
        if signed_in.get() && extensions.get() {
            crate::state::bridge::stream::start();
        } else {
            // Signed out, or an instance that does not speak this protocol: no stream to
            // keep, and a retry already sleeping must not wake up and reopen one.
            crate::state::bridge::stream::stop();
        }
    });

    // The door's state, not its count, is what the rail branches on: an instance that
    // answers 404 to the endpoint has no notifications to offer, and a reader should not
    // be shown a door that opens onto an error.
    let notice_door = Signal::derive(move || {
        let notices = crate::state::bridge::notifications::available().get();
        match notices {
            None => nodus_ui::NoticeDoor::Unknown,
            Some(false) => nodus_ui::NoticeDoor::Absent,
            Some(true) => {
                nodus_ui::NoticeDoor::Unread(crate::state::bridge::notifications::unread().get())
            }
        }
    });

    // The editor is a page, so the address decides whether it is open: arriving starts a
    // draft of the style the site is showing, and every other address throws the draft
    // away. A mode that a back button can leave while the mode itself stays on would be a
    // page that lies about where the reader is.
    Effect::new(move |_| {
        if route.get() == Route::StyleEditor && extensions.get() {
            crate::state::bridge::style::begin_edit();
        } else {
            crate::state::bridge::style::cancel_edit();
        }
    });

    view! {
        <nodus_ui::Frame
            signed_in=Signal::derive(move || signed_in.get())
            path=Signal::derive(move || router::route().get().path())
            extra=extra_doors()
            on_home=Callback::new(|()| router::navigate(Route::Home))
            // While the editor is open the shell paints the draft, so the page being
            // arranged is the page itself, not a copy of it in a settings card.
            style=Signal::derive(move || {
                if crate::state::bridge::style::editing().get() {
                    crate::state::bridge::style::draft().get()
                } else {
                    crate::state::bridge::style::active_style_signal().get()
                }
            })
            profile=Signal::derive(move || profile.get())
            // The rail's cards read the stores the rest of the app already filled; the
            // first visit asks for the two the site itself does not need.
            sidebar=Signal::derive(move || crate::state::bridge::sidebar::data())
            editor=Some(crate::state::bridge::style::editor())
            on_save=Some(Callback::new(|()| {
                leptos::task::spawn_local(async {
                    // Back to the screen the editor was opened from, and only once the
                    // arrangement is really stored: a save that failed leaves the reader
                    // in the editor with the draft still on screen.
                    if crate::state::bridge::style::save_edit().await.is_ok() {
                        router::navigate(Route::Settings(
                            nodus_pages::pages::settings::nav::SettingsSection::Account
                                .slug()
                                .to_owned(),
                        ));
                    }
                });
            }))
            on_cancel=Some(Callback::new(|()| {
                crate::state::bridge::style::cancel_edit();
                router::navigate(Route::Home);
            }))
            // Asking for a picture is the browser's job; the draft is ours. The callback
            // carries where the picture goes: an empty column wants one added, an open
            // picture editor wants the one in its place replaced.
            on_pick=Some(Callback::new(|target: nodus_ui::session_style::PictureTarget| {
                crate::state::bridge::style::pick_picture_for(target);
            }))
            notice=Some(Signal::derive(move || crate::state::bridge::style::notice().get()))
            notices=Some(notice_door)
            inner=std::sync::Arc::new(move || {
                        // The callback comes first, before the signed-in gate: the
                        // instance redirects here while the app is still signed out, and
                        // this screen is what turns the code into a session.
                        //
                        // Everything the API serves needs a token, so a signed-out
                        // visitor gets the same answer on every gated route: what the
                        // click was for, or the rail reads as broken.
                        let signed_in = signed_in.get();
                        match route.get() {
                            Route::OauthCallback => view! { <Callback/> }.into_any(),
                            Route::Feed if signed_in && extensions.get() && feed_accounts.get() => {
                                let header: ChildrenFn = std::sync::Arc::new(|| {
                                    view! { <crate::routes::home::TimelineHeader/> }.into_any()
                                });
                                view! {
                                    <nodus_route::route::RouteView
                                        runtime=crate::state::runtime::runtime()
                                        route=Route::Feed
                                        card=card.clone()
                                        header=Some(header)
                                    />
                                }.into_any()
                            },
                            Route::Feed if signed_in => unavailable(
                                "Recommended posts",
                                "This instance has no recommendation engine, so there is nothing \
                                 to rank here. Your home timeline still works.",
                            ),
                            Route::Home if signed_in => view! { <HomeTimeline/> }.into_any(),
                            // Local and Federated are the same screen as Home: the store, the
                            // paging cursor and the caption all come from the timeline kind. Their
                            // own arms exist because the screen is also the header block — the
                            // composer and the switch between timelines — and the shared route view
                            // takes that as a slot the client fills. Without these arms the reader
                            // who clicks Local lands on a page with no tab strip and cannot leave.
                            Route::Local if signed_in => view! { <LocalTimeline/> }.into_any(),
                            Route::Federated if signed_in => {
                                view! { <FederatedTimeline/> }.into_any()
                            }
                            // The middle of the editor is not being edited: the rail and the
                            // banners are. So it is the dumping ground — every card that
                            // exists, with the ones on the rail marked, one click from either
                            // side.
                            Route::StyleEditor if signed_in && extensions.get() => {
                                let has_staff = extra_doors().is_some();
                                view! {
                                    <nodus_ui::components::sidebar::ModuleTray
                                        layout=Signal::derive(move || {
                                            crate::state::bridge::style::draft().get().modules
                                        })
                                        nav=Signal::derive(move || {
                                            crate::state::bridge::style::draft().get().nav
                                        })
                                        staff_rail=Signal::derive(move || {
                                            crate::state::bridge::style::draft().get().staff_rail
                                        })
                                        has_staff=has_staff
                                        editor=Some(crate::state::bridge::style::editor())
                                    />
                                    <nodus_ui::components::sidebar::MediaTray
                                        pictures=Signal::derive(move || {
                                            crate::state::bridge::style::library().get()
                                        })
                                        status=Signal::derive(move || {
                                            crate::state::bridge::style::library_status().get()
                                        })
                                        editor=Some(crate::state::bridge::style::editor())
                                        on_upload=Some(Callback::new(
                                            |request: nodus_ui::session_style::UploadRequest| {
                                                crate::state::bridge::style::upload_to_library(
                                                    request,
                                                );
                                            },
                                        ))
                                        on_upload_click=Some(Callback::new(|()| {
                                            crate::state::bridge::style::pick_picture_for(
                                                nodus_ui::session_style::PictureTarget::Region(
                                                    nodus_ui::session_style::BannerRegionKind::Top,
                                                ),
                                            );
                                        }))
                                    />
                                }
                                .into_any()
                            }
                            Route::StyleEditor if signed_in => unavailable(
                                "Session styling",
                                "Styling is served by Nodus, and this instance does not offer \
                                 it. The rest of your settings still work.",
                            ),
                            Route::StyleEditor => {
                                view! { <SignIn note="Sign in to arrange your page."/> }
                                    .into_any()
                            }
                            // The drive is the account's own storage: it needs a session to
                            // name whose drive it is, and a grant that can read it.
                            Route::Drive if !signed_in => {
                                view! { <SignIn note="Sign in to use the drive."/> }.into_any()
                            }
                            Route::Drive if !drive_on.get() => unavailable(
                                "Drive",
                                "The drive is served by Nodus, and this session's grant does not \
                                 include it. Signing in again asks the instance for the drive \
                                 scopes.",
                            ),
                            Route::Drive => {
                                view! { <crate::routes::drive::DriveScreen/> }.into_any()
                            }
                            // The console itself is drawn by `RouteView`, which knows each
                            // section; only a signed-out visitor is sent to sign in.
                            Route::Admin(_) | Route::Moderation(_) if !signed_in => {
                                view! { <SignIn note="Sign in to moderate."/> }.into_any()
                            }
                            Route::Admin(_) if !extensions.get() || !admin_on.get() => {
                                unavailable(
                                    "Administration",
                                    "The console is served by Nodus, and this instance's \
                                     /api/v1/meta answer does not name the admin capability. On \
                                     Mastodon it lives in the instance's own web interface.",
                                )
                            }
                            Route::Moderation(_) if !extensions.get() || !moderation_on.get() => {
                                unavailable(
                                    "Moderation",
                                    "Moderation is served by Nodus, and this instance's \
                                     /api/v1/meta answer does not name the moderation \
                                     capability. On Mastodon it lives in the instance's own web \
                                     interface.",
                                )
                            }
                            // The reader's own notifications, drawn by the client because
                            // half of a row is this instance's moderation payload and not
                            // anything the SDK's `Notification` knows about.
                            Route::Notifications if signed_in => {
                                view! { <NotificationsView/> }.into_any()
                            }
                            // A shared link works without a session: the address names the
                            // host, so the post is read from there anonymously. A legacy
                            // `/status/{id}` link falls back to the instance this browser
                            // already used, or asks which one.
                            Route::Thread { host, id } => {
                                let host = if host.is_empty() {
                                    crate::state::bridge::session::remembered_host()
                                        .unwrap_or_default()
                                } else {
                                    host
                                };
                                view! {
                                    <nodus_route::route::RouteView
                                        runtime=crate::state::runtime::runtime()
                                        route=Route::Thread { host, id }
                                        card=card.clone()
                                    />
                                }.into_any()
                            }
                            Route::Account(id) if id.contains('/') => view! {
                                <nodus_route::route::RouteView
                                    runtime=crate::state::runtime::runtime()
                                    route=Route::Account(id)
                                    card=card.clone()
                                />
                            }
                            .into_any(),
                            // A bare handle means "here": the instance the reader is signed
                            // in at, then the one this deployment names, then the address
                            // they typed. An id alone cannot be read without knowing which
                            // instance minted it, so signing in is the honest fallback.
                            Route::Account(id) => match crate::state::bridge::session::remembered_host()
                                .or_else(|| crate::state::runtime::runtime().public_host())
                                .or_else(|| {
                                    web_sys::window()
                                        .and_then(|window| window.location().host().ok())
                                        .filter(|host| !host.trim().is_empty())
                                })
                            {
                                Some(host) => view! {
                                    <nodus_route::route::RouteView
                                        runtime=crate::state::runtime::runtime()
                                        route=Route::Account(format!("{host}/{id}"))
                                        card=card.clone()
                                    />
                                }
                                .into_any(),
                                None => {
                                    view! { <SignIn note="Sign in to read this."/> }.into_any()
                                }
                            }
                            // Every remaining screen is the shared route view, which already
                            // knows its own title, header and paging. Signing in is the only
                            // thing it cannot do for itself.
                            route if signed_in => view! {
                                <nodus_route::route::RouteView
                                    runtime=crate::state::runtime::runtime()
                                    route=route
                                    card=card.clone()
                                />
                            }
                            .into_any(),
                            _ => view! { <SignIn note="Sign in to read this."/> }.into_any(),
                        }
            })/>
        <Show when=move || signed_in.get()>
            <shell::MobileBar/>
        </Show>
        <crate::media::ImageModal/>
        <div class="hidden xl:block">
            <nodus_ui::AppFooter host=Signal::derive(move || {
                with_sdk(|sdk| sdk.state().session.get().map(|session| session.host))
            })/>
        </div>
    }
}

/// A route the account's instance cannot serve.
///
/// Named rather than blank: a reader arriving from a link needs to know which screen they
/// asked for, why it is not here, and where they can go instead — the same shape the shared
/// route view paints for a screen the stateless render cannot serve.
fn unavailable(title: &'static str, note: &'static str) -> AnyView {
    view! {
        <nodus_ui::Screen>
            <nodus_ui::PageHeader title=title description=note/>
            <nodus_ui::Note>
                <a class="link link-hover" href=Route::Home.path()>
                    "Go to the home timeline"
                </a>
            </nodus_ui::Note>
        </nodus_ui::Screen>
    }
    .into_any()
}

/// The doors only a moderator build draws, as the shared frame wants them.
///
/// The three facts a staff door needs — this protocol's extensions, the instance's answer
/// for the surface, and a role this account holds — are read here rather than inside the
/// card, so the frame knows whether to draw the heading from the same answer that picks
/// the rows. A card handed only its rows could not tell "no doors" from "not asked yet".
#[cfg(feature = "admin")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the lean build returns nothing, and one call site serves both"
)]
fn extra_doors() -> Option<nodus_ui::ExtraDoors> {
    use nodus_ui::capabilities::{Surface, staff_door};

    let extensions = crate::state::bridge::supports_extensions();
    let eligible = crate::state::bridge::admin::may_enter();
    let moderation_served = crate::state::bridge::capabilities::served(Surface::Moderation);
    let administration_served = crate::state::bridge::capabilities::served(Surface::Admin);
    let moderation = Signal::derive(move || {
        staff_door(extensions.get(), moderation_served.get(), eligible.get())
    });
    let administration = Signal::derive(move || {
        staff_door(
            extensions.get(),
            administration_served.get(),
            eligible.get(),
        )
    });
    Some(nodus_ui::ExtraDoors {
        present: Signal::derive(move || moderation.get() || administration.get()),
        rows: std::sync::Arc::new(move || staff_rail_items(moderation, administration).into_any()),
    })
}

/// The lean build has no moderation screens, so the rail has no extra doors.
#[cfg(not(feature = "admin"))]
fn extra_doors() -> Option<nodus_ui::ExtraDoors> {
    None
}

/// Whether two routes name the same rail section, even when they choose different subsections.
#[cfg(feature = "admin")]
fn same_section(current: &Route, target: &Route) -> bool {
    match (current, target) {
        (Route::Admin(_), Route::Admin(_)) | (Route::Moderation(_), Route::Moderation(_)) => true,
        _ => current == target,
    }
}

/// One entry in the rail: the class that marks the current screen, and a click that
/// keeps the address bar in step without a full page load.
///
/// `route` is owned and cloned twice because the active check and the click handler are
/// two closures; a route with an argument cannot be `Copy`, and leaking one to avoid the
/// clones would be a worse trade.
#[cfg(feature = "admin")]
#[component]
fn RailItem(label: &'static str, icon: Icon, hint: &'static str, route: Route) -> impl IntoView {
    let current = router::route();
    let marker = route.clone();
    let target = route;
    let path = target.path();

    view! {
        <nodus_ui::RailEntry
            label=label
            hint=hint
            href=path
            icon=icon
            active=Signal::derive(move || same_section(&current.get(), &marker))
            on_click=Some(Callback::new(move |()| router::navigate(target.clone())))
        />
    }
}

/// The moderation doors, in a build that carries the screens.
///
/// Both the instance and the account have to agree before a door is drawn. The instance
/// says which areas it serves at all (`moderation`, `admin` in the `/api/v1/meta` features),
/// and `admin::may_enter()` says whether this account could ever enter them: a rail entry
/// that leads to a refusal is a dead end, and painting one for a probe still in flight — or
/// for an account with no role evidence — would promise a door nobody can open.
///
/// Each door is already decided by `extra_doors`; this only paints the ones it let through,
/// so the heading and its rows cannot disagree.
#[cfg(feature = "admin")]
fn staff_rail_items(moderation: Signal<bool>, administration: Signal<bool>) -> impl IntoView {
    view! { <StaffRail moderation=moderation administration=administration/> }
}

/// The staff doors themselves, one per signal the shell gated.
#[cfg(feature = "admin")]
#[component]
fn StaffRail(moderation: Signal<bool>, administration: Signal<bool>) -> impl IntoView {
    view! {
        <Show when=move || moderation.get()>
            <RailItem
                label="Moderation"
                icon=Icon::Shield
                hint="Reports, accounts, registrations and blocked domains"
                route=Route::Moderation(crate::router::MODERATION_DEFAULT.to_owned())
            />
        </Show>
        <Show when=move || administration.get()>
            <RailItem
                label="Administration"
                icon=Icon::Globe
                hint="The dashboard, allowed domains and announcements"
                route=Route::Admin(crate::router::ADMIN_DEFAULT.to_owned())
            />
        </Show>
    }
}

/// The rail's picture of the signed-in reader.
///
/// A named function rather than a closure: the effect uses it twice (from the store when the
/// account is already there, and after the one fetch that puts it there), and a card built
/// two different ways is how the two paths drift apart.
fn profile_card(
    account: &nodus_client_core::model::Account,
    host: &str,
) -> nodus_ui::components::rail::ProfileCardData {
    let display_name = if account.display_name.trim().is_empty() {
        account.acct.clone()
    } else {
        account.display_name.clone()
    };
    nodus_ui::components::rail::ProfileCardData {
        display_name,
        handle: format!("@{}@{}", account.acct, host),
        avatar_url: account.avatar.clone(),
        // The rail outlives a session switch, so it must name the account the way another
        // instance understands it, not by the numbering of whichever instance is open now.
        href: nodus_web::shared::handles::profile_path_for(&account.acct, Some(host)),
    }
}
