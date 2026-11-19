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
use nodus_ui::chrome::StaffStanding;
use nodus_ui::Note;

use crate::router::{self, Route};
use crate::state::alive::Alive;
use crate::state::bridge::{admin, feature};

use self::accounts::AccountList;
use self::appeals::Appeals;
use self::federation::Federation;
use self::flags::Scans;
use self::registrations::Registrations;
use self::reports::ReportQueue;

mod accounts;
mod appeals;
mod federation;
mod flags;
mod registrations;
mod reports;

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

    // Scans are a scanner feature: an instance that does not advertise `scanners` has no
    // flags to show, so the tab is not drawn and the address says why.
    let scanners = feature("scanners");
    let shown = Signal::derive(move || {
        Section::ALL
            .iter()
            .filter(|section| match section {
                Section::Scans => scanners.get(),
                _ => true,
            })
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
                Section::Reports => view! { <ReportQueue/> }.into_any(),
                Section::Appeals => view! { <Appeals/> }.into_any(),
                Section::Scans if !scanners.get() => view! {
                    <Note class="text-sm opacity-70">
                        "This instance does not serve the scans panel: its /api/v1/meta answer does not name scanners among its features."
                    </Note>
                }
                .into_any(),
                Section::Scans => view! { <Scans/> }.into_any(),
                Section::Accounts => view! { <AccountList/> }.into_any(),
                Section::Registrations => view! { <Registrations/> }.into_any(),
                Section::Federation => view! { <Federation/> }.into_any(),
            }}
        </ModerationPage>
    }
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
