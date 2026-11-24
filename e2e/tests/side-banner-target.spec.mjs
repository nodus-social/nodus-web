//! A profile's "Add a left side banner" opens the editor on that banner.
//!
//! The editor is one page for every region, so the profile's button has to carry the region
//! it named: the banner marks itself as the target, the collection tray's click fills that
//! banner instead of the top one, and the mark lasts exactly the visit it was asked for.
//! The stubs below answer one drive file and the attach it needs, so the placement is
//! observed through both the outgoing request and the slot the editor paints.

import { expect, test } from '@playwright/test'

import { ACCOUNT_ID, CORS, hydrated, signedIn, stubApi } from '../fixtures.mjs'

/** A 1×1 PNG: enough for the tray's thumbnail to load. */
const PIXEL = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  'base64',
)

/** One drive file, in the shape `GET /api/v1/drive/files` answers. */
const DRIVE_FILE = {
  id: 'f1',
  folder_id: null,
  filename: 'sunset.png',
  mime_type: 'image/png',
  byte_size: 1234,
  sha256: '0'.repeat(64),
  visibility: 'private',
  processing_state: 'ready',
  created_at: '2026-11-20T12:00:00Z',
}

/**
 * Answers the drive read the tray makes and the attach a clicked picture needs.
 *
 * The broad drive route is registered first because Playwright consults routes last-first:
 * the two later routes win for the file's bytes and for the attach.
 */
async function stubDrive(page) {
  const attached = []
  await page.route('**/api/v1/drive**', (route) => {
    const path = new URL(route.request().url()).pathname
    if (path === '/api/v1/drive') {
      return route.fulfill({
        headers: CORS,
        json: { used_bytes: 1234, objects: 1, folders: 0 },
      })
    }
    if (path === '/api/v1/drive/folders') {
      return route.fulfill({ headers: CORS, json: { folders: [] } })
    }
    if (path === '/api/v1/drive/files') {
      return route.fulfill({ headers: CORS, json: [DRIVE_FILE] })
    }
    return route.fulfill({ status: 404, headers: CORS, json: { error: 'no route' } })
  })
  await page.route('**/api/v1/drive/files/*/content', (route) =>
    route.fulfill({ headers: CORS, contentType: 'image/png', body: PIXEL }),
  )
  await page.route('**/api/v1/drive/files/*/attach', (route) => {
    attached.push(route.request().url())
    return route.fulfill({
      headers: CORS,
      json: { attachment_id: '9001', url: 'https://files.example/placed.png' },
    })
  })
  return attached
}

/** The owner's own profile, after the stub has answered the shell's cold load. */
async function ownProfile(page) {
  await signedIn(page)
  await page.goto(`/accounts/${ACCOUNT_ID}`)
  await hydrated(page)
}

/**
 * A route change the app performs itself: `pushState` plus the `popstate` the router
 * already listens for. A `page.goto` would load a new document and throw away the
 * editing state this test is about.
 */
async function goInApp(page, path) {
  await page.evaluate((to) => {
    history.pushState({}, '', to)
    window.dispatchEvent(new PopStateEvent('popstate'))
  }, path)
}

test('a left side banner button opens the editor on the left banner', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  await ownProfile(page)
  const attached = await stubDrive(page)

  await page.getByRole('button', { name: 'Add a left side banner' }).click()
  await expect(page).toHaveURL(/\/settings\/styling\/editor$/)
  await expect(page.locator('[data-style-editor]')).toBeVisible()

  const left = page.locator('[data-session-banner="left"]')
  const top = page.locator('[data-session-banner="top"]')

  // The mark is in the markup, not only in the ring: a description is what a reader who
  // cannot see the colour difference gets, and what this spec can assert.
  await expect(left).toHaveAttribute('data-target-region', 'left')
  await expect(left).toHaveAttribute('aria-description', 'left side banner is the drop target')
  expect(await top.getAttribute('data-target-region')).toBeNull()
  // The editor opens on the banner instead of the top of the document.
  await expect(left).toBeInViewport()

  // The tray's click is the keyboard door onto the same edit a drag makes: it fills the
  // banner the profile named, not the top one.
  await page.locator('[data-library-picture="f1"] button').click()
  await expect(left.locator('img')).toHaveCount(1)
  await expect(top.locator('img')).toHaveCount(0)
  expect(attached).toHaveLength(1)
  expect(attached[0]).toContain('/api/v1/drive/files/f1/attach')
})

test('the target lasts one visit, so a later editor opens the ordinary way', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  await ownProfile(page)
  await stubDrive(page)

  // A different button carries a different region: the mark follows the request.
  await page.getByRole('button', { name: 'Add a right side banner' }).click()
  await expect(page.locator('[data-style-editor]')).toBeVisible()
  const right = page.locator('[data-session-banner="right"]')
  await expect(right).toHaveAttribute('data-target-region', 'right')

  // Leaving through Cancel ends the editing session and the request with it.
  await page.getByRole('button', { name: 'Cancel' }).click()
  await expect(page).toHaveURL(/\/home$/)

  await goInApp(page, '/settings/styling/editor')
  await expect(page.locator('[data-style-editor]')).toBeVisible()
  await expect(page.locator('[data-target-region]')).toHaveCount(0)

  // With no region asked for, the tray's click keeps its documented default.
  await page.locator('[data-library-picture="f1"] button').click()
  await expect(page.locator('[data-session-banner="top"] img')).toHaveCount(1)
  await expect(page.locator('[data-session-banner="right"] img')).toHaveCount(0)
})
