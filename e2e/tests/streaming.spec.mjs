//! Live updates: this instance pushes, and the page must not need a reload.
//
// The wire is plain SSE (`event:` + `data:` lines) on `/api/v1/streaming`. Playwright can
// answer that with a finished body, so every test here hands the client an exact sequence
// of frames and then watches the screen.
//
// One rule runs through all of them: the stream is opened *after* the page has painted.
// `route.fulfill` completes the body at once, and a frame that lands before the first list
// fetch would be overwritten by it — the test would then be measuring the fetch, not the
// stream.

import { expect, test } from '@playwright/test'
import { hydrated, notification, signedIn, status, stubApi } from '../fixtures.mjs'

/** The live endpoint, query string included. */
const STREAM = '**/api/v1/streaming**'

/** An SSE body: each frame as the server writes it, then the connection ends. */
function sse(...frames) {
  return frames.map(([event, data]) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('')
}

/** The headers the server sends, so the client sees the shape it sees in production. */
function stream(body) {
  return {
    status: 200,
    headers: {
      'content-type': 'text/event-stream',
      'cache-control': 'no-cache',
      'x-accel-buffering': 'no',
      'access-control-allow-origin': '*',
    },
    body,
  }
}

/** Records the requests the client makes to the live endpoint. */
function watchStreams(page) {
  const urls = []
  page.on('request', (request) => {
    if (request.url().includes('/api/v1/streaming')) urls.push(request.url())
  })
  return urls
}

test('a pushed notification appears without a reload', async ({ page }) => {
  const streams = watchStreams(page)
  const reads = []
  page.on('request', (request) => {
    if (request.url().includes('/api/v1/notifications')) reads.push(request.url())
  })
  await stubApi(page, { notifications: [] })

  // Held until the empty list is on screen: see the note at the top of the file.
  let held
  await page.route(STREAM, (route) => {
    held = route
  })

  await signedIn(page)
  await page.goto('/notifications')
  await hydrated(page)
  await expect(page.getByText('This instance has nothing for you yet.')).toBeVisible()

  // The same notification twice: a reconnect replaying the backlog must not double the row.
  const pushed = notification({ id: '6001', type: 'favourite' })
  await held.fulfill(stream(sse(['notification', pushed], ['notification', pushed])))

  await expect(page.locator('[data-notice-id="6001"]')).toBeVisible()
  await expect(page.locator('[data-notice-id="6001"]')).toHaveCount(1)
  // One read of the list: the row came down the stream, not from a second poll.
  expect(reads).toHaveLength(1)
  expect(streams).toHaveLength(1)
})

test('a deleted status leaves the list that was showing it', async ({ page }) => {
  await stubApi(page)
  let held
  await page.route(STREAM, (route) => {
    held = route
  })

  await signedIn(page)
  await page.goto('/home')
  await hydrated(page)
  await expect(page.getByText('Hello from the fixture.')).toBeVisible()

  // `delete` carries the status id as a JSON string, exactly as the server frames it.
  await held.fulfill(stream(sse(['delete', status().id])))
  await expect(page.getByText('Hello from the fixture.')).toHaveCount(0)
})

test('an instance without our capability never opens a stream', async ({ page }) => {
  const streams = watchStreams(page)
  await stubApi(page)
  // A Mastodon instance answers 404 here; that is the whole difference the client acts on.
  // Its streaming is a WebSocket, so a reader there keeps polling.
  await page.route('**/api/v1/meta', (route) =>
    route.fulfill({
      status: 404,
      headers: { 'content-type': 'application/json', 'access-control-allow-origin': '*' },
      body: JSON.stringify({ error: 'Record not found' }),
    }),
  )

  await signedIn(page)
  const answered = page.waitForResponse((response) => response.url().includes('/api/v1/meta'))
  await page.goto('/notifications')
  await hydrated(page)
  await answered
  // One more frame: the capability answer reaches the gate through an effect.
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))))

  expect(streams).toHaveLength(0)
  // And polling still works there: the list was read, and it is empty as stubbed.
  await expect(page.getByText('This instance has nothing for you yet.')).toBeVisible()
})

test('a stream that fails does not stop the page from polling', async ({ page }) => {
  const attempts = []
  await stubApi(page, { notifications: [notification({ id: '5002' })] })
  await page.route(STREAM, (route) => {
    attempts.push(route.request().url())
    return route.fulfill({
      status: 500,
      headers: { 'content-type': 'application/json', 'access-control-allow-origin': '*' },
      body: '{}',
    })
  })

  await signedIn(page)
  await page.goto('/notifications')
  await hydrated(page)

  await expect(page.locator('[data-notice-id="5002"]')).toBeVisible()
  // It tried, failed, and tried again a backoff later rather than giving up.
  await expect.poll(() => attempts.length, { timeout: 5_000 }).toBeGreaterThan(1)
})

test('a refused stream is not retried', async ({ page }) => {
  const attempts = []
  await stubApi(page)
  // Signed in, but the instance refuses the token: only `sign_in` reads this endpoint, so
  // answering 401 here tells the client what it would learn from the server itself.
  await page.route('**/api/v1/accounts/verify_credentials', (route) =>
    route.fulfill({
      status: 401,
      headers: { 'content-type': 'application/json', 'access-control-allow-origin': '*' },
      body: JSON.stringify({ error: 'The access token is invalid' }),
    }),
  )
  await page.route(STREAM, (route) => {
    attempts.push(route.request().url())
    return route.fulfill({
      status: 401,
      headers: { 'content-type': 'application/json', 'access-control-allow-origin': '*' },
      body: JSON.stringify({ error: 'The access token is invalid' }),
    })
  })

  await signedIn(page)
  await page.goto('/home')
  await hydrated(page)
  await expect.poll(() => attempts.length, { timeout: 5_000 }).toBe(1)

  // A retry would have happened by now: the first window is one second, the second two.
  // An absence needs a window to be absent in, so this wait is the measurement.
  await page.waitForTimeout(4_000)
  expect(attempts).toHaveLength(1)
})

test('one stream per account, however many times the shell mounts', async ({ page }) => {
  const streams = watchStreams(page)
  await stubApi(page)
  // Never fulfilled: the connection stays open, so a second one would be visible as a
  // second request rather than as a reconnect.
  let held
  await page.route(STREAM, (route) => {
    held = route
  })

  await signedIn(page)
  await page.goto('/notifications')
  await hydrated(page)
  await expect.poll(() => streams.length, { timeout: 5_000 }).toBe(1)

  // Two in-app navigations. In-app matters: a `goto` reloads the page, and a reload is
  // allowed to open its own stream. A click is the thing a reader does all day. The phone
  // shell keeps its own copy of both links in the document, so the visible one is the door.
  await page.locator('a[href$="/home"]:visible').first().click()
  await expect(page.getByText('Hello from the fixture.')).toBeVisible()
  await page.locator('a[href$="/notifications"]:visible').first().click()
  await expect(page.getByText('This instance has nothing for you yet.')).toBeVisible()

  expect(streams).toHaveLength(1)
  // The held route is still the first connection, still open, and still the only one.
  expect(held).toBeTruthy()
})
