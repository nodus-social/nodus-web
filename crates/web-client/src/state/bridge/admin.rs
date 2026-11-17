//! Moderation calls, and the one question that decides whether to show them.

use super::{admin_transport, instance_transport};
use nodus_client_core::actions::{AdminActions, RegistrationActions};
use nodus_client_core::model::{
    AccountDetail, AccountNote, AdminAccount, AdminAccountQuery, AdminAlert, AdminMeasure,
    AdminMeta, AdminOverview, Announcement, AnnouncementDraft, AssetUpload, AuditEntry,
    DomainAllow, DomainBlock, DomainImportOutcome, DomainSeverity, EmailDomainBlock, FlagState,
    ModerationAction, NewScanner, NodeLog, Report, ReportFilter, ReportState, ScanFlag,
    ScanSeverity, Scanner, ScannerAsset, ScannerRule, Topology, TriggerOutcome,
    WarningPreset,
};

/// How many rows a moderation screen asks for.
///
/// Small on purpose: the queue is a work list a human reads, and a moderator who scrolls
/// past fifty reports has stopped reading them.
pub const PAGE: u16 = 20;

/// What the moderation screens can offer right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Staff {
    /// The token opened the report queue: the panels work.
    Yes,
    /// The instance refused, with the probe's own reason.
    ///
    /// The text is kept so a stale grant can be told apart from an account that never
    /// had the role, and so the screen can offer a fresh grant instead of dead-ending.
    No(String),
    /// No admin token was granted, so nothing can be said yet.
    NeedsGrant,
}

/// What this session may moderate, asked by trying.
///
/// Not by reading a role: `role` is a field whose shape changed between versions, and a
/// probe that fails is the same 403 the real screens would hit. The distinction the
/// caller needs is "no grant yet" versus "the instance said no", so a missing grant is
/// reported as such instead of being flattened into `false`.
pub async fn staff() -> Staff {
    let Some(transport) = admin_transport() else {
        return Staff::NeedsGrant;
    };
    match transport.reports(ReportState::Open, 1).await {
        Ok(_) => Staff::Yes,
        Err(error) => Staff::No(format!(
            "The instance refused access to the report queue: {error}"
        )),
    }
}

/// Recent administrative actions.
pub async fn audit() -> Result<Vec<AuditEntry>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .audit(PAGE)
        .await
        .map_err(|error| error.to_string())
}

/// Announcements staff can see, published or not.
pub async fn admin_announcements() -> Result<Vec<Announcement>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .admin_announcements()
        .await
        .map_err(|error| error.to_string())
}

/// Publish an announcement.
pub async fn create_announcement(draft: AnnouncementDraft) -> Result<Announcement, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .create_announcement(draft)
        .await
        .map_err(|error| error.to_string())
}

/// Withdraw an announcement.
pub async fn delete_announcement(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .delete_announcement(id)
        .await
        .map_err(|error| error.to_string())
}

/// Reopens a handled report.
pub async fn reopen_report(id: &str) -> Result<Report, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .reopen_report(id)
        .await
        .map_err(|error| error.to_string())
}

/// Applies a decision to one account.
pub async fn moderate(id: &str, action: ModerationAction) -> Result<AdminAccount, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .moderate_account(id, action)
        .await
        .map_err(|error| error.to_string())
}

/// Domains the instance refuses.
pub async fn domain_blocks() -> Result<Vec<DomainBlock>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .domain_blocks(PAGE)
        .await
        .map_err(|error| error.to_string())
}

/// Refuses a domain.
pub async fn create_domain_block(
    domain: &str,
    severity: DomainSeverity,
) -> Result<DomainBlock, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .create_domain_block(domain, severity)
        .await
        .map_err(|error| error.to_string())
}

/// Stops refusing a domain.
pub async fn delete_domain_block(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .delete_domain_block(id)
        .await
        .map_err(|error| error.to_string())
}

/// Dashboard counts for the last `days` days.
///
/// The two dates are computed here rather than in the SDK: `js_sys::Date` is the only
/// clock this crate has, and the API wants `YYYY-MM-DD` strings.
pub async fn measures(keys: &[&str], days: f64) -> Result<Vec<AdminMeasure>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let end = js_sys::Date::new_0();
    let start = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(
        end.get_time() - days * 86_400_000.0,
    ));
    transport
        .measures(keys, &day(&start), &day(&end))
        .await
        .map_err(|error| error.to_string())
}

/// The day part of an instant, which is what the measures endpoint accepts.
fn day(date: &js_sys::Date) -> String {
    String::from(date.to_iso_string())
        .chars()
        .take(10)
        .collect()
}

/// Domains allowed through a general block.
pub async fn domain_allows() -> Result<Vec<DomainAllow>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .domain_allows(PAGE)
        .await
        .map_err(|error| error.to_string())
}

/// Lets a domain through.
pub async fn create_domain_allow(domain: &str) -> Result<DomainAllow, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .create_domain_allow(domain)
        .await
        .map_err(|error| error.to_string())
}

/// Withdraws an allow.
pub async fn delete_domain_allow(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .delete_domain_allow(id)
        .await
        .map_err(|error| error.to_string())
}

/// Accounts waiting for a moderator to decide.
pub async fn pending_accounts() -> Result<Vec<AdminAccount>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .pending_accounts(PAGE)
        .await
        .map_err(|error| error.to_string())
}

/// Lets an account in.
pub async fn approve_account(id: &str) -> Result<AdminAccount, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .approve_account(id)
        .await
        .map_err(|error| error.to_string())
}

/// Refuses an account.
pub async fn reject_account(id: &str) -> Result<AdminAccount, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .reject_account(id)
        .await
        .map_err(|error| error.to_string())
}

/// One page of the report queue, narrowed by the filter.
pub async fn reports_page(filter: &ReportFilter) -> Result<Vec<Report>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .reports_page(filter)
        .await
        .map_err(|error| error.to_string())
}

/// One report with its accounts and statuses filled in.
pub async fn report(id: &str) -> Result<Report, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .report(id)
        .await
        .map_err(|error| error.to_string())
}

/// Marks a report handled, recording why.
pub async fn resolve_report_with_note(id: &str, note: &str) -> Result<Report, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .resolve_report_with_note(id, note)
        .await
        .map_err(|error| error.to_string())
}

/// Claims a report for an account, or puts it back in the unclaimed queue.
pub async fn assign_report(id: &str, account_id: Option<&str>) -> Result<Report, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .assign_report(id, account_id)
        .await
        .map_err(|error| error.to_string())
}

/// The account directory, narrowed by the query.
pub async fn search_accounts(query: &AdminAccountQuery) -> Result<Vec<AdminAccount>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .search_accounts(query)
        .await
        .map_err(|error| error.to_string())
}

/// One account with its notes, flags and report count.
pub async fn account_detail(id: &str) -> Result<AccountDetail, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .account_detail(id)
        .await
        .map_err(|error| error.to_string())
}

/// Adds a private note to an account.
pub async fn add_account_note(id: &str, body: &str) -> Result<AccountNote, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .add_account_note(id, body)
        .await
        .map_err(|error| error.to_string())
}

/// Deletes one of an account's notes.
pub async fn delete_account_note(id: &str, note_id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .delete_account_note(id, note_id)
        .await
        .map_err(|error| error.to_string())
}

/// Applies a decision, recording why and optionally resolving the report behind it.
pub async fn moderate_with_note(
    id: &str,
    action: ModerationAction,
    note: &str,
    report_id: Option<&str>,
) -> Result<AdminAccount, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .moderate_account_with_note(id, action, note, report_id)
        .await
        .map_err(|error| error.to_string())
}

/// Grants or revokes administrator rights.
pub async fn set_staff(id: &str, staff: bool) -> Result<AdminAccount, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .set_staff(id, staff)
        .await
        .map_err(|error| error.to_string())
}

/// Allows or refuses lossless uploads from one account.
pub async fn set_lossless(id: &str, images: bool, videos: bool) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .set_lossless(id, images, videos)
        .await
        .map_err(|error| error.to_string())
}

/// Deletes the account and everything it owns. Irreversible.
pub async fn purge_account(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .purge_account(id)
        .await
        .map_err(|error| error.to_string())
}

/// Blocks many domains at one severity.
pub async fn import_domain_blocks(
    domains: &[&str],
    severity: DomainSeverity,
) -> Result<DomainImportOutcome, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .import_domain_blocks(domains, severity)
        .await
        .map_err(|error| error.to_string())
}

/// Mail domains registrations are refused from.
pub async fn email_domain_blocks() -> Result<Vec<EmailDomainBlock>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .email_domain_blocks(PAGE)
        .await
        .map_err(|error| error.to_string())
}

/// Refuses registrations from a mail domain.
pub async fn create_email_domain_block(
    domain: &str,
    allow_registration: bool,
) -> Result<EmailDomainBlock, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .create_email_domain_block(domain, allow_registration)
        .await
        .map_err(|error| error.to_string())
}

/// Stops refusing a mail domain.
pub async fn delete_email_domain_block(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .delete_email_domain_block(id)
        .await
        .map_err(|error| error.to_string())
}

/// Who the grant belongs to, what it may do, and how much is waiting.
pub async fn meta() -> Result<AdminMeta, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .admin_meta()
        .await
        .map_err(|error| error.to_string())
}

/// The flat counts behind the overview screen.
pub async fn overview() -> Result<AdminOverview, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .admin_overview()
        .await
        .map_err(|error| error.to_string())
}

/// What the instance noticed, most recent first.
pub async fn alerts(unacknowledged: bool) -> Result<Vec<AdminAlert>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .admin_alerts(unacknowledged, PAGE)
        .await
        .map_err(|error| error.to_string())
}

/// Records that an alert has been seen.
pub async fn acknowledge_alert(id: &str) -> Result<AdminAlert, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .acknowledge_alert(id)
        .await
        .map_err(|error| error.to_string())
}

/// Every process this instance knows about, asked just now.
pub async fn topology() -> Result<Topology, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .topology()
        .await
        .map_err(|error| error.to_string())
}

/// Asks one process to do its pass now instead of on its next tick.
pub async fn trigger(name: &str, work: &str) -> Result<TriggerOutcome, String> {
    admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?
        .trigger_node(name, work)
        .await
        .map_err(|error| error.to_string())
}

/// The last entries one node kept in memory.
pub async fn node_logs(name: &str, min_level: &str, limit: u16) -> Result<Vec<NodeLog>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .node_logs(name, min_level, limit)
        .await
        .map_err(|error| error.to_string())
}

/// The registered scanner services, with their rules.
pub async fn scanners() -> Result<Vec<Scanner>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .scanners()
        .await
        .map_err(|error| error.to_string())
}

/// Registers a scanner service.
pub async fn create_scanner(scanner: &NewScanner) -> Result<Scanner, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .create_scanner(scanner)
        .await
        .map_err(|error| error.to_string())
}

/// Replaces a scanner's settings.
pub async fn update_scanner(id: &str, scanner: &NewScanner) -> Result<Scanner, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .update_scanner(id, scanner)
        .await
        .map_err(|error| error.to_string())
}

/// Unregisters a scanner.
pub async fn delete_scanner(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .delete_scanner(id)
        .await
        .map_err(|error| error.to_string())
}

/// Asks a scanner what it can report and stores the answer.
pub async fn refresh_scanner(id: &str) -> Result<Scanner, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .refresh_scanner(id)
        .await
        .map_err(|error| error.to_string())
}

/// Decides what one label means at or above one threshold.
pub async fn set_scanner_rule(
    scanner_id: &str,
    label: &str,
    threshold: f64,
    severity: ScanSeverity,
) -> Result<ScannerRule, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .set_scanner_rule(scanner_id, label, threshold, severity)
        .await
        .map_err(|error| error.to_string())
}

/// Forgets a rule.
pub async fn delete_scanner_rule(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .delete_scanner_rule(id)
        .await
        .map_err(|error| error.to_string())
}

/// The content sets this instance holds.
pub async fn scanner_assets() -> Result<Vec<ScannerAsset>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .scanner_assets()
        .await
        .map_err(|error| error.to_string())
}

/// Uploads a content set.
pub async fn upload_scanner_asset(asset: &AssetUpload) -> Result<ScannerAsset, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .upload_scanner_asset(asset)
        .await
        .map_err(|error| error.to_string())
}

/// Deletes a content set no scanner points at.
pub async fn delete_scanner_asset(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .delete_scanner_asset(id)
        .await
        .map_err(|error| error.to_string())
}

/// Scan results a moderator has to judge; `None` asks for all of them.
pub async fn scan_flags(state: Option<FlagState>) -> Result<Vec<ScanFlag>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .scan_flags(state, PAGE)
        .await
        .map_err(|error| error.to_string())
}

/// Records a moderator's verdict on a flag.
pub async fn resolve_scan_flag(id: &str, state: FlagState) -> Result<ScanFlag, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .resolve_scan_flag(id, state)
        .await
        .map_err(|error| error.to_string())
}

/// Warnings a moderator can reuse.
pub async fn warning_presets() -> Result<Vec<WarningPreset>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .warning_presets()
        .await
        .map_err(|error| error.to_string())
}

/// Saves a new warning preset.
pub async fn create_warning_preset(title: &str, text: &str) -> Result<WarningPreset, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .create_warning_preset(title, text)
        .await
        .map_err(|error| error.to_string())
}

/// Deletes a preset.
pub async fn delete_warning_preset(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .delete_warning_preset(id)
        .await
        .map_err(|error| error.to_string())
}
