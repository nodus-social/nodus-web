//! The application shell: the rail, the screen the address bar asks for, and the chrome
//! around both.
//!
//! Composition only: every screen comes from `crate::routes`, the chrome from `nodus-view`
//! or `crate::shell`.

use crate::router::{self, Route};
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
    let card: CardRenderer = std::sync::Arc::new(|status| {
        view! { <crate::routes::status::StatusCard status/> }.into_any()
    });
    // The route table, taken once: the adopted root calls the same function, so a screen a
    // click reaches is the same one in both roots. `signed_in` and `extensions` are read out
    // again because the effects below use them too.
    let screens = crate::dispatch::Screens::current(card);
    let signed_in = screens.signed_in;
    let extensions = crate::state::bridge::supports_extensions();

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
                // One match, in `crate::dispatch`: the adopted shell root draws the same
                // arms once the reader navigates off the screen the document painted.
                screens.draw(route.get())
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
pub(crate) fn extra_doors() -> Option<nodus_ui::ExtraDoors> {
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
pub(crate) fn extra_doors() -> Option<nodus_ui::ExtraDoors> {
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
        avatar_shape: account.avatar_shape(),
        // The rail outlives a session switch, so it must name the account the way another
        // instance understands it, not by the numbering of whichever instance is open now.
        href: nodus_web::shared::handles::profile_path_for(&account.acct, Some(host)),
    }
}
