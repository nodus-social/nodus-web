//! The settings sections and the strip that switches between them.
//!
//! The list mirrors Mastodon's own settings, and every section but the shell's own forms
//! names the capability it needs. A tab is a promise that the page behind it works; drawing
//! one for a name the server never published is how a settings page fills with blank tabs.

use leptos::prelude::*;

use crate::capabilities::{Capabilities, Feature};

/// One section of Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Account,
    Profile,
    Appearance,
    Notifications,
    Preferences,
    Filters,
    Muted,
    Blocked,
    DomainBlocks,
    Lists,
    FeaturedTags,
    Sessions,
    TwoFactor,
    Export,
    Media,
    Instance,
}

impl SettingsSection {
    pub const ALL: [Self; 16] = [
        Self::Account,
        Self::Profile,
        Self::Appearance,
        Self::Notifications,
        Self::Preferences,
        Self::Filters,
        Self::Muted,
        Self::Blocked,
        Self::DomainBlocks,
        Self::Lists,
        Self::FeaturedTags,
        Self::Sessions,
        Self::TwoFactor,
        Self::Export,
        Self::Media,
        Self::Instance,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Profile => "profile",
            Self::Appearance => "appearance",
            Self::Notifications => "notifications",
            Self::Preferences => "preferences",
            Self::Filters => "filters",
            Self::Muted => "muted",
            Self::Blocked => "blocked",
            Self::DomainBlocks => "domain-blocks",
            Self::Lists => "lists",
            Self::FeaturedTags => "featured-tags",
            Self::Sessions => "sessions",
            Self::TwoFactor => "two-factor",
            Self::Export => "export",
            Self::Media => "media",
            Self::Instance => "instance",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Account => "Account",
            Self::Profile => "Profile",
            Self::Appearance => "Appearance",
            Self::Notifications => "Notifications",
            Self::Preferences => "Preferences",
            Self::Filters => "Filters",
            Self::Muted => "Muted",
            Self::Blocked => "Blocked",
            Self::DomainBlocks => "Domain blocks",
            Self::Lists => "Lists",
            Self::FeaturedTags => "Featured tags",
            Self::Sessions => "Sessions",
            Self::TwoFactor => "Two-factor",
            Self::Export => "Export",
            Self::Media => "Media",
            Self::Instance => "Instance",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Account => "Who you are signed in as, and the other accounts this browser holds.",
            Self::Profile => "Your display name, bio, clock and media choices.",
            Self::Appearance => "How this app looks on this browser.",
            Self::Notifications => "Which events this instance should tell you about.",
            Self::Preferences => "Defaults for what you post and what you read.",
            Self::Filters => "Phrases that hide a post or put it behind a warning.",
            Self::Muted => "Accounts whose posts you never see.",
            Self::Blocked => "Accounts that cannot follow you or see your posts.",
            Self::DomainBlocks => "Servers whose accounts you do not see.",
            Self::Lists => "Curated timelines of accounts you follow.",
            Self::FeaturedTags => "Tags pinned to your profile.",
            Self::Sessions => "Browsers and apps signed in to your account.",
            Self::TwoFactor => "A second proof when you sign in.",
            Self::Export => "A copy of what this instance holds about you.",
            Self::Media => "Every picture you uploaded, and what still holds each one.",
            Self::Instance => "What this server says about itself, and this app's sync and feeds.",
        }
    }

    /// The capability this section needs, or `None` for a form the shell itself provides.
    ///
    /// A `None` section claims no server surface at all: Account and Profile are the shell's
    /// own forms, Appearance is this browser's style, and Instance is the public instance
    /// document every protocol serves. Those tabs are drawn on every server because hiding
    /// them would remove working forms. A `Some` gate is the name the server must publish;
    /// see [`Self::drawn`] for what a missing name means.
    #[must_use]
    pub const fn gate(self) -> Option<Feature> {
        match self {
            Self::Account | Self::Profile | Self::Appearance | Self::Instance => None,
            // The policy endpoint is published as `notification_settings`; a server that
            // omits the name gets the page's explanation instead of controls that cannot
            // save.
            Self::Notifications => Some(Feature::NotificationSettings),
            Self::Preferences => Some(Feature::Preferences),
            Self::Filters => Some(Feature::Filters),
            Self::Muted => Some(Feature::Mutes),
            Self::Blocked => Some(Feature::Blocks),
            Self::DomainBlocks => Some(Feature::DomainBlocks),
            Self::Lists => Some(Feature::Lists),
            // TODO(owner: backend): the published `tags` name means follow/unfollow of a
            // tag, not the featured-tags list; `featured_tags` is not published yet.
            Self::FeaturedTags => None,
            Self::Sessions => Some(Feature::Sessions),
            Self::TwoFactor => Some(Feature::TwoFactor),
            Self::Export => Some(Feature::Exports),
            // The media library is a Nodus extension: it asks `GET|DELETE
            // /api/v1/media/library`, which Nodus publishes as `media_library`. Mastodon
            // and Misskey publish no such name, so the tab is drawn only where the
            // handshake names it.
            Self::Media => Some(Feature::MediaLibrary),
        }
    }

    /// Whether the strip offers this section as a tab.
    ///
    /// Every section is offered: each one either names its gate or is a form this shell
    /// owns, and [`Self::drawn`] is the only filter left — a known answer that omits the
    /// name withholds the tab while the section keeps its address.
    #[must_use]
    pub const fn offered(self) -> bool {
        true
    }

    /// Whether a paint with this answer should draw the section.
    ///
    /// The rule is the one every other door uses: no answer yet draws it, and an answer that
    /// omits the gate hides it. Hiding is the right failure for a settings tab — the page
    /// behind it would make a request this server has said it cannot answer.
    #[must_use]
    pub fn drawn(self, at: Option<&Capabilities>) -> bool {
        crate::capabilities::admits_feature(at, self.gate())
    }

    /// What a direct visit says when the server does not serve this section.
    ///
    /// The address is still real, so it gets the same sentence a missing page gets rather
    /// than a 404 or a blank tab.
    #[must_use]
    pub const fn absent(self) -> &'static str {
        match self {
            Self::Account | Self::Profile | Self::Appearance | Self::Instance => {
                "This instance does not serve this settings page."
            }
            Self::Notifications => "This instance does not publish notification settings.",
            Self::Preferences => "This instance does not serve posting preferences.",
            Self::Filters => "This instance does not serve filters.",
            Self::Muted => "This instance does not serve the mute list.",
            Self::Blocked => "This instance does not serve the block list.",
            Self::DomainBlocks => "This instance does not serve domain blocks.",
            Self::Lists => "This instance does not serve lists.",
            Self::FeaturedTags => "This instance does not serve featured tags.",
            Self::Sessions => "This instance does not serve a list of signed-in sessions.",
            Self::TwoFactor => "This instance does not serve two-factor authentication.",
            Self::Export => "This instance does not export account data.",
            Self::Media => "This instance does not serve a media library.",
        }
    }

    pub fn of(slug: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|section| section.slug() == slug)
            .unwrap_or(Self::Account)
    }
}

/// The tab strip at the top of Settings.
#[component]
pub fn SettingsTabs(
    #[prop(into)] section: Signal<SettingsSection>,
    #[prop(optional_no_strip)] on_navigate: Option<Callback<SettingsSection>>,
) -> impl IntoView {
    let at = crate::capabilities::serves();
    view! {
        <div role="tablist" class="tabs tabs-box mb-4 w-full flex-wrap sm:w-fit">
            <For
                each=move || {
                    SettingsSection::ALL
                        .into_iter()
                        .filter(|item| {
                            item.offered()
                                && item.drawn(at.as_ref().map(|at| at.get()).as_ref())
                        })
                        .collect::<Vec<_>>()
                }
                key=|item| item.slug()
                let:item
            >
                {
                    let href = format!("/settings/{}", item.slug());
                    view! {
                        <a
                            role="tab"
                            class="tab"
                            class:tab-active=move || section.get() == item
                            aria-selected=move || {
                                if section.get() == item { "true" } else { "false" }
                            }
                            href=href
                            title=item.hint()
                            on:click=move |event| {
                                if let Some(on_navigate) = on_navigate {
                                    event.prevent_default();
                                    on_navigate.run(item);
                                }
                            }
                        >
                            {item.label()}
                        </a>
                    }
                }
            </For>
        </div>
    }
}

#[cfg(test)]
mod tests;
