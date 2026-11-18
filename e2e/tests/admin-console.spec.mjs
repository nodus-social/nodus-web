//! The moderator console: the screens behind a grant, and what they send.
//!
//! Every test here boots with a grant and asks the stub what the console wrote. A panel
//! that renders a button but never sends its request looks identical in a screenshot, so
//! the assertions are on the recorded calls wherever the screen is meant to write.

import { expect, test } from '@playwright/test'

import {
  ACCOUNT_ID,
  adminAccount,
  adminApi,
  captureChartSpecs,
  chartSpecs,
  granted,
  hydrated,
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
  await expect(page.locator('canvas')).toHaveCount(4)

  // A canvas is not a graph. The wrapper reads properties off the spec, so a spec sent as
  // text draws an empty axis and still counts as a canvas — which is how an empty
  // dashboard shipped green. Assert the data, and the two series of the report chart.
  const specs = await chartSpecs(page)
  expect(specs).toHaveLength(4)
  for (const spec of specs) {
    expect(spec.labels.length).toBeGreaterThan(0)
    expect(spec.series.some((series) => series.values.length > 0)).toBe(true)
  }
  const reports = specs.find((spec) =>
    spec.series.some((series) => series.label === 'Opened'),
  )
  expect(reports.series.map((series) => series.label)).toEqual(['Opened', 'Resolved'])
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
