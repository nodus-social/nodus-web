//! Content: what a post shows, including what it deliberately hides.

import { expect, test } from '@playwright/test'

import { HOST, signedIn, status, stubApi, stubPublicApi, hydrated } from '../fixtures.mjs'

test('the timeline renders the post the API returned', async ({ page }) => {
  await stubApi(page, { statuses: [status({ content: '<p>First fixture post.</p>' })] })
  await signedIn(page)
  await page.goto('/')
  await hydrated(page)

  await expect(page.getByText('First fixture post.')).toBeVisible()
})

test('a sensitive post hides its media behind a label until asked', async ({ page }) => {
  const media = {
    id: 'm1',
    type: 'image',
    url: 'https://files.example/one.png',
    preview_url: 'https://files.example/one.png',
    description: 'a picture',
    meta_width: 800,
    meta_height: 600,
  }
  await stubApi(page, {
    statuses: [
      status({
        content: '<p>Behind a warning.</p>',
        sensitive: true,
        media_attachments: [media],
      }),
    ],
  })
  await signedIn(page)
  await page.goto('/')
  await hydrated(page)

  // The picture is present but blurred: the point of the gate is that the reader can
  // see that *something* is there, not that the bytes were withheld.
  const picture = page.locator('img[src*="one.png"]')
  await expect(picture).toHaveCSS('filter', /blur/)
  await expect(page.getByRole('button', { name: /sensitive content/i })).toBeVisible()

  await page.getByRole('button', { name: /sensitive content/i }).first().click()
  await expect(picture).not.toHaveCSS('filter', /blur/)
})

test('a post link opened without a session names its instance', async ({ page }) => {
  await stubApi(page)
  await stubPublicApi(page)
  await page.goto('/status/100')
  await hydrated(page)

  await expect(page.getByText('Read this post')).toBeVisible()
  // The shell stays, because the rail is how a visitor signs in; the page itself offers no
  // action that needs a token, which is what a shared link must not require.
  await expect(page.getByRole('button', { name: /favourite|boost|reply/i })).toHaveCount(0)
})

test('a hosted post link renders the post for an anonymous reader', async ({ page }) => {
  await stubApi(page, { statuses: [status({ content: '<p>Public and readable.</p>' })] })
  await stubPublicApi(page, { status: status({ content: '<p>Public and readable.</p>' }) })
  // The read goes through this app's own proxy, not straight to the instance: a visitor's
  // browser never talks to the host in the link.
  const read = page.waitForRequest((request) => request.url().includes('/public-api/status/'))
  await page.goto(`/status/${HOST}/100`)
  await hydrated(page)

  await expect(page.getByText('Public and readable.')).toBeVisible()
  expect((await read).url()).toContain(`/public-api/status/${HOST}/100`)
  await expect(page.getByRole('button', { name: /favourite|boost|reply/i })).toHaveCount(0)
})
