//! Sessions: the browsers signed in to the account, and ending one of them.
//!
//! The list is what the server says, rendered from the agent string down to a name a person
//! can pick out, with the current browser left alone. The DELETE assertions count requests
//! rather than rows so a second, accidental revoke cannot hide behind a repaint.

import { expect, test } from '@playwright/test'

import { CORS, FEATURES, hydrated, signedIn, stubApi } from '../fixtures.mjs'

/** A signed-in browser, shaped like the server's session row. */
function session(overrides = {}) {
  return {
    id: '11',
    created_at: '2026-11-01T09:00:00.000Z',
    last_used_at: '2026-11-23T09:00:00.000Z',
    user_agent:
      'Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0',
    ip: '203.0.113.0/24',
    current: false,
    ...overrides,
  }
}

const CURRENT = session({
  id: '12',
  current: true,
  last_used_at: null,
  user_agent:
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15',
  ip: '198.51.100.0/24',
})

const OTHER = session({ id: '11' })

/** Answers the session verbs and records every DELETE the client sends. */
async function sessionsApi(page, revokes) {
  await page.route('**/api/v1/auth/sessions**', (route) => {
    const request = route.request()
    const url = new URL(request.url())
    if (request.method() === 'GET') {
      return route.fulfill({ headers: CORS, json: [CURRENT, OTHER] })
    }
    if (request.method() === 'DELETE') {
      revokes.push(url.pathname + url.search)
      return route.fulfill({ status: 204, headers: CORS })
    }
    return route.fulfill({ status: 405, headers: CORS })
  })
}

test('two sessions render by name, and only the other one can be revoked', async ({ page }) => {
  const revokes = []
  await stubApi(page)
  await sessionsApi(page, revokes)
  await signedIn(page)
  await page.goto('/settings/sessions')
  await hydrated(page)

  const rows = page.locator('[data-session-row]')
  await expect(rows).toHaveCount(2)
  await expect(page.getByText('Safari on macOS')).toBeVisible()
  await expect(page.getByText('Firefox on Linux')).toBeVisible()
  await expect(page.getByText('Current session')).toBeVisible()
  await expect(rows.filter({ hasText: 'Current session' })).toHaveAttribute(
    'data-current',
    'true',
  )

  // The network as served, not guessed at or widened.
  await expect(page.getByText('203.0.113.0/24')).toBeVisible()

  const current = rows.filter({ hasText: 'Current session' }).getByRole('button', {
    name: 'Revoke',
  })
  await expect(current).toBeDisabled()
  await expect(current).toHaveAttribute(
    'title',
    'You cannot revoke the session you are using.',
  )

  await rows.filter({ hasText: 'Firefox on Linux' }).getByRole('button', { name: 'Revoke' }).click()

  await expect(rows).toHaveCount(1)
  expect(revokes).toEqual(['/api/v1/auth/sessions/11'])
})

test('a server without the capability explains itself instead of an empty list', async ({
  page,
}) => {
  const revokes = []
  await stubApi(page, { features: FEATURES.filter((name) => name !== 'sessions') })
  await sessionsApi(page, revokes)
  await signedIn(page)
  await page.goto('/settings/sessions')
  await hydrated(page)

  await expect(
    page.getByText('This instance does not serve a list of signed-in sessions.'),
  ).toBeVisible()
  await expect(page.locator('a[role="tab"][href="/settings/sessions"]')).toHaveCount(0)
  await expect(page.locator('[data-session-row]')).toHaveCount(0)
  expect(revokes).toEqual([])
})
