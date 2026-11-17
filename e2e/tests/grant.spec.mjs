//! The moderator grant: the OAuth round trip the "Grant admin access" button starts.
//!
//! This is the flow that was broken. It leaves the page, comes back through
//! `/oauth/callback`, exchanges a code for a token and only then stores it — so the test
//! drives the same redirects the instance would, and reads the pending record the client
//! stashed to answer with the `state` it is expecting.

import { expect, test } from '@playwright/test'

import {
  ACCOUNT_ID,
  GRANT_KEY,
  HOST,
  adminApi,
  hydrated,
  signedIn,
  stubApi,
} from '../fixtures.mjs'

/** The `sessionStorage` key the half-finished flow lives under. */
const PENDING_KEY = 'nodus-oauth/4'

/** The token the stubbed instance hands back. */
const GRANT = 'stub-admin-token'

/** The scopes the grant asks for, which are the point of the second token. */
const ADMIN_SCOPES = ['admin:read', 'admin:write']

/**
 * Answer app registration, authorization and the token exchange like an instance.
 *
 * The `state` comes back out of the authorize URL rather than out of storage: the record is
 * written just before the navigation, and reading it from the page while that navigation is
 * in flight would deadlock the test against the browser.
 */
async function instance(page, { authorize = 'grant' } = {}) {
  const asked = []
  await page.route('**/api/v1/apps', (route) =>
    route.fulfill({
      headers: { 'access-control-allow-origin': '*' },
      json: { id: '1', client_id: 'stub-client', client_secret: 'stub-secret' },
    }),
  )
  await page.route('**/oauth/authorize**', (route) => {
    const url = new URL(route.request().url())
    const state = url.searchParams.get('state') ?? ''
    const scope = url.searchParams.get('scope') ?? ''
    asked.push({ state, scope, clientId: url.searchParams.get('client_id') })
    const query =
      authorize === 'refuse'
        ? `error=access_denied&state=${state}`
        : `code=stub-code&state=${state}`
    return route.fulfill({
      status: 303,
      headers: { location: `http://127.0.0.1:4173/oauth/callback?${query}` },
    })
  })
  await page.route('**/oauth/token', (route) =>
    route.fulfill({
      headers: { 'access-control-allow-origin': '*' },
      json: {
        access_token: GRANT,
        token_type: 'bearer',
        scope: 'read write follow admin:read admin:write',
        created_at: 0,
      },
    }),
  )
  return asked
}

test('granting admin access stores a token for this account and lands back on the queue', async ({
  page,
}) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  const asked = await instance(page)
  await page.goto('/moderation')
  await hydrated(page)

  await expect(page.getByText('This needs a moderation token')).toBeVisible()
  await page.getByRole('button', { name: 'Grant admin access' }).click()

  // A grant is per instance and per account: another account's consent must never be
  // reused for this one. The flow leaves the page, so the record is read after it lands.
  await expect(page).toHaveURL(/\/moderation\/reports$/, { timeout: 15000 })
  expect(await page.evaluate((key) => localStorage.getItem(key), GRANT_KEY)).toBe(
    [HOST, ACCOUNT_ID, GRANT].join('\t'),
  )
  await expect(page.getByText('Sells something.')).toBeVisible()

  // The round trip has to ask for the admin scopes, for the account whose browser
  // consented, and it has to echo the state it stored.
  expect(asked).toHaveLength(1)
  expect(asked[0].clientId).toBe('stub-client')
  for (const scope of ADMIN_SCOPES) expect(asked[0].scope.split(' ')).toContain(scope)
  expect(asked[0].state).not.toBe('')
})

test('a refused authorization leaves no grant behind', async ({ page }) => {
  await stubApi(page)
  await adminApi(page)
  await signedIn(page)
  await instance(page, { authorize: 'refuse' })
  await page.goto('/moderation')
  await hydrated(page)

  await page.getByRole('button', { name: 'Grant admin access' }).click()

  // A refusal ends on the callback screen with the reason, and nothing stored.
  await expect(page.getByText(/did not authorize the app/)).toBeVisible({ timeout: 15000 })
  expect(await page.evaluate((key) => localStorage.getItem(key), GRANT_KEY)).toBeNull()
})

test('a grant already held opens the queue without asking again', async ({ page }) => {
  await stubApi(page)
  const calls = await adminApi(page)
  await signedIn(page)
  await page.addInitScript(
    ([key, record]) => localStorage.setItem(key, record),
    [GRANT_KEY, [HOST, ACCOUNT_ID, GRANT].join('\t')],
  )
  await page.goto('/moderation')
  await hydrated(page)

  await expect(page.getByText('Sells something.')).toBeVisible()
  const probe = calls.find((call) => call.url.includes('/admin/reports?'))
  expect(probe, 'the grant was not used to probe').toBeTruthy()
})
