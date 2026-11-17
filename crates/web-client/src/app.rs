//! The application shell: the rail, the screen the address bar asks for, and the chrome
//! around both.
//!
//! Composition only: every screen comes from `crate::routes`, the chrome from `nodus-view`
//! or `crate::shell`.

use crate::router::{self, Route};
use crate::routes::callback::Callback;
use crate::routes::home::HomeTimeline;
use crate::routes::sign_in::SignIn;
use crate::shell;
use crate::state::bridge::{init, revision, with_sdk};

use leptos::prelude::*;
#[cfg(feature = "admin")]
use leptos::task::spawn_local;
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

    // The editor is a page, so the address decides whether it is open: arriving starts a
    // draft of the style the site is showing, and every other address throws the draft
    // away. A mode that a back button can leave while the mode itself stays on would be a
    // page that lies about where the reader is.
    Effect::new(move |_| {
        if route.get() == Route::StyleEditor {
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
            // Asking for a picture is the browser's job; the draft is ours. This keeps the
            // reader in the editor: the empty side column becomes the picture they chose.
            on_pick=Some(Callback::new(|kind: nodus_ui::session_style::BannerRegionKind| {
                crate::state::bridge::style::pick_picture_for(kind);
            }))
            notice=Some(Signal::derive(move || crate::state::bridge::style::notice().get()))
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
                            Route::Feed if signed_in => {
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
                            Route::Home if signed_in => view! { <HomeTimeline/> }.into_any(),
                            // The middle of the editor is not being edited: the rail and the
                            // banners are. So it is the dumping ground — every card that
                            // exists, with the ones on the rail marked, one click from either
                            // side.
                            Route::StyleEditor if signed_in => {
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
                                }
                                .into_any()
                            }
                            Route::StyleEditor => {
                                view! { <SignIn note="Sign in to arrange your page."/> }
                                    .into_any()
                            }
                            // The console itself is drawn by `RouteView`, which knows each
                            // section; only a signed-out visitor is sent to sign in.
                            Route::Admin(_) | Route::Moderation(_) if !signed_in => {
                                view! { <SignIn note="Sign in to moderate."/> }.into_any()
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

/// The doors only a moderator build draws, as the shared frame wants them.
#[cfg(feature = "admin")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the lean build returns nothing, and one call site serves both"
)]
fn extra_doors() -> Option<nodus_ui::ExtraDoors> {
    Some(std::sync::Arc::new(|| staff_rail_items().into_any()))
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
/// Shown only once the instance has answered that this account may moderate: a rail entry
/// that leads to "you are not a moderator" is a dead end, and `admin::staff()` is the only
/// honest source for that answer. The check re-runs when the session or the grant changes,
/// because both can arrive after the first paint.
#[cfg(feature = "admin")]
fn staff_rail_items() -> impl IntoView {
    view! { <StaffRail/> }
}

/// The probe behind the moderation doors.
#[cfg(feature = "admin")]
#[component]
fn StaffRail() -> impl IntoView {
    let standing = RwSignal::new(None);
    Effect::new(move |_| {
        revision().get();
        spawn_local(async move {
            let answer = crate::state::bridge::admin::standing().await;
            standing.set(Some(answer));
        });
    });

    view! {
        <Show when=move || !matches!(standing.get(), Some(crate::state::bridge::admin::Staff::No(_)))>
            <RailItem
                label="Moderation"
                icon=Icon::Shield
                hint="Reports, accounts, registrations and blocked domains"
                route=Route::Moderation(crate::router::MODERATION_DEFAULT.to_owned())
            />
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
        href: Route::Account(account.id.clone()).path(),
    }
}
