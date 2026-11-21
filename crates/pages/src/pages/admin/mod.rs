//! Administration: the instance itself — what it counted, who it talks to, what it says.
//!
//! Distinct from Moderation on purpose. Moderating is a decision about people (reports,
//! accounts, approvals, blocked domains); administering is a decision about the server (the
//! dashboard, relays, allowed domains, announcements). Mastodon draws the same line, and a
//! moderator who is not an administrator should not have to read server levers to find the
//! report queue.
//!
//! The area — heading, gate, strip — is one tree for both paints; each section's body arrives
//! as `children`, because a body reads its own numbers and a page may not fetch.

use leptos::prelude::*;

use crate::chrome::section_strip::SectionTab;
use crate::chrome::{SectionStrip, StaffGate, StaffStanding};
use crate::icons::Icon;
use crate::{PageHeader, Screen};

pub mod announcements;
pub mod audit;
pub mod domains;
pub mod email;
pub mod escalation;
pub mod federation;
pub mod overview;
pub mod protocol;
pub mod replies;
pub mod rules;
pub mod scanners;
pub mod settings_transfer;
pub mod topology;

/// One section of the administration area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// The dashboard.
    Overview,
    /// The services that read content, and what their labels mean.
    Scanners,
    /// Domains allowed through a general block.
    Domains,
    /// Mail domains that may not sign up.
    Email,
    /// The instance's canned replies.
    Replies,
    /// What a number of live strikes adds, rung by rung.
    Escalation,
    /// What this instance still has to deliver to its peers.
    Federation,
    /// What the instance is telling everyone.
    Announcements,
    /// The rules this instance publishes, and the order a report cites them in.
    Rules,
    /// What staff have done.
    Audit,
    /// What the instance's own processes are doing.
    Topology,
    /// What API this server publishes.
    Protocol,
    /// This instance's moderation settings, carried out of it or into it.
    SettingsTransfer,
}

impl Section {
    /// Every section, in the order the strip shows them.
    pub const ALL: [Self; 13] = [
        Self::Overview,
        Self::Scanners,
        Self::Domains,
        Self::Email,
        Self::Replies,
        Self::Escalation,
        Self::Federation,
        Self::Announcements,
        Self::Rules,
        Self::Audit,
        Self::Topology,
        Self::Protocol,
        Self::SettingsTransfer,
    ];

    /// The last path segment, which is also the strip's key.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Scanners => "scanners",
            Self::Domains => "domains",
            Self::Email => "email",
            Self::Replies => "replies",
            Self::Escalation => "escalation",
            Self::Federation => "federation",
            Self::Announcements => "announcements",
            Self::Rules => "rules",
            Self::Audit => "audit",
            Self::Topology => "system",
            Self::Protocol => "protocol",
            Self::SettingsTransfer => "settings-transfer",
        }
    }

    /// The section an address names; anything else is the dashboard.
    #[must_use]
    pub fn of(slug: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|section| section.slug() == slug)
            .unwrap_or(Self::Overview)
    }

    /// The word on the tab.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Scanners => "Scanners",
            Self::Domains => "Allowed domains",
            Self::Email => "Mail domains",
            Self::Replies => "Canned replies",
            Self::Escalation => "Escalation",
            Self::Federation => "Deliveries",
            Self::Announcements => "Announcements",
            Self::Rules => "Instance rules",
            Self::Audit => "Audit log",
            Self::Topology => "System",
            Self::Protocol => "Protocol",
            Self::SettingsTransfer => "Settings transfer",
        }
    }

    /// The sentence on hover: what the section is for.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Overview => "What this instance counted this week",
            Self::Scanners => "The services that read content, and what their labels mean",
            Self::Domains => "Domains allowed through a general block",
            Self::Email => "Mail domains that may not sign up",
            Self::Replies => "The sentences a moderator can send",
            Self::Escalation => "What a number of strikes adds, and in which order",
            Self::Federation => {
                "What this instance still has to deliver to its peers, and why a delivery stopped"
            }
            Self::Announcements => "What the instance is telling everyone",
            Self::Rules => {
                "The rules this instance publishes, and the order a report cites them in"
            }
            Self::Audit => "Administrative actions recorded by this server",
            Self::Topology => "What each process is doing, and what it just said",
            Self::Protocol => "The client API the SDK selects for this server",
            Self::SettingsTransfer => {
                "Carry moderation settings to another instance, or bring them in"
            }
        }
    }

    /// The drawing beside the label.
    #[must_use]
    pub const fn icon(self) -> Icon {
        match self {
            Self::Overview | Self::Protocol => Icon::Globe,
            Self::Scanners => Icon::Shield,
            Self::Domains => Icon::Link,
            Self::Email => Icon::Mail,
            Self::Replies => Icon::Pen,
            Self::Escalation => Icon::Shield,
            Self::Federation => Icon::Link,
            Self::Announcements => Icon::Bell,
            Self::Rules => Icon::List,
            Self::Audit => Icon::List,
            Self::Topology => Icon::List,
            Self::SettingsTransfer => Icon::List,
        }
    }

    /// The tab the strip draws for this section.
    #[must_use]
    pub fn tab(self) -> SectionTab {
        SectionTab {
            slug: self.slug().to_owned(),
            label: self.label().to_owned(),
            hint: self.hint().to_owned(),
            icon: self.icon(),
        }
    }
}

/// The administration area for one section.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Leptos component receives its props by value"
)]
#[component]
pub fn AdminPage(
    /// The slug the address names.
    #[prop(into)]
    active: String,
    /// What the staff probe answered, or `None` while it is out.
    standing: Signal<Option<StaffStanding>>,
    /// The section slugs the instance advertises, or `None` for all of them.
    ///
    /// The `/api/v1/meta` answer arrives after the first paint, so this is a signal: a strip
    /// frozen at the first paint would keep offering a section the instance does not serve.
    /// The server, which has no probe, passes nothing and draws every tab.
    #[prop(optional_no_strip)]
    shown: Option<Signal<Vec<String>>>,
    /// Asks for the wider token, then retries the probe. Absent where nobody can ask.
    #[prop(optional_no_strip)]
    on_grant: Option<Callback<()>>,
    /// Asks the probe again, for the arms that end on "try later" rather than on an action.
    #[prop(optional_no_strip)]
    on_retry: Option<Callback<()>>,
    /// The last probe or grant failure, shown beside the gate. Absent on the server.
    #[prop(optional_no_strip)]
    error: Option<Signal<Option<String>>>,
    /// Whether a grant is on its way to the instance, so the gate can hold its button.
    #[prop(optional_no_strip)]
    granting: Option<Signal<bool>>,
    /// Opens a section inside the app, by slug. Absent on the server.
    #[prop(optional_no_strip)]
    on_navigate: Option<Callback<String>>,
    /// The section body.
    children: ChildrenFn,
) -> impl IntoView {
    let here = Section::of(&active);
    // The strip itself is reactive: `children` is a `ChildrenFn` and the filter reads a
    // signal, so a section the instance advertises later appears without a reload.
    let tabs = Signal::derive(move || {
        Section::ALL
            .iter()
            .filter(|section| match shown {
                None => true,
                Some(shown) => shown.get().iter().any(|slug| slug == section.slug()),
            })
            .map(|section| section.tab())
            .collect::<Vec<SectionTab>>()
    });

    view! {
        <Screen>
            <PageHeader
                title="Administration"
                description="This instance: what it counted, who it talks to, what it says."
            />

            <StaffGate
                standing=standing
                on_grant=on_grant
                on_retry=on_retry
                error=error
                granting=granting
            >
                <SectionStrip
                    base="/admin"
                    tabs=tabs
                    active=here.slug()
                    on_navigate=on_navigate
                />
                {children()}
            </StaffGate>
        </Screen>
    }
}
