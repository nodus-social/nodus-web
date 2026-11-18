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
//!
//! One more line is drawn here: a moderator who is not an administrator opens the area and
//! is told so. The instance's own rules are what make it true — the levers behind these
//! sections (relays, blocks, scanners, role grants) require the `admin` role, not merely
//! staff — so painting the sections and letting each request fail would be the console
//! pretending a permission it does not have.

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::admin::{AdminPage, Section};
use nodus_ui::chrome::StaffStanding;
use nodus_ui::Note;

use crate::router::{self, Route};
use crate::state::alive::Alive;
use crate::state::bridge::{admin, feature};

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
mod topology;

/// The administration area for one section.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Leptos component receives its props by value"
)]
#[component]
pub fn Admin(section: String) -> impl IntoView {
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

    // The instance decides which sections exist: a `features` name it does not publish is a
    // panel it does not serve, so the tab is not drawn and the section says why if the
    // address names it anyway. Everything else is this instance's own console.
    let scanners = feature("scanners");
    let topology = feature("topology");
    let protocol = feature("protocol");
    let shown = Signal::derive(move || {
        Section::ALL
            .iter()
            .filter(|section| match section {
                Section::Scanners => scanners.get(),
                Section::Topology => topology.get(),
                Section::Protocol => protocol.get(),
                _ => true,
            })
            .map(|section| section.slug().to_owned())
            .collect::<Vec<String>>()
    });

    view! {
        <AdminPage
            active=section.get_value().slug()
            shown=Some(shown)
            standing=standing
            error=Some(Signal::from(error))
            granting=Some(Signal::from(granting))
            on_grant=Some(grant)
            on_retry=Some(retry)
            on_navigate=Some(Callback::new(|slug: String| router::navigate(Route::Admin(slug))))
        >
            {move || {
                let moderator = matches!(
                    allowed.get(),
                    Some(admin::Staff::Yes { role: admin::Role::Moderator, .. })
                );
                if moderator {
                    return view! {
                        <Note class="text-sm opacity-70">
                            "These are the instance's own levers, and this account's staff\n                             role is moderator rather than administrator: the instance will\n                             refuse the requests behind them. Moderation is where this\n                             account's access applies — an administrator can raise the role\n                             here."
                        </Note>
                    }
                        .into_any();
                }
                match section.get_value() {
                    Section::Overview => view! { <Overview/> }.into_any(),
                    Section::Scanners if !scanners.get() => missing("scanners"),
                    Section::Scanners => view! { <Scanners/> }.into_any(),
                    Section::Domains => view! { <Domains/> }.into_any(),
                    Section::Email => view! { <EmailDomains/> }.into_any(),
                    Section::Replies => view! { <Replies/> }.into_any(),
                    Section::Announcements => view! { <Announcements/> }.into_any(),
                    Section::Audit => view! { <Audit/> }.into_any(),
                    Section::Topology if !topology.get() => missing("topology"),
                    Section::Topology => view! { <topology::System/> }.into_any(),
                    Section::Protocol if !protocol.get() => missing("protocol"),
                    Section::Protocol => view! { <Protocol/> }.into_any(),
                }
            }}
        </AdminPage>
    }
}

/// A section the instance's `/api/v1/meta` answer does not advertise.
fn missing(name: &'static str) -> AnyView {
    view! {
        <Note class="text-sm opacity-70">
            {format!(
                "This instance does not serve the {name} panel: its /api/v1/meta answer does not name it among its features."
            )}
        </Note>
    }
    .into_any()
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
