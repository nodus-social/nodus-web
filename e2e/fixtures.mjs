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
]

/** The scopes a role carries, as the two role answers send them. */
export function scopesOf(role) {
  return role === 'none' ? [] : ['admin:read', 'admin:write']
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
    remoteBlocks = [],
    audit = [],
  } = {},
) {
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
  await page.route(/\/api\/v1\/admin\/topology\/[^/]+\/trigger$/, (route) =>
    json(route, { accepted: true, detail: 'a pass is queued' }),
  )
  await page.route(/\/api\/v1\/admin\/topology$/, (route) => json(route, { version: '0.1.0', nodes }))
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
  await page.route(/\/api\/v1\/admin\/flags\?/, (route) => json(route, flags))
  await page.route(/\/api\/v1\/admin\/flags\/[^/]+\/resolve$/, (route) => json(route, flags[0] ?? scanFlag()))
  await page.route(/\/api\/v1\/admin\/domain_blocks/, (route) => json(route, blocks))
  await page.route(/\/api\/v1\/admin\/domain_blocks\/import$/, (route) => json(route, { blocked: 2, skipped: ['not a domain'] }))
  await page.route(/\/api\/v1\/admin\/domain_allows/, (route) => json(route, []))
  await page.route(/\/api\/v1\/admin\/federation\/remote-blocks/, (route) => {
    if (route.request().method() === 'DELETE') {
      record(route)
      return route.fulfill({ status: 204, headers: CORS })
    }
    return json(route, { blocks: remoteBlocks })
  })
  await page.route(/\/api\/v1\/admin\/email_domain_blocks/, (route) => json(route, mail))
  await page.route(/\/api\/v1\/admin\/warning_presets/, (route) => json(route, presets))
  await page.route(/\/api\/v1\/admin\/alerts/, (route) => json(route, alerts))
  await page.route(/\/api\/v1\/admin\/measures/, (route) => json(route, measures))
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
      scanning: { scanners: 1, enabled: 1, rules: 2, assets: 0, queued: 0 },
      severities: [],
      ...meta,
    }),
  )
  await page.route(/\/api\/v1\/admin\/audit/, (route) => json(route, audit))
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
