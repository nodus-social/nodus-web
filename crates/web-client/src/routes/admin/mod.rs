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
use nodus_ui::capabilities::{Surface, UnservedPage};
use nodus_ui::chrome::StaffStanding;

use crate::router::{self, Route};
use crate::state::alive::Alive;
use crate::state::bridge::{admin, capabilities};

use self::announcements::Announcements;
use self::audit::Audit;
use self::domains::Domains;
use self::email::EmailDomains;
use self::overview::Overview;
use self::protocol::Protocol;
use self::replies::Replies;
use self::rules::Rules;
use self::scanners::Scanners;
use self::settings_transfer::SettingsTransfer;

mod announcements;
mod audit;
mod domains;
mod email;
mod escalation;
mod federation;
mod overview;
mod protocol;
mod replies;
mod rules;
mod scanners;
mod settings_transfer;
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
    // Which door this account walks through. The instance's own levers are split: scanners
    // and mail-domain blocks answer only an administrator, while audit, announcements,
    // deliveries, exports and telemetry logs are served to moderators too. The tab strip
    // and the section bodies read this, not the grant's scopes.
    let administrator = Signal::derive(move || {
        matches!(
            allowed.get(),
            Some(admin::Staff::Yes {
                role: admin::Role::Admin,
                ..
            })
        )
    });

    // Every page is gated on its own published row. The area's coarse `admin` bit says this
    // instrument has an administration area at all; each page's row says which screens that
    // area actually holds. The two are kept apart because a server may serve the room and
    // not a screen in it, and the operator's rule is to omit the screen, never the room.
    let at = capabilities::serves();
    // Scanners and mail-domain blocks answer only an administrator. That is a role question,
    // not a capability one, so it sits beside the row rather than inside it: an instance that
    // serves the page still serves it to a moderator, who reads the panel's own
    // administrators-only note instead of being told the instance does not serve it at all.
    let page_served = move |page: Surface| page.served(&at.get());
    let page_in_role = move |page: Surface| {
        !matches!(page, Surface::AdminScanners | Surface::AdminEmail) || administrator.get()
    };
    let shown = Signal::derive(move || {
        Section::ALL
            .iter()
            .filter(|section| {
                Surface::admin_page(section.slug())
                    .is_none_or(|page| page_served(page) && page_in_role(page))
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
            {move || match section.get_value() {
                Section::Overview if !page_served(Surface::AdminOverview) => unserved(Surface::AdminOverview),
                Section::Overview => view! { <Overview/> }.into_any(),
                Section::Scanners if !page_served(Surface::AdminScanners) => unserved(Surface::AdminScanners),
                Section::Scanners => {
                    view! { <Scanners administrator=Some(administrator)/> }.into_any()
                }
                Section::Domains if !page_served(Surface::AdminDomains) => unserved(Surface::AdminDomains),
                Section::Domains => view! { <Domains/> }.into_any(),
                Section::Email if !page_served(Surface::AdminEmail) => unserved(Surface::AdminEmail),
                Section::Email => {
                    view! { <EmailDomains administrator=Some(administrator)/> }.into_any()
                }
                Section::Replies if !page_served(Surface::AdminReplies) => unserved(Surface::AdminReplies),
                Section::Replies => view! { <Replies/> }.into_any(),
                Section::Escalation if !page_served(Surface::AdminEscalation) => unserved(Surface::AdminEscalation),
                Section::Escalation => view! { <escalation::Escalation/> }.into_any(),
                Section::Federation if !page_served(Surface::AdminFederation) => unserved(Surface::AdminFederation),
                Section::Federation => view! { <federation::Deliveries/> }.into_any(),
                Section::Announcements if !page_served(Surface::AdminAnnouncements) => unserved(Surface::AdminAnnouncements),
                Section::Announcements => {
                    view! { <Announcements administrator=Some(administrator)/> }.into_any()
                }
                Section::Rules if !page_served(Surface::AdminRules) => unserved(Surface::AdminRules),
                Section::Rules => view! { <Rules/> }.into_any(),
                Section::Audit if !page_served(Surface::AdminAudit) => unserved(Surface::AdminAudit),
                Section::Audit => view! { <Audit/> }.into_any(),
                Section::Topology if !page_served(Surface::AdminTopology) => unserved(Surface::AdminTopology),
                Section::Topology => view! { <topology::System/> }.into_any(),
                Section::Protocol if !page_served(Surface::AdminProtocol) => unserved(Surface::AdminProtocol),
                Section::Protocol => view! { <Protocol/> }.into_any(),
                Section::SettingsTransfer if !page_served(Surface::AdminSettings) => unserved(Surface::AdminSettings),
                Section::SettingsTransfer => view! { <SettingsTransfer/> }.into_any(),
            }}
        </AdminPage>
    }
}

/// The bytes of a chosen file as text, or the sentence to show when they cannot be read.
async fn read_document_file(file: web_sys::File) -> Result<String, String> {
    let buffer = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| format!("Could not read that file: {error:?}"))?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    String::from_utf8(bytes).map_err(|_| "That file is not UTF-8 text.".to_owned())
}

/// A page this instance's area holds open but does not serve itself.
fn unserved(page: Surface) -> AnyView {
    view! { <UnservedPage surface=page/> }.into_any()
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
