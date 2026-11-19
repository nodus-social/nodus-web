//! Fixtures for the end-to-end tests.
//!
//! The client is the thing under test, so every response here is a constant: no
//! instance, no recommendation service, no database. That is what makes these tests
//! cheap enough to run on every commit — and honest, because a test that needed a live
//! instance would be measuring the instance.

export const HOST = 'stub.instance'
export const TOKEN = 'stub-token'
export const ACCOUNT_ID = '1'

export const account = {
  id: ACCOUNT_ID,
  acct: 'ada',
  display_name: 'Ada',
  note: '<p>Writes tests.</p>',
  avatar: 'https://files.example/ada.png',
  avatar_static: 'https://files.example/ada.png',
  header: 'https://files.example/header.png',
  bot: false,
  locked: false,
  followers_count: 3,
  following_count: 4,
  statuses_count: 5,
}

export const otherAccount = {
  ...account,
  id: '2',
  acct: 'grace',
  display_name: 'Grace',
}

/** A status, with the minimum an instance sends and the fields a view reads. */
export function status(overrides = {}) {
  return {
    id: '100',
    created_at: new Date('2026-11-07T12:00:00Z').toISOString(),
    content: '<p>Hello from the fixture.</p>',
    visibility: 'public',
    account,
    favourites_count: 1,
    reblogs_count: 2,
    replies_count: 0,
    favourited: false,
    reblogged: false,
    bookmarked: false,
    sensitive: false,
    spoiler_text: '',
    media_attachments: [],
    mentions: [],
    tags: [],
    ...overrides,
  }
}

/** The `localStorage` record the client writes at sign-in (`nodus-session/2`). */
export function sessionRecord({ host = HOST, token = TOKEN, id = ACCOUNT_ID, acct = 'ada' } = {}) {
  return ['nodus-session/2', host, token, id, acct].join('\n')
}

/**
 * Stub every request the client makes.
 *
 * One handler per shape rather than a generic echo: a generic echo lets a broken
 * request pass by matching the wrong route, which is the class of bug these tests
 * exist to catch.
 */
/**
 * Every stubbed response needs these.
 *
 * The client talks to the instance on its own origin (`https://stub.instance`), so the
 * browser treats each call as cross-origin: without `access-control-allow-origin` the
 * stubbed body is delivered and then thrown away by the CORS check, which looks exactly
 * like a transport failure and used to sign the test session out.
 */
const CORS = {
  'access-control-allow-origin': '*',
  'access-control-allow-headers': '*',
  'access-control-allow-methods': 'GET, POST, PUT, PATCH, DELETE, OPTIONS',
}

/**
 * Every capability name the client knows, as `/api/v1/meta` publishes them.
 *
 * A name the answer omits is a surface the instance does not serve, so the default stub
 * is a fully-equipped Nodus instance: a test that wants a narrower one passes `features`.
 */
export const FEATURES = [
  'feed_accounts',
  'events',
  'sync_token',
  'admin',
  'moderation',
  'scanners',
  'topology',
  'protocol',
  'remote_blocks',
  'polls',
  'media_quarantine',
  'moderation_rules',
  'federation_delivery',
  'domain_pauses',
  'report_forwarding',
  'settings_transfer',
]

/** The scopes a role carries, as the two role answers send them. */
export function scopesOf(role) {
  return role === 'none' ? [] : ['admin:read', 'admin:write']
}

/**
 * One moderation rule, as the rules panel reads it.
 *
 * `hits` is the instance's tally, not the screen's: it counts what the rule has matched
 * since it was written, which is the number an administrator checks before trusting it.
 */
export function moderationRule(overrides = {}) {
  return {
    id: '7001',
    name: 'Spam words',
    pattern: 'buy cheap',
    pattern_kind: 'keyword',
    scope: 'inbound',
    action: 'flag',
    enabled: true,
    hits: 4,
    created_by: ACCOUNT_ID,
    created_at: '2026-11-01T09:00:00.000Z',
    updated_at: '2026-11-01T09:00:00.000Z',
    ...overrides,
  }
}

/** An empty recommendation feed, in the envelope the discovery API uses. */
export function emptyFeed() {
  return {
    feed_id: 'stub-feed',
    generated_at: '2026-11-07T12:00:00.000Z',
    contract: '0.1.0',
    model_version: 1,
    similarity_age_hours: 0,
    items: [],
    next_cursor: null,
  }
}

export async function stubApi(
  page,
  { statuses = [status()], status: single, notifications = [], features = FEATURES, role = 'admin' } = {},
) {
  await page.route('**/api/v1/meta', (route) =>
    route.fulfill({ headers: CORS, json: { api: '1', contract: '0.1.0', min_contract: '0.1.0', features } }),
  )
  await page.route('**/api/v1/accounts/verify_credentials', (route) =>
    route.fulfill({ headers: CORS, json: account }),
  )
  await page.route('**/api/v1/timelines/home**', (route) => route.fulfill({ headers: CORS, json: statuses }))
  await page.route('**/api/v1/timelines/public**', (route) => route.fulfill({ headers: CORS, json: statuses }))
  await page.route(/\/api\/v1\/statuses\/[^/]+\/context$/, (route) =>
    route.fulfill({ headers: CORS, json: { ancestors: [], descendants: [] } }),
  )
  // Publishing: `POST /api/v1/statuses` has no id, so the id route above never sees it.
  await page.route('**/api/v1/statuses', (route) =>
    route.fulfill({ status: 200, headers: CORS, json: single ?? statuses[0] }),
  )
  await page.route('**/api/v1/scheduled_statuses**', (route) =>
    route.fulfill({ status: 200, headers: CORS, json: [] }),
  )
  await page.route(/\/api\/v1\/statuses\/[^/]+$/, (route) =>
    route.fulfill({ headers: CORS, json: single ?? statuses[0] }),
  )
  await page.route('**/api/v1/notifications**', (route) =>
    route.fulfill({ headers: CORS, json: notifications }),
  )
  await page.route('**/api/v1/me', (route) =>
    route.fulfill({ headers: CORS, json: {
        acct: 'ada',
        instance_host: HOST,
        sync_enabled: false,
        telemetry_opt_in: false,
        contract: '0.1.0',
      },
    }),
  )
  // The recommendation feed is its own service behind the same origin, so it is stubbed on
  // this origin rather than on the instance's.
  await page.route('**/api/v1/feed/accounts**', (route) => route.fulfill({ headers: CORS, json: emptyFeed() }))
  await page.route('**/api/v1/feed/posts**', (route) => route.fulfill({ headers: CORS, json: emptyFeed() }))
  await page.route('**/api/v1/events**', (route) => route.fulfill({ status: 204, headers: CORS }))
  await page.route('**/api/v1/sync/token**', (route) => route.fulfill({ status: 204, headers: CORS }))
  await page.route('**/api/v1/blocks**', (route) => route.fulfill({ headers: CORS, json: [] }))
  await page.route('**/api/v1/mutes**', (route) => route.fulfill({ headers: CORS, json: [] }))
  // Who this account is to the instance. The console asks this before it offers a grant,
  // so a signed-in account with no staff role is told so rather than handed a button.
  await page.route('**/api/v1/nodus/account/role', (route) =>
    route.fulfill({ headers: CORS, json: { role, scopes: scopesOf(role) } }),
  )
  // Registered last: Playwright consults handlers newest-first, so this sees the preflight
  // before any method-specific handler and defers everything else.
  await page.route('**/api/**', (route) =>
    route.request().method() === 'OPTIONS'
      ? route.fulfill({ status: 204, headers: CORS })
      : route.fallback(),
  )
}

/** Seed the stored session so the client boots signed in. */
export async function signedIn(page, { host = HOST, id = ACCOUNT_ID, acct = 'ada' } = {}) {
  await page.addInitScript(
    ([record, instance, accountId, who]) => {
      localStorage.setItem('nodus.session', record)
      localStorage.setItem('nodus.host', instance)
      localStorage.setItem(
        'nodus.accounts/1',
        ['nodus-accounts/1', [instance, 'stub-token', accountId, who].join('\t')].join('\n'),
      )
      localStorage.setItem('nodus.account', `${instance}|${accountId}`)
    },
    [sessionRecord({ host, id, acct }), host, id, acct],
  )
}


/** The grant `localStorage` record, keyed per account and instance (`nodus-admin/2`). */
export const GRANT_KEY = 'nodus-admin/2'

/** One stored grant, in the format the client reads back. */
export function grantRecord({ host = HOST, account = ACCOUNT_ID, token = 'stub-admin-token' } = {}) {
  return [host, account, token].join('\t')
}

/** Seed a moderator grant for the signed-in account. */
export async function granted(page, options = {}) {
  await page.addInitScript(
    ([key, record]) => localStorage.setItem(key, record),
    [GRANT_KEY, grantRecord(options)],
  )
}

/**
 * Stub the anonymous reads.
 *
 * A shared link is read through this app's own `/public-api/...` proxy, not from the
 * instance's `/api/...`: the browser asks Nodus, and Nodus performs the one public read.
 * Same origin, so no CORS header is needed here.
 */
export async function stubPublicApi(page, { status: single = status(), context = { ancestors: [], descendants: [] } } = {}) {
  await page.route('**/public-api/status/**/context', (route) =>
    route.fulfill({ json: context }),
  )
  await page.route('**/public-api/status/**', (route) => route.fulfill({ json: single }))
  await page.route('**/public-api/account/**', (route) => route.fulfill({ json: account }))
}


/**
 * The escalation policy, as the panel reads it.
 *
 * Three rungs, because a ladder of one says nothing about order: strike 1 flags, strike 2
 * silences for a week, strike 3 suspends until a person lifts it.
 */
export function escalationPolicy(overrides = {}) {
  return {
    name: 'default',
    enabled: true,
    window_days: 30,
    steps: [
      { strikes: 1, action: 'flag' },
      { strikes: 2, action: 'silence', duration_days: 7 },
      { strikes: 3, action: 'suspend' },
    ],
    updated_at: '2026-11-01T09:00:00.000Z',
    updated_by: ACCOUNT_ID,
    ...overrides,
  }
}

/**
 * One queued federation delivery, as the admin endpoint answers it.
 *
 * Defaults to a failed row with a next attempt: the shape most of the queue is in.
 */
export function delivery(overrides = {}) {
  return {
    id: 'd-1',
    inbox_url: 'https://peer.example/inbox',
    host: 'peer.example',
    activity_id: 'https://stub.instance/activities/1',
    activity_kind: 'Create',
    state: 'failed',
    attempts: 3,
    last_status_code: 503,
    last_error: 'the peer answered 503',
    next_attempt_at: '2026-11-19T07:00:00Z',
    created_at: '2026-11-19T06:00:00Z',
    delivered_at: null,
    ...overrides,
  }
}

/** The whole queue's counts per state, when a test wants numbers other than the rows'. */
export function deliveryCounts(overrides = {}) {
  return { pending: 0, failed: 0, gave_up: 0, delivered: 0, ...overrides }
}

/** An account as the moderator console receives it. */
export function adminAccount(overrides = {}) {
  return {
    id: ACCOUNT_ID,
    username: 'ada',
    domain: null,
    email: 'ada@example.test',
    ip: null,
    confirmed: true,
    silenced: false,
    suspended: false,
    disabled: false,
    approved: true,
    admin: true,
    open_reports: 0,
    notes_count: 1,
    lossless_images: false,
    lossless_videos: false,
    created_at: '2026-11-01T00:00:00.000Z',
    account,
    ...overrides,
  }
}

/** A report as the moderator console receives it, with the account it is about. */
export function report(overrides = {}) {
  return {
    id: '7001',
    state: 'open',
    action_taken: false,
    category: 'spam',
    comment: 'Sells something.',
    rule_ids: [],
    action_taken_note: '',
    action_taken_at: null,
    action_taken_by: null,
    assigned_account_id: null,
    assigned_account: null,
    reporter: account,
    created_at: '2026-11-07T12:00:00.000Z',
    updated_at: '2026-11-07T12:00:00.000Z',
    target_account: otherAccount,
    statuses: [],
    ...overrides,
  }
}

/** A scanner as the console receives it. */
export function scanner(overrides = {}) {
  return {
    id: '9001',
    name: 'nsfw',
    kind: 'image',
    endpoint: 'https://scanner.example',
    enabled: true,
    asset_id: null,
    contract: 1,
    labels: ['nsfw', 'gore'],
    manifest: { contract: 1 },
    manifest_at: '2026-11-07T12:00:00.000Z',
    timeout_ms: 5000,
    rules: [],
    ...overrides,
  }
}

/** A flag as the moderator console receives it. */
export function scanFlag(overrides = {}) {
  return {
    id: '8001',
    scan_id: '8000',
    status_id: '100',
    account_id: ACCOUNT_ID,
    reason: 'nsfw scored 0.82',
    score: 0.82,
    labels: ['nsfw (0.82)'],
    severity: 'flag',
    state: 'open',
    created_at: '2026-11-07T12:00:00.000Z',
    ...overrides,
  }
}

/** One moderated remote account, as the console reads it back. */
export function remoteAccount(overrides = {}) {
  return {
    actor_uri: 'https://peer.example/users/x',
    handle: 'x',
    domain: 'peer.example',
    action: 'suspend',
    reason: 'Ban evasion.',
    expires_at: null,
    expired: false,
    created_at: '2026-11-18T17:00:00Z',
    created_by: '1',
    created_by_handle: 'ada',
    ...overrides,
  }
}

/** One inbound block, as the console reads it back. */
export function remoteBlock(overrides = {}) {
  return {
    actor: 'https://peer.example/users/x',
    host: 'peer.example',
    created_at: '2026-11-18T17:00:00Z',
    ...overrides,
  }
}

/**
 * One paused domain, as the admin endpoint answers it.
 *
 * Defaults to a pause still in force, with an expiry three days out: the row most of a
 * paused list is in. `expired` is the instance's answer, not the console's arithmetic.
 */
export function pausedDomain(overrides = {}) {
  return {
    domain: 'quiet.example',
    reason: 'their queue is flooding us',
    expires_at: '2026-11-25T00:00:00Z',
    expired: false,
    created_at: '2026-11-18T09:00:00Z',
    created_by: ACCOUNT_ID,
    created_by_handle: 'ada',
    ...overrides,
  }
}

/** One followed block list, as the instance reports it after a sync. */
export function blockSubscription(overrides = {}) {
  return {
    id: 'sub-1',
    url: 'https://lists.example/blocklist.csv',
    format: 'csv',
    enabled: true,
    etag: 'W/"8"',
    last_modified: '2026-11-18T09:00:00Z',
    last_synced_at: '2026-11-18T09:05:00Z',
    last_error: null,
    created_at: '2026-11-17T09:00:00Z',
    ...overrides,
  }
}

/** One appeal against a decision about the appealing account, as the queue carries it. */
export function appeal(overrides = {}) {
  return {
    id: '3101',
    account_id: ACCOUNT_ID,
    handle: 'ada',
    restriction: null,
    action: 'suspend',
    reason: 'I was quoting the post, not selling anything.',
    status: 'pending',
    decision_note: null,
    decided_by: null,
    created_at: '2026-11-18T09:00:00.000Z',
    age_seconds: 7_200,
    decided_at: null,
    ...overrides,
  }
}

/**
 * One attachment a content set caught, as the quarantine list carries it.
 *
 * `set` is the name of the set that matched — the name a moderator acts on — and
 * `status_id` is the post it hung on.
 */
export function quarantinedMedia(overrides = {}) {
  return {
    id: '901',
    status_id: '9001',
    account_id: '2',
    content_type: 'image/png',
    url: 'https://peer.example/media/901.png',
    set: 'ncii-known-hashes',
    content_hash: '7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069',
    perceptual_hash: 'c0ffee',
    state: 'quarantined',
    created_at: '2026-11-18T10:00:00Z',
    ...overrides,
  }
}

/** The performance document for one window, with every counter the screen shows. */
export function performanceStats(overrides = {}) {
  return {
    signups: 5,
    active_accounts: 4,
    posts: 31,
    reports: { opened: 3, resolved: 7, reopened: 1, pending: 2 },
    decisions_by_action: { approve: 2, reject: 4, silence: 1, suspend: 0, enable: 1 },
    median_time_to_decision_seconds: 5_400,
    top_reporters: [{ account_id: '1', username: 'ada', domain: 'peer.example', count: 4 }],
    top_reported_domains: [{ domain: 'spam.example', count: 3 }],
    scanners: { queued: 1, flagged: 2, quarantined: 0 },
    ...overrides,
  }
}

/**
 * Stub the moderation and administration endpoints, and record what the console asked.
 *
 * Register this AFTER `stubApi`: Playwright consults handlers newest-first, so these win
 * over its catch-all. The returned `calls` array is the assertion surface for the screens
 * that write — a POST the console never sent cannot be faked by a stub.
 */
export async function adminApi(
  page,
  {
    reports = [report()],
    accounts = [adminAccount()],
    scanners = [],
    assets = [],
    flags = [scanFlag()],
    blocks = [],
    mail = [],
    presets = [],
    alerts = [],
    overview = {},
    measures = [],
    meta = {},
    role = 'admin',
    scopes = ['read', 'write', 'follow', 'admin:read', 'admin:write'],
    topology = null,
    logs = [],
    logStream = null,
    remoteBlocks = [],
    remoteAccounts = [],
    audit = [],
    announcements = [],
    bulkRefusals = {},
    blockImport = {
      parsed: 3,
      new: 2,
      updated: 1,
      unchanged: 0,
      invalid: [{ line: 4, reason: 'not a domain' }],
    },
    blockSubscriptions = [],
    blockSync = { status: 'ok', parsed: 3, new: 2, updated: 1, unchanged: 0, invalid: 0 },
    stats = performanceStats(),
    appeals = [appeal()],
    appealRefusals = {},
    quarantined = [quarantinedMedia()],
    rules = [moderationRule()],
    escalation = escalationPolicy(),
    escalationRefusal = null,
    deliveries = [],
    deliveryCounts: counts = null,
    domainPauses = [],
    forwarded = [],
    forwardRefusal = null,
    settings = {
      rules: [moderationRule()],
      instance_rules: [],
      warning_presets: [],
      escalation: escalationPolicy(),
      domain_pauses: [],
      domain_blocks: [],
      email_domain_blocks: [],
      scanner_rules: [],
      scanner_sets: [],
    },
    settingsReport = null,
    settingsRefusal = null,
  } = {},
) {
  // The list answers with the rows it holds: a pause the console sends is pushed here, so
  // the re-read after a write shows the same list the instance would.
  let paused = [...domainPauses]
  // Report forwarding: one list says what each report reached, and the POST is idempotent —
  // the first press queues a Flag (202) and a replay answers the row that already exists
  // (200), so a second press can never queue a second one.
  let sentForwards = [...forwarded]
  const forwardRow = (reportId) => ({
    id: `f${reportId}`,
    report_id: reportId,
    target_domain: 'peer.example',
    activity_uri: `https://nodus.example/activities/f${reportId}`,
    state: 'pending',
    attempts: 0,
    last_error: null,
    created_at: '2026-11-19T08:00:00Z',
    updated_at: '2026-11-19T08:00:00Z',
  })
  const calls = []
  const node = (name, status, extra = {}) => ({
    name,
    transport: 'unix',
    endpoint: `/var/lib/nodus/run/${name}.sock`,
    status,
    version: '0.1.0',
    uptime_seconds: 10_925,
    cpu_usage_percent: 1.5,
    rss_bytes: 24 * 1024 * 1024,
    queue_depth: null,
    latency_ms: 3,
    last_seen_at: null,
    last_error: null,
    version_mismatch: false,
    ...extra,
  })
  const nodes = topology ?? [node('api', 'healthy'), node('scan', 'dead', {
        latency_ms: 250,
        uptime_seconds: 0,
        cpu_usage_percent: 0,
        rss_bytes: 0,
        last_error: 'connection refused',
        last_seen_at: '2026-05-01T00:00:00Z',
      })]
  const record = (route) => calls.push({ method: route.request().method(), url: route.request().url(), body: route.request().postData() })
  const json = (route, body, status = 200) => {
    record(route)
    return route.fulfill({ status, headers: CORS, json: body })
  }

  await page.route(/\/api\/v1\/admin\/reports(\?|$)/, (route) => json(route, reports))
  await page.route(/\/api\/v1\/admin\/reports\/[^/]+\/(resolve|reopen|assign)$/, (route) =>
    json(route, reports[0] ?? report()),
  )
  await page.route(/\/api\/v1\/admin\/reports\/[^/]+$/, (route) => json(route, reports[0] ?? report()))
  await page.route(/\/api\/v1\/admin\/topology\/[^/]+\/logs/, (route) => json(route, logs))
  // The live tail, as server-sent events. Registered after the plain log route because
  // Playwright consults handlers newest-first and `/logs` is a prefix of `/logs/stream`.
  await page.route(/\/api\/v1\/admin\/topology\/[^/]+\/logs\/stream/, (route) => {
    record(route)
    if (!logStream) {
      // A node that offers no live tail: the console must fall back to its timer.
      return route.fulfill({ status: 404, headers: CORS, json: { error: 'no live log' } })
    }
    // One event per line, then a keep-alive comment: the shapes the instance writes.
    const body =
      logStream.map((line) => `data: ${JSON.stringify(line)}\n\n`).join('') + ': keep-alive\n\n'
    return route.fulfill({
      status: 200,
      headers: { ...CORS, 'content-type': 'text/event-stream' },
      body,
    })
  })
  await page.route(/\/api\/v1\/admin\/topology\/[^/]+\/trigger$/, (route) =>
    json(route, { accepted: true, detail: 'a pass is queued' }),
  )
  await page.route(/\/api\/v1\/admin\/topology$/, (route) => json(route, { version: '0.1.0', nodes }))
  // The escalation policy: one row, read whole and written whole. A refusal is returned as
  // the instance words it, because that sentence names the rung and is the only guidance.
  await page.route(/\/api\/v1\/admin\/escalation$/, (route) => {
    if (route.request().method() !== 'PUT') return json(route, escalation)
    record(route)
    if (escalationRefusal) {
      return route.fulfill({ status: 422, headers: CORS, json: { error: escalationRefusal } })
    }
    const body = JSON.parse(route.request().postData() ?? '{}')
    return route.fulfill({
      status: 200,
      headers: CORS,
      json: {
        ...escalation,
        enabled: body.enabled,
        window_days: body.window_days,
        steps: body.steps,
        updated_at: '2026-11-08T10:00:00.000Z',
        updated_by: ACCOUNT_ID,
      },
    })
  })
  // The delivery queue: the list answers with its rows and the whole queue's counts, and
  // the two retries answer with what they actually requeued.
  await page.route(/\/api\/v1\/admin\/federation\/deliveries(\?|$)/, (route) => {
    const counted = deliveryCounts()
    for (const row of deliveries) counted[row.state] = (counted[row.state] ?? 0) + 1
    return json(route, { deliveries, counts: counts ?? counted })
  })
  // Domain pauses: a temporary stop that lapses, unlike a block. The row the instance
  // answers with carries its own `expired`, which the console must not recompute.
  await page.route(/\/api\/v1\/admin\/federation\/domain-pauses(\?|$)/, (route) => {
    if (route.request().method() === 'GET') return json(route, paused)
    record(route)
    const body = JSON.parse(route.request().postData() ?? '{}')
    const row = pausedDomain({
      domain: body.domain,
      reason: body.reason ?? '',
      expires_at: body.expires_at ?? null,
      created_at: '2026-11-19T08:00:00Z',
    })
    paused = [...paused.filter((held) => held.domain !== row.domain), row]
    return route.fulfill({ status: 201, headers: CORS, json: row })
  })
  await page.route(/\/api\/v1\/admin\/federation\/domain-pauses\/[^/]+$/, (route) => {
    record(route)
    const domain = decodeURIComponent(new URL(route.request().url()).pathname.split('/').at(-1))
    if (route.request().method() === 'PUT') {
      const body = JSON.parse(route.request().postData() ?? '{}')
      const row = paused.find((held) => held.domain === domain)
      const updated = pausedDomain({ ...row, reason: body.reason, expires_at: body.expires_at ?? null })
      paused = paused.map((held) => (held.domain === domain ? updated : held))
      return json(route, updated)
    }
    paused = paused.filter((held) => held.domain !== domain)
    return route.fulfill({ status: 204, headers: CORS })
  })
  await page.route(/\/api\/v1\/admin\/federation\/deliveries\/[^/]+\/retry$/, (route) =>
    json(route, { id: route.request().url().split('/').at(-2), state: 'pending' }),
  )
  await page.route(/\/api\/v1\/admin\/federation\/deliveries\/retry-failed$/, (route) => {
    const body = JSON.parse(route.request().postData() ?? '{}')
    const requeued = deliveries.filter(
      (row) =>
        (row.state === 'failed' || row.state === 'gave_up') &&
        (!body.host || row.host === body.host),
    ).length
    return json(route, { requeued })
  })
  await page.route(/\/api\/v1\/admin\/accounts\?/, (route) => json(route, accounts))
  await page.route(/\/api\/v2\/admin\/accounts/, (route) => json(route, accounts))
  await page.route(/\/api\/v1\/admin\/accounts\/[^/]+\/action$/, (route) => json(route, accounts[0] ?? adminAccount()))
  await page.route(/\/api\/v1\/admin\/accounts\/[^/]+\/notes$/, (route) => json(route, { body: 'Noted.' }))
  await page.route(/\/api\/v1\/admin\/accounts\/[^/]+$/, (route) => {
    if (route.request().method() === 'DELETE') return json(route, {})
    return json(route, { ...(accounts[0] ?? adminAccount()), notes: [], flags: [], reports_against: 0 })
  })
  await page.route(/\/api\/v1\/admin\/scanners$/, (route) => json(route, scanners))
  await page.route(/\/api\/v1\/admin\/scanners\/[^/]+\/(refresh|rules)$/, (route) => json(route, scanners[0] ?? scanner()))
  await page.route(/\/api\/v1\/admin\/scanners\/[^/]+$/, (route) =>
    json(route, scanners[0] ?? scanner()),
  )
  await page.route(/\/api\/v1\/admin\/scanner_assets/, (route) => json(route, assets))
  // The bytes themselves, not a description of them: this is what the scanner was handed.
  await page.route(/\/api\/v1\/admin\/scanner_assets\/[^/]+\/download$/, (route) => {
    record(route)
    return route.fulfill({
      status: 200,
      headers: { ...CORS, 'content-type': 'application/octet-stream' },
      body: Buffer.from('a,b\n1,2\n'),
    })
  })
  await page.route(/\/api\/v1\/admin\/flags\?/, (route) => json(route, flags))
  await page.route(/\/api\/v1\/admin\/flags\/[^/]+\/resolve$/, (route) => json(route, flags[0] ?? scanFlag()))
  await page.route(/\/api\/v1\/admin\/domain_blocks/, (route) => json(route, blocks))
  // The in-place rewrite: the row comes back with the severity the form asked for. Registered
  // after the list route so this one answers PUT for a single block. It also matches
  // `/domain_blocks/import`, so the import route has to come after it: the later route wins.
  await page.route(/\/api\/v1\/admin\/domain_blocks\/[^/]+$/, (route) => {
    const id = route.request().url().split('/').pop()
    const held = blocks.find((block) => block.id === id) ?? blocks[0] ?? {}
    const form = new URLSearchParams(route.request().postData() ?? '')
    return json(route, { ...held, severity: form.get('severity') ?? held.severity })
  })
  await page.route(/\/api\/v1\/admin\/domain_blocks\/import$/, (route) => json(route, { blocked: 2, skipped: ['not a domain'] }))
  await page.route(/\/api\/v1\/admin\/domain_allows/, (route) => json(route, []))
  await page.route(/\/api\/v1\/admin\/federation\/remote-blocks/, (route) => {
    if (route.request().method() === 'DELETE') {
      record(route)
      return route.fulfill({ status: 204, headers: CORS })
    }
    return json(route, { blocks: remoteBlocks })
  })
  // The moderated accounts, one row each. The path carries the account's whole address as
  // one percent-encoded segment, so the row route is matched on the character after the
  // collection rather than on a slash.
  await page.route(/\/api\/v1\/admin\/federation\/remote-accounts\/.+$/, (route) => {
    if (route.request().method() === 'DELETE') {
      record(route)
      return route.fulfill({ status: 204, headers: CORS })
    }
    const held = remoteAccounts[0] ?? remoteAccount()
    const body = JSON.parse(route.request().postData() ?? '{}')
    // The instance answers with the row as it now stands, which is what the list redraws.
    return json(route, {
      ...held,
      action: body.action ?? held.action,
      reason: body.reason ?? held.reason,
      // A day is widened to the instant it starts, which is what the console sent.
      expires_at: body.expires_at ?? null,
      expired: false,
    })
  })
  await page.route(/\/api\/v1\/admin\/federation\/remote-accounts(\?|$)/, (route) =>
    json(route, remoteAccounts),
  )
  await page.route(/\/api\/v1\/admin\/email_domain_blocks/, (route) => json(route, mail))
  await page.route(/\/api\/v1\/admin\/warning_presets/, (route) => json(route, presets))
  await page.route(/\/api\/v1\/admin\/warning_presets\/[^/]+$/, (route) => {
    const id = route.request().url().split('/').pop()
    const held = presets.find((preset) => preset.id === id) ?? presets[0] ?? {}
    const body = JSON.parse(route.request().postData() ?? '{}')
    return json(route, { ...held, title: body.title ?? held.title, text: body.text ?? held.text })
  })
  await page.route(/\/api\/v1\/admin\/announcements$/, (route) => json(route, announcements))
  await page.route(/\/api\/v1\/admin\/announcements\/[^/]+$/, (route) => {
    if (route.request().method() === 'DELETE') {
      record(route)
      return route.fulfill({ status: 204, headers: CORS })
    }
    const id = route.request().url().split('/').pop()
    const held = announcements.find((item) => item.id === id) ?? announcements[0] ?? {}
    const form = new URLSearchParams(route.request().postData() ?? '')
    return json(route, { ...held, content: form.get('content') ?? held.content })
  })
  await page.route(/\/api\/v1\/admin\/alerts/, (route) => json(route, alerts))
  await page.route(/\/api\/v1\/admin\/measures/, (route) => json(route, measures))
  // A settings document, counted the way the instance counts it: every asked-for section
  // is present in the answer, and the counts come from what each one holds.
  await page.route(/\/api\/v1\/admin\/settings\/export/, (route) => {
    record(route)
    const asked = new URL(route.request().url()).searchParams.get('sections')
    const names = asked ? asked.split(',') : Object.keys(settings)
    const sections = Object.fromEntries(names.map((name) => [name, settings[name] ?? []]))
    return json(route, {
      format: 'nodus-admin-settings',
      version: 1,
      exported_at: '2026-11-19T09:00:00Z',
      instance: HOST,
      sections,
    })
  })
  // The import: the dry run reports and writes nothing; the write reports the same counts
  // with `dry_run: false`. A document the instance does not understand is refused in its
  // own words, which is the sentence the console must show unchanged.
  await page.route(/\/api\/v1\/admin\/settings\/import/, (route) => {
    record(route)
    const dryRun = new URL(route.request().url()).searchParams.get('dry_run') === 'true'
    if (settingsRefusal) {
      return json(route, { error: settingsRefusal }, 422)
    }
    const held = settingsReport ?? {
      rules: { created: 1, updated: 0, unchanged: 0, skipped: 0, conflicts: [] },
      escalation: { created: 0, updated: 1, unchanged: 0, skipped: 0, conflicts: [] },
    }
    return json(route, {
      format: 'nodus-admin-settings',
      version: 1,
      dry_run: dryRun,
      sections: held,
    })
  })
  // The block list as a file: the bytes are the answer, not a document around them.
  await page.route(/\/api\/v1\/admin\/domain-blocks\/export/, (route) => {
    record(route)
    const format = new URL(route.request().url()).searchParams.get('format') ?? 'csv'
    const body =
      format === 'json'
        ? JSON.stringify([{ domain: 'spam.example', severity: 'suspend' }])
        : 'domain,severity\nspam.example,suspend\n'
    return route.fulfill({
      status: 200,
      headers: { ...CORS, 'content-type': format === 'json' ? 'application/json' : 'text/csv' },
      body,
    })
  })
  await page.route(/\/api\/v1\/admin\/domain-blocks\/import/, (route) => {
    const dryRun = new URL(route.request().url()).searchParams.get('dry_run') === 'true'
    return json(route, { ...blockImport, applied: !dryRun })
  })
  await page.route(/\/api\/v1\/admin\/domain-blocks\/subscriptions\/[^/]+\/sync$/, (route) =>
    json(route, blockSync),
  )
  await page.route(/\/api\/v1\/admin\/domain-blocks\/subscriptions$/, (route) => {
    if (route.request().method() === 'POST') {
      const body = JSON.parse(route.request().postData() ?? '{}')
      return json(route, blockSubscription({ id: 'sub-new', url: body.url, format: body.format }))
    }
    return json(route, blockSubscriptions)
  })
  await page.route(/\/api\/v1\/admin\/domain-blocks\/subscriptions\/[^/]+$/, (route) => {
    if (route.request().method() === 'DELETE') {
      record(route)
      return route.fulfill({ status: 204, headers: CORS })
    }
    return json(route, blockSubscriptions[0] ?? blockSubscription())
  })
  // The window is answered from the fixture rather than echoed back: the console must read
  // the numbers it was sent, not the ones it asked for.
  // The appeal queue. The list is filtered by the status the console asked for, and a
  // decision rewrites the row in the fixture, so the redraw reads what the instance would
  // have stored. `appealRefusals` lets a test say another moderator got there first: the
  // row is decided behind this moderator's back and the answer is the instance's 409.
  const decideAppeal = (row, accept, note, who) => ({
    ...row,
    status: accept ? 'accepted' : 'rejected',
    decision_note: note || null,
    decided_by: who,
    decided_at: '2026-11-19T12:00:00.000Z',
    restriction: accept ? null : row.restriction,
  })
  await page.route(/\/api\/v1\/admin\/appeals\/[^/]+\/decide$/, (route) => {
    const id = decodeURIComponent(route.request().url().split('/').at(-2))
    const row = appeals.find((item) => item.id === id)
    if (!row) return json(route, { error: 'that appeal does not exist' }, 404)
    if (appealRefusals[id]) {
      Object.assign(row, decideAppeal(row, true, 'Handled from the other tab.', 'bob'))
      return json(route, { error: 'this appeal has already been decided' }, 409)
    }
    if (row.status !== 'pending') {
      return json(route, { error: 'this appeal has already been decided' }, 409)
    }
    const body = JSON.parse(route.request().postData() ?? '{}')
    Object.assign(row, decideAppeal(row, body.accept === true, body.note ?? '', 'ada'))
    return json(route, { ...row })
  })
  await page.route(/\/api\/v1\/admin\/appeals(\?|$)/, (route) => {
    const query = new URL(route.request().url()).searchParams
    const status = query.get('status') ?? ''
    const held = status && status !== 'all' ? appeals.filter((row) => row.status === status) : appeals
    const offset = Number(query.get('offset') ?? 0)
    const limit = Number(query.get('limit') ?? held.length)
    return json(route, held.slice(offset, offset + limit))
  })
  // The quarantine list. Releasing keeps the row and marks it released; deleting removes
  // it, because the attachment no longer exists. Both append the audit row the instance
  // writes, so the existing audit screen is where a test reads the decision back.
  await page.route(/\/api\/v1\/admin\/media\/[^/]+\/quarantine$/, (route) => {
    record(route)
    const id = decodeURIComponent(route.request().url().split('/').at(-2))
    const row = quarantined.find((item) => item.id === id)
    if (!row || row.state !== 'quarantined') {
      return json(
        route,
        { error: 'that attachment was not quarantined, so there is nothing to clear' },
        422,
      )
    }
    const body = JSON.parse(route.request().postData() ?? '{}')
    const release = body.release === true
    if (release) row.state = 'released'
    else quarantined.splice(quarantined.indexOf(row), 1)
    audit.unshift({
      id: `audit-media-${id}`,
      actor_id: ACCOUNT_ID,
      action: release ? 'release_media_quarantine' : 'delete_media_quarantine',
      target_type: 'media',
      target_id: id || row.id,
      detail: { status_id: row.status_id, set: row.set, reason: body.reason },
      created_at: '2026-11-19T12:00:00Z',
    })
    return json(route, {
      id,
      released: release,
      deleted: !release,
      status_id: row.status_id,
      reports_resolved: 1,
    })
  })
  // The rules engine. The patterns are matched for real — a keyword is a case-insensitive
  // substring and a regex is compiled by the browser — so a dry run in a test answers the
  // way the instance would, including the bytes and the text of the match.
  const ruleRefusal = (body) => {
    const kind = body.pattern_kind ?? 'keyword'
    const pattern = body.pattern ?? ''
    if (!pattern.trim()) return 'a rule needs a pattern'
    if (!['keyword', 'regex', 'domain'].includes(kind)) {
      return `pattern_kind must be one of keyword, regex, domain, not ${kind}`
    }
    if (kind === 'regex') {
      try {
        new RegExp(pattern, 'i')
      } catch (error) {
        return `the pattern is not a valid regex: parse error at byte 0: ${error.message}`
      }
    }
    return null
  }
  const ruleMatch = (rule, sample) => {
    if (rule.pattern_kind === 'regex') {
      let compiled
      try {
        compiled = new RegExp(rule.pattern, 'i')
      } catch (error) {
        return { refusal: `the pattern is not a valid regex: parse error at byte 0: ${error.message}` }
      }
      const found = compiled.exec(sample)
      if (!found) return { span: null }
      return { span: { start: found.index, end: found.index + found[0].length, text: found[0] } }
    }
    const at = sample.toLowerCase().indexOf(rule.pattern.toLowerCase())
    if (at < 0) return { span: null }
    return { span: { start: at, end: at + rule.pattern.length, text: sample.slice(at, at + rule.pattern.length) } }
  }
  await page.route(/\/api\/v1\/admin\/moderation-rules$/, (route) => {
    const method = route.request().method()
    if (method === 'GET') return json(route, rules)
    const body = JSON.parse(route.request().postData() ?? '{}')
    const refusal = ruleRefusal(body)
    if (refusal) return json(route, { error: refusal }, 422)
    const created = moderationRule({ id: `71${rules.length}0`, hits: 0, ...body })
    rules.push(created)
    return json(route, created, 201)
  })
  await page.route(/\/api\/v1\/admin\/moderation-rules\/[^/]+\/dry-run$/, (route) => {
    const id = decodeURIComponent(route.request().url().split('/').at(-2))
    const rule = rules.find((item) => item.id === id)
    const sample = JSON.parse(route.request().postData() ?? '{}').sample ?? ''
    const tested = ruleMatch(rule, sample)
    if (tested.refusal) return json(route, { error: tested.refusal }, 422)
    const span = tested.span
    const reason = span
      ? `rule "${rule.name}" (${rule.pattern_kind}) matched bytes ${span.start}..${span.end}: "${span.text}"`
      : `rule "${rule.name}" (${rule.pattern_kind}) did not match the sample`
    return json(route, {
      rule: {
        id: rule.id,
        name: rule.name,
        pattern: rule.pattern,
        pattern_kind: rule.pattern_kind,
        scope: rule.scope,
        action: rule.action,
        enabled: rule.enabled,
      },
      matched: Boolean(span),
      span,
      reason,
      scanned_bytes: new TextEncoder().encode(sample).length,
      truncated: false,
    })
  })
  await page.route(/\/api\/v1\/admin\/moderation-rules\/[^/]+$/, (route) => {
    const request = route.request()
    const id = decodeURIComponent(request.url().split('/').at(-1))
    const held = rules.find((item) => item.id === id)
    if (request.method() === 'DELETE') {
      if (held) rules.splice(rules.indexOf(held), 1)
      record(route)
      return route.fulfill({ status: 204, headers: CORS })
    }
    const body = JSON.parse(request.postData() ?? '{}')
    const refusal = ruleRefusal(body)
    if (refusal) return json(route, { error: refusal }, 422)
    if (held) Object.assign(held, body, { id })
    return json(route, held ?? moderationRule({ id, ...body }))
  })
  await page.route(/\/api\/v1\/admin\/media\/quarantined$/, (route) => json(route, quarantined))
  await page.route(/\/api\/v1\/admin\/stats(\?|$)/, (route) => json(route, stats))
  await page.route(/\/api\/v1\/admin\/overview$/, (route) =>
    json(route, {
      accounts: 12,
      local_accounts: 9,
      active_accounts: 4,
      statuses: 31,
      open_reports: 3,
      resolved_reports: 7,
      unassigned_reports: 2,
      pending_accounts: 1,
      silences: 0,
      suspensions: 1,
      domain_blocks: 2,
      email_domain_blocks: 0,
      staff: 1,
      ...overview,
    }),
  )
  // Registered after `stubApi`, so this answer — with its role — wins over the default.
  await page.route(/\/api\/v1\/nodus\/account\/role$/, (route) =>
    json(route, { role, scopes: scopesOf(role) }),
  )
  // The re-queue endpoint: each request adds one scan per subject, and the dashboard
  // reads its counters again afterwards, so this count has to move for the test to see it.
  let queuedScans = 0
  await page.route(/\/api\/v1\/admin\/scans$/, (route) => {
    queuedScans += 1
    return json(route, { queued: 1 })
  })
  await page.route(/\/api\/v1\/admin\/meta$/, (route) =>
    json(route, {
      account,
      staff: role !== 'none',
      role,
      scopes,
      counts: { open_reports: 3, unassigned_reports: 2, pending_accounts: 1, open_flags: 1 },
      domain: HOST,
      base_url: `https://${HOST}`,
      version: '4.3.0',
      scanning: { scanners: 1, enabled: 1, rules: 2, assets: 0, queued: queuedScans },
      severities: [],
      ...meta,
    }),
  )
  await page.route(/\/api\/v1\/admin\/audit/, (route) => json(route, audit))
  // The export is a file, not JSON, and the route above matches `audit.csv` too — so this
  // one comes later, which is the one Playwright consults first. The bytes echo the query
  // the console asked with, so a test can read the filter off the file it was given.
  await page.route(/\/api\/v1\/admin\/audit\.csv/, (route) => {
    record(route)
    const filter = new URL(route.request().url()).searchParams
    const row = ['action', 'target_type', 'actor_id']
      .map((key) => filter.get(key) ?? '')
      .join(',')
    return route.fulfill({
      status: 200,
      headers: { ...CORS, 'content-type': 'text/csv' },
      body: `action,target_type,actor_id\n${row}\n`,
    })
  })
  // The batch endpoints answer per id, and they are registered after the single-row routes
  // above because those match any one trailing segment — `/accounts/bulk` included.
  const bulk = (route) => {
    record(route)
    const body = JSON.parse(route.request().postData() ?? '{}')
    const ids = body.ids ?? []
    const results = ids.map((id) =>
      bulkRefusals[id]
        ? { id, ok: false, error: bulkRefusals[id] }
        : { id, ok: true },
    )
    return route.fulfill({
      status: 200,
      headers: CORS,
      json: { results, applied: results.filter((item) => item.ok).length },
    })
  }
  await page.route(/\/api\/v1\/admin\/accounts\/bulk$/, bulk)
  await page.route(/\/api\/v1\/admin\/reports\/bulk$/, bulk)
  await page.route(/\/api\/v1\/admin\/federation\/forwarded(\?|$)/, (route) => {
    const state = new URL(route.request().url()).searchParams.get('state')
    return json(route, state ? sentForwards.filter((row) => row.state === state) : sentForwards)
  })
  await page.route(/\/api\/v1\/admin\/reports\/[^/]+\/forward$/, (route) => {
    const reportId = new URL(route.request().url()).pathname.split('/').at(-2)
    const existing = sentForwards.find((row) => row.report_id === reportId)
    if (route.request().method() === 'GET') {
      if (!existing) {
        record(route)
        return route.fulfill({
          status: 404,
          headers: CORS,
          json: { error: { message: 'that report was never forwarded' } },
        })
      }
      return json(route, existing)
    }
    if (forwardRefusal) {
      record(route)
      return route.fulfill({
        status: forwardRefusal.status ?? 422,
        headers: CORS,
        json: { error: { message: forwardRefusal.message } },
      })
    }
    if (existing) return json(route, existing)
    const row = forwardRow(reportId)
    sentForwards = [row, ...sentForwards]
    record(route)
    return route.fulfill({ status: 202, headers: CORS, json: row })
  })
  return calls
}

/** One notification, shaped like the wire's. */
/**
 * Waits until the client tree is live.
 *
 * The server paints the rail and the page, so a link is clickable before any wasm runs;
 * clicking then is a full document load. Waiting for `window.__nodus` is waiting for the
 * client, which is what these tests are about.
 */
export async function hydrated(page) {
  await page.waitForFunction(() => window.__nodus === true, undefined, { timeout: 15000 })
}

export function notification({ id = '5001', type = 'favourite' } = {}) {
  return {
    id,
    type,
    created_at: '2024-01-01T00:00:00.000Z',
    account,
    status: status(),
  }
}

/**
 * Records every chart specification the page hands to the Chart.js wrapper.
 *
 * The wrapper reads properties (`labels`, `series`), so handing it JSON text draws an empty
 * axis and still counts as a canvas — the dashboard shipped that way once.
 */
export async function captureChartSpecs(page) {
  await page.addInitScript(() => {
    window.__chartSpecs = []
    let wrapper
    Object.defineProperty(window, 'nodusCharts', {
      configurable: true,
      get: () => wrapper,
      set: (value) => {
        wrapper = {
          mount: (element, spec) => {
            window.__chartSpecs.push(spec)
            return value.mount(element, spec)
          },
        }
      },
    })
  })
}

/** The chart specifications recorded so far, newest last. */
export function chartSpecs(page) {
  return page.evaluate(() => window.__chartSpecs || [])
}
