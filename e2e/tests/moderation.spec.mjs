//! Moderation and administration: what a staff account sees, and what it is asked for.
//!
//! The panels are behind a second token with `admin:read`, so the interesting states are
//! the ones before that token exists. A grant is stored per instance **and per account**:
//! a token acts as the account that consented, so one account's grant must never open the
//! panels for another.

import { expect, test } from '@playwright/test'

import {
  ACCOUNT_ID,
  FEATURES,
  GRANT_KEY,
  adminApi,
  appeal,
  granted,
  grantRecord,
  hydrated,
  otherAccount,
  quarantinedMedia,
  remoteAccount,
  report,
  signedIn,
  stubApi,
} from '../fixtures.mjs'

/** The sections the moderation screen offers, in reading order. */
const MODERATION = [
  'Reports',
  'Appeals',
  'Scans',
  'Media',
  'Accounts',
  'Registrations',
  'Federation',
  'Rules',
]

/** The sections the administration screen offers, in reading order. */
const ADMINISTRATION = [
  'Overview',
  'Scanners',
  'Allowed domains',
  'Mail domains',
  'Canned replies',
  'Escalation',
  'Deliveries',
  'Announcements',
  'Audit log',
  'System',
  'Protocol',
  'Settings transfer',
]

/** The request the console sent, once it has sent it. */
async function sent(calls, match) {
  await expect.poll(() => calls.filter(match).length, { timeout: 5000 }).toBeGreaterThan(0)
  return calls.filter(match).at(-1)
}

/** The row for one moderated account, found by the name people write it as. */
function row(page, name) {
  return page.locator('li', { hasText: name })
}

test('the appeal queue is a work list, and a decision is written once', async ({ page }) => {
  await stubApi(page)
  const calls = await adminApi(page, {
    // Newest first, as the instance orders them, with a decided one on top: the queue on
    // screen puts the waiting appeals first whatever order it was handed.
    appeals: [
      appeal({
        id: '3102',
        handle: 'bo',
        action: 'reject',
        reason: 'It was my own post.',
        status: 'accepted',
        decision_note: 'The sign-up was refused by mistake.',
        decided_by: 'ada',
        decided_at: '2026-11-19T09:00:00.000Z',
        age_seconds: 90_000,
      }),
      appeal({
        id: '3101',
        handle: 'ada',
        action: 'suspend',
        reason: 'I was quoting the post, not selling anything.',
        restriction: 'suspend',
        age_seconds: 7_200,
      }),
      appeal({
        id: '3103',
        handle: 'cy',
        action: 'silence',
        reason: 'I was joking, not spamming.',
        restriction: 'silence',
        age_seconds: 300,
      }),
    ],
    // The third row is decided by someone else while this moderator is reading it.
    appealRefusals: { '3103': true },
  })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/appeals')
  await hydrated(page)

  // The screen opens on the work: the count of what is waiting, and only what is waiting.
  await expect(page.getByText('2 appeals are waiting for a decision.')).toBeVisible()
  const queue = page.locator('li[data-appeal]')
  await expect(queue).toHaveCount(2)

  // A row says what was done and why in sentences, without anything being opened.
  const suspended = page.locator('li[data-appeal="3101"]')
  await expect(suspended).toContainText('The account was suspended.')
  await expect(suspended).toContainText('I was quoting the post, not selling anything.')
  await expect(suspended).toContainText('waited 2h')
  await expect(page.locator('li[data-appeal="3103"]')).toContainText('The account was silenced.')

  // A verdict needs a word from the moderator first: no default accept, and the note is
  // optional rather than required.
  const decide = suspended.getByRole('button', { name: 'Decide', exact: true })
  await expect(decide).toBeDisabled()
  await expect(suspended.getByLabel('Reject the appeal from ada')).toBeVisible()
  await suspended.getByLabel('Accept the appeal from ada').check()
  await expect(decide).toBeEnabled()
  await suspended.getByLabel('Note for ada').fill('The quote was fair use.')
  await decide.click()
  // A second press is not a second decision: the press holds the row until its answer lands.
  await decide.click({ timeout: 500 }).catch(() => {})

  const decidedCall = await sent(calls, (call) => call.method === 'POST')
  expect(new URL(decidedCall.url).pathname).toBe('/api/v1/admin/appeals/3101/decide')
  expect(JSON.parse(decidedCall.body)).toEqual({
    accept: true,
    note: 'The quote was fair use.',
  })
  expect(calls.filter((call) => call.method === 'POST')).toHaveLength(1)

  // The answer lands on the screen, in the singular the count now needs.
  await expect(page.getByText('1 appeal is waiting for a decision.')).toBeVisible()
  await expect(
    page.getByRole('status').filter({ hasText: 'Accepted. The suspension was lifted.' }),
  ).toBeVisible()
  // It left the work list, which is what accepting it means.
  await expect(suspended).toHaveCount(0)

  // The whole queue keeps the waiting first and shows a decided appeal as it now stands.
  await page.getByRole('button', { name: 'All', exact: true }).click()
  await expect(queue).toHaveCount(3)
  // The one still waiting is first; the two decided ones keep the instance's own order.
  expect(await queue.evaluateAll((rows) => rows.map((row) => row.dataset.appeal))).toEqual([
    '3103',
    '3102',
    '3101',
  ])
  await expect(suspended).toContainText('Accepted: the suspension was lifted.')
  await expect(suspended).toContainText('Decided by ada on 2026-11-19.')
  await expect(suspended).toContainText('The quote was fair use.')
  // Read-only: there is nothing left to decide on a decided row.
  await expect(suspended.getByRole('button', { name: 'Decide', exact: true })).toHaveCount(0)

  const rejected = page.locator('li[data-appeal="3102"]')
  await expect(rejected).toContainText('The registration was rejected.')
  await expect(rejected).toContainText('Accepted: the registration was approved.')
  await expect(rejected).toContainText('The sign-up was refused by mistake.')

  // Someone else decided the third one meanwhile. That is not an error to act on: the row
  // says who decided it, and the queue is redrawn from the instance, not from the answer.
  const stale = page.locator('li[data-appeal="3103"]')
  await stale.getByLabel('Accept the appeal from cy').check()
  await stale.getByRole('button', { name: 'Decide', exact: true }).click()
  await expect(
    page.getByRole('status').filter({ hasText: 'already decided by bob' }),
  ).toContainText('Nothing was changed.')
  await expect(page.locator('[role="alert"]')).toHaveCount(0)
  await expect(stale).toContainText('Decided by bob on 2026-11-19.')
  await expect(stale).toContainText('Handled from the other tab.')
  await expect(stale.getByRole('button', { name: 'Decide', exact: true })).toHaveCount(0)

  // Nothing is waiting any more, and the screen says so rather than showing an empty list.
  await page.getByRole('button', { name: 'Waiting', exact: true }).click()
  await expect(page.getByText('No appeal is waiting.')).toBeVisible()
})

test('a grant without admin:write reads the appeal queue but cannot decide', async ({ page }) => {
  await stubApi(page)
  await adminApi(page, { scopes: ['read', 'write', 'admin:read'] })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/appeals')
  await hydrated(page)

  const waiting = page.locator('li[data-appeal="3101"]')
  await expect(waiting).toContainText('The account was suspended.')
  // The reason is a read, so it is painted; the verdict is not this grant's to give.
  await expect(page.getByText(/does not include admin:write/)).toBeVisible()
  await expect(waiting.getByRole('button', { name: 'Decide', exact: true })).toBeDisabled()
})

/** The strip's labels, without the icons beside them. */
async function sections(page) {
  return page
    .locator('[role="tablist"] [role="tab"] span')
    .allInnerTexts()
    .then((labels) => labels.map((label) => label.trim()).filter(Boolean))
}

test('a moderator without a grant is asked for one', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  await page.goto('/moderation')
  await hydrated(page)

  await expect(page.getByText('This needs a moderation token')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Grant admin access' })).toBeVisible()
  // Nothing behind the gate is painted until the probe can answer.
  await expect(page.getByRole('tab', { name: 'Federation' })).toHaveCount(0)
})

test('a grant made by another account is not reused', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  // The same instance, the same browser — a different account consented.
  await granted(page, { account: '2' })
  await page.goto('/moderation')
  await hydrated(page)

  await expect(page.getByText('This needs a moderation token')).toBeVisible()
})

test('a per-instance grant from the old format opens nothing', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  // Version 1 stored `<host>\t<token>`; it does not name an account, so it is not a grant.
  await page.addInitScript(
    ([key, value]) => localStorage.setItem(key, value),
    ['nodus-admin/1', 'stub.instance\tstub-admin-token'],
  )
  await page.goto('/moderation')
  await hydrated(page)

  await expect(page.getByText('This needs a moderation token')).toBeVisible()
})

test('a grant opens every moderation section', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation')
  await hydrated(page)

  await expect(page.locator('[role="tablist"] [role="tab"]').first()).toBeVisible()
  expect(await sections(page)).toEqual(MODERATION)
  // The queue is what the screen opens on, so the grant is proven by its contents.
  await expect(page.getByText('Sells something.')).toBeVisible()
})

test('the administration screen lists the whole console', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/admin')
  await hydrated(page)

  await expect(page.locator('[role="tablist"] [role="tab"]').first()).toBeVisible()
  expect(await sections(page)).toEqual(ADMINISTRATION)
})

test('an opted-in browser keeps the grant it was given', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  await granted(page, { token: 'stub-admin-token' })
  await page.goto('/moderation/accounts')
  await hydrated(page)

  // The grant is read back from storage, not re-asked for on every visit.
  await expect(page.getByText('This needs a moderation token')).toHaveCount(0)
  const stored = await page.evaluate((key) => localStorage.getItem(key), GRANT_KEY)
  expect(stored).toBe(grantRecord({ account: ACCOUNT_ID }))
})

/** Whether the painted shell is the desktop one, where the rail carries the doors. */
function desktop() {
  return test.info().project.name === 'desktop'
}

/** The rail's door labels. */
async function doors(page) {
  return page
    .locator('nav a')
    .allInnerTexts()
    .then((labels) => labels.map((label) => label.trim().split('\n')[0]).filter(Boolean))
}

test('an account the instance does not call staff is told who can grant it', async ({ page }) => {
  // The instance's own answer, not a report the browser happened to read: this account has
  // no staff role, so there is nothing here for a second token to unlock.
  await stubApi(page, { role: 'none' })
  await adminApi(page, { role: 'none' })
  await signedIn(page)
  await page.goto('/moderation')
  await hydrated(page)

  await expect(page.getByText(/does not make your account staff/)).toBeVisible()
  await expect(page.getByText(/an administrator\s+has to grant it/)).toBeVisible()
  // No button: only an administrator can change this, and not from this browser.
  await expect(page.getByRole('button', { name: /Grant admin access/ })).toHaveCount(0)
  if (desktop()) {
    const labels = await doors(page)
    expect(labels).not.toContain('Moderation')
    expect(labels).not.toContain('Administration')
  }
})

test('a moderator is offered both doors, with the administrator sections gated', async ({ page }) => {
  await stubApi(page, { role: 'moderator' })
  await adminApi(page, { role: 'moderator' })
  await signedIn(page)
  await granted(page)
  await page.goto('/')
  await hydrated(page)

  if (desktop()) {
    await expect(page.locator('nav a').filter({ hasText: 'Moderation' })).toHaveCount(1)
    await expect(page.locator('nav a').filter({ hasText: 'Administration' })).toHaveCount(1)
  }

  await page.goto('/moderation')
  await hydrated(page)
  await expect(page.locator('[role="tablist"] [role="tab"]').first()).toBeVisible()
  // Scans is administrator-only, so a moderator's strip leaves it out.
  expect(await sections(page)).toEqual(MODERATION.filter((name) => name !== 'Scans'))

  // The administration area opens for staff, and its strip keeps only the sections a
  // moderator's role can actually read: scanners and mail-domain blocks are dropped.
  await page.goto('/admin/overview')
  await hydrated(page)
  await expect(page.locator('[role="tablist"] [role="tab"]').first()).toBeVisible()
  expect(await sections(page)).toEqual(
    ADMINISTRATION.filter((name) => name !== 'Scanners' && name !== 'Mail domains'),
  )
})

test('an administrator is offered both doors and the whole console', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/')
  await hydrated(page)

  if (desktop()) {
    await expect(page.locator('nav a').filter({ hasText: 'Moderation' })).toHaveCount(1)
    await expect(page.locator('nav a').filter({ hasText: 'Administration' })).toHaveCount(1)
  }

  await page.goto('/admin')
  await hydrated(page)
  await expect(page.locator('[role="tablist"] [role="tab"]').first()).toBeVisible()
  expect(await sections(page)).toEqual(ADMINISTRATION)
})

test('an instance whose features omit scanners paints no scanners panel', async ({ page }) => {
  const features = FEATURES.filter((name) => name !== 'scanners')
  await stubApi(page, { features })
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/admin/scanners')
  await hydrated(page)

  // The tab is not drawn, and the address says why rather than showing a panel that can
  // never fill: the capability list, not the status code, decides what exists.
  await expect(page.getByRole('tab', { name: 'Scanners' })).toHaveCount(0)
  await expect(page.getByText(/does not serve the scanners panel/)).toBeVisible()
  await expect(page.getByRole('tab', { name: 'Allowed domains' })).toBeVisible()
})

test('a grant without admin:write is shown the controls it cannot use', async ({ page }) => {
  await stubApi(page)
  await adminApi(page, { scopes: ['read', 'write', 'admin:read'] })
  await signedIn(page)
  await granted(page)
  await page.goto('/admin/overview')
  await hydrated(page)

  // Read-only staff: the panel paints, with the scope it is missing named beside it.
  await expect(page.getByText(/does not include admin:write/)).toBeVisible()
  await expect(page.getByText('Accounts', { exact: true })).toBeVisible()
})

test('a moderated remote account reads from its own row whether it is in force', async ({
  page,
}) => {
  await stubApi(page)
  await adminApi(page, {
    remoteAccounts: [
      // In force: no expiry, so it lapses only when someone lifts it.
      remoteAccount(),
      // Lapsed: the expiry has passed, and the row must not read like the one above.
      remoteAccount({
        actor_uri: 'https://peer.example/users/y',
        handle: 'y',
        action: 'silence',
        reason: 'Repeated spam.',
        expires_at: '2026-11-01T00:00:00Z',
        expired: true,
      }),
    ],
  })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/federation')
  await hydrated(page)

  const live = row(page, 'x@peer.example')
  await expect(live).toContainText('In force')
  await expect(live).toContainText('Suspend in force, with no expiry')
  await expect(live).toContainText('Ban evasion.')
  await expect(live).toContainText('Applied by ada on 2026-11-18')
  await expect(live).toContainText('https://peer.example/users/x')

  // An action whose day has passed is still a record, and says in words that it is doing
  // nothing: a reader must be able to trust the row without reading the date against today.
  const lapsed = row(page, 'y@peer.example')
  await expect(lapsed).toContainText('Expired on 2026-11-01: this silence is no longer in force')
  await expect(lapsed).not.toContainText('In force')
  await expect(lapsed).not.toContainText('in force until')
})

test('an account is suspended from its row and lifted after confirming', async ({ page }) => {
  await stubApi(page)
  const calls = await adminApi(page, {
    remoteAccounts: [remoteAccount({ action: 'silence', reason: null })],
  })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/federation')
  await hydrated(page)

  const name = 'x@peer.example'
  await page.getByLabel(`Action for ${name}`).selectOption('suspend')
  await page.getByLabel(`Reason for ${name}`).fill('Ban evasion.')
  await page.getByLabel(`Expiry for ${name}`).fill('2026-12-01')
  await page.getByRole('button', { name: 'Apply', exact: true }).click()

  const applied = await sent(calls, (call) => call.method === 'PUT')
  // The account's whole address is the path's one segment, percent-encoded.
  expect(decodeURIComponent(new URL(applied.url).pathname)).toContain(
    `/federation/remote-accounts/https://peer.example/users/x`,
  )
  // A date input names a day; the endpoint takes the instant that day starts.
  expect(JSON.parse(applied.body)).toEqual({
    action: 'suspend',
    reason: 'Ban evasion.',
    expires_at: '2026-12-01T00:00:00Z',
  })

  // Lifting asks first: the row is the only record that the account was ever acted on.
  await page.getByRole('button', { name: 'Lift', exact: true }).click()
  await expect(page.getByText('Lift this action?')).toBeVisible()
  await expect(row(page, name)).toBeVisible()
  await page.getByRole('button', { name: 'Lift action' }).click()

  const lifted = await sent(calls, (call) => call.method === 'DELETE')
  expect(decodeURIComponent(new URL(lifted.url).pathname)).toContain(
    `/federation/remote-accounts/https://peer.example/users/x`,
  )
  await expect(page.getByText('No remote account is moderated.')).toBeVisible()
})

test('a grant without admin:write can read moderated accounts but not move them', async ({
  page,
}) => {
  await stubApi(page)
  await adminApi(page, {
    scopes: ['read', 'write', 'admin:read'],
    remoteAccounts: [remoteAccount()],
  })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/federation')
  await hydrated(page)

  // The list is a read, so the row is painted with what it records.
  await expect(row(page, 'x@peer.example')).toContainText('Ban evasion.')
  // The three mutations need the second grant's write scope, and the row says so.
  await expect(page.getByLabel('Action for x@peer.example')).toBeDisabled()
  await expect(page.getByLabel('Reason for x@peer.example')).toBeDisabled()
  await expect(page.getByLabel('Expiry for x@peer.example')).toBeDisabled()
  await expect(page.getByRole('button', { name: 'Apply', exact: true })).toBeDisabled()
  await expect(page.getByRole('button', { name: 'Lift', exact: true })).toBeDisabled()
})

test('an instance that moderates no remote account says so', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/federation')
  await hydrated(page)

  await expect(page.getByText('No remote account is moderated.')).toBeVisible()
})

test('a quarantined picture names its set and its post, and releasing keeps it', async ({
  page,
}) => {
  await stubApi(page)
  const calls = await adminApi(page, { quarantined: [quarantinedMedia()], audit: [] })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/media')
  await hydrated(page)

  // The row names the attachment, the set that matched, and the post it belonged to, which
  // is the three things a moderator needs before either answer makes sense.
  const held = row(page, 'ncii-known-hashes')
  await expect(held).toContainText('Attachment 901 (image/png)')
  await expect(held).toContainText('Matched the set ncii-known-hashes.')
  await expect(held.getByRole('link', { name: 'Open the post it was on' })).toHaveAttribute(
    'href',
    '/s/9001',
  )

  // The two outcomes are not one dial with two ends: the words and the colours differ,
  // because one keeps the picture and the other destroys it.
  const keep = held.getByRole('button', { name: 'Release', exact: true })
  const destroy = held.getByRole('button', { name: 'Delete', exact: true })
  await expect(keep).toHaveClass(/btn-primary/)
  await expect(destroy).toHaveClass(/btn-error/)
  await expect(held).toContainText('Release keeps the picture')
  await expect(held).toContainText('Delete destroys the file')

  // Releasing asks its question and wants a reason before anything leaves.
  await keep.click()
  await expect(held).toContainText('Release this picture?')
  const confirm = held.getByRole('button', { name: 'Release the picture' })
  await expect(confirm).toBeDisabled()
  await held
    .getByLabel('Reason for the attachment 901')
    .fill('A press photo, not the file on the list.')
  await expect(confirm).toBeEnabled()
  await confirm.click()

  const cleared = await sent(calls, (call) => call.method === 'POST')
  expect(decodeURIComponent(new URL(cleared.url).pathname)).toBe(
    '/api/v1/admin/media/901/quarantine',
  )
  expect(JSON.parse(cleared.body)).toEqual({
    release: true,
    reason: 'A press photo, not the file on the list.',
  })

  // The picture is back, so the row leaves the work list and the sentence says what the
  // instance did rather than what the button said.
  await expect(page.getByText('No media is quarantined.')).toBeVisible()
  await expect(page.getByText(/Released\. The picture is back on its post/)).toBeVisible()

  // The decision is on record where every other staff action is.
  await page.goto('/admin/audit')
  await hydrated(page)
  await expect(page.getByText('release_media_quarantine')).toBeVisible()
  await expect(page.getByText(/media 901/)).toBeVisible()
})

test('deleting a quarantined picture destroys it and says so', async ({ page }) => {
  await stubApi(page)
  const calls = await adminApi(page, {
    quarantined: [quarantinedMedia({ id: '902', status_id: null })],
    audit: [],
  })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/media')
  await hydrated(page)

  const held = row(page, 'ncii-known-hashes')
  // An attachment that never reached a post says that instead of linking nowhere.
  await expect(held).toContainText('It was not attached to a post.')

  await held.getByRole('button', { name: 'Delete', exact: true }).click()
  await expect(held).toContainText('Delete this picture? The file is destroyed')
  const confirm = held.getByRole('button', { name: 'Delete the picture' })
  await expect(confirm).toBeDisabled()
  await held.getByLabel('Reason for the attachment 902').fill('It is the file on the list.')
  await confirm.click()

  const cleared = await sent(calls, (call) => call.method === 'POST')
  expect(JSON.parse(cleared.body)).toEqual({ release: false, reason: 'It is the file on the list.' })
  // The row is gone, not marked: there is nothing left to point at.
  await expect(page.getByText('No media is quarantined.')).toBeVisible()
  await expect(
    page.getByText(/Deleted\. The attachment is destroyed and cannot be recovered/),
  ).toBeVisible()
})

test('a grant without admin:write can read quarantined media but not clear it', async ({
  page,
}) => {
  await stubApi(page)
  await adminApi(page, { scopes: ['read', 'write', 'admin:read'] })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/media')
  await hydrated(page)

  // The list is a read, so the row paints; both outcomes are writes, so neither can press.
  const held = row(page, 'ncii-known-hashes')
  await expect(held).toContainText('Matched the set ncii-known-hashes.')
  await expect(held.getByRole('button', { name: 'Release', exact: true })).toBeDisabled()
  await expect(held.getByRole('button', { name: 'Delete', exact: true })).toBeDisabled()
  await expect(page.getByText(/does not include admin:write/)).toBeVisible()
})

test('an instance whose features omit media_quarantine paints no media panel', async ({
  page,
}) => {
  const features = FEATURES.filter((name) => name !== 'media_quarantine')
  await stubApi(page, { features })
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/media')
  await hydrated(page)

  await expect(page.getByRole('tab', { name: 'Media' })).toHaveCount(0)
  await expect(page.getByText(/does not quarantine media/)).toBeVisible()
  await expect(page.getByRole('tab', { name: 'Appeals' })).toBeVisible()
})

/// One forwarded report, as `GET /api/v1/admin/federation/forwarded` sends it.
function forward(overrides = {}) {
  return {
    id: 'f7001',
    report_id: '7001',
    target_domain: 'peer.example',
    activity_uri: 'https://nodus.example/activities/f7001',
    state: 'pending',
    attempts: 0,
    last_error: null,
    created_at: '2026-11-19T08:00:00Z',
    updated_at: '2026-11-19T08:00:00Z',
    ...overrides,
  }
}

/// A report about an account that lives on another instance.
function remoteReport(overrides = {}) {
  return report({ target_account: { ...otherAccount, acct: 'grace@peer.example' }, ...overrides })
}

test('a report on a remote account is forwarded once, then shows what became of it', async ({
  page,
}) => {
  await stubApi(page)
  const calls = await adminApi(page, { reports: [remoteReport()] })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/reports')
  await hydrated(page)

  const held = row(page, '@grace@peer.example')
  await held.getByPlaceholder('Note to the peer (optional)').fill('Please look at this.')
  await held.getByRole('button', { name: 'Forward to the peer' }).click()

  const pressed = await sent(
    calls,
    (call) => call.method === 'POST' && call.url.endsWith('/api/v1/admin/reports/7001/forward'),
  )
  expect(JSON.parse(pressed.body)).toEqual({ note: 'Please look at this.' })

  // The action becomes the state. The endpoint is idempotent and would only answer the row
  // that already exists, so there is nothing left to press — and no second Flag queued.
  await expect(held.getByTestId('forward-badge')).toHaveText('pending')
  await expect(held.getByTestId('forward-state')).toContainText('Forwarded to peer.example')
  await expect(held.getByTestId('forward-state')).toContainText('the peer has not been told yet')
  await expect(held.getByRole('button', { name: 'Forward to the peer' })).toHaveCount(0)
  expect(calls.filter((call) => call.method === 'POST')).toHaveLength(1)
})

test('a report on a local account offers no forward action', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/reports')
  await hydrated(page)

  // The subject is moderated here: the instance would refuse, and a control that can only
  // be refused is not offered.
  const held = row(page, '@grace')
  await expect(held).toBeVisible()
  await expect(held.getByRole('button', { name: 'Forward to the peer' })).toHaveCount(0)
  await expect(held.getByTestId('forward-state')).toHaveCount(0)
})

test('a refused forward shows the instance’s own sentence', async ({ page }) => {
  const reason = 'that domain is paused; nothing is delivered while the pause is in force'
  await stubApi(page)
  const calls = await adminApi(page, {
    reports: [remoteReport()],
    forwardRefusal: { status: 409, message: reason },
  })
  await signedIn(page)
  await granted(page)
  await page.goto('/moderation/reports')
  await hydrated(page)

  const held = row(page, '@grace@peer.example')
  await held.getByRole('button', { name: 'Forward to the peer' }).click()
  await sent(calls, (call) => call.method === 'POST' && call.url.endsWith('/forward'))

  // Verbatim, without the transport's `discovery returned 409:` lead-in: the sentence names
  // the reason, and it is the only explanation a moderator gets.
  await expect(held.getByTestId('forward-refusal')).toHaveText(reason)
  await expect(held.getByRole('button', { name: 'Forward to the peer' })).toBeVisible()
})

test('the federation screen groups what was forwarded by what became of it', async ({ page }) => {
  await stubApi(page)
  await adminApi(page, {
    forwarded: [
      forward({ id: 'f1', report_id: '7001', target_domain: 'peer.example', state: 'pending' }),
      forward({
        id: 'f2',
        report_id: '7002',
        target_domain: 'other.example',
        state: 'gave_up',
        attempts: 5,
        last_error: 'the peer answered 401',
      }),
      forward({ id: 'f3', report_id: '7003', target_domain: 'peer.example', state: 'delivered' }),
    ],
  })
  await signedIn(page)
  await granted(page)
  await page.goto('/admin/federation')
  await hydrated(page)

  await expect(page.getByRole('heading', { name: 'Report forwarding' })).toBeVisible()
  await expect(page.getByTestId('forwarded-group-pending')).toContainText('Pending (1)')
  await expect(page.getByTestId('forwarded-group-delivered')).toContainText('Delivered (1)')
  await expect(page.getByTestId('forwarded-group-gave_up')).toContainText('Given up (1)')
  await expect(page.getByTestId('forwarded-group-gave_up')).toContainText('other.example')
  // A row says how hard the queue tried and what the peer answered, in its own words.
  await expect(page.getByTestId('forwarded-group-gave_up')).toContainText('5 attempts')
  await expect(page.getByTestId('forwarded-group-gave_up')).toContainText('the peer answered 401')
  await expect(page.getByTestId('forwarded-empty')).toHaveCount(0)
})

test('the federation screen says when nothing was forwarded', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  await granted(page)
  await page.goto('/admin/federation')
  await hydrated(page)

  await expect(page.getByTestId('forwarded-empty')).toHaveText('Nothing has been forwarded.')
})
