//! The moderation area: one section at a time, behind the staff gate.
//!
//! The area and its sections were drawn in the client, so the server had nothing to paint
//! for `/moderation/{section}` beyond a skeleton. The heading, the gate and the strip are one
//! tree now; the section's body arrives as `children`, because each body reads its own list
//! and a page may not fetch. The section enum lives here rather than in the client for the
//! same reason the strip does: it is the strip's data, and one definition keeps the slug in
//! the address and the word on the tab from drifting apart.

use leptos::prelude::*;

use crate::chrome::section_strip::SectionTab;
use crate::chrome::{SectionStrip, StaffGate, StaffStanding};
use crate::icons::Icon;
use crate::{PageHeader, Screen};

pub mod accounts;
pub mod federation;
pub mod flags;
pub mod registrations;
pub mod reports;

/// The people lists (`/accounts/{id}/followers` and the viewer's mutes and blocks) share this
/// directory with the staff sections they are not part of; the file keeps the path the tree
/// gives it.
#[path = "[section]/mod.rs"]
pub mod people;

/// One section of the moderation area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// The report queue.
    Reports,
    /// Content the scanners flagged.
    Scans,
    /// Account standing.
    Accounts,
    /// Who is waiting to be let in.
    Registrations,
    /// Domains this instance refuses.
    Federation,
}

impl Section {
    /// Every section, in the order the strip shows them.
    pub const ALL: [Self; 5] = [
        Self::Reports,
        Self::Scans,
        Self::Accounts,
        Self::Registrations,
        Self::Federation,
    ];

    /// The last path segment, which is also the strip's key.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Reports => "reports",
            Self::Scans => "scans",
            Self::Accounts => "accounts",
            Self::Registrations => "registrations",
            Self::Federation => "federation",
        }
    }

    /// The section an address names; anything else is the queue.
    #[must_use]
    pub fn of(slug: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|section| section.slug() == slug)
            .unwrap_or(Self::Reports)
    }

    /// The word on the tab.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reports => "Reports",
            Self::Scans => "Scans",
            Self::Accounts => "Accounts",
            Self::Registrations => "Registrations",
            Self::Federation => "Federation",
        }
    }

    /// The sentence on hover: what the section is for.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Reports => "Posts someone reported, and what was decided",
            Self::Scans => "What the scanners found, and what a person decided",
            Self::Accounts => "Look an account up, and act on it",
            Self::Registrations => "Who is waiting, and how somebody may join",
            Self::Federation => "Domains this instance refuses",
        }
    }

    /// The drawing beside the label.
    #[must_use]
    pub const fn icon(self) -> Icon {
        match self {
            Self::Reports => Icon::Alert,
            Self::Scans => Icon::Image,
            Self::Accounts => Icon::Users,
            Self::Registrations => Icon::Check,
            Self::Federation => Icon::Shield,
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

/// The moderation area for one section.
///
/// The body is the caller's: each section reads its own list, and the page owns everything
/// around it — heading, gate, strip — so the order of those three cannot differ between the
/// paints.
#[expect(
    clippy::needless_pass_by_value,
    reason = "a Leptos component receives its props by value"
)]
#[component]
pub fn ModerationPage(
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
    #[prop(optional)]
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
                title="Moderation"
                description="Decisions about people: reports, accounts, registrations and blocked domains."
            />

            <StaffGate
                standing=standing
                on_grant=on_grant
                on_retry=on_retry
                error=error
                granting=granting
            >
                <SectionStrip
                    base="/moderation"
                    tabs=tabs
                    active=here.slug()
                    on_navigate=on_navigate
                />
                {children()}
            </StaffGate>
        </Screen>
    }
}
