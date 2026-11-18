//! Domains the instance refuses.

use leptos::prelude::*;
use leptos::task::spawn_local;
use nodus_client_core::model::{DomainBlock, DomainBlockDraft, DomainSeverity};
use nodus_pages::pages::moderation::federation::FederationPage;

use crate::state::alive::Alive;
use crate::state::bridge::admin;

/// Blocked domains, with an add form.
#[component]
pub(super) fn BlockedList() -> impl IntoView {
    let alive = Alive::new();
    let (blocks, set_blocks) = signal(Vec::<DomainBlock>::new());
    let (domain, set_domain) = signal(String::new());
    let (suspend, set_suspend) = signal(true);
    let (busy, set_busy) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (import_list, set_import_list) = signal(String::new());
    let (imported, set_imported) = signal(None::<String>);

    let reload = Callback::new(move |()| {
        spawn_local(async move {
            let outcome = admin::domain_blocks().await;
            alive.if_alive(|| {
                set_busy.set(true);
                match outcome {
                    Ok(list) => {
                        set_blocks.set(list);
                        set_error.set(None);
                    }
                    Err(message) => set_error.set(Some(message)),
                }
                set_busy.set(false);
            });
        });
    });

    Effect::new(move |_| reload.run(()));

    let add = Callback::new(move |()| {
        let name = domain.get_untracked().trim().to_lowercase();
        if name.is_empty() {
            set_error.set(Some("Name the domain to block.".to_owned()));
            return;
        }
        let severity = if suspend.get_untracked() {
            DomainSeverity::Suspend
        } else {
            DomainSeverity::Silence
        };
        spawn_local(async move {
            match admin::create_domain_block(&name, severity).await {
                Ok(_) => {
                    alive.if_alive(|| set_domain.set(String::new()));
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let remove = Callback::new(move |id: String| {
        spawn_local(async move {
            // A block that could not be removed is still in force; the list is the record
            // of that.
            if admin::delete_domain_block(&id).await.is_ok() {
                reload.run(());
            }
        });
    });

    // A severity change is a rewrite of the row, not a new block: the draft carries the
    // flags and the comment the row already had so the update does not erase them.
    let set_severity = Callback::new(move |(id, severity): (String, DomainSeverity)| {
        let Some(block) = blocks
            .get_untracked()
            .into_iter()
            .find(|held| held.id == id)
        else {
            return;
        };
        let draft = DomainBlockDraft {
            domain: block.domain,
            severity,
            reject_media: block.reject_media,
            reject_reports: block.reject_reports,
            obfuscate: block.obfuscate,
            public_comment: block.public_comment,
        };
        spawn_local(async move {
            match admin::update_domain_block(&id, &draft).await {
                Ok(updated) => alive.if_alive(move || {
                    set_blocks.update(move |held| {
                        if let Some(slot) = held.iter_mut().find(|held| held.id == updated.id) {
                            *slot = updated;
                        }
                    });
                    set_error.set(None);
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    let import_now = Callback::new(move |()| {
        let pasted = import_list.get_untracked();
        let domains: Vec<String> = pasted
            .lines()
            .map(|line| line.trim().trim_start_matches('#').trim().to_lowercase())
            .filter(|line| line.contains('.') && !line.contains(char::is_whitespace))
            .collect();
        if domains.is_empty() {
            set_error.set(Some("Paste at least one domain, one per line.".to_owned()));
            return;
        }
        let severity = if suspend.get_untracked() {
            DomainSeverity::Suspend
        } else {
            DomainSeverity::Silence
        };
        spawn_local(async move {
            let names: Vec<&str> = domains.iter().map(String::as_str).collect();
            match admin::import_domain_blocks(&names, severity).await {
                Ok(outcome) => {
                    alive.if_alive(|| {
                        set_imported.set(Some(match outcome.skipped.len() {
                            0 => format!("{} blocked.", outcome.blocked),
                            skipped => format!(
                                "{} blocked; {skipped} line(s) were not a domain.",
                                outcome.blocked
                            ),
                        }));
                        set_import_list.set(String::new());
                        set_error.set(None);
                    });
                    reload.run(());
                }
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    view! {
        <FederationPage
            blocks=blocks
            domain=domain
            suspend=suspend
            busy=busy
            error=error
            on_domain=Callback::new(move |value: String| set_domain.set(value))
            on_suspend=Callback::new(move |entire: bool| set_suspend.set(entire))
            on_block=add
            on_remove=remove
            on_severity=set_severity
            write=Some(admin::may_write())
            on_import=Callback::new(move |value: String| set_import_list.set(value))
            on_import_now=import_now
            imported=imported
        />
    }
}
