//! Two-factor is the instance's page, not a card this app can fill in.
//!
//! The second factor is proved by the instance's own login: `POST /auth/login`
//! (`nodus-backend/crates/auth/src/lib.rs`) verifies the password and, when the account has
//! a confirmed secret, answers a challenge that `POST /auth/2fa` resolves. Enrolment,
//! recovery codes and the off switch live in the same server-rendered family of pages
//! (`render_enrolment`, `render_enrolled`, `render_recovery_codes`), because they need the
//! `nodus_auth` cookie and the CSRF form token that only that origin holds. A copy drawn
//! here would need a JSON surface the instance does not serve, and a card that guessed at
//! "on" or "off" would be a decision dressed as an answer (`web/docs/02-architecture.md`
//! item 9). So the gated section is a door: one sentence saying who proves the factor, and
//! the link that opens their page.

use leptos::prelude::*;

use crate::components::panel::Panel;

/// The `two_factor` settings section: a pointer to the page that actually serves it.
#[component]
pub fn TwoFactorCard() -> impl IntoView {
    view! {
        <Panel
            title="Two-factor authentication"
            description="A second proof at sign-in, kept and proved by this instance."
        >
            <p class="text-sm opacity-70">
                "Enrol an authenticator, read the recovery codes and turn the factor off on \
                 this instance's own settings page. The code is asked for by the instance's \
                 login, not by this app."
            </p>
            <a class="btn btn-sm w-fit" href="/auth/2fa">
                "Open this instance's two-factor settings"
            </a>
        </Panel>
    }
}

#[cfg(test)]
mod tests {
    use super::TwoFactorCard;
    use leptos::prelude::*;

    /// The door is the whole feature: without the link the section says nothing it can pay
    /// for, and without the sentence a reader does not know who asks for the code.
    #[test]
    fn the_section_links_to_the_instance_page_that_serves_it() {
        let html = Owner::new().with(|| TwoFactorCard().to_html());
        assert!(
            html.contains(r#"href="/auth/2fa""#),
            "the instance's own page is linked: {html}"
        );
        assert!(
            html.contains("this instance's own settings page"),
            "the link is named for what it opens: {html}"
        );
        assert!(
            html.contains("asked for by the instance's login"),
            "the second factor's owner is stated: {html}"
        );
    }
}
