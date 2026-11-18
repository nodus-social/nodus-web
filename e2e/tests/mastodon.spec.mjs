// A signed-in reader whose instance is plain Mastodon.
//
// Nodus's own endpoints do not exist there, and the client must not treat that as a
// failure: the timeline, the notifications and the composer are all the Mastodon API, and
// everything the recommendation engine adds has to be absent rather than broken.
//
// A 404 is a normal answer from a Mastodon instance, so the browser logs it as a console
// error too. What must never happen is an uncaught exception or a screen that gives up.

import { expect, test } from '@playwright/test'
import { account, hydrated, signedIn, status, stubApi } from '../fixtures.mjs'

const MASTODON = 'mastodon.test'

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
  await page.route('**/api/v1/conversations**', (route) => route.fulfill(json([])))
  await page.route('**/api/v1/bookmarks**', (route) => route.fulfill(json([])))
  await page.route('**/api/v1/favourites**', (route) => route.fulfill(json([])))
  await page.route('**/api/v1/lists', (route) => route.fulfill(json([])))
  await page.route('**/api/v1/announcements', (route) => route.fulfill(json([])))
  await page.route('**/api/v2/search**', (route) =>
    route.fulfill(json({ accounts: [], statuses: [], hashtags: [] })),
  )
  await page.route('**/api/v1/instance', (route) =>
    route.fulfill(
      json({
        uri: MASTODON,
        title: 'Mastodon',
        version: '4.3.0',
        email: '',
        description: '',
        configuration: { accounts: {}, statuses: {} },
        registrations: false,
        stats: { user_count: 1, status_count: 1, domain_count: 1 },
      }),
    ),
  )

  // The Nodus surface: the recommendation engine's meta, its feeds, its reactions and its
  // per-account style document. A Mastodon instance answers none of them.
  await page.route('**/api/v1/meta', (route) => route.fulfill(missing))
  await page.route('**/api/v1/nodus/**', (route) => route.fulfill(missing))
  await page.route('**/api/v1/feed/**', (route) => route.fulfill(missing))
  await page.route('**/api/v1/reactions/**', (route) => route.fulfill(missing))
  await page.route('**/public-api/**', (route) => route.fulfill(missing))
}

/** Open a page, wait for the client, and report only what actually broke. */
async function open(page, path) {
  const crashes = []
  page.on('pageerror', (error) => crashes.push(String(error)))
  await page.goto(path)
  await hydrated(page)
  return crashes
}

test('the home timeline works without a recommendation engine', async ({ page }) => {
  await stubMastodon(page)
  await signedIn(page, { host: MASTODON })
  const crashes = await open(page, '/home')

  await expect(page.getByText('Hello from the fixture.')).toBeVisible()
  expect(crashes, crashes.join('\n')).toEqual([])
})

test('every screen opens when the instance only speaks Mastodon', async ({ page }) => {
  await stubMastodon(page)
  await signedIn(page, { host: MASTODON })

  for (const path of ['/home', '/notifications', '/favourites', '/lists', '/search', '/settings']) {
    const crashes = await open(page, path)
    const main = page.locator('main')
    await expect(main).toBeVisible()
    // A screen that failed to paint its first card leaves its heading alone, which is the
    // difference between "nothing here" and "this broke".
    await expect(main).not.toBeEmpty()
    expect(crashes, `${path}: ${crashes.join('\n')}`).toEqual([])
  }
})

test('a Mastodon session never asks for Nodus metadata', async ({ page }) => {
  await stubMastodon(page)
  await signedIn(page, { host: MASTODON })
  const asked = []
  page.on('request', (request) => asked.push(request.url()))
  await open(page, '/home')
  await expect(page.getByText('Hello from the fixture.')).toBeVisible()
  // Styling, reactions and the account-metadata document only exist on Nodus: asking a
  // Mastodon instance for them is a 404 in every console, and the answer is knowable
  // before the first request because the negotiation already said so.
  await expect
    .poll(() => asked.filter((url) => url.includes('/api/v1/nodus/')).length, { timeout: 2_000 })
    .toBe(0)
})

test('the composer still posts', async ({ page }) => {
  await stubMastodon(page)
  await signedIn(page, { host: MASTODON })
  let published = null
  await page.route('**/api/v1/statuses', (route) => {
    published = route.request().postDataJSON()
    return route.fulfill({
      status: 200,
      headers: { 'content-type': 'application/json', 'access-control-allow-origin': '*' },
      body: JSON.stringify(status({ id: '201', content: '<p>Posted from a Mastodon tab</p>' })),
    })
  })

  const crashes = await open(page, '/')
  const body = page.locator('main .ProseMirror')
  await body.click()
  await page.keyboard.type('Posted from a Mastodon tab')
  await expect(body).toContainText('Posted from a Mastodon tab')
  await page.getByRole('button', { name: 'Post', exact: true }).click()

  await expect.poll(() => published?.status).toBe('Posted from a Mastodon tab')
  expect(crashes, crashes.join('\n')).toEqual([])
})

test('the strip offers only the timelines the instance serves', async ({ page }) => {
  await stubMastodon(page)
  await signedIn(page)
  await page.goto('/home')
  await hydrated(page)
  // The recommendation becomes a tab only once the account's instance has answered that
  // it serves the extension surface; a Mastodon instance never does.
  await expect(page.getByRole('tab')).toHaveText(['Home', 'Local', 'Federated'])
  await expect(page.getByRole('tab', { name: 'Feed', exact: true })).toHaveCount(0)
})

test('the recommendation screen is unavailable, not an empty feed', async ({ page }) => {
  await stubMastodon(page)
  await signedIn(page, { host: MASTODON })
  // `/home/feed` is the recommendation tab's own address; a Mastodon account that follows
  // a link to it must be told what is missing rather than shown an empty ranking.
  const crashes = await open(page, '/home/feed')

  await expect(page.getByRole('heading', { name: 'Recommended posts', exact: true })).toBeVisible()
  await expect(page.getByRole('link', { name: 'Go to the home timeline' })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Feed', exact: true })).toHaveCount(0)
  expect(crashes, crashes.join('\n')).toEqual([])
})

test('settings keep the cards a Mastodon instance can fill', async ({ page }) => {
  await stubMastodon(page)
  await signedIn(page, { host: MASTODON })
  await open(page, '/settings')

  await expect(page.getByRole('heading', { name: 'Session', exact: true })).toBeVisible()
  // The styling summary is the style editor's front door, and the editor is ours alone.
  await expect(page.getByRole('heading', { name: 'Session style', exact: true })).toHaveCount(0)
  await expect(page.locator('[data-edit-session-style]')).toHaveCount(0)

  await page.goto('/settings/nodus')
  await hydrated(page)
  await expect(page.getByRole('heading', { name: 'Background sync', exact: true })).toHaveCount(0)
  const timelines = page
    .locator('section.card')
    .filter({ has: page.getByRole('heading', { name: 'Timelines', exact: true }) })
  await expect(timelines).toBeVisible()
  // Every other row is a browser preference and stays; the recommendation row is a switch
  // for a screen this instance cannot serve.
  await expect(timelines.locator('label').filter({ hasText: 'Feed' })).toHaveCount(0)
  await expect(timelines.locator('label').filter({ hasText: 'Local' })).toHaveCount(1)

  await page.goto('/settings/styling')
  await hydrated(page)
  await expect(page.getByRole('heading', { name: 'Session styling', exact: true })).toBeVisible()
})

test('a Nodus account keeps the recommendation and styling surfaces', async ({ page }) => {
  await stubApi(page)
  await signedIn(page)
  await open(page, '/settings')

  await expect(page.getByRole('heading', { name: 'Session style', exact: true })).toBeVisible()

  await page.goto('/settings/nodus')
  await hydrated(page)
  await expect(page.getByRole('heading', { name: 'Background sync', exact: true })).toBeVisible()
  const timelines = page
    .locator('section.card')
    .filter({ has: page.getByRole('heading', { name: 'Timelines', exact: true }) })
  await expect(timelines.locator('label').filter({ hasText: 'Feed' })).toHaveCount(1)

  await page.goto('/home/feed')
  await hydrated(page)
  await expect(page.getByRole('heading', { name: 'Feed', exact: true })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Recommended posts', exact: true })).toHaveCount(0)
})

test('a Mastodon rail offers no staff doors', async ({ page }) => {
  test.skip(test.info().project.name !== 'desktop', 'the rail is the desktop shell')
  await stubMastodon(page)
  await signedIn(page, { host: MASTODON })
  await open(page, '/home')

  const rail = page.locator('nav[aria-label="Primary navigation"]')
  await expect(rail.getByRole('link', { name: 'Moderation', exact: true })).toHaveCount(0)
  await expect(rail.getByRole('link', { name: 'Administration', exact: true })).toHaveCount(0)
})

test('a browser asks a Mastodon instance about extensions once, then remembers', async ({ page }) => {
  await stubMastodon(page)
  await signedIn(page)
  const asked = []
  page.on('request', (request) => {
    // Only the account's instance is the question here: this site's own engine probe is a
    // different fact and is asked on every load.
    if (request.url().includes(`${MASTODON}/api/v1/meta`)) asked.push(request.url())
  })

  await page.goto('/home')
  await hydrated(page)
  await expect(page.getByRole('tab')).toHaveText(['Home', 'Local', 'Federated'])
  const first = asked.length

  await page.reload()
  await hydrated(page)
  await expect(page.getByRole('tab')).toHaveText(['Home', 'Local', 'Federated'])
  // The answer is kept beside the session: a refused request on every load would be a
  // question asked over and over with the same answer.
  expect(asked.length).toBe(first)
  expect(await page.evaluate(() => localStorage.getItem('nodus-extensions/1'))).toContain('no')
})


test('an instance that cannot confirm the session says so and leaves the rail', async ({ page }) => {
  await stubMastodon(page)
  // Mastodon's API answering 500 is what a broken deployment looks like from here.
  await page.route('**/api/v1/accounts/verify_credentials', (route) =>
    route.fulfill({ status: 500, headers: { 'access-control-allow-origin': '*' }, body: 'down' }),
  )

  await signedIn(page)
  await page.goto('/home')
  await hydrated(page)

  const card = page.locator('[data-trouble]')
  await expect(card).toBeVisible({ timeout: 15_000 })
  await expect(card).toContainText('could not confirm the saved session')
  await expect(card.getByRole('link', { name: /Switch account/ })).toBeVisible()
})
