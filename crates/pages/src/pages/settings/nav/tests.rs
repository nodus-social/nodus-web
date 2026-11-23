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
        "sessions",
        "two_factor",
        "exports",
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
        SettingsSection::Notifications,
        SettingsSection::FeaturedTags,
        SettingsSection::Instance,
    ] {
        assert_eq!(section.gate(), None, "{section:?}");
        assert!(section.drawn(Some(&at)), "{section:?}");
        assert!(section.drawn(None), "{section:?}");
    }
}

/// A name absent from a known answer is a refusal: the six account-list tabs disappear and
/// the rest of the strip stays, which is the difference between a narrow server and a broken
/// settings page.
#[test]
fn a_server_that_omits_the_account_lists_hides_their_tabs() {
    let at = answer(&["preferences", "filters", "lists", "notifications"]);
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

/// No answer yet is not a refusal: the server paint and the first client paint must not
/// flicker the strip while the capability read is still in flight.
#[test]
fn a_paint_that_does_not_know_yet_draws_the_gated_tabs() {
    for section in SettingsSection::ALL {
        assert!(section.drawn(None), "{section:?}");
    }
}

#[test]
fn every_section_has_a_slug_that_round_trips() {
    for section in SettingsSection::ALL {
        assert_eq!(SettingsSection::of(section.slug()), section);
    }
    assert_eq!(SettingsSection::of("nodus"), SettingsSection::Account);
}
