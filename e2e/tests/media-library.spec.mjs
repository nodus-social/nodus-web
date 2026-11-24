//! The media library: what each picture is used by, and the one delete.
//!
//! `GET /api/v1/media/library` answers each row with a `usage`, and `DELETE` refuses
//! anything but `none` with a 422 naming what still holds it. These tests make the instance
//! answer three rows — one free, one on a post, one that goes stale between the read and the
//! click — and assert that the button exists exactly where the server would accept it and
//! that the refusal lands in the row it happened to.
//!
//! The screen has no tab (the handshake publishes no name for `/api/v1/media/library`), so
//! every test opens it by its address, which is how the screen is reached at all.

import { expect, test } from '@playwright/test'

import { hydrated, signedIn, stubApi } from '../fixtures.mjs'

/** When a picture uploaded three and a half hours ago reads as. */
const UPLOADED = new Date(Date.now() - (3 * 60 + 30) * 60_000).toISOString()

function row(page, description) {
  return page.locator('li').filter({ hasText: description })
}

/** The three rows the library answers, and one delete that the server refuses. */
function library() {
  return {
    unused: {
      id: '7',
      url: 'https://files.example/7.png',
      preview_url: 'https://files.example/7_small.png',
      description: 'A red bicycle',
      created_at: UPLOADED,
      usage: 'none',
      usage_id: null,
      duplicate_of: null,
    },
    used: {
      id: '8',
      url: 'https://files.example/8.png',
      preview_url: 'https://files.example/8_small.png',
      description: 'A green door',
      created_at: UPLOADED,
      usage: 'status',
      usage_id: '100',
      duplicate_of: null,
    },
    stale: {
      id: '9',
      url: 'https://files.example/9.png',
      preview_url: 'https://files.example/9_small.png',
      description: 'A blue boat',
      created_at: UPLOADED,
      usage: 'none',
      usage_id: null,
      duplicate_of: '7',
    },
  }
}

/**
 * Open the screen with the three rows stubbed.
 *
 * The library routes are registered after `stubApi`, so they win over its catch-all: the
 * stub's last word answers `/api/**` with `[]`, which would silently empty the screen.
 */
async function open(page) {
  const { unused, used, stale } = library()
  const deletes = []
  await stubApi(page)
  await page.route('**/api/v1/media/library**', (route) => {
    const request = route.request()
    if (request.method() === 'DELETE') {
      const path = new URL(request.url()).pathname
      deletes.push(path)
      // The boat has been attached to a post since the read, so the server names what
      // holds it and refuses. Everything else goes, which is what the button promised.
      if (path.endsWith('/9')) {
        return route.fulfill({
          status: 422,
          headers: { 'access-control-allow-origin': '*' },
          json: { error: 'still used by a post' },
        })
      }
      return route.fulfill({ status: 204, headers: { 'access-control-allow-origin': '*' } })
    }
    return route.fulfill({
      headers: { 'access-control-allow-origin': '*' },
      json: [unused, used, stale],
    })
  })
  await signedIn(page)
  await page.goto('/settings/media')
  await hydrated(page)
  return deletes
}

test('every row shows its picture, description, upload age and use in words', async ({ page }) => {
  await open(page)

  const free = row(page, 'A red bicycle')
  await expect(free.getByRole('img')).toHaveAttribute('src', 'https://files.example/7_small.png')
  await expect(free).toContainText('Uploaded 3h ago')
  await expect(free).toContainText('not used anywhere')

  await expect(row(page, 'A green door')).toContainText('posted')
})

test('a used picture offers no delete and says what holds it', async ({ page }) => {
  await open(page)

  const used = row(page, 'A green door')
  await expect(used.getByRole('button', { name: 'Delete' })).toHaveCount(0)
  await expect(used).toContainText('Cannot be deleted: still used by a post.')

  // The free row is the one that offers the button.
  await expect(row(page, 'A red bicycle').getByRole('button', { name: 'Delete' })).toBeVisible()
})

test('the count names the library and the filter narrows it', async ({ page }) => {
  await open(page)

  await expect(page.getByText('3 pictures, 2 unused')).toBeVisible()

  await page.getByRole('button', { name: 'posted', exact: true }).click()
  await expect(page.locator('li').filter({ hasText: 'A green door' })).toBeVisible()
  await expect(page.locator('li').filter({ hasText: 'A red bicycle' })).toHaveCount(0)

  await page.getByRole('button', { name: 'All', exact: true }).click()
  await expect(page.locator('li').filter({ hasText: 'A red bicycle' })).toBeVisible()
})

test('deleting an unused picture sends the delete and the row goes', async ({ page }) => {
  const deletes = await open(page)

  await row(page, 'A red bicycle').getByRole('button', { name: 'Delete' }).click()

  await expect(row(page, 'A red bicycle')).toHaveCount(0)
  // Still the other two: the deleted row went, the list did not.
  await expect(row(page, 'A green door')).toBeVisible()
  expect(deletes).toEqual(['/api/v1/media/library/7'])
})

test('a delete the server refuses is shown in that row, not as a toast', async ({ page }) => {
  const deletes = await open(page)

  await row(page, 'A blue boat').getByRole('button', { name: 'Delete' }).click()

  // The 422 names what still uses it, and the sentence lands in the row it is about.
  await expect(row(page, 'A blue boat').getByRole('alert')).toHaveText('still used by a post')
  await expect(row(page, 'A blue boat')).toBeVisible()
  // Exactly one alert, in this row: the green door's own "Cannot be deleted" line is the
  // static reason, and a refusal is never repeated anywhere else on the screen.
  await expect(page.getByRole('alert')).toHaveCount(1)
  await expect(page.getByRole('status')).toHaveCount(0)

  expect(deletes).toEqual(['/api/v1/media/library/9'])
})

test('the settings strip offers no media tab, and a direct visit works', async ({ page }) => {
  await open(page)

  await expect(page.locator('a[role="tab"][href="/settings/media"]')).toHaveCount(0)
  await expect(page.locator('a[role="tab"]').first()).toBeVisible()
})
