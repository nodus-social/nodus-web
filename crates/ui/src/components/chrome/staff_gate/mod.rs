//! The answers a staff probe can give, and what each one paints.
//!
//! Moderation and administration ask the same question — may this account see the panels —
//! and the answer is not yes/no: it can be "no grant in this browser yet", "this account is
//! not staff at all", or "the instance did not answer". Each of those is a different thing
//! to tell someone, and only one of them is worth offering a button for, so the gate paints
//! one arm per answer rather than one refusal card for all of them.
//!
//! The markup lived in the client, so the server could not paint it at all; it is one
//! component now, and the section behind the gate arrives as its children, which keeps the
//! arms in one place whichever screen is behind them.

use leptos::prelude::*;

/// What a staff probe answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaffStanding {
    /// The instance let the token read `admin/meta`: the panels work.
    Yes,
    /// The account is a moderator or administrator, but this browser holds no grant for it,
    /// so nothing has been asked of the instance yet.
    NeedsGrant,
    /// The instance says this account has no staff role. No grant can change that — an
    /// administrator has to, on the instance.
    NotStaff,
    /// A grant is held and the instance still refused it: the role is there but the token is
    /// not accepted, which another grant can fix.
    Refused,
    /// The question could not be put to the instance at all. Nothing is known either way.
    Unknown,
}

/// The answers, and the section that is only drawn for `Yes`.
///
/// `standing` is `None` while the probe is out. `on_grant` asks the instance for the wider
/// token; `on_retry` asks the probe again without leaving. Where a callback is absent the
/// button is kept in place and disabled, which is what a paint with no session to ask
/// against shows — it is never painted as if it would work.
#[component]
pub fn StaffGate(
    /// What the probe answered, or `None` while it is out.
    standing: Signal<Option<StaffStanding>>,
    /// Asks for `admin:read` and `admin:write`, then retries the probe. Absent where there
    /// is nowhere to ask.
    ///
    /// `optional_no_strip`, not `optional`: the page above forwards whatever its own caller
    /// gave it, which may be `None`, and a stripped setter would take a bare `Callback` and
    /// turn `None` into a present (therefore enabled) button.
    #[prop(optional_no_strip)]
    on_grant: Option<Callback<()>>,
    /// Asks the probe again, for the arms that end on "try later" rather than on an action.
    #[prop(optional_no_strip)]
    on_retry: Option<Callback<()>>,
    /// The last failure — a refused probe or a grant that could not start — shown above the
    /// arms so it survives whichever standing the probe ends on.
    #[prop(optional_no_strip)]
    error: Option<Signal<Option<String>>>,
    /// Whether a grant is already on its way to the instance. While it is, the button is
    /// disabled: the browser is leaving for the consent page, and a second press would
    /// start a second grant.
    #[prop(optional_no_strip)]
    granting: Option<Signal<bool>>,
    /// The section the gate opens on `Yes`.
    children: ChildrenFn,
) -> impl IntoView {
    // Taken out of the view once, so the gate can be read as one arm per answer rather than
    // one nested `Show`.
    let body = StoredValue::new(children);
    let error_text =
        Signal::derive(move || error.and_then(|error| error.get()).unwrap_or_default());
    let pending = Signal::derive(move || granting.is_some_and(|granting| granting.get()));
    let grant = Callback::new(move |()| {
        if let Some(on_grant) = on_grant {
            on_grant.run(());
        }
    });
    let retry = Callback::new(move |()| {
        if let Some(on_retry) = on_retry {
            on_retry.run(());
        }
    });

    view! {
        <Show when=move || !error_text.get().is_empty()>
            <div class="alert alert-error alert-soft py-2 text-sm" role="alert">
                {move || error_text.get()}
            </div>
        </Show>

        <Show when=move || standing.get().is_none()>
            <p class="text-sm opacity-70">"Checking what this account may do…"</p>
        </Show>

        <Show when=move || matches!(standing.get(), Some(StaffStanding::NeedsGrant))>
            <div class="card border border-base-300 bg-base-100 shadow-sm">
                <div class="card-body gap-3 p-4">
                    <h2 class="card-title text-base">"This needs a moderation token"</h2>
                    <p class="text-sm opacity-70">
                        "This instance says your account is staff, but this browser holds no
                         token for it. Granting one authorizes a second token for this
                         instance, with "
                        <code class="text-xs">"admin:read"</code>
                        " and "
                        <code class="text-xs">"admin:write"</code>
                        " — enough to read every report, suspend an account or block a domain."
                    </p>
                    <p class="text-sm opacity-70">
                        "Your ordinary session keeps its own, narrower token; this grant is
                         used only here, and can be forgotten again at any time."
                    </p>
                    <button
                        class="btn btn-sm w-fit"
                        disabled=move || on_grant.is_none() || pending.get()
                        on:click=move |_| grant.run(())
                    >
                        {move || {
                            if pending.get() { "Waiting for the instance…" } else { "Grant admin access" }
                        }}
                    </button>
                    <Show when=move || pending.get()>
                        <p class="text-xs opacity-60">
                            "Approve the request in the instance's own window; this page comes back here when it is done."
                        </p>
                    </Show>
                </div>
            </div>
        </Show>

        <Show when=move || matches!(standing.get(), Some(StaffStanding::NotStaff))>
            <div class="card border border-base-300 bg-base-100 shadow-sm">
                <div class="card-body gap-2 p-4">
                    <h2 class="card-title text-base">"Not a moderator here"</h2>
                    <p class="text-sm opacity-70">
                        "This instance does not make your account staff — an administrator
                         has to grant it."
                    </p>
                    <p class="text-sm opacity-70">
                        "A token is not the missing piece: this account has no staff role, so
                         a grant asked for here would be refused as well. Once an
                         administrator has granted the role, checking again is enough."
                    </p>
                    <button
                        class="btn btn-sm btn-ghost w-fit"
                        disabled=move || on_retry.is_none()
                        on:click=move |_| retry.run(())
                    >
                        "Check again"
                    </button>
                </div>
            </div>
        </Show>

        <Show when=move || matches!(standing.get(), Some(StaffStanding::Refused))>
            <div class="card border border-base-300 bg-base-100 shadow-sm">
                <div class="card-body gap-2 p-4">
                    <h2 class="card-title text-base">"This grant was refused"</h2>
                    <p class="text-sm opacity-70">
                        "The instance says your account is staff, but it did not accept the
                         token this browser is holding for it. Tokens are revoked on the
                         instance, and they also expire; granting a fresh one is the way
                         back."
                    </p>
                    <button
                        class="btn btn-sm w-fit"
                        disabled=move || on_grant.is_none() || pending.get()
                        on:click=move |_| grant.run(())
                    >
                        {move || {
                            if pending.get() { "Waiting for the instance…" } else { "Grant admin access again" }
                        }}
                    </button>
                </div>
            </div>
        </Show>

        <Show when=move || matches!(standing.get(), Some(StaffStanding::Unknown))>
            <div class="card border border-base-300 bg-base-100 shadow-sm">
                <div class="card-body gap-2 p-4">
                    <h2 class="card-title text-base">"Could not ask this instance"</h2>
                    <p class="text-sm opacity-70">
                        "The question — what may this account do here — did not reach the
                         server, so nothing has been decided about your access. This is
                         usually a connection or a session that ended, not a permission
                         problem."
                    </p>
                    <button
                        class="btn btn-sm w-fit"
                        disabled=move || on_retry.is_none() || pending.get()
                        on:click=move |_| retry.run(())
                    >
                        "Try again"
                    </button>
                </div>
            </div>
        </Show>

        <Show when=move || matches!(standing.get(), Some(StaffStanding::Yes))>
            {body.with_value(|body| body())}
        </Show>
    }
}
