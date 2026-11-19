//! Carrying an instance's moderation settings out as one document, and back in.
//!
//! Both halves are the same request and the same file: what the export counted is what an
//! import is asked about. The write is deliberately two presses — a count with `dry_run`,
//! then the write that count described — and every refusal the instance reports keeps its
//! own sentence, because the reason a block is refused is the thing the operator has to
//! read before agreeing to the rest.

use leptos::prelude::*;
use leptos::task::spawn_local;

use nodus_pages::pages::admin::settings_transfer::{
    SettingsCarried, SettingsChoice, SettingsConflictLine, SettingsImportReport,
    SettingsSectionReport, SettingsTransferPage as SettingsTransferView,
};

use crate::platform::download::save_bytes;
use crate::state::alive::Alive;
use crate::state::bridge::admin::{
    self, SettingsDocument, SettingsOutcome, SETTINGS_SECTIONS,
};

use super::read_document_file;

/// The sections an export offers, every one of them ticked to begin with.
fn fresh_choices() -> Vec<SettingsChoice> {
    admin::SETTINGS_SECTIONS
        .iter()
        .map(|key| SettingsChoice {
            key: (*key).to_owned(),
            label: label_of(key),
            chosen: true,
        })
        .collect()
}

/// The words this console uses for a section, so the bridge and the page agree.
fn label_of(key: &str) -> String {
    nodus_pages::pages::admin::settings_transfer::label_of(key)
}

/// What a downloaded document held, section by section.
fn carried_of(document: &SettingsDocument) -> SettingsCarried {
    let sections = document
        .counts
        .iter()
        .map(|count| (label_of(&count.key), count.count))
        .collect::<Vec<_>>();
    let total = sections.iter().map(|(_, count)| count).sum();
    SettingsCarried {
        format: document.format.clone(),
        version: document.version,
        instance: document.instance.clone(),
        sections,
        total,
    }
}

/// One section's answer, with its refusals named and its counts kept.
fn report_of(sections: &[(String, SettingsOutcome)], dry_run: bool) -> SettingsImportReport {
    let sections: Vec<SettingsSectionReport> = sections
        .iter()
        .map(|(key, outcome)| SettingsSectionReport {
            label: label_of(key),
            created: outcome.created,
            updated: outcome.updated,
            unchanged: outcome.unchanged,
            skipped: outcome.skipped,
            conflicts: outcome
                .conflicts
                .iter()
                .map(|conflict| SettingsConflictLine {
                    section: label_of(if conflict.section.is_empty() {
                        key
                    } else {
                        &conflict.section
                    }),
                    name: conflict.name.clone(),
                    reason: conflict.reason.clone(),
                })
                .collect(),
        })
        .collect();
    let conflicts = sections.iter().map(|section| section.conflicts.len()).sum();
    SettingsImportReport {
        dry_run,
        sections,
        conflicts,
    }
}

/// The sections ticked, as the wire names the import asks for.
fn chosen_keys(choices: &[SettingsChoice]) -> Vec<String> {
    let ticked = choices
        .iter()
        .filter(|choice| choice.chosen)
        .map(|choice| choice.key.clone())
        .collect::<Vec<_>>();
    // Nothing ticked is every section: the page keeps Export shut in that state, and an
    // import of every section is what the instance reads from a missing list too.
    if ticked.len() == SETTINGS_SECTIONS.len() {
        Vec::new()
    } else {
        ticked
    }
}

/// Settings out as one document, and back in as one document.
#[component]
pub fn SettingsTransfer() -> impl IntoView {
    let alive = Alive::new();
    let (choices, set_choices) = signal(fresh_choices());
    let (carried, set_carried) = signal(None::<SettingsCarried>);
    let (text, set_text) = signal(String::new());
    let (report, set_report) = signal(None::<SettingsImportReport>);
    let (exporting, set_exporting) = signal(false);
    let (checking, set_checking) = signal(false);
    let (applying, set_applying) = signal(false);
    let (acknowledged, set_acknowledged) = signal(false);
    let (error, set_error) = signal(None::<String>);

    let toggle = Callback::new(move |key: String| {
        set_choices.update(|choices| {
            if let Some(choice) = choices.iter_mut().find(|choice| choice.key == key) {
                choice.chosen = !choice.chosen;
            }
        });
    });

    let export = Callback::new(move |()| {
        let sections = chosen_keys(&choices.get_untracked());
        set_exporting.set(true);
        set_error.set(None);
        spawn_local(async move {
            match admin::settings_export(&sections).await {
                Ok(document) => {
                    // The saved file is the instance's own bytes: re-encoding them would
                    // make the document the client's rather than the instance's.
                    let name = "nodus-admin-settings.json";
                    let outcome = save_bytes(&document.raw, name).map(|()| carried_of(&document));
                    alive.if_alive(|| {
                        match outcome {
                            Ok(carried) => set_carried.set(Some(carried)),
                            Err(message) => set_error.set(Some(message)),
                        }
                        set_exporting.set(false);
                    });
                }
                Err(message) => alive.if_alive(|| {
                    set_error.set(Some(message));
                    set_exporting.set(false);
                }),
            }
        });
    });

    // A document that changed, either pasted or chosen as a file, is no longer the one that
    // was counted: the report goes, and with it the acknowledgement and the Apply it allowed.
    let record = Callback::new(move |value: String| {
        set_report.set(None);
        set_acknowledged.set(false);
        set_text.set(value);
    });

    let choose_file = Callback::new(move |file: Option<web_sys::File>| {
        let Some(file) = file else { return };
        spawn_local(async move {
            match read_document_file(file).await {
                Ok(value) => alive.if_alive(|| {
                    set_report.set(None);
                    set_acknowledged.set(false);
                    set_text.set(value);
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
        });
    });

    // The dry run: the whole point of the half. Counted before anything is offered for
    // writing, with the instance's 422 — an unknown format or version — kept word for word.
    let check = Callback::new(move |()| {
        let document = text.get_untracked();
        if document.trim().is_empty() {
            return;
        }
        let sections = chosen_keys(&choices.get_untracked());
        set_checking.set(true);
        set_error.set(None);
        set_acknowledged.set(false);
        spawn_local(async move {
            match admin::settings_import(&document, &sections, true).await {
                Ok(answer) => alive.if_alive(|| {
                    set_report.set(Some(report_of(&answer.sections, true)));
                }),
                Err(message) => alive.if_alive(|| {
                    set_report.set(None);
                    set_error.set(Some(message));
                }),
            }
            alive.if_alive(|| set_checking.set(false));
        });
    });

    let apply = Callback::new(move |()| {
        let document = text.get_untracked();
        let sections = chosen_keys(&choices.get_untracked());
        set_applying.set(true);
        set_error.set(None);
        spawn_local(async move {
            match admin::settings_import(&document, &sections, false).await {
                Ok(answer) => alive.if_alive(|| {
                    set_report.set(Some(report_of(&answer.sections, false)));
                    set_acknowledged.set(false);
                }),
                Err(message) => alive.if_alive(|| set_error.set(Some(message))),
            }
            alive.if_alive(|| set_applying.set(false));
        });
    });

    let acknowledge = Callback::new(move |value: bool| set_acknowledged.set(value));

    view! {
        <SettingsTransferView
            choices=choices
            carried=carried
            text=text
            report=report
            exporting=exporting
            checking=checking
            applying=applying
            acknowledged=acknowledged
            error=error
            on_toggle=toggle
            on_export=export
            on_text=record
            on_check=check
            on_apply=apply
            on_acknowledge=acknowledge
            write=Some(admin::may_write())
        >
            <label class="btn btn-ghost btn-sm">
                "Choose file"
                <input
                    type="file"
                    class="hidden"
                    accept=".json,application/json"
                    on:change=move |event| {
                        let input = event_target::<web_sys::HtmlInputElement>(&event);
                        choose_file.run(input.files().and_then(|files| files.get(0)));
                        input.set_value("");
                    }
                />
            </label>
        </SettingsTransferView>
    }
}
