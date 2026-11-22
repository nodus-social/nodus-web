//! Moderation calls, and the one question that decides whether to show them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::session::staff;
use super::{admin_scope, admin_transport, instance_transport};
use leptos::prelude::{Get, RwSignal, Signal, Update};
use nodus_client_core::actions::{AdminActions, RegistrationActions};
use nodus_client_core::model::{
    AccountDetail, AccountNote, AdminAccount, AdminAccountQuery, AdminAlert, AdminMeasure,
    AdminMeta, AdminOverview, Announcement, AnnouncementDraft, AssetUpload, AuditEntry,
    AuditFilter, DomainAllow, DomainBlock, DomainBlockDraft, DomainImportOutcome, DomainSeverity,
    EmailDomainBlock, FlagState, ModerationAction, NewScanner, NodeLog, Report, ReportFilter,
    ScanFlag, ScanRequest, ScanSeverity, Scanner, ScannerAsset, ScannerKind, ScannerRule, Topology,
    TriggerOutcome, WarningPreset,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{ReadableStreamDefaultReader, TextDecodeOptions, TextDecoder};

/// How many rows a moderation screen asks for.
///
/// Small on purpose: the queue is a work list a human reads, and a moderator who scrolls
/// past fifty reports has stopped reading them.
pub const PAGE: u16 = 20;

/// Which of the two staff roles an account holds, as the instance reports it.
///
/// The two are not one permission with a strength dial: the instance draws the line, and a
/// handful of its routes (role grants, scanners, block lists, topology triggers) answer only
/// to an administrator. The console reads the role so it draws the same line instead of
/// painting an administrator's levers for someone whose request would be refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Staff, and the narrower door: reports, accounts, registrations.
    Moderator,
    /// Staff, and the wider door: the instance's own levers.
    Admin,
}

impl Role {
    /// Reads the wire spelling of `role` in `admin/meta`. `none` — and any name this client
    /// predates — is not a role.
    ///
    /// Only reached with an accepted admin grant, so it answers what an own credentials
    /// document cannot: which of the two doors this grant opens.
    #[must_use]
    fn parse(name: &str) -> Option<Self> {
        match name {
            "moderator" => Some(Self::Moderator),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

/// What the console may show this account, as the instance last answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Staff {
    /// A staff account holding a grant the instance accepted: the panels work.
    ///
    /// `write` is the token's `admin:write`, not the role's. The instance issues both
    /// scopes to staff by default, but a grant may ask for less — the role endpoint's own
    /// documentation says a narrower token cannot write "either" — and a console that
    /// painted write controls from the role alone would paint buttons whose request
    /// answers 403.
    Yes {
        /// Which door this account walks through.
        role: Role,
        /// Whether the grant carries `admin:write`.
        write: bool,
    },
    /// The instance says this account holds no staff role at all.
    ///
    /// Kept apart from [`Self::NeedsGrant`] because a grant cannot fix it: only an
    /// administrator of the instance can give the role, and offering a consent screen that
    /// cannot help is a fake door.
    NotStaff,
    /// The account is staff, and this browser holds no grant to send.
    NeedsGrant,
    /// The account is staff, a grant is held, and the instance refused it: revoked, or
    /// issued for another account.
    Refused(String),
    /// The question could not be asked or could not be read. Nothing is known, so nothing
    /// is offered beyond a retry.
    Unknown(String),
}

/// The answer the last probe had, for the same session and grant.
///
/// The rail and each console section ask this question, and their effects re-run whenever
/// the store notifies — which the socket and the sync loop do constantly. Asking the
/// instance on every redraw turned a queued probe into a stream of requests, and each
/// refusal of an ungranted token is recorded by the server as a failed authentication, so
/// the alert list filled with the client's own polling. The answer only changes when the
/// session or the grant does, so that is what the answer is kept against.
///
/// The first paint mounts several askers at once, and they arrive before any answer
/// exists: one of them asks and the rest wait for the answer instead of asking again.
pub async fn standing() -> Staff {
    let Some(signature) = signature() else {
        return Staff::NeedsGrant;
    };
    for attempt in 0..64 {
        if let Some(verdict) = cached(&signature) {
            return verdict;
        }
        let ours = CLAIM.with(|claim| {
            let mut claim = claim.borrow_mut();
            if claim.as_ref() == Some(&signature) {
                false
            } else {
                *claim = Some(signature.clone());
                true
            }
        });
        if ours || attempt == 63 {
            let verdict = probe().await;
            remember(&signature, verdict.clone());
            CLAIM.with(|claim| {
                let mut claim = claim.borrow_mut();
                if claim.as_ref() == Some(&signature) {
                    *claim = None;
                }
            });
            return verdict;
        }
        // Another asker has the question in flight; its answer lands in the cache.
        gloo_timers::future::TimeoutFuture::new(16).await;
    }
    Staff::NeedsGrant
}

/// Drops the cached answers, because the session or the grant just changed.
pub fn forget_standing() {
    STANDING.with(|slot| slot.borrow_mut().take());
    META.with(|slot| slot.borrow_mut().take());
    standing_revision().update(|revision| *revision = revision.wrapping_add(1));
}

/// Whether the console may write at all, as a signal a control can disable itself with.
///
/// `false` until the answer arrives, which is the safe direction: a write control painted
/// before the probe answers is a control whose request the instance may refuse.
#[must_use]
pub fn may_write() -> Signal<bool> {
    let revision = standing_revision();
    Signal::derive(move || {
        revision.get();
        STANDING.with_borrow(|held| {
            held.as_ref()
                .is_some_and(|(_, verdict)| matches!(verdict, Staff::Yes { write: true, .. }))
        })
    })
}

/// Whether the account's staff role is administrator, as a signal a control can disable
/// itself with.
///
/// A separate question from [`may_write`]: the grant's scopes say what the token may do,
/// the role says what the instance will let this account do, and several routes — the
/// administrator flag, media retention, scanners — ask for `role == admin` whatever the
/// grant carries.
#[must_use]
pub fn may_administer() -> Signal<bool> {
    let revision = standing_revision();
    Signal::derive(move || {
        revision.get();
        STANDING.with_borrow(|held| {
            held.as_ref().is_some_and(|(_, verdict)| {
                matches!(
                    verdict,
                    Staff::Yes {
                        role: Role::Admin,
                        ..
                    }
                )
            })
        })
    })
}

/// Bumped whenever the cached verdict changes, so a signal derived from it repaints.
fn standing_revision() -> RwSignal<u64> {
    thread_local! {
        static REVISION: RwSignal<u64> = RwSignal::new(0);
    }
    REVISION.with(|revision| *revision)
}

/// The session and grant this answer belongs to.
///
/// A token that is missing, or belongs to another account, would produce a different
/// answer, so it must not share a cache entry.
fn signature() -> Option<Signature> {
    let (host, account) = admin_scope()?;
    Some((host, account, admin_transport().is_some()))
}

/// Instance, account and whether a grant is held.
type Signature = (String, String, bool);

thread_local! {
    /// The last verdict and the session it described.
    static STANDING: std::cell::RefCell<Option<(Signature, Staff)>> =
        const { std::cell::RefCell::new(None) };
    /// The session whose question is currently being asked.
    static CLAIM: std::cell::RefCell<Option<Signature>> = const { std::cell::RefCell::new(None) };
    /// `admin/meta` as this session and grant last read it.
    static META: std::cell::RefCell<Option<(Signature, MetaAnswer)>> =
        const { std::cell::RefCell::new(None) };
    /// The session whose `admin/meta` read is currently in flight.
    static META_CLAIM: std::cell::RefCell<Option<Signature>> =
        const { std::cell::RefCell::new(None) };
}

fn cached(signature: &Signature) -> Option<Staff> {
    STANDING.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|(cached, _)| cached == signature)
            .map(|(_, verdict)| verdict.clone())
    })
}

fn remember(signature: &Signature, verdict: Staff) {
    STANDING.with(|slot| *slot.borrow_mut() = Some((signature.clone(), verdict)));
    standing_revision().update(|revision| *revision = revision.wrapping_add(1));
}

/// The scope that makes the console's buttons send anything.
const WRITE: &str = "admin:write";

/// Asks the instance what this account is, and what this browser may do with it.
///
/// The question is `GET /api/v1/admin/meta`, whose answer names the role and lists the
/// scopes the token carries. Reading the report queue instead — what this used to do —
/// answered only "may I read": a moderator and an administrator were one verdict, and
/// every ungranted browser that asked got itself recorded by the server as a failed
/// authentication.
///
/// Two questions, in the order that costs least: without a grant, the role decides whether
/// offering one is honest at all; with a grant, meta both proves the token and names the
/// role. A refusal is not read as "not staff" on its own — the same 403 answers for a
/// plain account and for a revoked grant, and only the role endpoint tells those apart.
///
/// Private because it always asks, a request per redraw: callers go through [`standing`],
/// which keeps the answer for the session and grant it describes.
async fn probe() -> Staff {
    // No grant: a staff role is worth a consent screen, and no role at all means a consent
    // screen could not help, so it is not offered. A role the instance reports on the
    // session token itself — the Mastodon shape — needs no consent screen at all, so it is
    // a working door with `write: false`.
    if admin_transport().is_none() {
        return match staff::own_role().await {
            Ok(Some(own)) if own.opens_door() => Staff::Yes {
                role: console_role(own),
                write: false,
            },
            Ok(Some(_)) => Staff::NeedsGrant,
            Ok(None) => Staff::NotStaff,
            Err(problem) => Staff::Unknown(problem),
        };
    }
    match meta_answer().await {
        Ok(answer) => Staff::Yes {
            // A meta answer without a role comes from an instance older than the contract
            // this console reads. The narrower door is the honest reading of an absent
            // field: painting an administrator's levers for a moderator is a door that
            // leads to a refusal.
            role: answer.role.unwrap_or(Role::Moderator),
            write: answer.meta.scopes.iter().any(|scope| scope == WRITE),
        },
        // Staff but refused, or not staff at all: only the role says which.
        Err(MetaProblem::Refused) => match staff::own_role().await {
            Ok(Some(_own)) => Staff::Refused(
                "The instance refused this grant: it was revoked, or it belongs to another \
                 account."
                    .to_owned(),
            ),
            Ok(None) => Staff::NotStaff,
            Err(problem) => Staff::Unknown(problem),
        },
        Err(MetaProblem::Failed(problem)) => Staff::Unknown(problem),
    }
}

/// What `admin/meta` answered: the document the SDK already models, plus the role the
/// console's contract adds beside it.
#[derive(Debug, Clone)]
struct MetaAnswer {
    /// Who the grant belongs to, what it may do, and how much is waiting.
    meta: AdminMeta,
    /// `moderator` or `admin`; `None` on an instance that predates the field.
    role: Option<Role>,
}

/// `admin/meta` as this client reads it.
///
/// A local envelope rather than a new field on `AdminMeta`: the SDK's model is not this
/// crate's to change, and `flatten` leaves the rest of the document decoding exactly as it
/// did before the role was added — an instance that never sends `role` still reads.
#[derive(Debug, serde::Deserialize)]
struct MetaEnvelope {
    #[serde(flatten)]
    meta: AdminMeta,
    #[serde(default)]
    role: Option<String>,
}

/// Why `admin/meta` did not answer.
enum MetaProblem {
    /// The instance answered 401/403: this token is not, or is no longer, allowed in.
    Refused,
    /// Anything else — a network failure, a 5xx, a body that could not be read — with the
    /// sentence the console shows for it.
    Failed(String),
}

/// The console's reading of a role the own credentials document named.
///
/// The document spells the broader door only when it means it; every other staff account
/// gets the narrower one, which is the honest reading of a role the client cannot rank.
fn console_role(own: staff::OwnRole) -> Role {
    if own.administrator {
        Role::Admin
    } else {
        Role::Moderator
    }
}

/// Reads `admin/meta` from `transport`, body and role together.
async fn read_meta(transport: &nodus_mastodon::BrowserInstance) -> Result<MetaAnswer, MetaProblem> {
    let url = transport
        .url("/api/v1/admin/meta")
        .map_err(|error| MetaProblem::Failed(error.to_string()))?;
    let bytes = transport
        .fetch_bytes(transport.get(url))
        .await
        .map_err(|error| match error {
            nodus_client_core::Error::Unauthorized => MetaProblem::Refused,
            other => MetaProblem::Failed(format!(
                "The instance did not answer with the console's metadata: {other}"
            )),
        })?;
    let envelope: MetaEnvelope = serde_json::from_slice(&bytes).map_err(|error| {
        MetaProblem::Failed(format!(
            "The instance's console metadata could not be read: {error}"
        ))
    })?;
    Ok(MetaAnswer {
        meta: envelope.meta,
        role: envelope.role.as_deref().and_then(Role::parse),
    })
}

/// Asks `admin/meta` once per console.
///
/// The probe and the dashboard both need this document, and the guard on [`standing`]
/// covers only the probe: without a claim of its own the dashboard's read would race the
/// probe's and the console would open on two identical requests. One answer is kept for the
/// session and grant it describes, because the document describes the console rather than a
/// moment — the SDK's own note is "fetched once when the console opens".
async fn meta_answer() -> Result<MetaAnswer, MetaProblem> {
    let Some(signature) = signature() else {
        return Err(MetaProblem::Failed("Not signed in.".to_owned()));
    };
    let Some(transport) = admin_transport() else {
        return Err(MetaProblem::Failed(
            "No admin grant is held, so the console cannot ask.".to_owned(),
        ));
    };
    for attempt in 0..64 {
        if let Some(answer) = META.with(|slot| {
            slot.borrow()
                .as_ref()
                .filter(|(cached, _)| cached == &signature)
                .map(|(_, answer)| answer.clone())
        }) {
            return Ok(answer);
        }
        let ours = META_CLAIM.with(|claim| {
            let mut claim = claim.borrow_mut();
            if claim.as_ref() == Some(&signature) {
                false
            } else {
                *claim = Some(signature.clone());
                true
            }
        });
        if ours || attempt == 63 {
            let answer = read_meta(&transport).await;
            if let Ok(answer) = &answer {
                META.with(|slot| *slot.borrow_mut() = Some((signature.clone(), answer.clone())));
            }
            META_CLAIM.with(|claim| {
                let mut claim = claim.borrow_mut();
                if claim.as_ref() == Some(&signature) {
                    *claim = None;
                }
            });
            return answer;
        }
        // Another asker has the read in flight; its answer lands in the cache.
        gloo_timers::future::TimeoutFuture::new(16).await;
    }
    Err(MetaProblem::Failed(
        "The instance's console metadata could not be read.".to_owned(),
    ))
}

/// One page of the audit log, narrowed by the filter the screen holds.
pub async fn audit(filter: &AuditFilter) -> Result<Vec<AuditEntry>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .audit(filter)
        .await
        .map_err(|error| error.to_string())
}

/// The same filter, asked of the endpoint that answers a file rather than JSON.
///
/// The export is the log as text the instance writes, not a shape this console decodes, so
/// it is handed back as the bytes that arrived. The filter's words are the only part the
/// console chooses; paging is left out because an export is the whole result, not a page.
pub async fn audit_csv(filter: &AuditFilter) -> Result<Vec<u8>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/audit.csv")
        .map_err(|error| error.to_string())?;
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in [
            ("action", filter.action.as_deref()),
            ("target_type", filter.target_type.as_deref()),
            ("actor_id", filter.actor_id.as_deref()),
        ] {
            if let Some(value) = value.filter(|text| !text.is_empty()) {
                pairs.append_pair(key, value);
            }
        }
    }
    transport
        .fetch_bytes(transport.get(url))
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

/// Rewrites an announcement's text in place.
///
/// The draft carries the row's own dates and published flag: the endpoint replaces the row,
/// so editing the words must not silently unpublish or reschedule the announcement.
/// Requires `admin:write`.
pub async fn update_announcement(
    id: &str,
    draft: AnnouncementDraft,
) -> Result<Announcement, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .update_announcement(id, draft)
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

/// Changes how much of a domain is refused, keeping everything else the block carries.
///
/// The endpoint replaces the whole row, so a draft holding only the new severity would drop
/// the flags and the comment that were already there: the caller passes the row back.
/// Requires `admin:write`.
pub async fn update_domain_block(
    id: &str,
    draft: &DomainBlockDraft,
) -> Result<DomainBlock, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .update_domain_block(id, draft)
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

/// One line the block-list importer could not read.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ImportProblem {
    /// The line number in the list that was sent, counting the header as line one.
    pub line: usize,
    /// Why the line could not become a block.
    pub reason: String,
}

/// What importing a list did, or would do.
///
/// The same shape answers a dry run and an apply: a preview that counts differently from
/// the write it previews would be worse than no preview.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ImportReport {
    /// Every line the parser looked at, valid or not.
    pub parsed: usize,
    /// Blocks the list would add.
    pub new: usize,
    /// Blocks already held whose severity or flags the list changes.
    pub updated: usize,
    /// Blocks the list repeats exactly.
    pub unchanged: usize,
    /// The lines that could not be read, each with its reason and line number.
    pub invalid: Vec<ImportProblem>,
}

/// Every section a settings document can carry, in the order an import applies them.
///
/// The instance's own order, so a document written here applies in the same sequence
/// elsewhere: content sets before the rules that name a scanner, the domain lists before
/// the single policy rows, and moderation rules last.
pub const SETTINGS_SECTIONS: [&str; 9] = [
    "rules",
    "instance_rules",
    "warning_presets",
    "escalation",
    "domain_pauses",
    "domain_blocks",
    "email_domain_blocks",
    "scanner_rules",
    "scanner_sets",
];

/// What a settings document holds for one section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSectionCount {
    /// The section's name, as the wire spells it.
    pub key: String,
    /// How many entries the document carries for it: rows, or the one policy row.
    pub count: usize,
}

/// A settings document as downloaded, with what it holds.
///
/// The bytes are kept beside the counts because they are the file: what a browser saves
/// must be what the instance answered, not a re-encoding of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsDocument {
    /// The document's name, as the instance wrote it.
    pub format: String,
    /// The version it was written in.
    pub version: u64,
    /// The instance it came from, when the answer names one.
    pub instance: Option<String>,
    /// How much it carries per section, in [`SETTINGS_SECTIONS`] order.
    pub counts: Vec<SettingsSectionCount>,
    /// The document itself.
    pub raw: Vec<u8>,
}

/// One conflict: a row the instance understood and refused or could not place.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct SettingsConflict {
    /// The section the row belongs to.
    #[serde(default)]
    pub section: String,
    /// The row's own name, so the operator can find it in the document.
    #[serde(default)]
    pub name: String,
    /// Why it was not applied, in the instance's words. Shown whole: this is the sentence
    /// that decides whether the operator wants the import at all.
    #[serde(default)]
    pub reason: String,
}

/// What one section's import would do, or did.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct SettingsOutcome {
    /// Rows the instance does not hold yet.
    #[serde(default)]
    pub created: u64,
    /// Rows it holds and would change.
    #[serde(default)]
    pub updated: u64,
    /// Rows it already matches.
    #[serde(default)]
    pub unchanged: u64,
    /// Rows it cannot apply.
    #[serde(default)]
    pub skipped: u64,
    /// Every row it refused or could not place, each with its reason.
    #[serde(default)]
    pub conflicts: Vec<SettingsConflict>,
}

/// An import's answer: what each section would do, or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsImport {
    /// Whether the instance only counted. It says so itself rather than the client
    /// trusting which request it sent.
    pub dry_run: bool,
    /// One outcome per section the document carried, in [`SETTINGS_SECTIONS`] order.
    pub sections: Vec<(String, SettingsOutcome)>,
}

/// The query string naming only the sections asked for.
///
/// An empty list is left off entirely: the instance reads a missing `sections` as "all of
/// them", and spelling out nothing at all would be a different request.
fn settings_query(sections: &[String]) -> String {
    let chosen: Vec<&str> = SETTINGS_SECTIONS
        .iter()
        .copied()
        .filter(|key| sections.iter().any(|asked| asked == key))
        .collect();
    chosen.join(",")
}

/// `GET /api/v1/admin/settings/export` — the settings document, and what it holds.
pub async fn settings_export(sections: &[String]) -> Result<SettingsDocument, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/settings/export")
        .map_err(|error| error.to_string())?;
    let query = settings_query(sections);
    if !query.is_empty() {
        url.query_pairs_mut().append_pair("sections", &query);
    }
    let raw = transport
        .fetch_bytes(transport.get(url))
        .await
        .map_err(|error| error.to_string())?;
    let document: serde_json::Value = serde_json::from_slice(&raw).map_err(|error| {
        format!("The instance answered with something that is not a settings document: {error}")
    })?;
    let format = document
        .get("format")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let version = document
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default();
    let instance = document
        .get("instance")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let held = document
        .get("sections")
        .and_then(serde_json::Value::as_object);
    // Counted here rather than by the screen: a section the document does not carry at all
    // is shown as missing, and a section it carries empty is shown as zero. The two are
    // different facts, and an operator exporting a file needs to know which they have.
    let counts = SETTINGS_SECTIONS
        .iter()
        .filter_map(|key| {
            let value = held?.get(*key)?;
            Some(SettingsSectionCount {
                key: (*key).to_owned(),
                count: match value {
                    serde_json::Value::Array(rows) => rows.len(),
                    serde_json::Value::Object(fields) => usize::from(!fields.is_empty()),
                    serde_json::Value::Null => 0,
                    _ => 1,
                },
            })
        })
        .collect();
    Ok(SettingsDocument {
        format,
        version,
        instance,
        counts,
        raw,
    })
}

/// `POST /api/v1/admin/settings/import` — what would change, or the change itself.
///
/// The body is the document exactly as it arrived: the instance reads the format and the
/// version out of it, and refuses what it does not know with a 422 whose sentence is the
/// only explanation the operator gets.
pub async fn settings_import(
    document: &str,
    sections: &[String],
    dry_run: bool,
) -> Result<SettingsImport, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/settings/import")
        .map_err(|error| error.to_string())?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("dry_run", if dry_run { "true" } else { "false" });
        let query = settings_query(sections);
        if !query.is_empty() {
            pairs.append_pair("sections", &query);
        }
    }
    let answer: serde_json::Value = transport
        .fetch(transport.post_body(url, "application/json", document.as_bytes().to_vec()))
        .await
        .map_err(|error| error.to_string())?;
    let reported = answer
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(dry_run);
    let held = answer
        .get("sections")
        .and_then(serde_json::Value::as_object);
    let mut outcomes = Vec::new();
    if let Some(held) = held {
        for key in SETTINGS_SECTIONS {
            let Some(value) = held.get(key) else { continue };
            let outcome =
                serde_json::from_value::<SettingsOutcome>(value.clone()).map_err(|error| {
                    format!("The instance's answer for {key} is not an import report: {error}")
                })?;
            outcomes.push((key.to_owned(), outcome));
        }
    }
    Ok(SettingsImport {
        dry_run: reported,
        sections: outcomes,
    })
}

/// The bytes of a block list, in the format asked for.
///
/// The export is streamed CSV or JSON rather than a DTO, so it is fetched as bytes and
/// handed to the download helper as they arrived: re-encoding it here would only be a way
/// to write a different file than the instance produced.
pub async fn export_block_list(format: &str) -> Result<Vec<u8>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/domain-blocks/export")
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut().append_pair("format", format);
    transport
        .fetch_bytes(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// Parses a block list, or applies it when `dry_run` is false.
///
/// The body is the list itself, not a form around it: the instance sniffs CSV from JSON by
/// the first non-space character, so the content type only has to be honest.
pub async fn import_block_list(text: &str, dry_run: bool) -> Result<ImportReport, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/domain-blocks/import")
        .map_err(|error| error.to_string())?;
    // Spelled either way rather than left off for the apply: the request records which of
    // the two it was, so a log of an import never needs the surrounding state to say whether
    // anything was written.
    url.query_pairs_mut()
        .append_pair("dry_run", if dry_run { "true" } else { "false" });
    let content_type = if text.trim_start().starts_with(['[', '{']) {
        "application/json"
    } else {
        "text/csv"
    };
    transport
        .fetch(transport.post_body(url, content_type, text.as_bytes().to_vec()))
        .await
        .map_err(|error| error.to_string())
}

/// A block list this instance follows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct BlockSubscription {
    pub id: String,
    /// Where the list is fetched from.
    pub url: String,
    /// `csv` or `json`, as the peer publishes it.
    pub format: String,
    /// Whether the sweep still refreshes it.
    #[serde(default)]
    pub enabled: bool,
    /// When the list was last read successfully.
    #[serde(default)]
    pub last_synced_at: Option<String>,
    /// Why the last attempt failed, when it did. The row stays listed with this on it.
    #[serde(default)]
    pub last_error: Option<String>,
}

/// The lists this instance follows, with the result of each one's last attempt.
pub async fn block_subscriptions() -> Result<Vec<BlockSubscription>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/domain-blocks/subscriptions")
        .map_err(|error| error.to_string())?;
    transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// Follows one published list.
pub async fn subscribe_block_list(url: &str, format: &str) -> Result<BlockSubscription, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let target = transport
        .url("/api/v1/admin/domain-blocks/subscriptions")
        .map_err(|error| error.to_string())?;
    let body = serde_json::to_vec(&serde_json::json!({ "url": url, "format": format }))
        .map_err(|error| error.to_string())?;
    transport
        .fetch(transport.post_body(target, "application/json", body))
        .await
        .map_err(|error| error.to_string())
}

/// Stops following one list. The blocks it already brought in stay.
pub async fn unsubscribe_block_list(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "domain-blocks", "subscriptions", id])
        .map_err(|error| error.to_string())?;
    transport
        .send_empty(transport.delete(url))
        .await
        .map_err(|error| error.to_string())
}

/// What pulling one subscription just did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct SyncReport {
    /// `applied` when the peer sent a list, `not_modified` when it answered `304`.
    pub status: String,
    pub parsed: usize,
    pub new: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub invalid: usize,
}

/// Pulls one list now instead of waiting for the sweep.
pub async fn sync_block_subscription(id: &str) -> Result<SyncReport, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&[
            "api",
            "v1",
            "admin",
            "domain-blocks",
            "subscriptions",
            id,
            "sync",
        ])
        .map_err(|error| error.to_string())?;
    transport
        .fetch(transport.post(url))
        .await
        .map_err(|error| error.to_string())
}

/// How moderation performed in one window, as `GET /api/v1/admin/stats` answers it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct AdminStats {
    pub signups: i64,
    pub active_accounts: i64,
    pub posts: i64,
    pub reports: ReportTally,
    pub decisions_by_action: DecisionTally,
    /// `null`, not `0`, when the window holds no decision.
    pub median_time_to_decision_seconds: Option<f64>,
    pub top_reporters: Vec<ReporterTally>,
    pub top_reported_domains: Vec<DomainTally>,
    pub scanners: ScannerTally,
}

/// Report counts in the window.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ReportTally {
    pub opened: i64,
    pub resolved: i64,
    pub reopened: i64,
    pub pending: i64,
}

/// Decisions taken in the window, by the action they applied.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DecisionTally {
    pub approve: i64,
    pub reject: i64,
    pub silence: i64,
    pub suspend: i64,
    pub enable: i64,
}

/// One account that filed reports in the window.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ReporterTally {
    pub account_id: String,
    pub username: String,
    #[serde(default)]
    pub domain: Option<String>,
    pub count: i64,
}

/// One domain the window's reports named.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DomainTally {
    pub domain: String,
    pub count: i64,
}

/// What the scanners did in the window.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ScannerTally {
    pub queued: i64,
    pub flagged: i64,
    pub quarantined: i64,
}

/// The moderation performance document for one range: `7d`, `30d` or `90d`.
pub async fn stats(range: &str) -> Result<AdminStats, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/stats")
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut().append_pair("range", range);
    transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// An inbound block, as the instance returns it: who was blocked, by which peer, and when.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RemoteBlock {
    /// The blocked account, as a URI on the peer.
    pub actor: String,
    /// The peer that sent the block, as its bare host.
    pub host: String,
    /// When the block arrived, as the instance wrote it down.
    pub created_at: String,
}

/// The envelope the remote block read answers with.
#[derive(serde::Deserialize)]
struct RemoteBlocks {
    blocks: Vec<RemoteBlock>,
}

/// Peers that have told this instance they block one of its accounts, newest first.
///
/// A Nodus extension rather than a Mastodon endpoint, so a screen that shows it is gated on
/// `supports_extensions`.
pub async fn remote_blocks() -> Result<Vec<RemoteBlock>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/federation/remote-blocks")
        .map_err(|error| error.to_string())?;
    let answer: RemoteBlocks = transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())?;
    Ok(answer.blocks)
}

/// Clears one peer's block, which is what lets delivery to it resume.
pub async fn delete_remote_block(host: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/federation/remote-blocks")
        .map_err(|error| error.to_string())?;
    // The endpoint names the row in a JSON body rather than in the path: the one shape no
    // helper on the shared instance takes.
    let body = serde_json::to_vec(&serde_json::json!({ "host": host }))
        .map_err(|error| error.to_string())?;
    let mut request = transport.delete(url);
    request
        .headers
        .push(("Content-Type".to_owned(), "application/json".to_owned()));
    request.body = Some(body);
    transport
        .send_empty(request)
        .await
        .map_err(|error| error.to_string())
}

/// One account on another instance that this one silences or suspends.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RemoteAccount {
    /// The account's absolute address, which is how it is changed and lifted.
    pub actor_uri: String,
    /// The account's local name on the instance it lives on.
    pub handle: String,
    /// The instance the account lives on, when the address named one.
    #[serde(default)]
    pub domain: Option<String>,
    /// `silence` or `suspend`, as the instance spells it.
    pub action: String,
    /// Why, as the administrator who applied it wrote it down.
    #[serde(default)]
    pub reason: Option<String>,
    /// When it lapses, in RFC 3339, when it does.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Whether the instance's own clock has already passed that moment.
    ///
    /// The instance answers this rather than the console working it out, so a row and the
    /// instance it describes cannot disagree about which actions are still biting.
    #[serde(default)]
    pub expired: bool,
    /// When it was applied.
    pub created_at: String,
    /// Who applied it, as a name this instance still knows.
    #[serde(default)]
    pub created_by_handle: Option<String>,
}

/// The remote accounts this instance moderates, newest first.
///
/// A Nodus extension like the remote blocks beside it: an instance that does not advertise
/// the surface is not asked.
pub async fn remote_accounts() -> Result<Vec<RemoteAccount>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url_segments(&["api", "v1", "admin", "federation", "remote-accounts"])
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("limit", &PAGE.to_string());
    // A bare array rather than an envelope: the row list is the whole answer.
    transport
        .fetch::<Vec<RemoteAccount>>(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// Applies a silence or a suspension to one remote account.
///
/// `expires_on` is the date input's own `YYYY-MM-DD`, empty for an action that does not
/// lapse; the endpoint takes a timestamp, so the day is widened to the instant it starts.
/// The address is the path's one segment: `url_segments` percent-encodes it, and the route
/// behind it is a wildcard so the encoded slashes arrive intact.
pub async fn apply_remote_account(
    actor_uri: &str,
    action: &str,
    reason: &str,
    expires_on: &str,
) -> Result<RemoteAccount, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&[
            "api",
            "v1",
            "admin",
            "federation",
            "remote-accounts",
            actor_uri,
        ])
        .map_err(|error| error.to_string())?;
    let mut body = serde_json::json!({ "action": action });
    if !reason.trim().is_empty() {
        body["reason"] = serde_json::Value::String(reason.trim().to_owned());
    }
    match expires_on.trim() {
        "" => {}
        // A date input says which day, not which instant: an action that lapses on a day
        // lapses when that day starts.
        day if day.len() == 10 => {
            body["expires_at"] = serde_json::Value::String(format!("{day}T00:00:00Z"));
        }
        other => body["expires_at"] = serde_json::Value::String(other.to_owned()),
    }
    let body = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
    // A body on a `PUT` whose path names the row: the one shape no helper on the shared
    // instance takes, so the request is built here.
    let mut request = transport.put(url);
    request
        .headers
        .push(("Content-Type".to_owned(), "application/json".to_owned()));
    request.body = Some(body);
    transport
        .fetch::<RemoteAccount>(request)
        .await
        .map_err(|error| error.to_string())
}

/// Lifts one remote account's action, so whatever it was doing stops.
pub async fn lift_remote_account(actor_uri: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&[
            "api",
            "v1",
            "admin",
            "federation",
            "remote-accounts",
            actor_uri,
        ])
        .map_err(|error| error.to_string())?;
    // Nothing to send: the row's address is the whole request.
    transport
        .send_empty(transport.delete(url))
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

/// Sends one report to its peer, or answers the forward that already exists.
///
/// The endpoint is idempotent and answers the first row on a replay (200) instead of
/// queueing a second Flag (202), so a press that follows an earlier one cannot duplicate
/// the delivery — which is why the queue shows the state after the first press rather than
/// inviting the second.
///
/// A refusal (a local account, a blocked or paused domain) comes back as the instance's own
/// sentence in the error envelope; the caller shows it as it stands.
pub async fn forward_report(report_id: &str, note: &str) -> Result<Forwarded, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url(&format!("/api/v1/admin/reports/{report_id}/forward"))
        .map_err(|error| error.to_string())?;
    let body = serde_json::to_vec(&serde_json::json!({ "note": note.trim() }))
        .map_err(|error| error.to_string())?;
    let mut request = transport.post(url);
    request
        .headers
        .push(("Content-Type".to_owned(), "application/json".to_owned()));
    request.body = Some(body);
    transport
        .fetch::<Forwarded>(request)
        .await
        .map_err(|error| error.to_string())
}

/// Every report this instance forwarded, newest first, narrowed by state or peer.
pub async fn forwarded(
    state: Option<&str>,
    domain: Option<&str>,
    limit: u16,
) -> Result<Vec<Forwarded>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/federation/forwarded")
        .map_err(|error| error.to_string())?;
    {
        let mut serializer = url.query_pairs_mut();
        if let Some(state) = state.filter(|value| !value.is_empty()) {
            serializer.append_pair("state", state);
        }
        if let Some(domain) = domain.map(str::trim).filter(|value| !value.is_empty()) {
            serializer.append_pair("domain", domain);
        }
        serializer.append_pair("limit", &limit.to_string());
    }
    transport
        .fetch::<Vec<Forwarded>>(transport.get(url))
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
///
/// The same document [`standing`] reads, through the same cache: the probe asks it when the
/// console opens and the dashboard asks it for the scanner counts, and two requests for one
/// document would be two chances for the console to disagree with itself about a grant.
pub async fn meta() -> Result<AdminMeta, String> {
    meta_answer()
        .await
        .map(|answer| answer.meta)
        .map_err(|problem| match problem {
            MetaProblem::Refused => {
                "The instance refused this grant's console metadata.".to_owned()
            }
            MetaProblem::Failed(problem) => problem,
        })
}

/// `admin/meta` read again, replacing the answer kept for this grant.
///
/// The document describes the console rather than a moment, which is why it is read once —
/// but `scanning.queued` counts work in flight, so a screen that has just changed the queue
/// asks for a fresh copy instead of painting the one the console opened with.
pub async fn meta_refreshed() -> Result<AdminMeta, String> {
    META.with(|slot| *slot.borrow_mut() = None);
    meta().await
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

/// How many lines a followed drawer holds before the oldest are dropped.
pub const HELD_LINES: usize = 500;

/// How long a stream that ended on its own waits before it asks for the tail again.
const RECONNECT_GAP_MS: u32 = 1_000;

/// One live log tail, and the way to hang up on it.
///
/// `fetch` plus a body reader rather than `EventSource`: the endpoint authenticates with a
/// bearer header, which `EventSource` cannot send, and the drawer needs to hold the read
/// itself so that closing it releases the instance's stream instead of leaving it open.
#[derive(Clone)]
pub struct LogTail {
    stopped: Rc<Cell<bool>>,
    reader: Rc<RefCell<Option<ReadableStreamDefaultReader>>>,
}

/// How one connection to the log stream ended.
enum TailEnd {
    /// The drawer was closed or moved on: nothing left to read.
    Stopped,
    /// The instance closed the stream on its own; asking again is the caller's call.
    Closed,
}

impl LogTail {
    /// A tail that has not asked for anything yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stopped: Rc::new(Cell::new(false)),
            reader: Rc::new(RefCell::new(None)),
        }
    }

    /// Hangs up: the reader is cancelled so the pending read wakes at once and the
    /// instance's own stream is dropped rather than kept by a screen that is gone.
    pub fn stop(&self) {
        self.stopped.set(true);
        if let Some(reader) = self.reader.borrow_mut().take() {
            let _ = reader.cancel();
        }
    }

    /// Reads `name`'s log as it happens, handing every entry to `on_line`.
    ///
    /// Returns when [`LogTail::stop`] is called, or with the instance's refusal when the
    /// stream cannot be held — the caller falls back to reading the drawer on a timer.
    /// A stream the instance ends by itself is not a refusal: a node restarting closes it,
    /// so it is opened again after [`RECONNECT_GAP_MS`].
    pub async fn follow<F>(
        &self,
        name: &str,
        min_level: &str,
        limit: u16,
        mut on_line: F,
    ) -> Result<(), String>
    where
        F: FnMut(NodeLog),
    {
        loop {
            if self.stopped.get() {
                return Ok(());
            }
            match self.once(name, min_level, limit, &mut on_line).await? {
                TailEnd::Stopped => return Ok(()),
                TailEnd::Closed => {
                    gloo_timers::future::TimeoutFuture::new(RECONNECT_GAP_MS).await;
                }
            }
        }
    }

    /// One connection: opens the stream and reads it until it ends.
    async fn once<F>(
        &self,
        name: &str,
        min_level: &str,
        limit: u16,
        on_line: &mut F,
    ) -> Result<TailEnd, String>
    where
        F: FnMut(NodeLog),
    {
        let transport = admin_transport()
            .or_else(instance_transport)
            .ok_or_else(|| "Not signed in.".to_owned())?;
        // The same path shape the console's other calls take: the name is one segment, so
        // `url_segments` encodes it and the query carries the filter.
        let mut url = transport
            .url_segments(&["api", "v1", "admin", "topology", name, "logs", "stream"])
            .map_err(|error| error.to_string())?;
        url.set_query(Some(&format!("min_level={min_level}&limit={limit}")));

        let window = web_sys::window().ok_or_else(|| "This browser has no window.".to_owned())?;
        let init = web_sys::RequestInit::new();
        init.set_method("GET");
        let headers = web_sys::Headers::new().map_err(browser_error)?;
        // The token travels in a header, which is why this is not an `EventSource`.
        let token = transport.token().expose();
        if !token.is_empty() {
            headers
                .set("Authorization", &format!("Bearer {token}"))
                .map_err(browser_error)?;
        }
        init.set_headers_headers(&headers);

        let request =
            web_sys::Request::new_with_str_and_init(url.as_str(), &init).map_err(browser_error)?;
        let answer = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(browser_error)?;
        let answer: web_sys::Response = answer
            .dyn_into()
            .map_err(|_| "The instance's answer was not a response.".to_owned())?;
        if !answer.ok() {
            return Err(format!(
                "The instance answered {} for the live log.",
                answer.status()
            ));
        }
        let body = answer
            .body()
            .ok_or_else(|| "The live log carried no body.".to_owned())?;
        let reader: ReadableStreamDefaultReader = body
            .get_reader()
            .dyn_into()
            .map_err(|_| "The answer's body could not be read.".to_owned())?;
        *self.reader.borrow_mut() = Some(reader);

        let decoder = TextDecoder::new().map_err(browser_error)?;
        let options = TextDecodeOptions::new();
        // `stream` keeps a multi-byte character split across two chunks from being cut.
        options.set_stream(true);
        let mut buffer = String::new();
        loop {
            if self.stopped.get() {
                return Ok(TailEnd::Stopped);
            }
            let Some(reader) = self.reader.borrow().clone() else {
                return Ok(TailEnd::Stopped);
            };
            let chunk = match JsFuture::from(reader.read()).await {
                Ok(chunk) => chunk,
                // A cancelled read rejects; that is how `stop` wakes this loop.
                Err(error) => {
                    if self.stopped.get() {
                        return Ok(TailEnd::Stopped);
                    }
                    return Err(browser_error(error));
                }
            };
            if read_field(&chunk, "done").and_then(|value| value.as_bool()) == Some(true) {
                return if self.stopped.get() {
                    Ok(TailEnd::Stopped)
                } else {
                    Ok(TailEnd::Closed)
                };
            }
            let value = read_field(&chunk, "value")
                .ok_or_else(|| "A chunk of the live log arrived without bytes.".to_owned())?;
            let bytes = js_sys::Uint8Array::new(&value);
            buffer.push_str(
                &decoder
                    .decode_with_js_u8_array_and_options(&bytes, &options)
                    .map_err(browser_error)?,
            );
            for event in events(&mut buffer) {
                if event.name == "error" {
                    return Err(event.data);
                }
                let entry: NodeLog = serde_json::from_str(&event.data)
                    .map_err(|error| format!("A log entry could not be read: {error}"))?;
                on_line(entry);
            }
        }
    }
}

impl Default for LogTail {
    fn default() -> Self {
        Self::new()
    }
}

/// One server-sent event, as the stream wrote it.
struct Event {
    name: String,
    data: String,
}

/// Cuts the complete events off the front of `buffer`, leaving a partial one behind.
fn events(buffer: &mut String) -> Vec<Event> {
    let mut out = Vec::new();
    loop {
        let Some((at, width)) = frame_end(buffer) else {
            return out;
        };
        let frame: String = buffer.drain(..at + width).collect();
        let mut name = String::from("message");
        let mut data = String::new();
        let mut seen = false;
        for line in frame.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            // `:` lines are the connection's keep-alive comments.
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("event:") {
                name = rest.trim_start().to_owned();
            } else if let Some(rest) = line.strip_prefix("data:") {
                if seen {
                    data.push('\n');
                }
                data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                seen = true;
            }
        }
        if seen || name != "message" {
            out.push(Event { name, data });
        }
    }
}

/// Where the next event ends in `buffer`, and whether the blank line is one or two bytes.
fn frame_end(buffer: &str) -> Option<(usize, usize)> {
    let plain = buffer.find("\n\n").map(|at| (at, 2));
    let carried = buffer.find("\r\n\r\n").map(|at| (at, 4));
    match (plain, carried) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (left, right) => left.or(right),
    }
}

/// One field of a reader's `{done, value}` chunk.
fn read_field(chunk: &wasm_bindgen::JsValue, name: &str) -> Option<wasm_bindgen::JsValue> {
    js_sys::Reflect::get(chunk, &wasm_bindgen::JsValue::from_str(name)).ok()
}

/// A browser's own words for a refused call.
fn browser_error(error: wasm_bindgen::JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            js_sys::Reflect::get(&error, &wasm_bindgen::JsValue::from_str("message"))
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| "The browser refused the live log.".to_owned())
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

/// The bytes of one content set, exactly as the scanner received them.
pub async fn download_scanner_asset(id: &str) -> Result<Vec<u8>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .download_scanner_asset(id)
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

/// The scanner kinds this instance has switched on, each named once.
///
/// A scan request names one kind and the instance queues it for the scanners of that kind
/// only, so asking for a subject means asking once per kind the instance actually runs.
async fn kinds_in_use() -> Result<Vec<ScannerKind>, String> {
    let mut kinds: Vec<ScannerKind> = Vec::new();
    for scanner in scanners().await? {
        if scanner.enabled && !kinds.contains(&scanner.kind) {
            kinds.push(scanner.kind);
        }
    }
    Ok(kinds)
}

/// Hands one subject to each of `kinds`, answering how many scanners took it.
async fn queue_for(
    kinds: &[ScannerKind],
    subject_type: &str,
    subject_id: &str,
) -> Result<u64, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut queued = 0;
    for kind in kinds {
        let request = ScanRequest {
            subject_type: subject_type.to_owned(),
            subject_id: subject_id.to_owned(),
            kind: *kind,
            url: None,
            text: None,
            mime: None,
        };
        queued += transport
            .enqueue_scan(&request)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(queued)
}

/// Queues one post for every kind of scanner this instance runs.
///
/// The instance keeps one row per scanner and subject and drops a repeat, so asking again
/// costs nothing and picks up a scanner that was registered after the post was first read.
pub async fn scan_subject(subject_id: &str) -> Result<u64, String> {
    let kinds = kinds_in_use().await?;
    if kinds.is_empty() {
        return Err("No scanner is enabled, so nothing was queued.".to_owned());
    }
    queue_for(&kinds, "status", subject_id).await
}

/// Queues every post on the open review queue again, answering the posts and the scans.
///
/// The dashboard has no subject to name, and the review queue is the one list of targets
/// the console already holds, so its "scan now" is "hand these to the scanners again".
pub async fn scan_open_queue() -> Result<(usize, u64), String> {
    let kinds = kinds_in_use().await?;
    if kinds.is_empty() {
        return Err("No scanner is enabled, so nothing was queued.".to_owned());
    }
    let mut subjects: Vec<String> = Vec::new();
    for flag in scan_flags(Some(FlagState::Open)).await? {
        if let Some(status) = flag.status_id.filter(|id| !id.is_empty()) {
            if !subjects.contains(&status) {
                subjects.push(status);
            }
        }
    }
    let mut queued = 0;
    for subject in &subjects {
        queued += queue_for(&kinds, "status", subject).await?;
    }
    Ok((subjects.len(), queued))
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

/// Rewrites a preset in place, so its identity — and the warnings already sent with it —
/// stays. Requires `admin:write`.
pub async fn update_warning_preset(
    id: &str,
    title: &str,
    text: &str,
) -> Result<WarningPreset, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    transport
        .update_warning_preset(id, title, text)
        .await
        .map_err(|error| error.to_string())
}

/// What a batch answered, id by id.
///
/// The API applies each id on the request's own transaction and answers per id, because a
/// moderator who ticked two hundred accounts needs to know which three did not go through,
/// not merely that the request was refused. The refusals are kept with the ids they name so
/// the screen can put each sentence on its own row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatchOutcome {
    /// How many ids the instance changed.
    pub applied: u64,
    /// Each id it refused, with the sentence it gave.
    pub refusals: Vec<(String, String)>,
}

/// One id's line in a batch answer.
#[derive(Debug, serde::Deserialize)]
struct BatchItem {
    id: String,
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

/// A batch answer as the instance writes it.
#[derive(Debug, serde::Deserialize)]
struct BatchAnswer {
    #[serde(default)]
    results: Vec<BatchItem>,
}

impl BatchAnswer {
    /// Splits the per-id lines into a count and the refusals.
    fn into_outcome(self) -> BatchOutcome {
        let mut outcome = BatchOutcome::default();
        for item in self.results {
            if item.ok {
                outcome.applied += 1;
            } else {
                outcome.refusals.push((
                    item.id,
                    item.error
                        .unwrap_or_else(|| "the instance refused this id".to_owned()),
                ));
            }
        }
        outcome
    }
}

/// Posts a batch document to `path` and splits the per-id answer.
async fn post_batch(path: &str, body: &serde_json::Value) -> Result<BatchOutcome, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport.url(path).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    let answer: BatchAnswer = transport
        .fetch(transport.post_body(url, "application/json", bytes))
        .await
        .map_err(|error| error.to_string())?;
    Ok(answer.into_outcome())
}

/// Applies one action to many accounts. Requires `admin:write`.
///
/// `reason` is the sentence the audit row carries for every account in the batch; the
/// instance records one row per account it changed.
pub async fn bulk_moderate(
    ids: &[String],
    action: &str,
    reason: &str,
) -> Result<BatchOutcome, String> {
    post_batch(
        "/api/v1/admin/accounts/bulk",
        &serde_json::json!({
            "ids": ids,
            "action": action,
            "reason": reason,
            // This instance does not send mail yet; the field is accepted and ignored.
            "notify": false,
        }),
    )
    .await
}

/// Claims, closes or reopens many reports at once. Requires `admin:write`.
pub async fn bulk_reports(
    ids: &[String],
    action: &str,
    assignee: Option<&str>,
    note: &str,
) -> Result<BatchOutcome, String> {
    post_batch(
        "/api/v1/admin/reports/bulk",
        &serde_json::json!({
            "ids": ids,
            "action": action,
            "assignee_id": assignee,
            "note": note,
        }),
    )
    .await
}

/// One appeal in the moderator's queue, as `GET /api/v1/admin/appeals` answers it.
///
/// The row names the restriction the appeal was written against and, separately, the one in
/// force now: an accepted appeal lifts it, and a moderator re-reading a closed row should be
/// able to see both without guessing which is which.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct AdminAppeal {
    pub id: String,
    /// The account that appealed.
    #[serde(default)]
    pub account_id: String,
    /// `@user`, or `@user@host` for a remote account.
    pub handle: String,
    /// The restriction in force *now*; `None` once the decision accepted the appeal.
    #[serde(default)]
    pub restriction: Option<String>,
    /// The restriction the appeal was written against: `suspend`, `silence` or `reject`.
    #[serde(default)]
    pub action: String,
    /// Why the person wrote, in their own words.
    #[serde(default)]
    pub reason: String,
    /// `pending`, `accepted` or `rejected`.
    #[serde(default)]
    pub status: String,
    /// The note that came with the decision; the person was sent it.
    #[serde(default)]
    pub decision_note: Option<String>,
    /// The account that decided it.
    #[serde(default)]
    pub decided_by: Option<String>,
    /// When it was opened.
    #[serde(default)]
    pub created_at: String,
    /// Whole seconds it has waited.
    #[serde(default)]
    pub age_seconds: i64,
    /// When it was decided, absent while it is still waiting.
    #[serde(default)]
    pub decided_at: Option<String>,
}

/// One page of appeals, newest first, narrowed to `status`.
///
/// `status` is one of `pending`, `accepted`, `rejected` or `all`; the instance answers 422 to
/// anything else, which is why the screen picks from the same four words.
pub async fn appeals(status: &str, offset: u32) -> Result<Vec<AdminAppeal>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/appeals")
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("status", status)
        .append_pair("limit", &PAGE.to_string())
        .append_pair("offset", &offset.to_string());
    transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// What a decision attempt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decided {
    /// The appeal was open, and this answer just closed it.
    Row(AdminAppeal),
    /// Somebody got there first. A `409` is not a failed request — it says the queue the
    /// moderator was reading is stale, and the screen's job is to say who decided it
    /// rather than to print the refusal.
    AlreadyDecided,
}

/// Decides one pending appeal; `accept` lifts the restriction it names.
///
/// The note is optional, and trimming it here rather than sending `"   "` keeps the
/// instance's own "empty means none" rule the only one in play.
pub async fn decide_appeal(id: &str, accept: bool, note: &str) -> Result<Decided, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "appeals", id, "decide"])
        .map_err(|error| error.to_string())?;
    let note = note.trim();
    let body = serde_json::to_vec(&serde_json::json!({
        "accept": accept,
        "note": (!note.is_empty()).then_some(note),
    }))
    .map_err(|error| error.to_string())?;
    match transport
        .fetch::<AdminAppeal>(transport.post_body(url, "application/json", body))
        .await
    {
        Ok(row) => Ok(Decided::Row(row)),
        Err(nodus_client_core::Error::Status { status: 409, .. }) => Ok(Decided::AlreadyDecided),
        Err(error) => Err(error.to_string()),
    }
}

/// One attachment a content set caught, as `GET /api/v1/admin/media/quarantined` answers it.
///
/// `set` is the name of the content set that matched, which is the name a moderator acts on;
/// `status_id` is the post it hung on, absent for an attachment that never reached one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct QuarantinedMedia {
    pub id: String,
    #[serde(default)]
    pub status_id: Option<String>,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub set: Option<String>,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub perceptual_hash: Option<String>,
    /// `quarantined` or `released`; deleting removes the row altogether.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub created_at: String,
}

/// Every attachment a content set has acted on, newest first.
///
/// The instance caps this read rather than paging it, so the screen shows what it was sent
/// and does not claim there is nothing behind it.
pub async fn quarantined_media() -> Result<Vec<QuarantinedMedia>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/media/quarantined")
        .map_err(|error| error.to_string())?;
    transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// What the instance answered when one attachment was cleared.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct MediaVerdict {
    pub id: String,
    #[serde(default)]
    pub released: bool,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub status_id: Option<String>,
    #[serde(default)]
    pub reports_resolved: u64,
}

/// What a decision attempt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cleared {
    /// The attachment was still waiting, and this answer just cleared it.
    Row(MediaVerdict),
    /// Somebody got there first, or the file is already gone. A `422` and a `404` both say
    /// the row the moderator was reading is stale, and the screen reads the list again
    /// rather than printing the refusal.
    AlreadyCleared,
}

/// Clears one quarantined attachment; `release` keeps the picture, and anything else deletes
/// the file. Both need a reason, which the instance records in the audit row.
pub async fn decide_media(id: &str, release: bool, reason: &str) -> Result<Cleared, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "media", id, "quarantine"])
        .map_err(|error| error.to_string())?;
    let body = serde_json::to_vec(&serde_json::json!({
        "release": release,
        "reason": reason.trim(),
    }))
    .map_err(|error| error.to_string())?;
    match transport
        .fetch::<MediaVerdict>(transport.post_body(url, "application/json", body))
        .await
    {
        Ok(verdict) => Ok(Cleared::Row(verdict)),
        Err(nodus_client_core::Error::Status {
            status: 404 | 422, ..
        }) => Ok(Cleared::AlreadyCleared),
        Err(error) => Err(error.to_string()),
    }
}

/// One moderation rule, as `GET /api/v1/admin/moderation-rules` answers it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ModerationRule {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub pattern_kind: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub hits: i64,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// The bytes a dry run matched, and the text they were.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RuleSpan {
    #[serde(default)]
    pub start: usize,
    #[serde(default)]
    pub end: usize,
    #[serde(default)]
    pub text: String,
}

/// The rule a dry run tested, as the instance echoes it back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DryRunRule {
    #[serde(default)]
    pub id: String,
}

/// What `POST …/{id}/dry-run` answered.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RuleDryRun {
    #[serde(default)]
    pub rule: Option<DryRunRule>,
    #[serde(default)]
    pub matched: bool,
    #[serde(default)]
    pub span: Option<RuleSpan>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub scanned_bytes: usize,
    #[serde(default)]
    pub truncated: bool,
}

/// The body one rule is written with. Every field is a replacement, which is what both
/// `POST` and `PUT` mean here.
fn rule_body(
    name: &str,
    pattern: &str,
    pattern_kind: &str,
    scope: &str,
    action: &str,
    enabled: bool,
) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&serde_json::json!({
        "name": name,
        "pattern": pattern,
        "pattern_kind": pattern_kind,
        "scope": scope,
        "action": action,
        "enabled": enabled,
    }))
    .map_err(|error| error.to_string())
}

fn rules_url() -> Result<(nodus_mastodon::BrowserInstance, url::Url), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/moderation-rules")
        .map_err(|error| error.to_string())?;
    Ok((transport, url))
}

/// Every rule the instance holds. Requires `admin:read`.
pub async fn moderation_rules() -> Result<Vec<ModerationRule>, String> {
    let (transport, url) = rules_url()?;
    transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// Writes one rule. Requires `admin:write`; a pattern that cannot compile is refused with
/// the instance's own sentence, which the screen shows rather than swallows.
pub async fn create_moderation_rule(
    name: &str,
    pattern: &str,
    pattern_kind: &str,
    scope: &str,
    action: &str,
    enabled: bool,
) -> Result<ModerationRule, String> {
    let (transport, url) = rules_url()?;
    let body = rule_body(name, pattern, pattern_kind, scope, action, enabled)?;
    transport
        .fetch(transport.post_body(url, "application/json", body))
        .await
        .map_err(|error| error.to_string())
}

/// Rewrites one rule in place. Requires `admin:write`.
pub async fn update_moderation_rule(
    id: &str,
    name: &str,
    pattern: &str,
    pattern_kind: &str,
    scope: &str,
    action: &str,
    enabled: bool,
) -> Result<ModerationRule, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "moderation-rules", id])
        .map_err(|error| error.to_string())?;
    let body = rule_body(name, pattern, pattern_kind, scope, action, enabled)?;
    transport
        .fetch(transport.put_body(url, body))
        .await
        .map_err(|error| error.to_string())
}

/// Deletes one rule. Requires `admin:write`.
pub async fn delete_moderation_rule(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "moderation-rules", id])
        .map_err(|error| error.to_string())?;
    transport
        .send_empty(transport.delete(url))
        .await
        .map_err(|error| error.to_string())
}

/// Tries one rule against a sample body. Requires `admin:read`; nothing is written.
pub async fn dry_run_moderation_rule(id: &str, sample: &str) -> Result<RuleDryRun, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "moderation-rules", id, "dry-run"])
        .map_err(|error| error.to_string())?;
    let body = serde_json::to_vec(&serde_json::json!({ "sample": sample }))
        .map_err(|error| error.to_string())?;
    transport
        .fetch(transport.post_body(url, "application/json", body))
        .await
        .map_err(|error| error.to_string())
}

/// One rung of the escalation policy, as the instance writes it down.
///
/// `duration_days` is absent for a rung that does not lapse, which is how the instance's own
/// store spells it: `skip_serializing_if` keeps an untouched rung out of the request body
/// instead of sending a null the form never asked for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EscalationStep {
    /// The live strike count this rung answers.
    pub strikes: i64,
    /// `warn`, `flag`, `silence` or `suspend`.
    pub action: String,
    /// How long a temporary action lasts, in days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_days: Option<i64>,
}

/// The escalation policy as `GET|PUT /api/v1/admin/escalation` answers it.
///
/// `name` and `updated_at` are the instance's, not the form's: the request sends only
/// `enabled`, `window_days` and `steps`, and the answer says which policy row now stands.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EscalationPolicy {
    /// The name the audit trail cites.
    #[serde(default)]
    pub name: String,
    /// Whether the ladder is applied.
    pub enabled: bool,
    /// How long a strike counts, in days.
    pub window_days: i64,
    /// The ladder, in the order the instance reads it.
    #[serde(default)]
    pub steps: Vec<EscalationStep>,
    /// When it was last written.
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Who wrote it.
    #[serde(default)]
    pub updated_by: Option<String>,
}

/// The escalation policy this instance applies.
pub async fn escalation_policy() -> Result<EscalationPolicy, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/escalation")
        .map_err(|error| error.to_string())?;
    transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// Writes the ladder, and answers with the policy as stored.
///
/// A rung the store cannot reason about comes back as a 422 whose sentence names the rung
/// by position; that sentence is carried through unchanged, because it is the only thing
/// that tells an administrator which row to fix.
pub async fn save_escalation_policy(
    enabled: bool,
    window_days: i64,
    steps: &[EscalationStep],
) -> Result<EscalationPolicy, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/escalation")
        .map_err(|error| error.to_string())?;
    let body = serde_json::to_vec(&serde_json::json!({
        "enabled": enabled,
        "window_days": window_days,
        "steps": steps,
    }))
    .map_err(|error| error.to_string())?;
    let mut request = transport.put(url);
    request
        .headers
        .push(("Content-Type".to_owned(), "application/json".to_owned()));
    request.body = Some(body);
    transport
        .fetch(request)
        .await
        .map_err(|error| error.to_string())
}

/// The rung an account would meet next, as the account payload carries it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct NextEscalation {
    /// The live strike count that triggers it.
    pub strikes: i64,
    /// `warn`, `flag`, `silence` or `suspend`.
    pub action: String,
    /// How long it lasts, when it lapses.
    #[serde(default)]
    pub duration_days: Option<i64>,
}

/// One row of a moderator's account search, as the instance sends it.
///
/// The SDK's `AdminAccount` predates the two escalation fields, and a flattened deserialize
/// drops what it does not know. Reading the row here instead — the same request, the same
/// payload — keeps the strikes without a second round trip or a second model of the account.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct AccountRow {
    /// Every field the directory has always carried.
    #[serde(flatten)]
    pub account: AdminAccount,
    /// Live strikes inside the policy window. Absent on an instance that predates them.
    #[serde(default)]
    pub strike_count: Option<i64>,
    /// The rung the next strike would meet, or `None` when the ladder has run out.
    #[serde(default)]
    pub next_escalation: Option<NextEscalation>,
    /// The staff role the instance spells out: `none`, `moderator` or `admin`. Absent on an
    /// instance that predates roles, where the administrator flag is the only word for it.
    #[serde(default)]
    pub role: Option<String>,
}

/// The account directory, narrowed by the query, with the escalation each row carries.
pub async fn search_accounts(query: &AdminAccountQuery) -> Result<Vec<AccountRow>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/accounts")
        .map_err(|error| error.to_string())?;
    // The same parameters the SDK's search sends, in its order, so switching between the two
    // cannot change which rows an instance answers with.
    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    if let Some(origin) = query.origin {
        pairs.push(("origin", origin.as_str().to_owned()));
    }
    if let Some(status) = query.status {
        pairs.push(("status", status.as_str().to_owned()));
    }
    for (key, value) in [
        ("username", query.username.as_ref()),
        ("display_name", query.display_name.as_ref()),
        ("email", query.email.as_ref()),
        ("by_domain", query.by_domain.as_ref()),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            pairs.push((key, value.clone()));
        }
    }
    if query.staff {
        pairs.push(("staff", "true".to_owned()));
    }
    if let Some(limit) = query.limit {
        pairs.push(("limit", limit.to_string()));
    }
    if let Some(offset) = query.offset {
        pairs.push(("offset", offset.to_string()));
    }
    {
        let mut serializer = url.query_pairs_mut();
        for (key, value) in &pairs {
            serializer.append_pair(key, value);
        }
    }
    transport
        .fetch::<Vec<AccountRow>>(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// One strike in an account's history, as `GET /api/v1/admin/accounts/{id}/strikes` sends it.
///
/// There is no moderator field in the answer: the instance stores the action and the reason,
/// not the person, so a screen can say what happened but not who did it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Strike {
    pub id: String,
    /// The rule the strike was charged under, when one was named.
    #[serde(default)]
    pub rule_id: Option<String>,
    /// The rule's name as the instance holds it now; `None` once the rule is gone.
    #[serde(default)]
    pub rule_name: Option<String>,
    /// `warn`, `flag`, `silence` or `suspend`.
    pub action: String,
    #[serde(default)]
    pub reason: String,
    pub created_at: String,
    pub expires_at: String,
    /// Whether the strike still counts against the account.
    #[serde(default)]
    pub live: bool,
}

/// What a role change or a warning answered: the account as the instance now holds it.
///
/// Only the fields the two controls report are read — the role for one, the live strike
/// count and the next rung for the other. Restating the whole directory row here would be a
/// second copy of a shape that already exists.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct AccountAfter {
    /// `none`, `moderator` or `admin`, as the instance now spells it.
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub username: String,
    /// Live strikes inside the policy window, after the write.
    #[serde(default)]
    pub strike_count: Option<i64>,
    /// The rung the next strike would meet.
    #[serde(default)]
    pub next_escalation: Option<NextEscalation>,
}

/// Sets one account's staff role (`none`, `moderator` or `admin`).
///
/// The route answers to `role == admin`, and the instance refuses to demote the last
/// administrator or the caller's own account; either refusal comes back as the instance's
/// own sentence, which the screen shows as it stands.
pub async fn set_role(id: &str, role: &str) -> Result<AccountAfter, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "accounts", id, "role"])
        .map_err(|error| error.to_string())?;
    let body = serde_json::to_vec(&serde_json::json!({ "role": role }))
        .map_err(|error| error.to_string())?;
    transport
        .fetch(transport.put_body(url, body))
        .await
        .map_err(|error| error.to_string())
}

/// Warns one account, with a preset's words or the moderator's own.
///
/// The instance writes the notification and the strike in one call — there is no quiet
/// variant — so the answer's strike count is what the row shows, and the sentence beside it
/// says the account was told.
pub async fn send_warning(
    id: &str,
    preset_id: Option<&str>,
    text: &str,
) -> Result<AccountAfter, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "accounts", id, "warn"])
        .map_err(|error| error.to_string())?;
    let mut body = serde_json::Map::new();
    body.insert("text".to_owned(), serde_json::Value::from(text));
    if let Some(preset_id) = preset_id.filter(|value| !value.is_empty()) {
        body.insert("preset_id".to_owned(), serde_json::Value::from(preset_id));
    }
    let body =
        serde_json::to_vec(&serde_json::Value::Object(body)).map_err(|error| error.to_string())?;
    transport
        .fetch(transport.post_body(url, "application/json", body))
        .await
        .map_err(|error| error.to_string())
}

/// Every strike on one account, newest first. Requires `admin:read`.
pub async fn account_strikes(id: &str) -> Result<Vec<Strike>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "accounts", id, "strikes"])
        .map_err(|error| error.to_string())?;
    transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// One queued federation delivery, as `GET /api/v1/admin/federation/deliveries` sends it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Delivery {
    /// The row's id, which the retry names.
    pub id: String,
    /// The inbox the activity is posted to.
    pub inbox_url: String,
    /// The peer, as the queue keys it.
    pub host: String,
    /// The activity's own id.
    #[serde(default)]
    pub activity_id: String,
    /// What kind of activity it is, when the queue recorded one.
    #[serde(default)]
    pub activity_kind: Option<String>,
    /// `pending`, `delivered`, `failed` or `gave_up`.
    pub state: String,
    /// How many times it has been tried.
    pub attempts: i32,
    /// What the peer answered last.
    #[serde(default)]
    pub last_status_code: Option<i32>,
    /// What went wrong last, as the transport worded it.
    #[serde(default)]
    pub last_error: Option<String>,
    /// When the queue will try it again.
    pub next_attempt_at: String,
    /// When it was queued.
    pub created_at: String,
    /// When it got through, for a delivered row.
    #[serde(default)]
    pub delivered_at: Option<String>,
}

/// The whole queue's count per state, not one page's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
pub struct DeliveryCounts {
    /// Waiting to be sent.
    pub pending: i64,
    /// Tried, failed, and waiting for its next attempt.
    pub failed: i64,
    /// Out of attempts.
    pub gave_up: i64,
    /// Reached the peer.
    pub delivered: i64,
}

/// One page of the delivery queue, with the counts the filter buttons show.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DeliveryPage {
    /// The rows, newest first.
    pub deliveries: Vec<Delivery>,
    /// The whole queue's counts.
    #[serde(default)]
    pub counts: DeliveryCounts,
}

/// One report this instance sent to a peer, as the forwarding endpoints send it.
///
/// The same row appears on both screens that read it: the report queue uses it to say what a
/// single report's forward reached, and `/admin/federation` holds every one of them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Forwarded {
    /// The forward's own id.
    #[serde(default)]
    pub id: String,
    /// The report it carries.
    #[serde(default)]
    pub report_id: String,
    /// The peer it was addressed to.
    #[serde(default)]
    pub target_domain: String,
    /// The activity a peer receives, as it was signed.
    #[serde(default)]
    pub activity_uri: String,
    /// `pending`, `delivered`, `failed` or `gave_up`.
    #[serde(default)]
    pub state: String,
    /// How many times delivery has been tried.
    #[serde(default)]
    pub attempts: i32,
    /// What the peer or the queue last said went wrong.
    #[serde(default)]
    pub last_error: Option<String>,
    /// When the forward was queued.
    #[serde(default)]
    pub created_at: String,
    /// When it last changed.
    #[serde(default)]
    pub updated_at: String,
}

/// One page of the delivery queue, narrowed by state and peer.
pub async fn deliveries(
    state: Option<&str>,
    host: Option<&str>,
    offset: i64,
    limit: u16,
) -> Result<DeliveryPage, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/federation/deliveries")
        .map_err(|error| error.to_string())?;
    {
        let mut serializer = url.query_pairs_mut();
        if let Some(state) = state.filter(|value| !value.is_empty()) {
            serializer.append_pair("state", state);
        }
        if let Some(host) = host.map(str::trim).filter(|value| !value.is_empty()) {
            serializer.append_pair("host", host);
        }
        serializer.append_pair("limit", &limit.to_string());
        if offset > 0 {
            serializer.append_pair("offset", &offset.to_string());
        }
    }
    transport
        .fetch::<DeliveryPage>(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// Sends one delivery again now.
pub async fn retry_delivery(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url(&format!("/api/v1/admin/federation/deliveries/{id}/retry"))
        .map_err(|error| error.to_string())?;
    transport
        .fetch::<serde_json::Value>(transport.post(url))
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// The number the server says it actually re-queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Requeued {
    /// Failed and given-up rows now waiting again.
    pub requeued: i64,
}

/// Sends every failed and given-up delivery again, optionally for one peer.
///
/// The answer's `requeued` is returned rather than a bare `Ok`: "done" does not say whether
/// anything was actually waiting, and an operator who fixed a key needs that number.
pub async fn retry_failed(host: Option<&str>) -> Result<i64, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/federation/deliveries/retry-failed")
        .map_err(|error| error.to_string())?;
    let body = serde_json::to_vec(&serde_json::json!({
        "host": host.map(str::trim).filter(|value| !value.is_empty()),
    }))
    .map_err(|error| error.to_string())?;
    let mut request = transport.post(url);
    request
        .headers
        .push(("Content-Type".to_owned(), "application/json".to_owned()));
    request.body = Some(body);
    transport
        .fetch::<Requeued>(request)
        .await
        .map(|answer| answer.requeued)
        .map_err(|error| error.to_string())
}

/// One paused domain, as `GET /api/v1/admin/federation/domain-pauses` sends it.
///
/// An expired pause stays in the list: the row is a record of something that happened, and
/// the instance's `expired` says whether it is still biting.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct PausedDomain {
    /// The bare hostname the pause was set on.
    pub domain: String,
    /// Why, as the administrator wrote it.
    #[serde(default)]
    pub reason: String,
    /// When it lapses, in RFC 3339, when it does.
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Whether the instance's clock has already passed that moment.
    #[serde(default)]
    pub expired: bool,
    /// When it was set.
    pub created_at: String,
    /// Who set it, as a name this instance still knows.
    #[serde(default)]
    pub created_by_handle: Option<String>,
}

/// Every domain this instance has paused, expired ones included.
///
/// A Nodus extension: an instance whose `/api/v1/meta` does not name `domain_pauses` is not
/// asked, and the panel is not drawn.
pub async fn domain_pauses() -> Result<Vec<PausedDomain>, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let mut url = transport
        .url("/api/v1/admin/federation/domain-pauses")
        .map_err(|error| error.to_string())?;
    url.query_pairs_mut()
        .append_pair("limit", &PAGE.to_string());
    // A bare array rather than an envelope: the row list is the whole answer.
    transport
        .fetch::<Vec<PausedDomain>>(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// Pauses one domain, with the reason it was set and an optional expiry day.
///
/// `expires_on` is the date input's own `YYYY-MM-DD`, empty for a pause with no expiry; the
/// endpoint takes a timestamp, so the day is widened to the instant it starts. A pause is
/// not a block: it lapses, and the peer is never told.
pub async fn pause_domain(
    domain: &str,
    reason: &str,
    expires_on: &str,
) -> Result<PausedDomain, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/federation/domain-pauses")
        .map_err(|error| error.to_string())?;
    let mut body = serde_json::json!({ "domain": domain.trim(), "reason": reason.trim() });
    match expires_on.trim() {
        "" => {}
        day if day.len() == 10 => {
            body["expires_at"] = serde_json::Value::String(format!("{day}T00:00:00Z"));
        }
        other => body["expires_at"] = serde_json::Value::String(other.to_owned()),
    }
    let body = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
    let mut request = transport.post(url);
    request
        .headers
        .push(("Content-Type".to_owned(), "application/json".to_owned()));
    request.body = Some(body);
    transport
        .fetch::<PausedDomain>(request)
        .await
        .map_err(|error| error.to_string())
}

/// Lifts one domain's pause now, naming the domain in the path.
pub async fn lift_domain_pause(domain: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "federation", "domain-pauses", domain])
        .map_err(|error| error.to_string())?;
    // A `DELETE` with no body: the domain is already the path's segment.
    transport
        .send_empty(transport.delete(url))
        .await
        .map_err(|error| error.to_string())
}

/// One of the instance's own rules, as `GET /api/v1/admin/rules` answers it.
///
/// This is the document a report cites — `instance_rules`, which the sign-up form and the
/// about page publish — and not the `/admin/moderation-rules` panel beside it, whose rows
/// are patterns the scanning engine matches. The two are separate resources and separate
/// capabilities, so they are separate types here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct InstanceRule {
    pub id: String,
    #[serde(default)]
    pub text: String,
    /// Zero-based position, which is the order a reader cites: "rule 3".
    #[serde(default)]
    pub ordinal: i32,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

fn instance_rules_url() -> Result<(nodus_mastodon::BrowserInstance, url::Url), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/admin/rules")
        .map_err(|error| error.to_string())?;
    Ok((transport, url))
}

/// The instance's rules, in the order it publishes them. Requires `admin:read`.
pub async fn instance_rules() -> Result<Vec<InstanceRule>, String> {
    let (transport, url) = instance_rules_url()?;
    transport
        .fetch(transport.get(url))
        .await
        .map_err(|error| error.to_string())
}

/// Adds one rule. `ordinal` places it; absent appends, which is what the add form does.
pub async fn create_instance_rule(
    text: &str,
    ordinal: Option<i32>,
) -> Result<InstanceRule, String> {
    let (transport, url) = instance_rules_url()?;
    let mut body = serde_json::json!({ "text": text.trim() });
    if let Some(ordinal) = ordinal {
        body["ordinal"] = serde_json::json!(ordinal);
    }
    let body = serde_json::to_vec(&body).map_err(|error| error.to_string())?;
    let mut request = transport.post(url);
    request
        .headers
        .push(("Content-Type".to_owned(), "application/json".to_owned()));
    request.body = Some(body);
    transport
        .fetch::<InstanceRule>(request)
        .await
        .map_err(|error| error.to_string())
}

/// Rewrites a rule's text, moves it, or both.
///
/// `text: None` leaves the words alone and `ordinal: None` leaves the position alone, which
/// is how the endpoint is written: a reorder does not have to resend the sentence, and an
/// edit does not have to restate where the rule already is.
pub async fn update_instance_rule(
    id: &str,
    text: Option<&str>,
    ordinal: Option<i32>,
) -> Result<InstanceRule, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "rules", id])
        .map_err(|error| error.to_string())?;
    let mut body = serde_json::Map::new();
    if let Some(text) = text {
        body.insert(
            "text".to_owned(),
            serde_json::Value::String(text.trim().to_owned()),
        );
    }
    if let Some(ordinal) = ordinal {
        body.insert("ordinal".to_owned(), serde_json::json!(ordinal));
    }
    let body =
        serde_json::to_vec(&serde_json::Value::Object(body)).map_err(|error| error.to_string())?;
    transport
        .fetch::<InstanceRule>(transport.put_body(url, body))
        .await
        .map_err(|error| error.to_string())
}

/// Removes one rule. A report that cited it keeps the id it cited.
pub async fn delete_instance_rule(id: &str) -> Result<(), String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&["api", "v1", "admin", "rules", id])
        .map_err(|error| error.to_string())?;
    transport
        .send_empty(transport.delete(url))
        .await
        .map_err(|error| error.to_string())
}

/// What a refresh of one remote actor did, in the three shapes the instance answers with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// The peer answered and the stored profile was compared against it. The list of field
    /// names is the instance's own: it says which fields actually moved, and an empty list
    /// means the peer's document already matched what this instance holds.
    Changed {
        /// `display_name`, `note`, `avatar_url`, `header_url`, `moved_to` — only the ones
        /// that changed.
        changed: Vec<String>,
        /// The account's name as the refreshed document spells it, so a screen can say
        /// what the name is now rather than only that it moved.
        display_name: String,
        /// The account's address, `someone@instance.example`.
        acct: String,
    },
    /// `422`: the peer answered with a document for some other actor, so nothing was
    /// written. This is a mismatch to report, not a refresh to retry.
    Mismatch(String),
    /// `502`: the peer could not be reached. The sentence carries the peer's own error.
    Unreachable(String),
}

/// The account as the refresh answer carries it, cut down to what the screen says.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct RefreshedActor {
    #[serde(default)]
    pub acct: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, serde::Deserialize)]
struct ActorRefreshAnswer {
    #[serde(default)]
    changed: Vec<String>,
    actor: RefreshedActor,
}

/// Re-reads one remote actor from its own instance, now, instead of waiting for it to
/// federate something new.
///
/// The address is the path's one segment — `url_segments` percent-encodes it, and the route
/// behind it is a wildcard, so the encoded slashes arrive intact — with the `/refresh`
/// suffix the handler strips back off.
pub async fn refresh_remote_actor(actor_uri: &str) -> Result<RefreshOutcome, String> {
    let transport = admin_transport()
        .or_else(instance_transport)
        .ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url_segments(&[
            "api",
            "v1",
            "admin",
            "federation",
            "actors",
            actor_uri,
            "refresh",
        ])
        .map_err(|error| error.to_string())?;
    match transport
        .fetch::<ActorRefreshAnswer>(transport.post(url))
        .await
    {
        Ok(answer) => Ok(RefreshOutcome::Changed {
            changed: answer.changed,
            display_name: answer.actor.display_name,
            acct: answer.actor.acct,
        }),
        // The three arms are read apart because they mean different things to a reader:
        // an unreachable peer is the network's, a mismatched document is the peer's, and
        // anything else is this instance's.
        Err(nodus_client_core::Error::Status {
            status: 422,
            message,
        }) => Ok(RefreshOutcome::Mismatch(message)),
        Err(nodus_client_core::Error::Status {
            status: 502,
            message,
        }) => Ok(RefreshOutcome::Unreachable(message)),
        Err(error) => Err(error.to_string()),
    }
}
