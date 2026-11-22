//! Moderation: decisions about people — reports, accounts, approvals, blocked domains.
//!
//! Administration (`/admin`) is the other half: the dashboard, relays, allowed domains and
//! announcements. The split follows Mastodon's own, and follows the reader: a moderator
//! looking for a report should not have to read server levers on the way.
//!
//! The area — heading, staff gate and section strip — is drawn in `nodus-view`, so the server
//! paints the same three boxes the client mounts. What stays here is what only the client can
//! know: what the staff probe answered, and which section's body to hand the page.

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::moderation::{ModerationPage, Section};
use nodus_ui::capabilities::{Surface, UnservedPage};
use nodus_ui::chrome::StaffStanding;

use crate::router::{self, Route};
use crate::state::alive::Alive;
use crate::state::bridge::{admin, capabilities};

use self::accounts::AccountList;
use self::appeals::Appeals;
use self::federation::Federation;
use self::flags::Scans;
use self::media::QuarantinedMediaList;
use self::registrations::Registrations;
use self::reports::ReportQueue;
use self::rules::Rules;

mod accounts;
mod appeals;
mod federation;
mod flags;
mod media;
mod registrations;
mod reports;
mod rules;

/// The moderation area for one section.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Leptos component receives its props by value"
)]
#[component]
pub fn Moderation(section: String) -> impl IntoView {
    let section = StoredValue::new(Section::of(&section));
    let alive = Alive::new();
    // `None` is "not asked yet". The gate reads the rest from the verdict: no grant in this
    // browser, no staff role at all, a refused grant, a question that did not arrive, or
    // the panels themselves — and each of those is a different thing to say.
    let (allowed, set_allowed) = signal(None::<admin::Staff>);
    // A refused probe and a failed start both have a sentence the reader can act on; it
    // sits next to the gate rather than replacing it, so the retry stays reachable.
    let (error, set_error) = signal(None::<String>);
    // The grant leaves for the instance's consent page. Until that navigation happens the
    // button must not start a second grant, and a reader who comes back without finishing
    // must get the button back.
    let (granting, set_granting) = signal(false);
    let probe = {
        move || {
            spawn_local(async move {
                let verdict = admin::standing().await;
                alive.if_alive(|| {
                    // The sentence that comes with a refused or unanswerable probe is the
                    // only place the instance's own words appear; it stays above the gate.
                    match &verdict {
                        admin::Staff::Refused(reason) | admin::Staff::Unknown(reason) => {
                            set_error.set(Some(reason.clone()));
                        }
                        _ => set_error.set(None),
                    }
                    set_allowed.set(Some(verdict));
                });
            });
        }
    };
    Effect::new(move |_| probe());
    let retry = Callback::new(move |()| {
        set_allowed.set(None);
        set_error.set(None);
        probe();
    });
    // The grant itself is browser-only — it leaves for the instance's own authorize page —
    // so it lives here and reaches the gate as a verb. The path it was asked from travels
    // with the flow so the callback can put the reader back here.
    let grant = Callback::new(move |()| {
        set_error.set(None);
        set_granting.set(true);
        let return_to = Route::Moderation(section.get_value().slug().to_owned()).path();
        spawn_local(async move {
            match crate::routes::settings::admin::request_grant(&return_to).await {
                Ok(()) => {
                    TimeoutFuture::new(8_000).await;
                    alive.if_alive(|| {
                        set_granting.set(false);
                        retry.run(());
                    });
                }
                Err(message) => alive.if_alive(|| {
                    set_granting.set(false);
                    set_error.set(Some(message));
                }),
            }
        });
    });

    // The page speaks the shared standing, not the bridge's: the gate is markup, so its
    // three arms are the view's vocabulary, and the probe's answer is translated once here.
    let standing = Signal::derive(move || allowed.get().as_ref().map(standing_of));
    // The Scans tab reads the instance's scanner routes, and those answer only an
    // administrator. A moderator opening it would find a panel every request refuses.
    let administrator = Signal::derive(move || {
        matches!(
            allowed.get(),
            Some(admin::Staff::Yes {
                role: admin::Role::Admin,
                ..
            })
        )
    });

    // Every section is gated on its own published row, the same way the administration
    // pages are: the area's coarse `moderation` bit says this instrument has a moderation
    // area at all, and each section's row says whether that area holds this queue. A server
    // may serve the room and not a queue in it, and the operator's rule is to omit the
    // queue, never the room.
    let at = capabilities::serves();
    // The scan queue is served only to an administrator. That is a role question, not a
    // capability one, so it sits beside the row rather than inside it.
    let page_served = move |section: Surface| {
        let administrator = administrator.get();
        let served = section.served(&at.get());
        match section {
            Surface::ModerationFlags => served && administrator,
            _ => served,
        }
    };
    let shown = Signal::derive(move || {
        Section::ALL
            .iter()
            .filter(|section| Surface::moderation_section(section.slug()).is_none_or(page_served))
            .map(|section| section.slug().to_owned())
            .collect::<Vec<String>>()
    });

    view! {
        <ModerationPage
            active=section.get_value().slug()
            shown=Some(shown)
            standing=standing
            error=Some(Signal::from(error))
            granting=Some(Signal::from(granting))
            on_grant=Some(grant)
            on_retry=Some(retry)
            on_navigate=Callback::new(|slug: String| router::navigate(Route::Moderation(slug)))
        >
            {move || match section.get_value() {
                Section::Reports if !page_served(Surface::ModerationReports) => unserved(Surface::ModerationReports),
                Section::Reports => view! { <ReportQueue/> }.into_any(),
                Section::Appeals if !page_served(Surface::ModerationAppeals) => unserved(Surface::ModerationAppeals),
                Section::Appeals => view! { <Appeals/> }.into_any(),
                Section::Scans if !page_served(Surface::ModerationFlags) => unserved(Surface::ModerationFlags),
                Section::Scans => view! { <Scans/> }.into_any(),
                Section::Media if !page_served(Surface::ModerationMedia) => unserved(Surface::ModerationMedia),
                Section::Media => view! { <QuarantinedMediaList/> }.into_any(),
                Section::Accounts if !page_served(Surface::ModerationAccounts) => unserved(Surface::ModerationAccounts),
                Section::Accounts => view! { <AccountList/> }.into_any(),
                Section::Registrations if !page_served(Surface::ModerationRegistrations) => unserved(Surface::ModerationRegistrations),
                Section::Registrations => view! { <Registrations/> }.into_any(),
                Section::Federation if !page_served(Surface::ModerationFederation) => unserved(Surface::ModerationFederation),
                Section::Federation => view! { <Federation/> }.into_any(),
                Section::Rules if !page_served(Surface::ModerationRules) => unserved(Surface::ModerationRules),
                Section::Rules => view! { <Rules/> }.into_any(),
            }}
        </ModerationPage>
    }
}

/// A section this instance's area holds open but does not serve itself.
fn unserved(section: Surface) -> AnyView {
    view! { <UnservedPage surface=section/> }.into_any()
}

/// The bridge's verdict, in the gate's words.
fn standing_of(staff: &admin::Staff) -> StaffStanding {
    match staff {
        admin::Staff::Yes { .. } => StaffStanding::Yes,
        admin::Staff::NotStaff => StaffStanding::NotStaff,
        admin::Staff::NeedsGrant => StaffStanding::NeedsGrant,
        admin::Staff::Refused(_) => StaffStanding::Refused,
        admin::Staff::Unknown(_) => StaffStanding::Unknown,
    }
}
