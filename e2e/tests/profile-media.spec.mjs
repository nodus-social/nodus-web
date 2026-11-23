//! The four things the reader can change on their own profile, and the line between them.
//!
//! A profile picture and an account header are account media: an upload to the media
//! endpoint, then a credentials patch naming the attachment. A side banner is session
//! artwork: the profile only opens the style editor that already owns placement. The stubs
//! below count the two writes separately so a control that reaches for the wrong one fails.

import { expect, test } from '@playwright/test'

import { CORS, account, otherAccount, hydrated, signedIn, stubApi } from '../fixtures.mjs'

const UPLOAD = '**/api/v2/media'
const CREDENTIALS = '**/api/v1/accounts/update_credentials'
const STRANGER = '**/api/v1/accounts/2'

/**
 * Answers the media upload, the credentials patch, and a stranger's account read.
 *
 * `stubApi` registers its catch-all first; every route here is added afterwards, so it is
 * consulted before that catch-all and before the fixture's generic account read.
 */
async function mediaStub(page, { refusal = null } = {}) {
  const counts = { uploads: 0, patches: 0, patched: [] }
  await page.route(UPLOAD, async (route) => {
    if (route.request().method() !== 'POST') return route.fallback()
    counts.uploads += 1
    return route.fulfill({
      headers: CORS,
      json: { id: '9001', type: 'image', url: 'https://files.example/uploaded.png' },
    })
  })
  await page.route(STRANGER, (route) =>
    route.fulfill({ headers: CORS, json: otherAccount }),
  )
  await page.route(CREDENTIALS, async (route) => {
    if (route.request().method() !== 'PATCH') return route.fallback()
    counts.patches += 1
    counts.patched.push(route.request().postData() ?? '')
    if (refusal) {
      return route.fulfill({
        status: 422,
        headers: CORS,
        json: { error: refusal },
      })
    }
    return route.fulfill({
      headers: CORS,
      json: { ...account, avatar: 'https://files.example/uploaded.png' },
    })
  })
  return counts
}

async function ownProfile(page) {
  await signedIn(page)
  await page.goto(`/accounts/${account.id}`)
  await hydrated(page)
}

test("the reader's own profile offers four distinct changes, a stranger's none", async ({
  page,
}) => {
  await stubApi(page, { notifications: [] })
  await mediaStub(page)
  await ownProfile(page)

  for (const label of [
    'Change profile picture',
    'Change header image',
    'Add a left side banner',
    'Add a right side banner',
  ]) {
    await expect(page.getByRole('button', { name: label })).toBeVisible()
  }

  // The side-banner control reaches the editor that already owns placement rather than
  // growing a second one on the profile.
  await page.getByRole('button', { name: 'Add a left side banner' }).click()
  await expect(page).toHaveURL(/\/settings\/styling\/editor$/)

  await page.goto('/accounts/2')
  await hydrated(page)
  await expect(page.getByRole('button', { name: 'Change profile picture' })).toHaveCount(0)
  await expect(page.getByRole('button', { name: 'Change header image' })).toHaveCount(0)
  await expect(page.getByRole('button', { name: /side banner/ })).toHaveCount(0)
})

test('a profile picture uploads once and patches the credentials once', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  const counts = await mediaStub(page)
  await ownProfile(page)

  const chooser = page.waitForEvent('filechooser')
  await page.getByRole('button', { name: 'Change profile picture' }).click()
  await (await chooser).setFiles({
    name: 'avatar.png',
    mimeType: 'image/png',
    buffer: Buffer.from('not really a png, but the stub does not care'),
  })

  await expect(page.getByRole('status')).toHaveText('Profile picture updated.')
  expect(counts.uploads).toBe(1)
  expect(counts.patches).toBe(1)
  // The credentials form names the attachment the upload returned.
  expect(counts.patched[0]).toContain('9001')
  expect(counts.patched[0]).toContain('name="avatar"')
})

test('cancelling the file dialog sends nothing', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  const counts = await mediaStub(page)
  await ownProfile(page)

  const chooser = page.waitForEvent('filechooser')
  await page.getByRole('button', { name: 'Change header image' }).click()
  await (await chooser).setFiles([])
  await page.waitForTimeout(500)

  expect(counts.uploads).toBe(0)
  expect(counts.patches).toBe(0)
})

test('a refused credentials patch shows its message beside the controls', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  await mediaStub(page, { refusal: 'The instance refused that picture.' })
  await ownProfile(page)

  const chooser = page.waitForEvent('filechooser')
  await page.getByRole('button', { name: 'Change profile picture' }).click()
  await (await chooser).setFiles({
    name: 'avatar.png',
    mimeType: 'image/png',
    buffer: Buffer.from('not really a png, but the stub does not care'),
  })

  await expect(page.getByRole('alert')).toContainText('The instance refused that picture.')
})
