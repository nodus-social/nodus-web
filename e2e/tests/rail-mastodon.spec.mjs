// The rail on a plain Mastodon instance.
//
// A Mastodon instance has no `/api/v1/meta`, so every Nodus-only question it is asked comes
// back 404. Navigation is not one of those questions: the rail is the client's own furniture
// and must be drawn whether or not the instance serves anything beyond the Mastodon API.

import { expect, test } from '@playwright/test'
import { account, hydrated, signedIn, status, stubApi } from '../fixtures.mjs'

const MASTODON = 'mastodon.luxuride.eu'

/** Answer like Mastodon: the standard API, and a 404 for everything Nodus adds. */
async function stubMastodon(page) {
  const json = (value) => ({
    status: 200,
    headers: { 'content-type': 'application/json', 'access-control-allow-origin': '*' },
    body: JSON.stringify(value),
  })
  const missing = {
    status: 404,
    headers: { 'content-type': 'application/json', 'access-control-allow-origin': '*' },
    body: JSON.stringify({ error: 'Record not found' }),
  }

  await stubApi(page, { statuses: [status()] })
  await page.route('**/api/v1/accounts/verify_credentials', (route) => route.fulfill(json(account)))
  await page.route('**/api/v1/timelines/home**', (route) => route.fulfill(json([status()])))
  await page.route('**/api/v1/notifications**', (route) => route.fulfill(json([])))
  await page.route('**/api/v1/meta', (route) => route.fulfill(missing))
  await page.route('**/api/v1/nodus/**', (route) => route.fulfill(missing))
  await page.route('**/api/v1/feed/**', (route) => route.fulfill(missing))
}

test('a Mastodon instance still draws the rail', async ({ page }) => {
  test.skip(test.info().project.name !== 'desktop', 'the rail is the desktop shell')
  await stubMastodon(page)
  await signedIn(page, { host: MASTODON })
  const crashes = []
  page.on('pageerror', (error) => crashes.push(String(error)))

  await page.goto('/home')
  await hydrated(page)

  const rail = page.locator('[data-rail="left"]')
  await expect(rail).toBeVisible()
  // The rail is navigation, not a panel the instance serves: Home and Settings must be
  // reachable even though `/api/v1/meta` answered 404.
  await expect(rail.getByRole('link', { name: 'Home', exact: true }).first()).toBeVisible()
  await expect(rail.getByRole('link', { name: 'Settings', exact: true }).first()).toBeVisible()
  expect(crashes, crashes.join('\n')).toEqual([])
})
