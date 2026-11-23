//! Notifications: what the instance has told this reader, and what they can do about it.
//!
//! The moderation notice is the one row with a consequence: it can be appealed, and the
//! answer has to be readable after the decision. The rest is grouping and unread state,
//! which the instance does not tell us, so the client folds and remembers it itself.

import { expect, test } from '@playwright/test'

import { CORS, account, hydrated, notification, signedIn, status, stubApi } from '../fixtures.mjs'

/** A moderation notice, shaped like the server's `NotificationDto`. */
function warning(overrides = {}) {
  return {
    id: '7001',
    type: 'moderation',
    created_at: '2026-11-18T09:00:00.000Z',
    account,
    note: 'A moderator has suspended this account.',
    moderation: {
      action: 'suspend',
      reason: 'Repeated harassment in replies.',
      rule: 'Be kind to each other.',
      expires_at: '2026-12-02T09:00:00.000Z',
      can_appeal: true,
      appeal: null,
    },
    group_key: 'moderation:7001',
    group_count: 1,
    ...overrides,
  }
}

test('an instance with nothing for this account says exactly that', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  await signedIn(page)
  await page.goto('/notifications')
  await hydrated(page)

  await expect(page.getByText('This instance has nothing for you yet.')).toBeVisible()
})

test('a moderation notice can be appealed, and then waits for a decision', async ({ page }) => {
  await stubApi(page, { notifications: [warning()] })
  const posted = []
  await page.route('**/api/v1/appeals', (route) => {
    if (route.request().method() !== 'POST') return route.fulfill({ json: [] })
    posted.push(JSON.parse(route.request().postData() ?? '{}'))
    return route.fulfill({
      json: {
        id: '9001',
        action: 'suspend',
        reason: 'I was quoting the post, not harassing anyone.',
        status: 'pending',
        decision_note: null,
        created_at: '2026-11-20T08:00:00.000Z',
        decided_at: null,
      },
    })
  })
  await signedIn(page)
  await page.goto('/notifications')
  await hydrated(page)

  // What happened, why, under which rule, and until when — all four before any button.
  await expect(page.getByText('The account was suspended.')).toBeVisible()
  await expect(page.getByText('Reason: Repeated harassment in replies.')).toBeVisible()
  await expect(page.getByText('Rule: Be kind to each other.')).toBeVisible()
  await expect(page.getByText('Expires on 2026-12-02.')).toBeVisible()

  await page.getByRole('button', { name: 'Appeal' }).click()

  await expect(page.getByText('Your appeal is waiting for a decision')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Appeal' })).toHaveCount(0)
  expect(posted).toHaveLength(1)
  expect(posted[0].reason.length).toBeGreaterThan(0)
})

test('a decided appeal names the decision and who made it', async ({ page }) => {
  await stubApi(page, {
    notifications: [
      warning({
        moderation: {
          action: 'suspend',
          reason: 'Repeated harassment in replies.',
          rule: 'Be kind to each other.',
          expires_at: null,
          can_appeal: false,
          appeal: {
            id: '9001',
            action: 'suspend',
            reason: 'I was quoting the post, not harassing anyone.',
            status: 'accepted',
            decision_note: 'Read the thread, lifted the suspension.',
            created_at: '2026-11-18T10:00:00.000Z',
            decided_at: '2026-11-19T10:00:00.000Z',
            decided_by: 'ada',
          },
        },
      }),
    ],
  })
  await signedIn(page)
  await page.goto('/notifications')
  await hydrated(page)

  await expect(page.getByText('Accepted: the suspension was lifted.')).toBeVisible()
  await expect(page.getByText('Decided by ada on 2026-11-19.')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Appeal' })).toHaveCount(0)
})

test('favourites on one post collapse into a single row with and 2 others', async ({ page }) => {
  const rows = ['5101', '5102', '5103'].map((id) => notification({ id, type: 'favourite' }))
  for (const [index, row] of rows.entries()) {
    row.group_key = 'favourite:100'
    row.group_count = 1
    row.created_at = new Date(Date.UTC(2026, 10, 18, 9, index)).toISOString()
  }
  await stubApi(page, { statuses: [status()], notifications: rows })
  await signedIn(page)
  await page.goto('/notifications')
  await hydrated(page)

  await expect(page.locator('[data-notice-kind="favourite"]')).toHaveCount(1)
  await expect(page.getByText('and 2 others')).toBeVisible()
  await expect(page.locator('[data-unread-dot]')).toHaveCount(1)

  await page.getByRole('button', { name: 'Mark all read' }).click()

  await expect(page.locator('[data-unread-dot]')).toHaveCount(0)
})

test('an instance without the notifications API keeps the door and says why', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  await page.route('**/api/v1/notifications**', (route) =>
    route.fulfill({ status: 404, body: 'not found' }),
  )
  await signedIn(page)
  await page.goto('/notifications')
  await hydrated(page)

  // A failed read is not a capability answer: the rail keeps the ordinary catalogue door,
  // the screen explains why it cannot be served, and only the bell (which needs a count)
  // stays away.
  await expect(page.getByText('This instance does not serve notifications.')).toBeVisible()
  // The shell names its navigation differently at desktop and phone widths; both keep the
  // ordinary door, so read the link from whichever navigation this viewport drew.
  await expect(
    page.getByRole('navigation').getByRole('link', { name: 'Notifications' }),
  ).toBeVisible()
  await expect(page.locator('[data-notice-door]')).toHaveCount(0)
})

test('clearing the list asks first, and a refusal keeps every row', async ({ page }) => {
  await stubApi(page, { notifications: [warning()] })
  const cleared = []
  await page.route('**/api/v1/notifications/clear', (route) => {
    cleared.push(route.request().method())
    return route.fulfill({ headers: CORS, json: {} })
  })
  await signedIn(page)
  await page.goto('/notifications')
  await hydrated(page)

  await page.locator('[data-clear-all]').click()
  await expect(page.locator('[data-clear-question]')).toBeVisible()
  await expect(page.locator('[data-notice-kind]')).toHaveCount(1)

  await page.locator('[data-clear-cancel]').click()
  await expect(page.locator('[data-clear-question]')).toHaveCount(0)
  expect(cleared).toEqual([])

  await page.locator('[data-clear-all]').click()
  await page.locator('[data-clear-confirm]').click()

  await expect(page.getByText('This instance has nothing for you yet.')).toBeVisible()
  await expect(page.locator('[data-notice-kind]')).toHaveCount(0)
  expect(cleared).toEqual(['POST'])
})
