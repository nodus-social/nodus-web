//! Carrying moderation settings between instances: out as one document, in as one document.
//!
//! Both halves answer the same question — what is actually in this file? — because an
//! operator who exports nine empty sections, or applies a document whose blocks were
//! refused, should learn it from the screen and not from the instance afterwards. So the
//! export counts what it downloaded, and the import counts before it writes, showing every
//! refusal with the instance's own reason and holding Apply until those reasons are read.

use leptos::prelude::*;

use crate::components::panel::Panel;

/// The sections a settings document can carry, and the words this console uses for them.
///
/// The order is the instance's own — the order an import applies them in — so two exports
/// read the same way and a report lists the sections the way the document does.
pub const SETTINGS_SECTIONS: [(&str, &str); 9] = [
    ("rules", "Moderation rules"),
    ("instance_rules", "Instance rules"),
    ("warning_presets", "Warning presets"),
    ("escalation", "Escalation ladder"),
    ("domain_pauses", "Domain pauses"),
    ("domain_blocks", "Domain blocks"),
    ("email_domain_blocks", "Email domain blocks"),
    ("scanner_rules", "Scanner rules"),
    ("scanner_sets", "Scanner sets"),
];

/// The word this console uses for a section's wire name.
#[must_use]
pub fn label_of(key: &str) -> String {
    SETTINGS_SECTIONS
        .iter()
        .find(|(name, _)| *name == key)
        .map_or_else(|| key.to_owned(), |(_, label)| (*label).to_owned())
}

/// One section an export can ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsChoice {
    /// The section's wire name.
    pub key: String,
    /// The word on the checkbox.
    pub label: String,
    /// Whether this export includes it.
    pub chosen: bool,
}

/// What a downloaded document turned out to hold.
///
/// `sections` carries only the sections the instance actually returned. A section asked
/// for and absent is listed by the page as missing rather than as zero, because "the
/// document says nothing about pauses" and "the document says there are no pauses" are
/// different things to a person about to restore an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsCarried {
    /// The document's name.
    pub format: String,
    /// The version it was written in.
    pub version: u64,
    /// The instance it came from.
    pub instance: Option<String>,
    /// How many entries it carries per label, and how many in total.
    pub sections: Vec<(String, usize)>,
    /// The sum of every section's count.
    pub total: usize,
}

/// One row an import refused, with the instance's whole reason for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsConflictLine {
    /// The section the row belongs to, in this console's words.
    pub section: String,
    /// The row's own name.
    pub name: String,
    /// Why it was not applied.
    pub reason: String,
}

/// What an import would do to one section, or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSectionReport {
    /// The section, in this console's words.
    pub label: String,
    /// Rows the instance does not hold yet.
    pub created: u64,
    /// Rows it holds and would change.
    pub updated: u64,
    /// Rows it already matches.
    pub unchanged: u64,
    /// Rows it cannot apply.
    pub skipped: u64,
    /// Every row it refused, each with its reason.
    pub conflicts: Vec<SettingsConflictLine>,
}

/// A whole import's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsImportReport {
    /// Whether the instance only counted.
    pub dry_run: bool,
    /// One row per section the document carried.
    pub sections: Vec<SettingsSectionReport>,
    /// How many rows were refused, across every section.
    pub conflicts: usize,
}

impl SettingsImportReport {
    /// The rows each section would write, added up.
    #[must_use]
    pub fn written(&self) -> u64 {
        self.sections
            .iter()
            .map(|section| section.created + section.updated)
            .sum()
    }
}

/// One document in, checked, and only then written.
///
/// The apply is the press that changes a running instance, and a document can carry a
/// refusal — a block whose severity would drop — that the operator needs to read before
/// deciding. So the dry run always comes first, its counts are what Apply agrees to, and a
/// document with conflicts holds Apply shut until the box beside them is ticked.
#[component]
pub fn SettingsTransferPage(
    /// The sections an export can ask for, and which are ticked.
    #[prop(into)]
    choices: Signal<Vec<SettingsChoice>>,
    /// What the last downloaded document held, when one has arrived.
    #[prop(into)]
    carried: Signal<Option<SettingsCarried>>,
    /// The document as pasted, or as a file was read into.
    #[prop(into)]
    text: Signal<String>,
    /// The last check or apply, when one has answered.
    #[prop(into)]
    report: Signal<Option<SettingsImportReport>>,
    /// Whether an export is on its way.
    #[prop(into)]
    exporting: Signal<bool>,
    /// Whether a check is on its way.
    #[prop(into)]
    checking: Signal<bool>,
    /// Whether an apply is on its way.
    #[prop(into)]
    applying: Signal<bool>,
    /// Whether the operator says they have read the refusals.
    #[prop(into)]
    acknowledged: Signal<bool>,
    /// The last refusal, in the instance's own words where it had any.
    #[prop(into)]
    error: Signal<Option<String>>,
    /// Ticks or unticks one section.
    #[prop(optional)]
    on_toggle: Option<Callback<String>>,
    /// Downloads the ticked sections. Absent where nothing can be sent.
    #[prop(optional)]
    on_export: Option<Callback<()>>,
    /// Records the document text.
    #[prop(optional)]
    on_text: Option<Callback<String>>,
    /// Counts the document without writing it.
    #[prop(optional)]
    on_check: Option<Callback<()>>,
    /// Writes the checked document.
    #[prop(optional)]
    on_apply: Option<Callback<()>>,
    /// Records the acknowledgement.
    #[prop(optional)]
    on_acknowledge: Option<Callback<bool>>,
    /// The file input the caller owns, so the bytes are read where the browser is.
    children: ChildrenFn,
    /// Whether this browser's grant carries `admin:write`. Absent means "assume yes".
    #[prop(optional_no_strip)]
    write: Option<Signal<bool>>,
) -> impl IntoView {
    let write = crate::props::writable(write);
    let none_chosen = move || choices.get().iter().all(|choice| !choice.chosen);
    // The dry run is the only thing Apply may act on, and it describes the text beside it:
    // a document edited after its check has no report, and the counters above the button
    // are gone rather than stale.
    let checked = move || report.get().is_some_and(|report| report.dry_run);
    let conflicts = move || report.get().is_some_and(|report| report.conflicts > 0);
    let held = move || conflicts() && !acknowledged.get();
    view! {
        <div class="flex flex-col gap-3">
            <Show when=move || !write.get()>
                <crate::components::chrome::Note class="text-sm text-warning">
                    {crate::props::NO_WRITE}
                </crate::components::chrome::Note>
            </Show>

            <Show when=move || error.get().is_some()>
                <p class="text-sm text-error" role="alert">
                    {move || error.get().unwrap_or_default()}
                </p>
            </Show>

            <Panel
                title="Carry settings out"
                description="One document holding the sections ticked here. What it carries is counted beside the button before it leaves, so an empty section is known before it is restored somewhere else."
            >
                <fieldset class="flex flex-col gap-1">
                    <legend class="mb-1 text-sm font-medium">"Sections"</legend>
                    <For each=move || choices.get() key=|choice| choice.key.clone() let:choice>
                        {
                            let key = StoredValue::new(choice.key.clone());
                            let chosen = choice.chosen;
                            view! {
                                <label class="flex items-center gap-2 text-sm">
                                    <input
                                        type="checkbox"
                                        class="checkbox checkbox-sm"
                                        prop:checked=chosen
                                        disabled=on_toggle.is_none()
                                        on:change=move |_| {
                                            if let Some(on_toggle) = on_toggle {
                                                on_toggle.run(key.get_value());
                                            }
                                        }
                                    />
                                    <span>{choice.label.clone()}</span>
                                </label>
                            }
                        }
                    </For>
                </fieldset>

                <div class="flex flex-wrap items-center gap-2">
                    <button
                        class="btn btn-ghost btn-sm"
                        disabled=move || exporting.get() || none_chosen() || on_export.is_none()
                        on:click=move |_| {
                            if let Some(on_export) = on_export {
                                on_export.run(());
                            }
                        }
                    >
                        {move || if exporting.get() { "Exporting…" } else { "Export document" }}
                    </button>
                    <Show when=move || none_chosen()>
                        <p class="text-sm opacity-60">"Tick at least one section."</p>
                    </Show>
                </div>

                {move || {
                    carried
                        .get()
                        .map(|carried| {
                            let total = carried.total;
                            let format = carried.format.clone();
                            let version = carried.version;
                            let instance = carried.instance.clone();
                            let sections = carried.sections.clone();
                            view! {
                                <div class="rounded-box border border-base-300 bg-base-200/60 p-3">
                                    <p class="mb-2 text-sm font-medium">
                                        {format!("The document holds {total} {}.", if total == 1 { "entry" } else { "entries" })}
                                    </p>
                                    <p class="mb-2 text-xs opacity-60">
                                        {format!("format {format}, version {version}")}
                                        {instance.map(|instance| format!(", from {instance}"))}
                                    </p>
                                    <Show when=move || total == 0>
                                        <p class="mb-2 text-sm text-warning" role="status">
                                            "Every section in this document is empty. Restoring it would change nothing."
                                        </p>
                                    </Show>
                                    <dl class="grid grid-cols-2 gap-2 text-sm sm:grid-cols-3">
                                        {sections
                                            .iter()
                                            .map(|(label, count)| {
                                                let label = label.clone();
                                                let count = *count;
                                                view! {
                                                    <div class="flex items-baseline gap-2" data-testid="settings-count">
                                                        <dt class="truncate text-xs opacity-60">{label.clone()}</dt>
                                                        <dd class="font-semibold">{count}</dd>
                                                    </div>
                                                }
                                            })
                                            .collect_view()}
                                    </dl>
                                </div>
                            }
                        })
                }}
            </Panel>

            <Panel
                title="Bring settings in"
                description="Paste a document or choose the file. It is counted first: every section's rows, and every row the instance would refuse, before anything is written."
            >
                <textarea
                    class="textarea textarea-bordered min-h-32 font-mono text-xs"
                    placeholder="{\"format\":\"nodus-admin-settings\",\"version\":1,…}"
                    aria-label="Settings document"
                    prop:value=move || text.get()
                    disabled=on_text.is_none()
                    on:input=move |event| {
                        if let Some(on_text) = on_text {
                            on_text.run(event_target_value(&event));
                        }
                    }
                />
                <div class="flex flex-wrap items-center gap-2">
                    {children()}
                    <button
                        class="btn btn-ghost btn-sm"
                        disabled=move || checking.get() || on_check.is_none() || text.get().trim().is_empty()
                        on:click=move |_| {
                            if let Some(on_check) = on_check {
                                on_check.run(());
                            }
                        }
                    >
                        {move || if checking.get() { "Checking…" } else { "Check document" }}
                    </button>
                </div>

                {move || {
                    report
                        .get()
                        .map(|report| {
                            let dry_run = report.dry_run;
                            let written = report.written();
                            let sections = report.sections.clone();
                            let refused = report.conflicts;
                            let lines = sections
                                .iter()
                                .flat_map(|section| section.conflicts.clone())
                                .collect::<Vec<SettingsConflictLine>>();
                            let count = sections.len();
                            view! {
                                <div class="rounded-box border border-base-300 bg-base-200/60 p-3">
                                    <p class="mb-2 text-sm font-medium">
                                        {if dry_run {
                                            "Nothing has been written yet.".to_owned()
                                        } else {
                                            format!("The document was applied: {written} written.")
                                        }}
                                    </p>
                                    <p class="mb-2 text-xs opacity-60">
                                        {format!("{count} {} in the document", if count == 1 { "section" } else { "sections" })}
                                    </p>
                                    <table class="table table-sm">
                                        <thead>
                                            <tr>
                                                <th>"Section"</th>
                                                <th>"Created"</th>
                                                <th>"Updated"</th>
                                                <th>"Unchanged"</th>
                                                <th>"Skipped"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {sections
                                                .iter()
                                                .map(|section| {
                                                    view! {
                                                        <tr>
                                                            <td class="text-sm">{section.label.clone()}</td>
                                                            <td>{section.created}</td>
                                                            <td>{section.updated}</td>
                                                            <td>{section.unchanged}</td>
                                                            <td>{section.skipped}</td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>

                                    <Show when=move || { refused > 0 }>
                                        <p class="mt-3 text-sm font-medium">
                                            {format!(
                                                "{refused} {} not applied",
                                                if refused == 1 { "row was" } else { "rows were" }
                                            )}
                                        </p>
                                        // One line each, with the reason whole: a block whose
                                        // severity would drop is refused for a reason the
                                        // operator has to be able to read before agreeing.
                                        <ul class="mt-1 flex flex-col gap-2 text-sm">
                                            {lines
                                                .iter()
                                                .map(|line| {
                                                    view! {
                                                        <li class="rounded-box bg-base-100 p-2" data-testid="settings-conflict">
                                                            <p class="font-medium">
                                                                {format!("{}: {}", line.section.clone(), line.name.clone())}
                                                            </p>
                                                            <p class="text-error">{line.reason.clone()}</p>
                                                        </li>
                                                    }
                                                })
                                                .collect_view()}
                                        </ul>
                                    </Show>
                                </div>
                            }
                        })
                }}

                <div class="flex flex-wrap items-center gap-2">
                    <Show when=move || conflicts() && checked()>
                        <label class="flex items-center gap-2 text-sm">
                            <input
                                type="checkbox"
                                class="checkbox checkbox-sm"
                                prop:checked=move || acknowledged.get()
                                disabled=on_acknowledge.is_none()
                                on:change=move |event| {
                                    if let Some(on_acknowledge) = on_acknowledge {
                                        on_acknowledge.run(event_target_checked(&event));
                                    }
                                }
                            />
                            <span>"I have read why these rows were refused, and want the rest applied."</span>
                        </label>
                    </Show>
                    <button
                        class="btn btn-primary btn-sm"
                        disabled=move || applying.get() || !checked() || held() || on_apply.is_none()
                        title=move || {
                            if !checked() {
                                "Check the document first: Apply acts on the counts above it."
                            } else if held() {
                                "Read the refused rows and tick the box beside them."
                            } else {
                                ""
                            }
                        }
                        on:click=move |_| {
                            if let Some(on_apply) = on_apply {
                                on_apply.run(());
                            }
                        }
                    >
                        {move || if applying.get() { "Applying…" } else { "Apply" }}
                    </button>
                    <Show when=move || !checked()>
                        <p class="text-sm opacity-60">"Check the document to see what it would change."</p>
                    </Show>
                </div>
            </Panel>
        </div>
    }
}
