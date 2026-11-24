//! The routed screen: one `Route` match, drawn by every root.
//!
//! Two roots can own a document — [`crate::app::App`], which mounts its own tree, and
//! [`crate::shell_root::ShellRoot`], which adopts the shell the host painted. A link a reader
//! clicks must land on the same screen in both, so the match lives here once: a root supplies
//! the answers it reads (the session, the instance's capabilities, the card renderer) and gets
//! the screen back. A second copy of this list is a screen that exists in one root and not the
//! other, which is exactly what a navigation inside the adopted shell used to do.

use std::sync::Arc;

use leptos::prelude::*;
use nodus_ui::components::CardRenderer;

use crate::router::Route;
use crate::routes::callback::Callback;
use crate::routes::home::{FederatedTimeline, HomeTimeline, LocalTimeline};
use crate::routes::notifications::NotificationsView;
use crate::routes::sign_in::SignIn;

/// Every fact the `Route` match reads, answered once by the root that draws it.
///
/// Held together so a root cannot answer three of the four: the guards and the rail's doors
/// then read the same signals, and a screen that changes its mind about a surface changes it
/// in every place at once.
#[derive(Clone)]
pub(crate) struct Screens {
    /// Who is signed in. A `Memo`, not a closure: the match reads this, and a plain closure
    /// would re-run it on every state notification — rebuilding the current screen (disposing
    /// its owner, dropping its state, re-firing its effects) each time the background tick or
    /// any SDK write bumped `revision`. A memo only notifies when the *value* changes, which is
    /// exactly when the screen has to change too.
    pub(crate) signed_in: Memo<bool>,
    /// The recommendation, styling and staff surfaces are ours; a Mastodon account has no
    /// server behind them. The route arms below and the rail read the same answer.
    extensions: Signal<bool>,
    feed_accounts: Signal<bool>,
    admin_on: Signal<bool>,
    moderation_on: Signal<bool>,
    drive_on: Memo<bool>,
    card: CardRenderer,
}

impl Screens {
    /// Asks the services the root has already installed (`bridge::init`, `router::install`)
    /// for every answer the match reads, and puts the negotiated capabilities in scope for
    /// the shared screens.
    pub(crate) fn current(card: CardRenderer) -> Self {
        let revision = crate::state::bridge::revision();
        let signed_in = Memo::new(move |_| {
            revision.get();
            crate::state::bridge::with_sdk(|sdk| sdk.state().session.get().is_some())
        });
        // The staff card is not painted until a door survives its gate, and a component that
        // is not painted cannot run the probe that would open one. Asking here, in the shell
        // that every screen paints, is what keeps the question from depending on its own
        // answer.
        #[cfg(feature = "admin")]
        crate::state::bridge::admin::ensure_standing();
        // The negotiated answer, put in scope for the shared screens: the rail, the screens
        // and their explanations all read this one value instead of a per-call argument, so
        // the browser and the server paint from the same rule (the server simply has none to
        // read).
        nodus_ui::capabilities::provide_serves(crate::state::bridge::capabilities::serves());
        // Each surface this build draws, answered by the SDK rather than by the raw list: a
        // Mastodon or Misskey server publishes no `features` at all, and its baseline is what
        // says it serves administration, moderation and the drive.
        use nodus_ui::capabilities::Surface;
        let feed_accounts = crate::state::bridge::capabilities::served(Surface::Feed);
        let admin_on = crate::state::bridge::capabilities::served(Surface::Admin);
        let moderation_on = crate::state::bridge::capabilities::served(Surface::Moderation);
        // `/api/v1/meta` does publish a `drive` feature (`drive`, and beside it `drive_share`
        // and `drive_attach`), so the door follows that name and falls back to the grant the
        // session already carries — an older instance, or one whose answer has not arrived
        // yet, still has the user's scopes to go on. The stored session is `read write follow`
        // (see `session::scopes`), so `read` is the wider grant a drive-readable token
        // carries; a token without `read:drive` still gets the server's 403, which the screen
        // shows rather than hides.
        let drive_feature = crate::state::bridge::capabilities::served(Surface::Drive);
        let drive_on =
            Memo::new(move |_| drive_feature.get() || crate::state::bridge::drive::may_read());
        Self {
            signed_in,
            extensions: crate::state::bridge::supports_extensions(),
            feed_accounts,
            admin_on,
            moderation_on,
            drive_on,
            card,
        }
    }

    /// The screen a route asks for, as the browser would draw it.
    #[expect(
        clippy::too_many_lines,
        reason = "one arm per route: the match is the router, and splitting it hides a missing screen"
    )]
    pub(crate) fn draw(&self, route: Route) -> AnyView {
        let signed_in = self.signed_in;
        let extensions = self.extensions;
        let feed_accounts = self.feed_accounts;
        let admin_on = self.admin_on;
        let moderation_on = self.moderation_on;
        let drive_on = self.drive_on;
        let card = Arc::clone(&self.card);
        // The callback comes first, before the signed-in gate: the instance redirects here
        // while the app is still signed out, and this screen is what turns the code into a
        // session.
        //
        // Everything the API serves needs a token, so a signed-out visitor gets the same
        // answer on every gated route: what the click was for, or the rail reads as broken.
        let signed_in_now = signed_in.get();
        match route {
            Route::OauthCallback => view! { <Callback/> }.into_any(),
            Route::Feed if signed_in_now && extensions.get() && feed_accounts.get() => {
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
                }
                .into_any()
            }
            Route::Feed if signed_in_now => unavailable(
                "Recommended posts",
                "This instance has no recommendation engine, so there is nothing \
                 to rank here. Your home timeline still works.",
            ),
            Route::Home if signed_in_now => view! { <HomeTimeline/> }.into_any(),
            // Local and Federated are the same screen as Home: the store, the paging cursor
            // and the caption all come from the timeline kind. Their own arms exist because
            // the screen is also the header block — the composer and the switch between
            // timelines — and the shared route view takes that as a slot the client fills.
            // Without these arms the reader who clicks Local lands on a page with no tab
            // strip and cannot leave.
            Route::Local if signed_in_now => view! { <LocalTimeline/> }.into_any(),
            Route::Federated if signed_in_now => view! { <FederatedTimeline/> }.into_any(),
            // The middle of the editor is not being edited: the rail and the banners are. So
            // it is the dumping ground — every card that exists, with the ones on the rail
            // marked, one click from either side.
            Route::StyleEditor if signed_in_now && extensions.get() => {
                let has_staff = crate::app::extra_doors().is_some();
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
            Route::StyleEditor if signed_in_now => unavailable(
                "Session styling",
                "Styling is served by Nodus, and this instance does not offer \
                 it. The rest of your settings still work.",
            ),
            Route::StyleEditor => {
                view! { <SignIn note="Sign in to arrange your page."/> }.into_any()
            }
            // The drive is the account's own storage: it needs a session to name whose drive
            // it is, and a grant that can read it.
            Route::Drive if !signed_in_now => {
                view! { <SignIn note="Sign in to use the drive."/> }.into_any()
            }
            Route::Drive if !drive_on.get() => unavailable(
                "Drive",
                "The drive is served by Nodus, and this session's grant does not \
                 include it. Signing in again asks the instance for the drive \
                 scopes.",
            ),
            Route::Drive => view! { <crate::routes::drive::DriveScreen/> }.into_any(),
            // The console itself is drawn by `RouteView`, which knows each section; only a
            // signed-out visitor is sent to sign in.
            Route::Admin(_) | Route::Moderation(_) if !signed_in_now => {
                view! { <SignIn note="Sign in to moderate."/> }.into_any()
            }
            Route::Admin(_) if !extensions.get() || !admin_on.get() => unavailable(
                "Administration",
                "The console is served by Nodus, and this instance's \
                 /api/v1/meta answer does not name the admin capability. On \
                 Mastodon it lives in the instance's own web interface.",
            ),
            Route::Moderation(_) if !extensions.get() || !moderation_on.get() => unavailable(
                "Moderation",
                "Moderation is served by Nodus, and this instance's \
                 /api/v1/meta answer does not name the moderation \
                 capability. On Mastodon it lives in the instance's own web \
                 interface.",
            ),
            // The reader's own notifications, drawn by the client because half of a row is
            // this instance's moderation payload and not anything the SDK's `Notification`
            // knows about.
            Route::Notifications if signed_in_now => view! { <NotificationsView/> }.into_any(),
            // A shared link works without a session: the address names the host, so the post
            // is read from there anonymously. A legacy `/status/{id}` link falls back to the
            // instance this browser already used, or asks which one.
            Route::Thread { host, id } => {
                let host = if host.is_empty() {
                    crate::state::bridge::session::remembered_host().unwrap_or_default()
                } else {
                    host
                };
                view! {
                    <nodus_route::route::RouteView
                        runtime=crate::state::runtime::runtime()
                        route=Route::Thread { host, id }
                        card=card.clone()
                    />
                }
                .into_any()
            }
            Route::Account(id) if id.contains('/') => view! {
                <nodus_route::route::RouteView
                    runtime=crate::state::runtime::runtime()
                    route=Route::Account(id)
                    card=card.clone()
                />
            }
            .into_any(),
            // A bare handle means "here": the instance the reader is signed in at, then the
            // one this deployment names, then the address they typed. An id alone cannot be
            // read without knowing which instance minted it, so signing in is the honest
            // fallback.
            Route::Account(id) => match crate::state::bridge::session::remembered_host()
                .or_else(|| crate::state::runtime::runtime().public_host())
                .or_else(|| {
                    web_sys::window()
                        .and_then(|window| window.location().host().ok())
                        .filter(|host| !host.trim().is_empty())
                }) {
                Some(host) => view! {
                    <nodus_route::route::RouteView
                        runtime=crate::state::runtime::runtime()
                        route=Route::Account(format!("{host}/{id}"))
                        card=card.clone()
                    />
                }
                .into_any(),
                None => view! { <SignIn note="Sign in to read this."/> }.into_any(),
            },
            // Every remaining screen is the shared route view, which already knows its own
            // title, header and paging. Signing in is the only thing it cannot do for itself.
            route if signed_in_now => view! {
                <nodus_route::route::RouteView
                    runtime=crate::state::runtime::runtime()
                    route=route
                    card=card.clone()
                />
            }
            .into_any(),
            _ => view! { <SignIn note="Sign in to read this."/> }.into_any(),
        }
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
