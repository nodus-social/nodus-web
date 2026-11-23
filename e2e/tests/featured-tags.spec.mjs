//! The featured-tag editor: the instance offers names, the reader takes one.

import { expect, test } from '@playwright/test'

import { CORS, hydrated, signedIn, stubApi } from '../fixtures.mjs'

test('the instance offers tags, and taking one features it', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  const featured = []
  await page.route('**/api/v1/featured_tags/suggestions', (route) =>
    route.fulfill({
      headers: CORS,
      json: [
        { id: 'rust', name: 'rust', statuses_count: 12, last_status_at: null },
        { id: 'leptos', name: 'leptos', statuses_count: 4, last_status_at: null },
      ],
    }),
  )
  await page.route('**/api/v1/featured_tags', (route) => {
    if (route.request().method() === 'POST') {
      // The SDK sends a form for this endpoint, as Mastodon's `POST /featured_tags` is
      // specified with `name` as a form field.
      featured.push(Object.fromEntries(new URLSearchParams(route.request().postData() ?? '')))
      return route.fulfill({
        headers: CORS,
        json: { id: '1', name: 'rust', statuses_count: 12, last_status_at: null },
      })
    }
    return route.fulfill({ headers: CORS, json: [] })
  })
  await signedIn(page)
  await page.goto('/settings/featured-tags')
  await hydrated(page)

  await expect(page.locator('[data-tag-suggestions]')).toBeVisible()
  await expect(page.locator('[data-tag-suggestion]')).toHaveCount(2)

  await page.locator('[data-tag-suggestion]', { hasText: '#rust' }).click()

  await expect(page.locator('[data-tag-suggestion]')).toHaveCount(1)
  expect(featured).toEqual([{ name: 'rust' }])
})

test('an instance without suggestions leaves the form as it was', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  await page.route('**/api/v1/featured_tags/suggestions', (route) =>
    route.fulfill({ status: 404, headers: CORS, body: 'not found' }),
  )
  await page.route('**/api/v1/featured_tags', (route) =>
    route.fulfill({ headers: CORS, json: [] }),
  )
  await signedIn(page)
  await page.goto('/settings/featured-tags')
  await hydrated(page)

  await expect(page.locator('[data-tag-suggestions]')).toHaveCount(0)
  await expect(page.getByPlaceholder('rust')).toBeVisible()
})
