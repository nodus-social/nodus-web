//! Mail domains this instance refuses.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_client_core::model::EmailDomainBlock;
use nodus_pages::pages::admin::email::EmailDomainsPage;

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// The mail domains this instance refuses, and the one being added.
#[component]
pub fn EmailDomains() -> impl IntoView {
    let alive = Alive::new();
    let (items, set_items) = signal(Vec::<EmailDomainBlock>::new());
    let (domain, set_domain) = signal(String::new());
    let (allow_registration, set_allow_registration) = signal(false);
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);

    let reload = Callback::new(move |()| {
        spawn_local(async move {
            let outcome = admin::email_domain_blocks().await;
            alive.if_alive(|| {
                set_busy.set(true);
                match outcome {
                    Ok(list) => {
                        set_items.set(list);
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| reload.run(()));

    let block = Callback::new(move |()| {
        let name = domain.get_untracked().trim().to_lowercase();
        if !name.contains('.') || name.contains('@') {
            set_error.set(Some(
                "Enter the mail domain only, such as example.com.".to_owned(),
            ));
            return;
        }
        let allow = allow_registration.get_untracked();
        spawn_local(async move {
            match admin::create_email_domain_block(&name, allow).await {
                Ok(_) => {
                    alive.if_alive(|| {
                        set_domain.set(String::new());
                        set_error.set(None);
                    });
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let withdraw = Callback::new(move |id: String| {
        spawn_local(async move {
            if admin::delete_email_domain_block(&id).await.is_ok() {
                reload.run(());
            }
        });
    });

    view! {
        <EmailDomainsPage
            items=items
            domain=domain
            allow_registration=allow_registration
            busy=busy
            error=error
            on_domain=Callback::new(move |value: String| set_domain.set(value))
            on_allow_registration=Callback::new(move |value: bool| set_allow_registration.set(value))
            on_block=block
            on_withdraw=withdraw
            write=Some(crate::state::bridge::admin::may_write())
        />
    }
}
