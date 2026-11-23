//! The banners an account published, as another reader sees them.
//!
//! The document is a narrow copy of the owner's three banner regions, read from
//! `GET /api/v1/accounts/:id/style_public` and drawn in the profile's content column. The
//! stubs below answer that one address; `stubApi`'s catch-all answers everything else, and
//! the profile is expected to treat a 404 as "nothing published", not as a failure to report.

import { expect, test } from '@playwright/test'

import { CORS, account, hydrated, signedIn, stubApi } from '../fixtures.mjs'

const STYLE_PUBLIC = `**/api/v1/accounts/${account.id}/style_public`

/** One picture slot in the wire shape `nodus-backend` publishes. */
function slot(url, alt, sensitive = false) {
  return { url, alt, sensitive }
}

/** The narrow document: only the three banners, never the owner's navigation or rail. */
function published(overrides = {}) {
  return {
    top: { split: 'horizontal', slots: [] },
    left: {
      split: 'vertical',
      slots: [slot('https://files.example/left.png', 'Left banner')],
    },
    right: {
      split: 'vertical',
      slots: [slot('https://files.example/right.png', 'Right banner')],
    },
    ...overrides,
  }
}

async function profileWithBanners(page, answer) {
  await stubApi(page, { notifications: [] })
  await page.route(STYLE_PUBLIC, (route) => {
    if (answer.status) {
      return route.fulfill({ status: answer.status, headers: CORS, json: answer.json })
    }
    return route.fulfill({ headers: CORS, json: answer.json })
  })
  await signedIn(page)
  await page.goto(`/accounts/${account.id}`)
  await hydrated(page)
}

test('a published style draws both side banners in the profile column', async ({ page }) => {
  await profileWithBanners(page, { json: published() })

  const left = page.locator('[data-profile-banner="left"]')
  const right = page.locator('[data-profile-banner="right"]')
  await expect(left.locator('img')).toHaveAttribute('src', 'https://files.example/left.png')
  await expect(right.locator('img')).toHaveAttribute('src', 'https://files.example/right.png')
  await expect(left.locator('img')).toBeVisible()
  await expect(right.locator('img')).toBeVisible()

  // A published banner belongs to the account's page: it sits in normal flow above the
  // profile card, not fixed or sticky against the viewport.
  for (const region of [left, right]) {
    const position = await region.evaluate((element) => getComputedStyle(element).position)
    expect(position).toBe('static')
  }
  const bannerBox = await left.boundingBox()
  const headerBox = await page.locator('header.card').boundingBox()
  expect(bannerBox.y).toBeLessThan(headerBox.y)
})

test('a sensitive published banner starts covered and reveals on request', async ({ page }) => {
  await profileWithBanners(page, {
    json: published({
      left: {
        split: 'vertical',
        slots: [slot('https://files.example/left.png', 'Left banner', true)],
      },
    }),
  })

  const covered = page.locator('[data-profile-banner="left"] [data-style-sensitive]')
  await expect(covered).toBeVisible()
  await expect(covered.locator('img')).toHaveClass(/blur-2xl/)
  const reveal = covered.locator('[data-style-sensitive-reveal]')
  await expect(reveal).toBeVisible()
  await expect(reveal).toHaveText('Sensitive picture — show')

  await reveal.click()
  await expect(reveal).toBeHidden()
  await expect(covered.locator('img')).not.toHaveClass(/blur-2xl/)
  await expect(covered.locator('img')).toBeVisible()

  // The reveal is this view's alone: nothing about it is written back to the instance.
  await page.reload()
  await hydrated(page)
  await expect(
    page.locator('[data-profile-banner="left"] [data-style-sensitive-reveal]'),
  ).toBeVisible()
})

test('a never-published account draws no banners and no error', async ({ page }) => {
  await profileWithBanners(page, {
    status: 404,
    json: { error: 'This account has published no style' },
  })

  await expect(page.locator('[data-profile-banner]')).toHaveCount(0)
  await expect(page.getByRole('alert')).toHaveCount(0)
})
