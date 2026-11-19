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
  granted,
  grantRecord,
  hydrated,
  remoteAccount,
  signedIn,
  stubApi,
} from '../fixtures.mjs'

/** The sections the moderation screen offers, in reading order. */
const MODERATION = ['Reports', 'Scans', 'Accounts', 'Registrations', 'Federation']

/** The sections the administration screen offers, in reading order. */
const ADMINISTRATION = [
  'Overview',
  'Scanners',
  'Allowed domains',
  'Mail domains',
  'Canned replies',
  'Announcements',
  'Audit log',
  'System',
  'Protocol',
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

test('a moderator is offered Moderation and not the administration door', async ({ page }) => {
  await stubApi(page, { role: 'moderator' })
  await adminApi(page, { role: 'moderator' })
  await signedIn(page)
  await granted(page)
  await page.goto('/')
  await hydrated(page)

  if (desktop()) {
    await expect(page.locator('nav a').filter({ hasText: 'Moderation' })).toHaveCount(1)
    await expect(page.locator('nav a').filter({ hasText: 'Administration' })).toHaveCount(0)
  }

  await page.goto('/moderation')
  await hydrated(page)
  await expect(page.locator('[role="tablist"] [role="tab"]').first()).toBeVisible()
  expect(await sections(page)).toEqual(MODERATION)

  // The administration screens are the instance's own levers and this account is not an
  // administrator: the sections are painted with the reason they would be refused.
  await page.goto('/admin/overview')
  await hydrated(page)
  await expect(page.getByText(/staff\s+role is moderator rather than administrator/)).toBeVisible()
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
