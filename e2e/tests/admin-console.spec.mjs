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

  // The rail and the gate both ask "may this account moderate?" and both re-run on every
  // store notification, which the socket and the sync loop produce constantly. Each ask
  // used to be its own request — and a refusal of an ungranted token is recorded by the
  // server as a failed authentication, so the alert list filled with the client's polling.
  await page.waitForTimeout(2500)
  const probes = calls.filter((call) => call.url.includes('/api/v1/admin/reports'))
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
