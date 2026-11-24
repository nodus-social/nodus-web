//! Browser push: one subscription, one PUT per toggle, and the delete that comes first.
//!
//! No real push service is reachable from this lab — there is no FCM project and no Mozilla
//! autopush endpoint behind the fixture hostname — so every test here fakes the browser side
//! (`PushManager`, `navigator.serviceWorker`) and the server side (routes). That is honest
//! about what is being measured: the *order* of the client's requests and what it sends in
//! them. Whether a push actually renders a notification is the worker's own business and is
//! not observable from this harness.
//!
//! The VAPID key has two sources and the tests cover both, because the deployed instance and
//! the next one differ: `vapid_key` on the credentials document is where the key is going
//! (Mastodon's own location), and `server_key` on the subscription row is what an account
//! subscribed on another device already has. The last test is the account neither source
//! answers for, which must say so instead of offering a toggle that cannot work.

import { expect, test } from '@playwright/test'

import { FEATURES, hydrated, signedIn, status, stubApi } from '../fixtures.mjs'

/** A real-looking uncompressed P-256 point: `0x04` then 64 bytes. 65 bytes is the length
 * `applicationServerKey` insists on, so a decoder that drops or adds a byte fails loudly. */
const SERVER_KEY = Buffer.from([4, ...Array.from({ length: 64 }, (_, index) => index + 1)]).toString(
  'base64url',
)

const ENDPOINT = 'https://push.example/device-1'

/** The four kinds the sender filters on, as the server's `alerts` map spells them. */
const ALERTS = { mention: true, favourite: true, reblog: true, follow: true }

/** The row `GET /api/v1/push/subscription` answers once the account has a subscription. */
function subscription(alerts = ALERTS) {
  return { id: '1', endpoint: ENDPOINT, server_key: SERVER_KEY, alerts }
}

/**
 * Fake the browser's push interfaces.
 *
 * `Object.create(ServiceWorkerRegistration.prototype)` rather than a plain object: the client
 * checks the answer with `instanceof`, and a literal would be refused before `pushManager` was
 * ever read. Every call is appended to `window.__push.log` in the order the client makes it,
 * which is how the delete-before-unbind test sees the order at all.
 */
async function fakePush(page, { subscribed = false } = {}) {
  await page.addInitScript(({ endpoint, subscribed }) => {
    window.__push = { log: [], subscribes: 0, unsubscribes: 0, key: null, script: null }

    const makeSubscription = () => {
      const subscription = Object.create(PushSubscription.prototype)
      Object.defineProperty(subscription, 'endpoint', { value: endpoint })
      subscription.getKey = (name) =>
        name === 'p256dh'
          ? Uint8Array.from([4, ...Array.from({ length: 64 }, (_, index) => index + 2)]).buffer
          : Uint8Array.from([9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9]).buffer
      subscription.unsubscribe = () => {
        window.__push.unsubscribes += 1
        window.__push.log.push('unsubscribe')
        return Promise.resolve(true)
      }
      return subscription
    }

    // `subscribed` starts the fake where a browser that subscribed on an earlier visit is:
    // the worker is registered and a subscription is already held.
    let held = subscribed ? makeSubscription() : null
    let scripted = subscribed
    const manager = {
      getSubscription: () => Promise.resolve(held),
      subscribe: (options) => {
        window.__push.subscribes += 1
        const key = options && options.applicationServerKey
        window.__push.key = key
          ? { isView: ArrayBuffer.isView(key), bytes: key.byteLength ?? key.length ?? 0 }
          : null
        held = makeSubscription()
        return Promise.resolve(held)
      },
    }

    const registration = Object.create(ServiceWorkerRegistration.prototype)
    Object.defineProperty(registration, 'pushManager', { value: manager })

    Object.defineProperty(navigator, 'serviceWorker', {
      configurable: true,
      value: {
        register: (url) => {
          window.__push.script = url
          scripted = true
          return Promise.resolve(registration)
        },
        getRegistration: () => Promise.resolve(scripted ? registration : undefined),
        ready: Promise.resolve(registration),
      },
    })
    Object.defineProperty(window, 'PushManager', {
      configurable: true,
      value: function PushManager() {},
    })
  }, { endpoint: ENDPOINT, subscribed })
}

/** A browser with neither interface, as an old Safari is. */
async function unsupportedBrowser(page) {
  await page.addInitScript(() => {
    delete window.PushManager
    delete Navigator.prototype.serviceWorker
  })
}

/**
 * The instance document with `configuration.vapid.public_key` on it — the instance as it
 * looks after `nodus-backend 6522812`, where the public key lives in the document every
 * visitor already reads. `stubApi` answers this route without a `vapid` object at all, which
 * is the same "no key" the field being omitted means, so a test opts in rather than out.
 */
async function instanceWithKey(page, { publicKey = SERVER_KEY } = {}) {
  await page.route('**/api/v2/instance', (route) =>
    route.fulfill({
      headers: { 'access-control-allow-origin': '*' },
      json: {
        domain: 'stub.instance',
        title: 'Stub instance',
        version: '4.3.0',
        configuration: {
          statuses: { max_characters: 500, max_media_attachments: 4 },
          vapid: { public_key: publicKey },
        },
      },
    }),
  )
}

/**
 * The subscription endpoint, with the verb behaviour the server actually has: 404 until a row
 * exists, `POST` stores and answers the row, `PUT` answers the row, `DELETE` answers 204.
 */
async function subscriptionApi(page, { row = null, refuse = null } = {}) {
  const calls = { gets: 0, posts: [], puts: [], deletes: [], order: [] }
  await page.route('**/api/v1/push/subscription', async (route) => {
    const request = route.request()
    const method = request.method()
    if (method === 'GET') {
      calls.gets += 1
      return row
        ? route.fulfill({ headers: { 'access-control-allow-origin': '*' }, json: row })
        : route.fulfill({
            status: 404,
            headers: { 'access-control-allow-origin': '*' },
            json: { error: 'Record not found' },
          })
    }
    if (method === 'POST') {
      calls.posts.push(request.postDataJSON())
      return route.fulfill({ headers: { 'access-control-allow-origin': '*' }, json: subscription() })
    }
    if (method === 'PUT') {
      calls.puts.push(request.postDataJSON())
      return route.fulfill({ headers: { 'access-control-allow-origin': '*' }, json: subscription() })
    }
    if (method === 'DELETE') {
      calls.deletes.push(request.postData())
      calls.order.push('delete')
      await page.evaluate(() => window.__push && window.__push.log.push('delete'))
      if (refuse) {
        return route.fulfill({
          status: 422,
          headers: { 'access-control-allow-origin': '*' },
          json: { error: refuse },
        })
      }
      return route.fulfill({ status: 204, headers: { 'access-control-allow-origin': '*' } })
    }
    return route.fallback()
  })
  return calls
}

const toggle = (page) => page.getByLabel('Push notifications in this browser')
const kind = (page, label) => page.getByLabel(label, { exact: true })

// The primary path: the instance document publishes the key, which becomes true on the lab
// after the next rebuild of `nodus-api`. Everything below runs against the fake because no
// FCM/Mozilla endpoint exists here to deliver to.
test('enabling subscribes once, with the key the instance document published', async ({
  page,
}) => {
  await fakePush(page)
  await stubApi(page, { features: [...FEATURES, 'push'], statuses: [status()] })
  await instanceWithKey(page)
  const calls = await subscriptionApi(page)
  await signedIn(page)
  await page.goto('/settings/notifications')
  await hydrated(page)

  await expect(
    page.getByRole('heading', { name: 'Notifications in this browser' }),
  ).toBeVisible()
  await expect(toggle(page)).toBeEnabled()

  await toggle(page).check()
  await expect(page.getByText('Notify me about')).toBeVisible()

  // One subscription, and the worker was registered at the path the SSR host serves.
  const faked = await page.evaluate(() => window.__push)
  expect(faked.subscribes).toBe(1)
  expect(faked.script).toBe('/sw.js')
  // The key the browser was handed is the decoded point, not the base64url text: a client
  // that skipped decoding would pass a 87-character string where 65 bytes belong.
  expect(faked.key).toEqual({ isView: true, bytes: 65 })

  expect(calls.posts).toHaveLength(1)
  expect(calls.posts[0].subscription.endpoint).toBe(ENDPOINT)
  expect(calls.posts[0].subscription.keys.p256dh).toEqual(expect.any(String))
  expect(calls.posts[0].subscription.keys.auth).toEqual(expect.any(String))
  expect(calls.posts[0].data.alerts).toEqual(ALERTS)

  // One PUT per toggle, and only the kind that changed travels: the server keeps every kind
  // the body omits, so re-sending all four would turn a checkbox into a full overwrite.
  await kind(page, 'Boosts').uncheck()
  await expect.poll(() => calls.puts.length).toBe(1)
  expect(calls.puts[0].data.alerts.reblog).toBe(false)

  await kind(page, 'Follows').uncheck()
  await expect.poll(() => calls.puts.length).toBe(2)
  expect(calls.puts[1].data.alerts.follow).toBe(false)
  expect(calls.posts).toHaveLength(1)

  // Four kinds, because four is what the sender filters on.
  await expect(page.locator('input[name="mention"]')).toHaveCount(1)
  await expect(page.locator('input[name="favourite"]')).toHaveCount(1)
  await expect(page.locator('input[name="reblog"]')).toHaveCount(1)
  await expect(page.locator('input[name="follow"]')).toHaveCount(1)
})

test('an account subscribed on another device gets its key from the subscription row', async ({
  page,
}) => {
  // No `vapid_key` anywhere: the instance as the lab runs it today, with the key reachable
  // only through the row this account already has. The browser is where a subscribed browser
  // is: worker registered, subscription held.
  await fakePush(page, { subscribed: true })
  await stubApi(page, { features: [...FEATURES, 'push'], statuses: [status()] })
  const calls = await subscriptionApi(page, { row: subscription({ ...ALERTS, reblog: false }) })
  await signedIn(page)
  await page.goto('/settings/notifications')
  await hydrated(page)

  await expect(toggle(page)).toBeEnabled()
  await expect(toggle(page)).toBeChecked()
  // The row's own alerts are what the checkboxes show, not the client's default of "all on".
  await expect(kind(page, 'Boosts')).not.toBeChecked()
  await expect(kind(page, 'Mentions')).toBeChecked()

  await toggle(page).uncheck()

  // The delete lands before the browser lets go: if the browser went first and the delete
  // then failed, the server would hold a row for an endpoint no browser has any more.
  await expect.poll(() => calls.deletes.length).toBe(1)
  // The unbind follows the answer to the delete, so it is awaited rather than read straight
  // away: a poll that only watched the delete would race the browser's own unsubscribe.
  await expect
    .poll(() => page.evaluate(() => window.__push && window.__push.unsubscribes))
    .toBe(1)
  const faked = await page.evaluate(() => window.__push)
  expect(faked.log).toEqual(['delete', 'unsubscribe'])
  // The endpoint travels in the body, because the server deletes the row it is told about.
  expect(decodeURIComponent(calls.deletes[0])).toContain(`subscription[endpoint]=${ENDPOINT}`)
})

test('a refused delete keeps the browser subscription', async ({ page }) => {
  await fakePush(page, { subscribed: true })
  await stubApi(page, { features: [...FEATURES, 'push'], statuses: [status()] })
  const refusal = 'This subscription could not be removed.'
  await subscriptionApi(page, { row: subscription(), refuse: refusal })
  await signedIn(page)
  await page.goto('/settings/notifications')
  await hydrated(page)

  await expect(toggle(page)).toBeChecked()
  await toggle(page).uncheck()

  // In place, where the toggle is — never a toast that leaves with the next render.
  // `getByText`, not `getByRole('alert')`: the tab also reports its own policy read, which is
  // a second alert on this page and not the one this test is about.
  await expect(page.getByText(refusal)).toBeVisible()
  const faked = await page.evaluate(() => window.__push)
  expect(faked.unsubscribes).toBe(0)
  // The card still says subscribed, because the server still has the row that delivers.
  await expect(toggle(page)).toBeChecked()
})

test('a client the instance published no key to is told so, and offered no toggle', async ({
  page,
}) => {
  await fakePush(page)
  await stubApi(page, { features: [...FEATURES, 'push'], statuses: [status()] })
  // No vapid_key on the credentials document, and no row: both sources answer nothing.
  await subscriptionApi(page)
  await signedIn(page)
  await page.goto('/settings/notifications')
  await hydrated(page)

  await expect(
    page.getByText('This instance did not publish a push key to this client'),
  ).toBeVisible()
  await expect(toggle(page)).toHaveCount(0)
  const faked = await page.evaluate(() => window.__push)
  expect(faked.subscribes).toBe(0)
})

test('a browser with no push support is told so in one sentence', async ({ page }) => {
  await unsupportedBrowser(page)
  await stubApi(page, { features: [...FEATURES, 'push'], statuses: [status()] })
  await instanceWithKey(page)
  await subscriptionApi(page)
  await signedIn(page)
  await page.goto('/settings/notifications')
  await hydrated(page)

  await expect(
    page.getByText('This browser cannot receive push notifications'),
  ).toBeVisible()
  await expect(toggle(page)).toHaveCount(0)
})

test('an instance that does not advertise push draws no card', async ({ page }) => {
  await fakePush(page)
  // The default feature list, which names every surface except `push`.
  await stubApi(page, { statuses: [status()] })
  await instanceWithKey(page)
  await subscriptionApi(page)
  await signedIn(page)
  await page.goto('/settings/notifications')
  await hydrated(page)

  await expect(
    page.getByRole('heading', { name: 'Notifications in this browser' }),
  ).toHaveCount(0)
})
