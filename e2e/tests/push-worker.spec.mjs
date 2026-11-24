//! The service worker's half of Web Push: a `push` event becomes a notification, and a
//! `notificationclick` opens the payload's URL.
//!
//! Delivery is already proven elsewhere (`nodus-backend/crates/api/tests/push.rs` decrypts
//! what the sender emits, against stub push services), and settings prove the client
//! subscribes (`push.spec.mjs`). What neither reaches is the worker itself: no FCM project
//! and no Mozilla autopush endpoint exist in this lab, so no push service ever delivers
//! here.
//!
//! So the event SOURCE is faked and the worker CODE is real. `ServiceWorker.deliverPushMessage`
//! is Chromium's own delivery path — the browser builds the `PushEvent` and dispatches it to
//! the registered worker — and `/sw.js` is the same `crates/web-server/assets/sw.js` the SSR
//! host embeds (`serve.mjs` serves it from that path). `showNotification` is real too: the
//! spec runs the full Chromium build (`channel: 'chromium'`; the headless shell answers
//! `Notification.permission === 'denied'` and rejects with "No notification permission has
//! been granted for this origin") and asserts that every call resolves.
//!
//! What is NOT asserted is the browser's notification LIST. Chromium's manager drops or
//! misreports banners once several browsers paint at the same origin concurrently — every
//! dropped case still showed `showNotification` resolving, with no rejection — and headless
//! CI runs no desktop notification daemon at all. Gating on `getNotifications()` therefore
//! flakes locally (~5% of runs at twelve workers) and fails outright on CI, so the assertions
//! stay at the boundary the worker controls: the exact title and options handed to the real
//! `showNotification`. For the repeat case that is the tag, which is precisely what makes
//! Chromium replace one banner instead of stacking a second (same origin + same tag replaces);
//! the test asserts the worker reuses the tag for one target and does not for another.
//!
//! Two platform calls are stubbed: `registration.showNotification` is wrapped to record the
//! raw options — `Notification` exposes neither `renotify` nor the option object — and then
//! calls the real method; `clients.openWindow` records the target instead of navigating,
//! because a synthetic event carries no transient user activation and the real method
//! answers "InvalidAccessError: Not allowed to open a window" before any navigation starts.
//! The notification handed to `notificationclick` is synthesized for the same reason the list
//! is not read: `NotificationEvent` requires a real `Notification`, and no real one can be
//! obtained reliably. It carries the same `data.url` a delivered notification would.

import { expect, test } from '@playwright/test'

/** The static server's origin, and the scope `/sw.js` is registered at (`serve.mjs` serves
 * the file at the root, which is what the SSR host's `service-worker-allowed` header means). */
const ORIGIN = 'http://127.0.0.1:4173'
const SCOPE = `${ORIGIN}/`

/** A mention, shaped exactly as `payload_for` in `nodus-backend/crates/api/src/webpush.rs`
 * builds one: the tag carries the status id, `renotify` is present because another mention
 * of the same post is news, and the deep link is the status permalink. */
const MENTION = {
  title: 'Ada',
  body: 'did you see this?',
  tag: 'mention:100',
  data: { url: '/@ada/100' },
  icon: '/icon.png',
  renotify: true,
}

/** Another sender pass over the same notification: same tag and target, changed words. */
const MENTION_AGAIN = { ...MENTION, body: 'a second pass' }

/** A mention on a different status — the case a shared tag would swallow. */
const OTHER_MENTION = { ...MENTION, body: 'a second post', tag: 'mention:101', data: { url: '/@ada/101' } }

/** A follow, shaped as the sender builds it: `renotify` is literally absent, because a
 * second notice about one follower repeats one fact. */
const FOLLOW = {
  title: 'Grace',
  body: 'You have a new follower',
  tag: 'follow:2',
  data: { url: '/@grace' },
  icon: '/icon.png',
}

// The full Chromium build, not the headless shell: only it can grant notification permission.
// The channel is set here rather than in playwright.config.mjs so the rest of the suite keeps
// the cheaper shell. The browser also gets no session bus, so its notifications stay in its
// own manager instead of popping up on the developer's desktop.
test.use({
  channel: 'chromium',
  serviceWorkers: 'allow',
  launchOptions: { env: { ...process.env, DBUS_SESSION_BUS_ADDRESS: 'unix:path=/nonexistent' } },
})

/**
 * Register the real worker and open the browser's push-delivery channel to it.
 *
 * `ServiceWorker.enable` reports the registration id that `deliverPushMessage` addresses.
 * The event fires for existing registrations too, so enabling after the registration is
 * enough; the id is polled rather than read straight away because CDP events arrive on the
 * session's socket.
 */
async function workerWithPush(page, context) {
  await context.grantPermissions(['notifications'], { origin: ORIGIN })
  const cdp = await context.newCDPSession(page)
  const registrations = []
  cdp.on('ServiceWorker.workerRegistrationUpdated', (event) => {
    registrations.push(
      ...event.registrations.filter(
        (registration) => !registration.isDeleted && registration.scopeURL === SCOPE,
      ),
    )
  })

  await page.goto('/sw.js')
  const scope = await page.evaluate(async () => {
    const registration = await navigator.serviceWorker.register('/sw.js')
    await navigator.serviceWorker.ready
    return registration.scope
  })
  expect(scope).toBe(SCOPE)

  const worker = context.serviceWorkers().at(0) ?? (await context.waitForEvent('serviceworker'))
  expect(await worker.evaluate(() => Notification.permission)).toBe('granted')
  await cdp.send('ServiceWorker.enable')
  await expect.poll(() => registrations.length, { timeout: 15_000 }).toBeGreaterThan(0)

  return { worker, cdp, registrationId: registrations.at(-1).registrationId }
}

/**
 * Observe the two platform calls the worker makes.
 *
 * Both are properties of a platform object the worker resolves at event time, so defining
 * them on the instance shadows the prototype without touching the worker's own code.
 */
async function recordPlatformCalls(worker) {
  await worker.evaluate(() => {
    self.__pushRecorder = { shown: [], opened: [], closed: 0, errors: [] }
    const registration = self.registration
    const show = registration.showNotification.bind(registration)
    Object.defineProperty(registration, 'showNotification', {
      configurable: true,
      value: (title, options = {}) => {
        // Structured-clone the options now: a live object would let a later mutation in
        // the worker rewrite what the test saw.
        self.__pushRecorder.shown.push({ title, options: JSON.parse(JSON.stringify(options)) })
        return show(title, options).catch((error) => {
          self.__pushRecorder.errors.push(String(error))
          throw error
        })
      },
    })
    Object.defineProperty(self.clients, 'openWindow', {
      configurable: true,
      value: (url) => {
        self.__pushRecorder.opened.push(url)
        return Promise.resolve(null)
      },
    })
  })
}

/** Dispatch a push event through Chromium's own delivery path. */
async function deliver(cdp, registrationId, payload) {
  await cdp.send('ServiceWorker.deliverPushMessage', {
    origin: ORIGIN,
    registrationId,
    data: JSON.stringify(payload),
  })
}

const recorder = (worker) => worker.evaluate(() => self.__pushRecorder)

/** Wait for the worker to have handed the browser `count` notifications. */
async function shown(worker, count) {
  await expect
    .poll(async () => (await recorder(worker)).shown.length, { timeout: 15_000 })
    .toBe(count)
  const { shown: made, errors } = await recorder(worker)
  // The wrapped call goes through to the real platform method: a rejection here would mean
  // the browser refused the notification, which the pass-through assertions would hide.
  expect(errors).toEqual([])
  return made
}

test('a push event shows the sender’s title, body, tag and deep link', async ({ page, context }) => {
  const { worker, cdp, registrationId } = await workerWithPush(page, context)
  await recordPlatformCalls(worker)

  await deliver(cdp, registrationId, MENTION)
  const [handed] = await shown(worker, 1)

  // Exactly what the worker called the real showNotification with: the sender's answers,
  // decoded, including the icon and the deep link the click handler will read.
  expect(handed).toEqual({
    title: 'Ada',
    options: {
      body: 'did you see this?',
      tag: 'mention:100',
      // The sender said a replacement is news; the worker passes that through.
      renotify: true,
      data: { url: '/@ada/100' },
      icon: '/icon.png',
    },
  })
})

test('one tag per target: a repeat reuses the tag, a different status does not', async ({
  page,
  context,
}) => {
  const { worker, cdp, registrationId } = await workerWithPush(page, context)
  await recordPlatformCalls(worker)

  await deliver(cdp, registrationId, MENTION)
  await shown(worker, 1)
  await deliver(cdp, registrationId, MENTION_AGAIN)
  const repeated = await shown(worker, 2)

  // Same tag and origin is what makes the browser replace the banner instead of stacking a
  // second one; the newer body still reaches the browser, so the visible words update.
  expect(repeated.map((call) => call.options.tag)).toEqual(['mention:100', 'mention:100'])
  expect(repeated.map((call) => call.options.body)).toEqual(['did you see this?', 'a second pass'])

  // A mention on another status must not share the tag, or it would replace this one.
  await deliver(cdp, registrationId, OTHER_MENTION)
  const different = await shown(worker, 3)
  expect(different.at(-1).options.tag).toBe('mention:101')
  expect(different.at(-1).options.data).toEqual({ url: '/@ada/101' })
})

test('a click opens the notification’s own URL and closes the banner', async ({ page, context }) => {
  const { worker, cdp, registrationId } = await workerWithPush(page, context)
  await recordPlatformCalls(worker)
  await deliver(cdp, registrationId, MENTION)
  await shown(worker, 1)

  // `notificationclick` has no CDP delivery method, so the event is built inside the worker
  // around a notification carrying the deep link the push payload specified. `ExtendableEvent`
  // is the real base class (so the handler's `waitUntil` is the platform one); only the
  // `notification` object is shaped here, because `NotificationEvent` requires a real
  // `Notification` and the browser's list is not readable reliably (see the file header).
  await worker.evaluate(() => {
    const notification = {
      data: { url: '/@ada/100' },
      close: () => {
        self.__pushRecorder.closed += 1
      },
    }
    const event = new ExtendableEvent('notificationclick')
    Object.defineProperty(event, 'notification', { value: notification })
    self.dispatchEvent(event)
  })

  await expect.poll(async () => (await recorder(worker)).opened.length).toBe(1)
  const final = await recorder(worker)
  expect(final.closed).toBe(1)
  // `openWindow` resolves the deep link against the registration scope, as the worker does.
  expect(final.opened).toEqual([`${ORIGIN}/@ada/100`])
})

test('a follow payload’s missing renotify is passed through, not invented', async ({
  page,
  context,
}) => {
  const { worker, cdp, registrationId } = await workerWithPush(page, context)
  await recordPlatformCalls(worker)

  await deliver(cdp, registrationId, FOLLOW)
  const [handed] = await shown(worker, 1)

  // The sender omits `renotify` for a follow, so the worker's `=== true` leaves it false:
  // a replacement is silent. `renotify` is not exposed on `Notification`, so the wrapped
  // call is where it is read.
  expect(handed.options).toHaveProperty('renotify', false)
  expect(handed.options.tag).toBe('follow:2')
  expect(handed.options.data).toEqual({ url: '/@grace' })
  expect(handed.title).toBe('Grace')
  expect(handed.options.body).toBe('You have a new follower')
})
