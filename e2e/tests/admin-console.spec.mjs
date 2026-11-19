//! The moderator console: the screens behind a grant, and what they send.
//!
//! Every test here boots with a grant and asks the stub what the console wrote. A panel
//! that renders a button but never sends its request looks identical in a screenshot, so
//! the assertions are on the recorded calls wherever the screen is meant to write.

import { expect, test } from '@playwright/test'

import {
  ACCOUNT_ID,
  FEATURES,
  adminAccount,
  adminApi,
  blockSubscription,
  captureChartSpecs,
  chartSpecs,
  delivery,
  deliveryCounts,
  escalationPolicy,
  granted,
  hydrated,
  moderationRule,
  pausedDomain,
  performanceStats,
  remoteBlock,
  report,
  scanFlag,
  scanner,
  signedIn,
  stubApi,
} from '../fixtures.mjs'

/** A week of measures, one row per key, the way the API answers. */
function measures(days = 7) {
  const dates = []
  for (let day = 0; day < days; day += 1) {
    dates.push(new Date(Date.UTC(2026, 10, 1 + day)).toISOString().slice(0, 10))
  }
  return [
    'new_users',
    'active_user_count',
    'interactions',
    'opened_reports',
    'resolved_reports',
    'open_reports',
  ].map((key) => ({
    key,
    total: String(days),
    previous_total: '1',
    data: dates.map((date, index) => ({ date, value: String(index % 3) })),
  }))
}

/**
 * Open one screen once the client tree is live.
 *
 * A click on a server-painted button before hydration reaches no listener and looks
 * exactly like a screen that does nothing.
 */
async function open(page, path) {
  await page.goto(path)
  await hydrated(page)
}

/** Boot the console signed in, with a grant, and with every admin endpoint stubbed. */
/** The request the console sent, once it has actually sent it. */
async function sent(calls, match) {
  await expect.poll(() => calls.filter(match).length, { timeout: 5000 }).toBeGreaterThan(0)
  return calls.filter(match).at(-1)
}

async function consoleAs(page, options = {}) {
  await captureChartSpecs(page)
  await stubApi(page)
  const calls = await adminApi(page, options)
  await signedIn(page)
  await granted(page)
  return calls
}

test('the dashboard counts what the server reports', async ({ page }) => {
  await consoleAs(page, { measures: measures() })
  await open(page, '/admin/overview')

  await expect(page.getByText('Accounts', { exact: true })).toBeVisible()
  await expect(page.getByText('12', { exact: true }).first()).toBeVisible()
  await expect(page.getByText('Open reports', { exact: true })).toBeVisible()
  await expect(page.getByText('Last seven days')).toBeVisible()
  // The server paints the signed-out title; the client names the page it actually shows.
  await expect(page).toHaveTitle(/^Administration \u00b7 /)
  // The graphs are canvases: a spec that never reached Chart.js would leave the panel
  // showing its empty-state caption instead.
  await expect(page.locator('canvas')).toHaveCount(6)

  // A canvas is not a graph. The wrapper reads properties off the spec, so a spec sent as
  // text draws an empty axis and still counts as a canvas — which is how an empty
  // dashboard shipped green. Assert the data, and the two series of the report chart.
  const specs = await chartSpecs(page)
  expect(specs).toHaveLength(6)
  for (const spec of specs) {
    expect(spec.labels.length).toBeGreaterThan(0)
    expect(spec.series.some((series) => series.values.length > 0)).toBe(true)
  }
  const reports = specs.find((spec) =>
    spec.series.some((series) => series.label === 'Opened'),
  )
  expect(reports.series.map((series) => series.label)).toEqual(['Opened', 'Resolved'])

  // The performance panel paints its counts as numbers, so the window can be read without
  // hovering a chart: the median is a sentence-or-duration beside its label.
  await expect(page.getByText('Median time to decision')).toBeVisible()
  await expect(page.getByText('1h 30m')).toBeVisible()
  await expect(page.getByText('ada@peer.example')).toBeVisible()
})

test('the performance panel asks for the window that was switched to', async ({ page }) => {
  const calls = await consoleAs(page, { stats: performanceStats() })
  await open(page, '/admin/overview')

  await expect(page.getByText('Moderation performance')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Last 7 days' })).toHaveAttribute('aria-pressed', 'true')

  await page.getByRole('button', { name: 'Last 30 days' }).click()
  const asked = await sent(calls, (call) => call.url.includes('/api/v1/admin/stats?range=30d'))
  expect(asked.method).toBe('GET')
  await expect(page.getByRole('button', { name: 'Last 30 days' })).toHaveAttribute('aria-pressed', 'true')
  await expect(page.getByRole('button', { name: 'Last 7 days' })).toHaveAttribute('aria-pressed', 'false')
})

test('the console asks the instance once, however often it redraws', async ({ page }) => {
  const calls = await consoleAs(page, { reports: [report()] })
  await open(page, '/admin/overview')
  await expect(page.getByText('Accounts', { exact: true })).toBeVisible()

  // The rail and the gate both ask "what is this account's standing?" and both re-run on
  // every store notification, which the socket and the sync loop produce constantly. The
  // answer is the instance's staff metadata, cached per `(host, account, grant)`: each ask
  // used to be its own request, and a refusal of an ungranted token is recorded by the
  // server as a failed authentication, so the alert list filled with the client's polling.
  await page.waitForTimeout(2500)
  const probes = calls.filter((call) => call.url.includes('/api/v1/admin/meta'))
  expect(probes).toHaveLength(1)
})

test('a report is resolved with the note the moderator wrote', async ({ page }) => {
  const calls = await consoleAs(page, { reports: [report()] })
  await open(page, '/moderation/reports')

  await page.getByRole('button', { name: /reported by/ }).first().click()
  await page
    .getByPlaceholder('What was done, for the record (optional)')
    .fill('Warned and removed the post.')
  await page.getByRole('button', { name: 'Resolve', exact: true }).click()

  const resolve = await sent(calls, (call) => call.url.endsWith('/api/v1/admin/reports/7001/resolve'))
  expect(resolve.method).toBe('POST')
  expect(JSON.parse(resolve.body)).toEqual({ note: 'Warned and removed the post.' })
})

test('the queue filters by the category the moderator typed', async ({ page }) => {
  const calls = await consoleAs(page)
  await open(page, '/moderation/reports')

  await page.getByPlaceholder('Category').fill('spam')
  await page.getByRole('button', { name: 'Apply' }).click()

  await expect
    .poll(() => calls.filter((call) => call.url.includes('/admin/reports?')).length)
    .toBeGreaterThan(1)
  const last = calls.filter((call) => call.url.includes('/admin/reports?')).at(-1)
  expect(last.url).toContain('category=spam')
  expect(last.url).toContain('state=open')
})

test('account search asks the server, and the detail panel can act', async ({ page }) => {
  const calls = await consoleAs(page, {
    accounts: [adminAccount(), adminAccount({ id: '2', username: 'grace', email: 'grace@example.test' })],
  })
  await open(page, '/moderation/accounts')

  await page.getByPlaceholder('User name').fill('grace')
  await expect
    .poll(() =>
      calls.some((call) => call.url.includes('/admin/accounts?') && call.url.includes('username=grace')),
    )
    .toBe(true)

  await page.getByRole('button', { name: /grace/ }).first().click()
  await expect(page.getByRole('button', { name: 'Delete for good' })).toBeVisible()

  await page.getByPlaceholder('What should the next moderator know?').fill('Second warning.')
  await page.getByRole('button', { name: 'Silence', exact: true }).click()

  const action = await sent(calls, (call) => call.url.endsWith(`/api/v1/admin/accounts/${ACCOUNT_ID}/action`))
  expect(action.body).toContain('type=silence')
  expect(action.body).toContain('text=Second+warning.')
})

test('a scanner is added with the endpoint and secret the administrator typed', async ({ page }) => {
  const calls = await consoleAs(page)
  await open(page, '/admin/scanners')

  await page.getByPlaceholder('Name').fill('nsfw')
  await page.getByPlaceholder('https://scanner.example:8080').fill('https://scanner.example')
  await page.getByPlaceholder('Shared secret').fill('s3cret')
  await page.getByRole('button', { name: 'Add scanner' }).click()

  const create = await sent(calls, (call) => call.method === 'POST' && call.url.endsWith('/api/v1/admin/scanners'),)
  expect(JSON.parse(create.body)).toMatchObject({
    name: 'nsfw',
    kind: 'image',
    endpoint: 'https://scanner.example',
    secret: 's3cret',
  })
})

test('a scanner that is already registered lists its rules and can be paused', async ({ page }) => {
  const calls = await consoleAs(page, { scanners: [scanner()] })
  await open(page, '/admin/scanners')

  await expect(page.getByText('nsfw', { exact: false }).first()).toBeVisible()
  await expect(page.getByText('Unruled labels on this scanner: nsfw, gore')).toBeVisible()

  await page.getByRole('button', { name: 'Disable' }).click()
  const update = await sent(calls, (call) => call.method === 'PUT' && call.url.endsWith('/api/v1/admin/scanners/9001'),)
  expect(JSON.parse(update.body)).toMatchObject({ enabled: false })
})

test('a flag is dismissed from the review queue', async ({ page }) => {
  const calls = await consoleAs(page, { flags: [scanFlag()] })
  await open(page, '/moderation/scans')

  await expect(page.getByText('nsfw scored 0.82')).toBeVisible()
  await page.getByRole('button', { name: 'Nothing wrong' }).click()

  const resolve = await sent(calls, (call) => call.url.endsWith('/api/v1/admin/flags/8001/resolve'))
  expect(JSON.parse(resolve.body)).toEqual({ state: 'dismissed' })
})

test('a pasted list of domains is blocked in one request', async ({ page }) => {
  const calls = await consoleAs(page)
  await open(page, '/moderation/federation')

  // Inserted as one paste rather than typed key by key: a block list arrives from a
  // file, and the box has to keep every line of it.
  const list = page.getByPlaceholder('one.domain')
  await list.click()
  await page.keyboard.insertText('# a comment\nbad.example\nnot a domain\nworse.example')
  await expect(list).toHaveValue(/worse\.example/)
  await page.getByRole('button', { name: 'Block these' }).click()

  const importCall = await sent(calls, (call) => call.url.endsWith('/api/v1/admin/domain_blocks/import'))
  expect(JSON.parse(importCall.body).domains).toEqual(['bad.example', 'worse.example'])
  await expect(page.getByText('2 blocked; 1 line(s) were not a domain.')).toBeVisible()
})

test('a mail domain is blocked from the administration screen', async ({ page }) => {
  const calls = await consoleAs(page)
  await open(page, '/admin/email')

  await page.getByPlaceholder('mail.example').fill('mail.example')
  await page.getByRole('button', { name: 'Block' }).click()

  const create = await sent(calls, (call) => call.method === 'POST' && call.url.endsWith('/api/v1/admin/email_domain_blocks'),)
  expect(create.body).toContain('domain=mail.example')
})

test('a peer that blocks an account is listed and can be cleared', async ({ page }) => {
  const calls = await consoleAs(page, { remoteBlocks: [remoteBlock()] })
  await open(page, '/admin/domains')

  await expect(page.getByText('Remote blocks')).toBeVisible()
  await expect(page.getByText('peer.example', { exact: true })).toBeVisible()
  await expect(page.getByText('https://peer.example/users/x')).toBeVisible()

  // The clear asks first: the record is the only sign the peer asked to be skipped.
  await page.getByRole('button', { name: 'Clear', exact: true }).click()
  await expect(page.getByText('https://peer.example/users/x')).toBeVisible()
  await page.getByRole('button', { name: 'Clear block' }).click()

  const cleared = await sent(calls, (call) => call.method === 'DELETE' && call.url.endsWith('/api/v1/admin/federation/remote-blocks'))
  expect(JSON.parse(cleared.body)).toEqual({ host: 'peer.example' })
  await expect(page.getByText('No peer has blocked us.')).toBeVisible()
})

test('the block list is exported as the bytes the instance sent', async ({ page }) => {
  await consoleAs(page)
  await open(page, '/admin/domains')

  const download = page.waitForEvent('download')
  await page.getByRole('button', { name: 'Export CSV' }).click()
  const file = await download
  expect(file.suggestedFilename()).toBe('domain-blocks.csv')
  const stream = await file.createReadStream()
  const chunks = []
  for await (const chunk of stream) chunks.push(chunk)
  expect(Buffer.concat(chunks).toString()).toContain('domain,severity')
})

test('an import is counted before it is applied, and a bad line is named', async ({ page }) => {
  const calls = await consoleAs(page, {
    blockImport: {
      parsed: 3,
      new: 2,
      updated: 1,
      unchanged: 0,
      invalid: [{ line: 4, reason: 'not a domain' }],
    },
  })
  await open(page, '/admin/domains')

  await page.getByLabel('Block list', { exact: true }).fill('domain,severity\nspam.example,suspend\nbad')
  await page.getByRole('button', { name: 'Check list' }).click()

  await expect(page.getByText('Parsed', { exact: true })).toBeVisible()
  await expect(page.getByText('Nothing has been written yet.')).toBeVisible()
  await expect(page.getByText('Line 4: not a domain')).toBeVisible()
  // The preview is a dry run: the first request must not have written anything.
  const checks = calls.filter((call) => call.url.includes('/admin/domain-blocks/import'))
  expect(checks).toHaveLength(1)
  expect(checks[0].url).toContain('dry_run=true')

  await page.getByRole('button', { name: 'Apply', exact: true }).click()
  const applied = await sent(calls, (call) => call.url.includes('/admin/domain-blocks/import?dry_run=false'))
  expect(applied.method).toBe('POST')
  await expect(page.getByText('The list was applied.')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Apply', exact: true })).toHaveCount(0)
})

test('a followed list shows why it failed and can be synced now', async ({ page }) => {
  const calls = await consoleAs(page, {
    blockSubscriptions: [blockSubscription({ last_error: 'the list timed out' })],
  })
  await open(page, '/admin/domains')

  await expect(page.getByText('Subscribed lists')).toBeVisible()
  const broken = page.getByText('the list timed out')
  await expect(broken).toBeVisible()
  await expect(broken).toHaveClass(/text-error/)
  // A list that failed is still listed: its row is how the moderator sees the failure.
  await expect(page.getByText('https://lists.example/blocklist.csv')).toBeVisible()

  await page.getByRole('button', { name: 'Sync now' }).click()
  const sync = await sent(calls, (call) => call.url.endsWith('/api/v1/admin/domain-blocks/subscriptions/sub-1/sync'))
  expect(sync.method).toBe('POST')

  await page.getByLabel('List address').fill('https://other.example/list.json')
  await page.getByLabel('List format').selectOption('json')
  await page.getByRole('button', { name: 'Follow list' }).click()
  const follow = await sent(calls, (call) => call.method === 'POST' && call.url.endsWith('/api/v1/admin/domain-blocks/subscriptions'))
  expect(JSON.parse(follow.body)).toEqual({ url: 'https://other.example/list.json', format: 'json' })
})

test('a canned reply is saved with its title and text', async ({ page }) => {
  const calls = await consoleAs(page)
  await open(page, '/admin/replies')

  await page.getByPlaceholder('Title').fill('First warning')
  await page.getByPlaceholder('What the report is about, and what happens next.').fill('Be kind.')
  await page.getByRole('button', { name: 'Save' }).click()

  const create = await sent(calls, (call) => call.method === 'POST' && call.url.endsWith('/api/v1/admin/warning_presets'),)
  expect(JSON.parse(create.body)).toEqual({ title: 'First warning', text: 'Be kind.' })
})

test('the accounts screen browses the directory and pages through it', async ({ page }) => {
  const many = Array.from({ length: 20 }, (_, index) =>
    adminAccount({ id: String(index + 1), username: `ada${index}` }),
  )
  const calls = await consoleAs(page, { accounts: many })
  await open(page, '/moderation/accounts')

  // The first read is the directory itself, not the answer to a search nobody typed.
  const first = await sent(calls, (call) => call.url.includes('/api/v1/admin/accounts?'))
  expect(new URL(first.url).searchParams.get('limit')).toBe('20')
  await expect(page.getByRole('button', { name: 'Load more accounts' })).toBeVisible()

  // A domain is a filter like any other, and it is a first page again, not a deeper one.
  await page.getByPlaceholder('Domain (remote accounts)').fill('example.test')
  const narrowed = await sent(calls, (call) => call.url.includes('by_domain=example.test'))
  expect(new URL(narrowed.url).searchParams.has('offset')).toBe(false)

  await page.getByRole('button', { name: 'Load more accounts' }).click()
  await sent(calls, (call) => new URL(call.url).searchParams.get('offset') === '20')

  // Clearing every filter is the way back to the whole directory.
  const unfiltered = (call) => {
    const params = new URL(call.url).searchParams
    return call.url.includes('/api/v1/admin/accounts?') && !params.has('by_domain')
  }
  const before = calls.filter(unfiltered).length
  await page.getByRole('button', { name: 'Browse all accounts' }).click()
  await expect.poll(() => calls.filter(unfiltered).length).toBeGreaterThan(before)
})

test('the audit log is narrowed and paged by its filters', async ({ page }) => {
  const rows = Array.from({ length: 20 }, (_, index) => ({
    id: String(index + 1),
    actor_id: '7',
    action: 'resolve_report',
    target_type: 'report',
    target_id: '9',
    detail: {},
    created_at: '2026-11-01T10:00:00Z',
  }))
  const calls = await consoleAs(page, { audit: rows })
  await open(page, '/admin/audit')

  await expect(page.getByText('resolve_report').first()).toBeVisible()
  const first = await sent(calls, (call) => call.url.includes('/api/v1/admin/audit'))
  expect(new URL(first.url).searchParams.get('limit')).toBe('20')
  await expect(page.getByRole('button', { name: 'Load more' })).toBeVisible()

  await page.getByPlaceholder('Action, e.g. resolve_report').fill('suspend_account')
  const narrowed = await sent(calls, (call) => call.url.includes('action=suspend_account'))
  expect(new URL(narrowed.url).searchParams.has('offset')).toBe(false)

  await page.getByPlaceholder('Target type, e.g. report').fill('account')
  await sent(calls, (call) => call.url.includes('target_type=account'))

  await page.getByRole('button', { name: 'Load more' }).click()
  await sent(calls, (call) => new URL(call.url).searchParams.get('offset') === '20')
})

test('the audit log is exported as the file the filter names', async ({ page }) => {
  const rows = [
    {
      id: '1',
      actor_id: '7',
      action: 'resolve_report',
      target_type: 'report',
      target_id: '9',
      detail: {},
      created_at: '2026-11-01T10:00:00Z',
    },
  ]
  const calls = await consoleAs(page, { audit: rows })
  await open(page, '/admin/audit')

  await page.getByPlaceholder('Action, e.g. resolve_report').fill('resolve_report')
  await page.getByPlaceholder('Target type, e.g. report').fill('report')

  // The bytes are handed to the browser as a file rather than decoded into a model, so the
  // proof is the download itself: its name is the console's, its content from the API.
  const arrived = page.waitForEvent('download')
  await page.getByRole('button', { name: 'Export CSV' }).click()
  const download = await arrived
  expect(download.suggestedFilename()).toBe('audit.csv')

  const asked = await sent(calls, (call) => call.url.includes('/api/v1/admin/audit.csv'))
  const filter = new URL(asked.url).searchParams
  expect(filter.get('action')).toBe('resolve_report')
  expect(filter.get('target_type')).toBe('report')
  // An export is the whole result, so it asks for no page of it.
  expect(filter.has('limit')).toBe(false)
  expect(filter.has('offset')).toBe(false)

  const stream = await download.createReadStream()
  const chunks = []
  for await (const chunk of stream) chunks.push(chunk)
  expect(Buffer.concat(chunks).toString()).toBe(
    'action,target_type,actor_id\nresolve_report,report,\n',
  )
})

test('a domain block changes severity without losing what it carried', async ({ page }) => {
  const calls = await consoleAs(page, {
    blocks: [
      {
        id: '9',
        domain: 'example.test',
        created_at: '2026-11-01T00:00:00Z',
        severity: 'suspend',
        reject_media: true,
        reject_reports: false,
        obfuscate: true,
        public_comment: 'Handled by staff.',
      },
    ],
  })
  await open(page, '/moderation/federation')

  await expect(page.getByText('example.test')).toBeVisible()
  await expect(page.getByText('Handled by staff.')).toBeVisible()

  // The rewrite is the row, not a new block: the severity changes and every other field
  // — the comment above all — is sent back as it was.
  await page.getByLabel('Severity for example.test').selectOption('silence')

  const rewrite = await sent(
    calls,
    (call) => call.method === 'PUT' && call.url.endsWith('/api/v1/admin/domain_blocks/9'),
  )
  const form = new URLSearchParams(rewrite.body)
  expect(form.get('severity')).toBe('silence')
  expect(form.get('public_comment')).toBe('Handled by staff.')
  expect(form.get('reject_media')).toBe('true')
  expect(form.get('obfuscate')).toBe('true')
  expect(form.get('domain')).toBe('example.test')
})

test('a canned reply is edited in place instead of replaced', async ({ page }) => {
  const calls = await consoleAs(page, {
    presets: [
      { id: '11', title: 'First warning', text: 'Be kind.', created_at: '2026-11-01T00:00:00Z' },
    ],
  })
  await open(page, '/admin/replies')

  await page.getByRole('button', { name: 'Edit', exact: true }).click()
  await page.getByLabel('Edited preset title').fill('Second warning')
  await page.getByLabel('Edited preset text').fill('Be kind, please.')
  await page.getByRole('button', { name: 'Save changes' }).click()

  const rewrite = await sent(
    calls,
    (call) => call.method === 'PUT' && call.url.endsWith('/api/v1/admin/warning_presets/11'),
  )
  expect(JSON.parse(rewrite.body)).toEqual({ title: 'Second warning', text: 'Be kind, please.' })
})

test('an announcement is edited in place and keeps its schedule', async ({ page }) => {
  const calls = await consoleAs(page, {
    announcements: [
      {
        id: '12',
        content: 'Maintenance tonight.',
        published: true,
        all_day: true,
        published_at: '2026-11-01T00:00:00Z',
        starts_at: '2026-11-02',
        ends_at: '2026-11-03',
      },
    ],
  })
  await open(page, '/admin/announcements')

  await page.getByRole('button', { name: 'Edit', exact: true }).click()
  await page.getByLabel('Edited announcement').fill('Maintenance tomorrow.')
  await page.getByRole('button', { name: 'Save changes' }).click()

  const rewrite = await sent(
    calls,
    (call) => call.method === 'PUT' && call.url.endsWith('/api/v1/admin/announcements/12'),
  )
  const form = new URLSearchParams(rewrite.body)
  expect(form.get('content')).toBe('Maintenance tomorrow.')
  expect(form.get('published')).toBe('true')
  expect(form.get('all_day')).toBe('true')
  expect(form.get('starts_at')).toBe('2026-11-02')
  expect(form.get('ends_at')).toBe('2026-11-03')
})

test('a content set downloads as the bytes the scanner was given', async ({ page }) => {
  const calls = await consoleAs(page, {
    assets: [
      {
        id: '7001',
        name: 'hashes.txt',
        kind: 'hashes',
        digest: 'abc123def4567890',
        size: 8,
        content_type: 'text/plain',
        created_at: '2026-11-01T00:00:00Z',
      },
    ],
  })
  await open(page, '/admin/scanners')

  // The bytes are handed to the browser as a file rather than decoded into a model, so the
  // proof is the download itself: its name comes from the asset, its content from the API.
  const arrived = page.waitForEvent('download')
  await page.getByRole('button', { name: 'Download' }).click()
  const download = await arrived
  expect(download.suggestedFilename()).toBe('hashes.txt')

  await sent(calls, (call) => call.url.endsWith('/api/v1/admin/scanner_assets/7001/download'))
  const stream = await download.createReadStream()
  const chunks = []
  for await (const chunk of stream) chunks.push(chunk)
  expect(Buffer.concat(chunks).toString()).toBe('a,b\n1,2\n')
})

test('the dashboard hands the review queue to the scanners again', async ({ page }) => {
  const calls = await consoleAs(page, {
    scanners: [scanner(), scanner({ id: '9002', name: 'topics', kind: 'text' })],
    flags: [
      scanFlag(),
      scanFlag({ id: '8002', scan_id: '8003', status_id: '101', created_at: '2026-11-07T09:00:00.000Z' }),
    ],
  })
  await open(page, '/admin/overview')

  // The dashboard has no subject of its own: what it hands over is the review queue.
  const queued = page.getByText('Queued scans', { exact: true }).locator('..')
  await expect(queued.locator('p').last()).toHaveText('0')

  await page.getByRole('button', { name: 'Scan now' }).click()

  // The endpoint takes one kind at a time, so two enabled kinds are two requests per post,
  // and the two flags on the queue are the two posts: four, and no post twice.
  await expect
    .poll(() => calls.filter((call) => call.url.endsWith('/api/v1/admin/scans')).length)
    .toBe(4)
  const sentScans = calls
    .filter((call) => call.url.endsWith('/api/v1/admin/scans'))
    .map((call) => JSON.parse(call.body))
  expect(sentScans.map((body) => `${body.subject_type}:${body.subject_id}`).sort()).toEqual([
    'status:100',
    'status:100',
    'status:101',
    'status:101',
  ])
  expect(sentScans.map((body) => body.kind).sort()).toEqual(['image', 'image', 'text', 'text'])

  // The counters the panel paints are the server's, so they move only after a fresh read.
  await expect(queued.locator('p').last()).toHaveText('4')
})

test('a flagged post is handed to the scanners again', async ({ page }) => {
  const calls = await consoleAs(page, { flags: [scanFlag()], scanners: [scanner()] })
  await open(page, '/moderation/scans')

  await page.getByRole('button', { name: 'Scan now' }).click()

  const queue = await sent(calls, (call) => call.url.endsWith('/api/v1/admin/scans'))
  expect(JSON.parse(queue.body)).toMatchObject({
    subject_type: 'status',
    subject_id: '100',
    kind: 'image',
  })
  await expect(page.getByText('Queued 1 scan for this post.')).toBeVisible()
})

test('a batch of accounts is applied one by one, and a refusal stays on its own row', async ({
  page,
}) => {
  const many = [
    adminAccount({ id: '1', username: 'ada' }),
    adminAccount({ id: '2', username: 'grace', admin: true }),
  ]
  const calls = await consoleAs(page, {
    accounts: many,
    bulkRefusals: { 2: 'demote an administrator before taking action against them' },
  })
  await open(page, '/moderation/accounts')

  const boxes = page.getByRole('checkbox', { name: /^Select / })
  await boxes.nth(0).check()
  await boxes.nth(1).check()
  await expect(page.getByText('2 selected')).toBeVisible()

  await page.getByLabel('Bulk action').selectOption('silence')
  await page.getByPlaceholder('Reason (recorded in the log)').fill('Repeated spam')
  await page.getByRole('button', { name: 'Apply to selection' }).click()

  const batch = await sent(calls, (call) =>
    call.url.endsWith('/api/v1/admin/accounts/bulk'),
  )
  expect(JSON.parse(batch.body)).toEqual({
    ids: ['1', '2'],
    action: 'silence',
    reason: 'Repeated spam',
    notify: false,
  })

  // One request, one line for the batch, and the sentence the instance gave for id 2 on
  // the row that id names — not one toast for a partially applied batch.
  await expect(page.getByRole('status')).toContainText('1 applied, 1 refused')
  await expect(
    page.getByText('demote an administrator before taking action against them'),
  ).toBeVisible()

  // The ticks did their work: the bar is gone and the rows are unticked, so the next
  // batch is a fresh decision rather than a leftover.
  await expect(page.getByText('2 selected')).toHaveCount(0)
  await expect(boxes.nth(0)).not.toBeChecked()
  await expect(boxes.nth(1)).not.toBeChecked()
})

test('a batch of reports is closed with one note, and a refusal stays on its own row', async ({
  page,
}) => {
  const queue = [report({ id: '7001' }), report({ id: '7002' })]
  const calls = await consoleAs(page, {
    reports: queue,
    bulkRefusals: { 7002: 'that report does not exist' },
  })
  await open(page, '/moderation/reports')

  const boxes = page.getByRole('checkbox', { name: /^Select the report/ })
  await boxes.nth(0).check()
  await boxes.nth(1).check()

  await page.getByLabel('Bulk action').selectOption('resolve')
  await page.getByPlaceholder('Note (Resolve)').fill('Closed in one pass.')
  await page.getByRole('button', { name: 'Apply to selection' }).click()

  const batch = await sent(calls, (call) => call.url.endsWith('/api/v1/admin/reports/bulk'))
  expect(JSON.parse(batch.body)).toEqual({
    ids: ['7001', '7002'],
    action: 'resolve',
    assignee_id: null,
    note: 'Closed in one pass.',
  })

  await expect(page.getByRole('status')).toContainText('1 applied, 1 refused')
  await expect(page.getByText('that report does not exist')).toBeVisible()
  await expect(page.getByText('2 selected')).toHaveCount(0)
  await expect(boxes.nth(0)).not.toBeChecked()
  await expect(boxes.nth(1)).not.toBeChecked()
})

test('a grant without admin:write is shown no bulk bar and no ticks', async ({ page }) => {
  await consoleAs(page, {
    accounts: [adminAccount({ id: '1', username: 'ada' })],
    scopes: ['read', 'write', 'follow', 'admin:read'],
  })
  await open(page, '/moderation/accounts')

  // The console says why, and then shows no way to ask: a bar whose button is disabled is
  // still a promise the console cannot keep.
  await expect(page.getByText(/does not include admin:write/)).toBeVisible()
  await expect(page.getByRole('group', { name: 'Bulk account action' })).toHaveCount(0)
  await expect(page.getByRole('checkbox', { name: /^Select / })).toHaveCount(0)
})

test('the rules screen lists a pattern with its scope, action and hit count', async ({ page }) => {
  await consoleAs(page, { rules: [moderationRule()] })
  await open(page, '/moderation/rules')

  await expect(page.getByRole('heading', { name: 'Moderation rules' })).toBeVisible()
  await expect(page.getByTestId('rule-row')).toHaveCount(1)
  await expect(page.getByTestId('rule-pattern')).toHaveText('buy cheap')
  const summary = page.getByTestId('rule-summary')
  await expect(summary).toContainText('keyword')
  await expect(summary).toContainText('inbound')
  await expect(summary).toContainText('flag')
  await expect(summary).toContainText('4 hits')
})

test('an instance with no rules says so instead of showing an empty list', async ({ page }) => {
  await consoleAs(page, { rules: [] })
  await open(page, '/moderation/rules')

  await expect(page.getByTestId('rules-empty')).toHaveText('No rule is active.')
  await expect(page.getByTestId('rule-row')).toHaveCount(0)
})

test('a rule is created with the pattern, kind, scope and action the administrator chose', async ({
  page,
}) => {
  const calls = await consoleAs(page, { rules: [] })
  await open(page, '/moderation/rules')
  await expect(page.getByTestId('rules-empty')).toBeVisible()

  await page.getByLabel('Rule name').fill('Crypto pitches')
  await page.getByLabel('Rule pattern').fill('(?:free|cheap) crypto')
  await page.getByLabel('Pattern kind').selectOption('regex')
  await page.getByLabel('Scope').selectOption('both')
  await page.getByLabel('Action').selectOption('silence')
  await page.getByRole('button', { name: 'Create rule' }).click()

  const create = await sent(
    calls,
    (call) => call.method === 'POST' && call.url.endsWith('/api/v1/admin/moderation-rules'),
  )
  expect(JSON.parse(create.body)).toEqual({
    name: 'Crypto pitches',
    pattern: '(?:free|cheap) crypto',
    pattern_kind: 'regex',
    scope: 'both',
    action: 'silence',
    enabled: true,
  })
  // The list is read again after a write: the hit count and the id are the instance's.
  await expect.poll(() => calls.filter((call) => call.method === 'GET').length).toBeGreaterThan(1)
  await expect(page.getByTestId('rule-row')).toHaveCount(1)
})

test('a rule is edited in place and deleted only after the confirmation', async ({ page }) => {
  const calls = await consoleAs(page, { rules: [moderationRule()] })
  await open(page, '/moderation/rules')

  const row = page.getByTestId('rule-row')
  await row.getByRole('button', { name: 'Edit' }).click()
  await expect(row.getByLabel('Rule pattern')).toHaveValue('buy cheap')
  await row.getByLabel('Rule pattern').fill('buy\\s+cheap')
  await row.getByLabel('Pattern kind').selectOption('regex')
  await page.getByRole('button', { name: 'Save changes' }).click()

  const update = await sent(
    calls,
    (call) => call.method === 'PUT' && call.url.endsWith('/api/v1/admin/moderation-rules/7001'),
  )
  expect(JSON.parse(update.body)).toMatchObject({
    pattern: 'buy\\s+cheap',
    pattern_kind: 'regex',
  })

  await page.getByRole('button', { name: 'Delete' }).click()
  await expect(page.getByTestId('delete-confirm')).toBeVisible()
  // Merely opening the confirmation must not delete anything.
  expect(calls.filter((call) => call.method === 'DELETE')).toHaveLength(0)
  await page.getByRole('button', { name: 'Yes, delete' }).click()

  const removed = await sent(
    calls,
    (call) => call.method === 'DELETE' && call.url.endsWith('/api/v1/admin/moderation-rules/7001'),
  )
  expect(removed.body).toBeNull()
  await expect(page.getByTestId('rules-empty')).toBeVisible()
})

test('a dry run says what matched, and where, and says plainly when nothing did', async ({
  page,
}) => {
  const calls = await consoleAs(page, { rules: [moderationRule()] })
  await open(page, '/moderation/rules')

  await page.getByRole('button', { name: 'Test' }).click()
  const sample = page.getByLabel('Sample body')
  await sample.fill('they said buy cheap followers here')
  await page.getByRole('button', { name: 'Dry run' }).click()

  const answer = page.getByTestId('dry-run-answer')
  await expect(answer).toBeVisible()
  await expect(page.getByTestId('dry-run-verdict')).toHaveText('Matched')
  // The instance's own sentence names the rule, the kind and the bytes; the screen repeats
  // it rather than paraphrasing what the pattern did.
  await expect(answer).toContainText('rule "Spam words" (keyword) matched bytes 10..19')
  await expect(answer).toContainText('bytes 10..19: buy cheap')
  await expect(answer).toContainText('Scanned 34 bytes.')

  // A regex that matches nothing must read as a miss, with the reason, not as silence.
  const row = page.getByTestId('rule-row')
  await row.getByRole('button', { name: 'Edit' }).click()
  await row.getByLabel('Pattern kind').selectOption('regex')
  await row.getByLabel('Rule pattern').fill('\\bdeadbeef\\b')
  await page.getByRole('button', { name: 'Save changes' }).click()
  await expect(page.getByText('Rule saved.')).toBeVisible()

  // The dry-run panel stayed open across the save, so the sample is simply replaced.
  await expect(sample).toBeVisible()
  await sample.fill('nothing here matches that')
  await page.getByRole('button', { name: 'Dry run' }).click()
  await expect(page.getByTestId('dry-run-verdict')).toHaveText('No match')
  await expect(answer).toContainText('did not match the sample')

  const tried = calls.filter((call) => call.url.endsWith('/dry-run'))
  expect(tried).toHaveLength(2)
  expect(JSON.parse(tried.at(-1).body)).toEqual({ sample: 'nothing here matches that' })
})

test('a pattern that cannot compile is refused with the instance message', async ({ page }) => {
  await consoleAs(page, { rules: [] })
  await open(page, '/moderation/rules')

  await page.getByLabel('Rule name').fill('Broken')
  await page.getByLabel('Rule pattern').fill('(unclosed')
  await page.getByLabel('Pattern kind').selectOption('regex')
  await page.getByRole('button', { name: 'Create rule' }).click()

  const alert = page.getByRole('alert')
  await expect(alert).toBeVisible()
  await expect(alert).toContainText('the pattern is not a valid regex')
  await expect(page.getByTestId('rules-empty')).toBeVisible()
})

test('the escalation ladder reads rung by rung, and says its order out loud', async ({ page }) => {
  await consoleAs(page, { escalation: escalationPolicy() })
  await open(page, '/admin/escalation')

  await expect(page.getByRole('heading', { name: 'Escalation' })).toBeVisible()
  // The whole ladder in one sentence, before the rungs are read one at a time: strike 2
  // silences, strike 3 suspends, and the window the counts live in is part of it.
  await expect(page.getByTestId('escalation-sequence')).toHaveText(
    'Inside a 30-day window: flag at 1 strike, then silence at 2 strikes, then suspend at 3 strikes.',
  )
  await expect(page.getByTestId('escalation-step')).toHaveCount(3)
  await expect(page.getByTestId('escalation-ordinal')).toHaveText([
    '1 strike',
    '2 strikes',
    '3 strikes',
  ])
  await expect(page.getByTestId('escalation-step-line')).toHaveText([
    'Strike 1 flags the account for a moderator',
    'Strike 2 silences the account for 7 days',
    'Strike 3 suspends the account with no end date',
  ])
  // The rule above every rung, in words: an escalation adds, it never lifts.
  await expect(page.getByTestId('escalation-never-weakens')).toContainText(
    'An escalation never weakens a restriction already in force',
  )
})

test('saving the ladder sends the rungs in the order they are shown', async ({ page }) => {
  const calls = await consoleAs(page, { escalation: escalationPolicy() })
  await open(page, '/admin/escalation')

  await page.getByTestId('escalation-add').click()
  const rungs = page.getByTestId('escalation-step')
  await expect(rungs).toHaveCount(4)
  const added = rungs.nth(3)
  await added.getByTestId('escalation-action').selectOption('suspend')
  await added.getByTestId('escalation-duration').fill('30')
  await page.getByTestId('escalation-window').fill('14')
  await page.getByTestId('escalation-save').click()

  const written = await sent(
    calls,
    (call) => call.method === 'PUT' && call.url.endsWith('/api/v1/admin/escalation'),
  )
  // The order on screen is the order sent: a ladder the instance reorders is a ladder the
  // administrator did not author.
  expect(JSON.parse(written.body)).toEqual({
    enabled: true,
    window_days: 14,
    steps: [
      { strikes: 1, action: 'flag' },
      { strikes: 2, action: 'silence', duration_days: 7 },
      { strikes: 3, action: 'suspend' },
      { strikes: 4, action: 'suspend', duration_days: 30 },
    ],
  })
  await expect(page.getByRole('status')).toContainText('Saved')
})

test('a refused ladder is shown the instance sentence verbatim, beside the rung it names', async ({
  page,
}) => {
  const refusal = 'step 3: strike counts must ascend, 2 does not follow 4'
  await consoleAs(page, { escalationRefusal: refusal })
  await open(page, '/admin/escalation')

  const rungs = page.getByTestId('escalation-step')
  await rungs.nth(0).getByTestId('escalation-strikes').fill('4')
  await rungs.nth(2).getByTestId('escalation-strikes').fill('2')
  await page.getByTestId('escalation-save').click()

  // Verbatim: the sentence names the step, and is the only guidance the administrator gets.
  await expect(page.getByRole('alert')).toContainText(refusal)
  // The rung it names is ringed, and the refused draft is still there to be corrected.
  await expect(rungs.nth(2)).toHaveClass(/ring-error/)
  await expect(rungs.nth(0).getByTestId('escalation-strikes')).toHaveValue('4')
})

test('an instance whose features omit moderation serves no escalation panel', async ({ page }) => {
  const features = FEATURES.filter((name) => name !== 'moderation')
  await stubApi(page, { features })
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/admin/escalation')
  await hydrated(page)

  await expect(page.getByRole('tab', { name: 'Escalation' })).toHaveCount(0)
  await expect(page.getByText(/does not serve the escalation panel/)).toBeVisible()
})

test('an account row says its strikes and what the next one costs', async ({ page }) => {
  await consoleAs(page, {
    accounts: [
      adminAccount({ strike_count: 3, next_escalation: { strikes: 4, action: 'suspend' } }),
    ],
  })
  await open(page, '/moderation/accounts')

  // The count and the next rung in one line, because a moderator deciding a case should not
  // have to join an audit log against a policy to know what the next report would cost.
  await expect(page.getByTestId('account-strikes')).toHaveText('3 strikes, next: suspend at 4')
})

test('an account whose payload carries no strike count shows no strike line', async ({ page }) => {
  await consoleAs(page, { accounts: [adminAccount()] })
  await open(page, '/moderation/accounts')

  // Absent, not "0 strikes": an older instance that does not count strikes must not be
  // described as counting none.
  await expect(page.getByTestId('account-strikes')).toHaveCount(0)
})

test('the delivery queue counts its states, and a refusal reads unlike an outage', async ({
  page,
}) => {
  const refused = delivery({
    id: 'd-1',
    host: 'refuser.example',
    state: 'gave_up',
    attempts: 1,
    last_status_code: 401,
    last_error: 'the peer answered 401',
  })
  const down = delivery({
    id: 'd-2',
    host: 'down.example',
    state: 'gave_up',
    attempts: 6,
    last_status_code: 503,
    last_error: 'the peer answered 503',
  })
  await consoleAs(page, {
    deliveries: [refused, down],
    // The counts are the whole queue's, not this page's: three delivered rows are not on
    // screen and still make the All filter say eight.
    deliveryCounts: deliveryCounts({ pending: 2, failed: 1, gave_up: 2, delivered: 3 }),
  })
  await open(page, '/admin/federation')

  await expect(page.getByRole('heading', { name: 'Federation delivery' })).toBeVisible()
  await expect(page.getByTestId('delivery-filter-all')).toHaveText('All (8)')
  await expect(page.getByTestId('delivery-filter-pending')).toHaveText('Pending (2)')
  await expect(page.getByTestId('delivery-filter-failed')).toHaveText('Failed (1)')
  await expect(page.getByTestId('delivery-filter-gave_up')).toHaveText('Gave up (2)')
  await expect(page.getByTestId('delivery-filter-delivered')).toHaveText('Delivered (3)')
  await expect(page.getByTestId('delivery-row')).toHaveCount(2)

  // The two ways a delivery stops are told apart in words, because one can be retried and
  // the other has to wait for the peer's key to agree with ours.
  const reasons = page.getByTestId('delivery-reason')
  await expect(reasons.nth(0)).toContainText('answered 401')
  await expect(reasons.nth(0)).toContainText('will not accept our signature')
  await expect(reasons.nth(1)).toContainText('was down')
  await expect(reasons.nth(1)).toContainText('answered 503')
})

test('the state and host filters are the request, and leave the first page', async ({ page }) => {
  const calls = await consoleAs(page, { deliveries: [delivery({ state: 'failed' })] })
  await open(page, '/admin/federation')

  await page.getByTestId('delivery-filter-failed').click()
  await sent(
    calls,
    (call) => call.method === 'GET' && call.url.includes('/deliveries?') && call.url.includes('state=failed'),
  )

  await page.getByTestId('delivery-host').fill('peer.example')
  await page.getByTestId('delivery-host').press('Enter')
  await sent(
    calls,
    (call) => call.method === 'GET' && call.url.includes('state=failed') && call.url.includes('host=peer.example'),
  )
})

test('retrying one delivery asks a second time, and sends that row', async ({ page }) => {
  const calls = await consoleAs(page, { deliveries: [delivery({ id: 'd-1', state: 'failed' })] })
  await open(page, '/admin/federation')

  await page.getByTestId('delivery-retry').click()
  await expect(page.getByTestId('delivery-retry-confirm')).toBeVisible()
  // The first press only asks: no request leaves until it is answered.
  expect(calls.filter((call) => call.url.endsWith('/retry'))).toHaveLength(0)

  await page.getByTestId('delivery-retry-confirm').click()
  const written = await sent(
    calls,
    (call) => call.method === 'POST' && call.url.endsWith('/deliveries/d-1/retry'),
  )
  expect(written.url).toContain('/api/v1/admin/federation/deliveries/d-1/retry')
  await expect(page.getByTestId('delivery-notice')).toContainText('Retry queued')
})

test('retrying everything failed says how many the server requeued', async ({ page }) => {
  await consoleAs(page, {
    deliveries: [
      delivery({ id: 'd-1', state: 'gave_up' }),
      delivery({ id: 'd-2', state: 'failed' }),
      delivery({ id: 'd-3', state: 'delivered', delivered_at: '2026-11-19T06:30:00Z' }),
    ],
  })
  await open(page, '/admin/federation')

  await page.getByTestId('delivery-retry-failed').click()
  // The number is the server's: "done" would not say whether anything was waiting.
  await expect(page.getByTestId('delivery-notice')).toHaveText(
    'Requeued 2 failed deliveries. They will be attempted again now.',
  )
})

test('an empty queue says nothing is waiting', async ({ page }) => {
  await consoleAs(page, { deliveries: [] })
  await open(page, '/admin/federation')

  await expect(page.getByTestId('delivery-empty')).toHaveText('Nothing is waiting to be delivered.')
})

test('a paused domain says what a pause is, and who set it and until when', async ({ page }) => {
  await consoleAs(page, {
    domainPauses: [
      pausedDomain({ domain: 'quiet.example', reason: 'their queue is flooding us' }),
    ],
  })
  await open(page, '/admin/federation')

  await expect(page.getByRole('heading', { name: 'Paused domains' })).toBeVisible()
  // The difference from a block is in words, twice: once for the panel and once in the
  // row's standing. A pause lapses and is never announced; a block is permanent and told.
  const difference = page.getByTestId('pause-versus-block')
  await expect(difference).toContainText('temporary and unannounced')
  await expect(difference).toContainText('the peer is never told')
  await expect(difference).toContainText('permanent and announced')

  await expect(page.getByTestId('pause-row')).toHaveCount(1)
  await expect(page.getByTestId('pause-row-domain')).toHaveText('quiet.example')
  await expect(page.getByTestId('pause-state')).toHaveText('Paused')
  await expect(page.getByTestId('pause-standing')).toContainText('Paused until 2026-11-25')
  await expect(page.getByTestId('pause-standing')).toContainText('resume on their own')
  await expect(page.getByTestId('pause-reason-text')).toHaveText('their queue is flooding us')
  await expect(page.getByTestId('pause-attribution')).toHaveText(
    'Paused by ada on 2026-11-18',
  )
})

test('a lapsed pause reads as expired rather than as paused', async ({ page }) => {
  await consoleAs(page, {
    domainPauses: [
      pausedDomain({
        domain: 'old.example',
        expires_at: '2026-11-01T00:00:00Z',
        // The instance's own answer: it has already lapsed.
        expired: true,
      }),
    ],
  })
  await open(page, '/admin/federation')

  // The mistake this guards: a lapsed row that still reads as in force makes a list of
  // five look like five live pauses, and sends a moderator looking for a lift it does not
  // need. Expired is a word in the badge and a sentence in the row.
  await expect(page.getByTestId('pause-state')).toHaveText('Expired')
  await expect(page.getByTestId('pause-standing')).toContainText('Expired on 2026-11-01')
  await expect(page.getByTestId('pause-standing')).toContainText('no longer in force')
  // Nothing to lift: the pause has already stopped by itself.
  await expect(page.getByTestId('pause-lift')).toHaveCount(0)
})

test('a pause with no expiry says it lasts until it is lifted', async ({ page }) => {
  await consoleAs(page, {
    domainPauses: [pausedDomain({ domain: 'steady.example', expires_at: null })],
  })
  await open(page, '/admin/federation')

  await expect(page.getByTestId('pause-standing')).toContainText(
    'Paused with no expiry: it lasts until it is lifted',
  )
})

test('an empty pause list says no domain is paused', async ({ page }) => {
  await consoleAs(page, { domainPauses: [] })
  await open(page, '/admin/federation')

  await expect(page.getByTestId('pause-empty')).toHaveText('No domain is paused.')
})

test('pausing a domain sends its reason and the day it lapses', async ({ page }) => {
  const calls = await consoleAs(page, { domainPauses: [] })
  await open(page, '/admin/federation')

  await page.getByTestId('pause-domain').fill('noisy.example')
  await page.getByTestId('pause-reason').fill('waiting for their admin to answer')
  await page.getByTestId('pause-expiry').fill('2026-12-01')
  await page.getByTestId('pause-submit').click()

  const written = await sent(
    calls,
    (call) => call.method === 'POST' && call.url.endsWith('/api/v1/admin/federation/domain-pauses'),
  )
  const body = JSON.parse(written.body)
  expect(body.domain).toBe('noisy.example')
  expect(body.reason).toBe('waiting for their admin to answer')
  // A date input names a day; the endpoint takes an instant, so the day is widened to its
  // start rather than the pause silently losing its expiry.
  expect(body.expires_at).toBe('2026-12-01T00:00:00Z')
  await expect(page.getByTestId('pause-notice')).toContainText('noisy.example is paused')
  await expect(page.getByTestId('pause-row-domain')).toHaveText('noisy.example')
})

test('lifting a pause asks a second time, and says what resumes', async ({ page }) => {
  const calls = await consoleAs(page, { domainPauses: [pausedDomain({ domain: 'quiet.example' })] })
  await open(page, '/admin/federation')

  await page.getByTestId('pause-lift').click()
  await expect(page.getByTestId('pause-lift-confirm')).toBeVisible()
  // The first press only asks: nothing leaves until it is answered.
  expect(calls.filter((call) => call.method === 'DELETE')).toHaveLength(0)

  await page.getByTestId('pause-lift-confirm').click()
  const written = await sent(
    calls,
    (call) =>
      call.method === 'DELETE' &&
      call.url.endsWith('/api/v1/admin/federation/domain-pauses/quiet.example'),
  )
  expect(written.url).toContain('/domain-pauses/quiet.example')
  await expect(page.getByTestId('pause-notice')).toContainText('The pause on quiet.example is lifted')
})

test('an instance without federation_delivery offers no delivery panel', async ({ page }) => {
  const features = FEATURES.filter((name) => name !== 'federation_delivery')
  await stubApi(page, { features })
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/admin/federation')
  await hydrated(page)

  await expect(page.getByRole('tab', { name: 'Deliveries' })).toHaveCount(0)
  await expect(page.getByText(/does not serve the delivery panel/)).toBeVisible()
})
