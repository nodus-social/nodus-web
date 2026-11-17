//! Administration: the instance itself — what it counted, who it talks to, what it says.
//!
//! Distinct from Moderation on purpose. Moderating is a decision about people (reports,
//! accounts, approvals, blocked domains); administering is a decision about the server (the
//! dashboard, relays, allowed domains, announcements). Mastodon draws the same line, and a
//! moderator who is not an administrator should not have to read server levers to find the
//! report queue.
//!
//! The area — heading, staff gate and section strip — is drawn in `nodus-view`. What stays
//! here is the staff probe and which section's body to hand the page.

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::admin::{AdminPage, Section};
use nodus_ui::chrome::StaffStanding;

use crate::router::{self, Route};
use crate::state::alive::Alive;
use crate::state::bridge::admin;

use self::announcements::Announcements;
use self::audit::Audit;
use self::domains::Domains;
use self::email::EmailDomains;
use self::overview::Overview;
use self::protocol::Protocol;
use self::replies::Replies;
use self::scanners::Scanners;

mod announcements;
mod audit;
mod domains;
mod email;
mod overview;
mod protocol;
mod replies;
mod scanners;

/// The administration area for one section.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Leptos component receives its props by value"
)]
#[component]
pub fn Admin(section: String) -> impl IntoView {
    let section = StoredValue::new(Section::of(&section));
    let alive = Alive::new();
    // `None` is "not asked yet", `Some(NeedsGrant)` is "this token lacks the scope", and the
    // gate answers each of those differently: a spinner, a grant prompt, or the screens.
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
                let verdict = admin::staff().await;
                alive.if_alive(|| {
                    if let admin::Staff::No(reason) = &verdict {
                        set_error.set(Some(reason.clone()));
                    } else {
                        set_error.set(None);
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
        let return_to = Route::Admin(section.get_value().slug().to_owned()).path();
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

    // The page speaks the shared standing; the probe's answer is translated once here.
    let standing = Signal::derive(move || allowed.get().as_ref().map(standing_of));

    view! {
        <AdminPage
            active=section.get_value().slug()
            standing=standing
            error=Some(Signal::from(error))
            granting=Some(Signal::from(granting))
            on_grant=Some(grant)
            on_navigate=Some(Callback::new(|slug: String| router::navigate(Route::Admin(slug))))
        >
            {move || match section.get_value() {
                Section::Overview => view! { <Overview/> }.into_any(),
                Section::Scanners => view! { <Scanners/> }.into_any(),
                Section::Domains => view! { <Domains/> }.into_any(),
                Section::Email => view! { <EmailDomains/> }.into_any(),
                Section::Replies => view! { <Replies/> }.into_any(),
                Section::Announcements => view! { <Announcements/> }.into_any(),
                Section::Audit => view! { <Audit/> }.into_any(),
                Section::Protocol => view! { <Protocol/> }.into_any(),
            }}
        </AdminPage>
    }
}

/// The bridge's verdict, in the gate's words.
fn standing_of(staff: &admin::Staff) -> StaffStanding {
    match staff {
        admin::Staff::Yes => StaffStanding::Yes,
        admin::Staff::No(_) => StaffStanding::No,
        admin::Staff::NeedsGrant => StaffStanding::NeedsGrant,
    }
}
