//! Moderation calls, and the one question that decides whether to show them.

use super::{admin_scope, admin_transport, instance_transport};
use leptos::prelude::{Get, RwSignal, Signal, Update};
use nodus_client_core::actions::{AdminActions, RegistrationActions};
use nodus_client_core::model::{
    AccountDetail, AccountNote, AdminAccount, AdminAccountQuery, AdminAlert, AdminMeasure,
    AdminMeta, AdminOverview, Announcement, AnnouncementDraft, AssetUpload, AuditEntry,
    AuditFilter,
    DomainAllow, DomainBlock, DomainBlockDraft, DomainImportOutcome, DomainSeverity,
    EmailDomainBlock, FlagState,
    ModerationAction, NewScanner, NodeLog, Report, ReportFilter, ScanFlag,
    ScanRequest, ScanSeverity, Scanner, ScannerAsset, ScannerKind, ScannerRule, Topology,
    TriggerOutcome, WarningPreset,
};

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
    /// Reads the wire spelling of `role` in `admin/meta` and in
    /// `GET /api/v1/nodus/account/role`. `none` — and any name this client predates — is
    /// not a role.
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
    // screen could not help, so it is not offered.
    if admin_transport().is_none() {
        return match ask_role().await {
            Ok(Some(_role)) => Staff::NeedsGrant,
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
        Err(MetaProblem::Refused) => match ask_role().await {
            Ok(Some(_role)) => Staff::Refused(
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

/// `GET /api/v1/nodus/account/role`, which any signed-in account may ask.
#[derive(Debug, serde::Deserialize)]
struct RoleAnswer {
    /// `none`, `moderator` or `admin`.
    #[serde(default)]
    role: String,
}

/// What the instance says this account's role is, asked with the ordinary session.
///
/// The session's own token, not the grant: the point of the question is to learn whether a
/// grant would help before asking for one, and a refused grant token answers 401 to it.
/// `Ok(None)` is the instance's `none` — a plain account, told so in one request instead of
/// being walked into a permission error only a database edit could fix.
async fn ask_role() -> Result<Option<Role>, String> {
    let transport = instance_transport().ok_or_else(|| "Not signed in.".to_owned())?;
    let url = transport
        .url("/api/v1/nodus/account/role")
        .map_err(|error| error.to_string())?;
    let bytes = transport
        .fetch_bytes(transport.get(url))
        .await
        .map_err(|error| format!("The instance did not say what role this account holds: {error}"))?;
    let answer: RoleAnswer = serde_json::from_slice(&bytes).map_err(|error| {
        format!("The instance's answer about this account's role could not be read: {error}")
    })?;
    Ok(Role::parse(&answer.role))
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
