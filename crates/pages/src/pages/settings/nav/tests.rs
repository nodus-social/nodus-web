//! The strip's one rule: a tab is drawn exactly when the session's answer can pay for it.

use super::*;

fn answer(names: &[&str]) -> Capabilities {
    let names = names
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    Capabilities::from_parts("0.1.0", &names)
}

/// Every name the sections can ask for, as a server that serves all of them would publish.
fn every_gate() -> Capabilities {
    answer(&[
        "preferences",
        "filters",
        "mutes",
        "blocks",
        "domain_blocks",
        "lists",
        "notification_settings",
        "sessions",
        "two_factor",
        "exports",
        "media_library",
    ])
}

#[test]
fn a_server_that_publishes_every_gate_draws_every_tab() {
    let at = every_gate();
    let drawn = SettingsSection::ALL
        .into_iter()
        .filter(|section| section.drawn(Some(&at)))
        .collect::<Vec<_>>();
    assert_eq!(drawn, SettingsSection::ALL);
}

/// A section that claims no server surface is drawn even when the answer is empty: it is a
/// form this shell owns, and hiding it would remove working settings from every server.
#[test]
fn a_section_that_asks_for_nothing_is_drawn_even_on_an_empty_answer() {
    let at = answer(&[]);
    for section in [
        SettingsSection::Account,
        SettingsSection::Profile,
        SettingsSection::Appearance,
        SettingsSection::FeaturedTags,
        SettingsSection::Instance,
    ] {
        assert_eq!(section.gate(), None, "{section:?}");
        assert!(section.drawn(Some(&at)), "{section:?}");
        assert!(section.drawn(None), "{section:?}");
    }
}

/// A name absent from a known answer is a refusal: the six gated tabs disappear and the
/// rest of the strip stays, which is the difference between a narrow server and a broken
/// settings page.
#[test]
fn a_server_that_omits_the_account_lists_hides_their_tabs() {
    let at = answer(&[
        "preferences",
        "filters",
        "lists",
        "notifications",
        "notification_settings",
    ]);
    for section in [
        SettingsSection::Muted,
        SettingsSection::Blocked,
        SettingsSection::DomainBlocks,
        SettingsSection::Sessions,
        SettingsSection::TwoFactor,
        SettingsSection::Export,
    ] {
        assert!(!section.drawn(Some(&at)), "{section:?}");
    }
    for section in [
        SettingsSection::Preferences,
        SettingsSection::Filters,
        SettingsSection::Lists,
        SettingsSection::Account,
        SettingsSection::Profile,
        SettingsSection::Appearance,
        SettingsSection::Notifications,
        SettingsSection::FeaturedTags,
        SettingsSection::Instance,
    ] {
        assert!(section.drawn(Some(&at)), "{section:?}");
    }
}

/// The policy tab is a capability like the account lists: without the name the strip does
/// not draw it, and the section's sentence explains the refusal on a direct visit.
#[test]
fn a_server_without_the_notification_policy_hides_the_tab() {
    let at = answer(&["preferences", "filters", "lists"]);
    assert!(!SettingsSection::Notifications.drawn(Some(&at)));
}

/// No answer yet is not a refusal: the server paint and the first client paint must not
/// flicker the strip while the capability read is still in flight.
#[test]
fn a_paint_that_does_not_know_yet_draws_the_gated_tabs() {
    for section in SettingsSection::ALL {
        assert!(section.drawn(None), "{section:?}");
    }
}

/// The Mastodon baseline is not the narrow answer: Mastodon really serves its account
/// lists, sessions, two-factor and data export, so a session it describes draws all six.
/// A server that serves none of them publishes none of the names, which is the test above.
#[test]
fn the_mastodon_baseline_draws_the_six_it_really_serves() {
    let at = Capabilities::mastodon();
    for section in [
        SettingsSection::Muted,
        SettingsSection::Blocked,
        SettingsSection::DomainBlocks,
        SettingsSection::Sessions,
        SettingsSection::TwoFactor,
        SettingsSection::Export,
    ] {
        assert!(section.drawn(Some(&at)), "{section:?}");
    }
}

/// The media library is a real gate now that Nodus publishes `media_library`: the tab is
/// drawn where the handshake names it, withheld where the published list omits it, and the
/// section's sentence is what a direct visit gets on such a server.
#[test]
fn the_media_tab_follows_the_published_name() {
    assert_eq!(SettingsSection::Media.gate(), Some(Feature::MediaLibrary));
    assert!(SettingsSection::Media.offered());

    let served = answer(&["media_library"]);
    assert!(SettingsSection::Media.drawn(Some(&served)));

    let narrow = answer(&["preferences"]);
    assert!(!SettingsSection::Media.drawn(Some(&narrow)));
    assert_eq!(
        SettingsSection::Media.absent(),
        "This instance does not serve a media library."
    );
}

/// The library is a Nodus extension, so the baseline a Mastodon session gets answers
/// `media_library` false: no tab, even though the address still explains itself.
#[test]
fn the_mastodon_baseline_hides_the_nodus_media_library() {
    let at = Capabilities::mastodon();
    assert!(!SettingsSection::Media.drawn(Some(&at)));
    assert!(SettingsSection::Media.drawn(None));
}

#[test]
fn every_section_has_a_slug_that_round_trips() {
    for section in SettingsSection::ALL {
        assert_eq!(SettingsSection::of(section.slug()), section);
    }
    assert_eq!(SettingsSection::of("nodus"), SettingsSection::Account);
}
