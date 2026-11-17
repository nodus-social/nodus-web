//! Moderation and administration: what a staff account sees, and what it is asked for.
//!
//! The panels are behind a second token with `admin:read`, so the interesting states are
//! the ones before that token exists. A grant is stored per instance **and per account**:
//! a token acts as the account that consented, so one account's grant must never open the
//! panels for another.

import { expect, test } from '@playwright/test'

import { ACCOUNT_ID, GRANT_KEY, granted, grantRecord, hydrated, signedIn, stubApi, adminApi } from '../fixtures.mjs'

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
  'Protocol',
]

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
