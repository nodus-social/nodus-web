//! Addresses: a handle opens the profile it names, and a shared post keeps its instance.
//!
//! The canonical forms this site links are `/accounts/{host}/{name}` and
//! `/status/{host}/{id}`, but the addresses people paste come from everywhere else on the
//! network: `/@name@host` and `/@name@host/{id}` have to land too.

import { expect, test } from '@playwright/test'

import { HOST, hydrated, signedIn, stubApi, stubPublicApi } from '../fixtures.mjs'

test('a handle opens the profile it names, through the public read', async ({ page }) => {
  await stubApi(page)
  await stubPublicApi(page)
  const asked = page.waitForRequest((request) =>
    request.url().includes('/public-api/account/'),
  )
  await page.goto(`/@ada@${HOST}`)
  await hydrated(page)

  const request = await asked
  expect(new URL(request.url()).pathname).toBe(`/public-api/account/${HOST}/ada`)
  await expect(page.getByText('Ada').first()).toBeVisible()
})

test('a shared post link in handle form opens the thread', async ({ page }) => {
  await stubApi(page)
  await stubPublicApi(page)
  const asked = page.waitForRequest((request) =>
    request.url().includes('/public-api/status/'),
  )
  await page.goto(`/@ada@${HOST}/100`)
  await hydrated(page)

  const request = await asked
  expect(new URL(request.url()).pathname).toBe(`/public-api/status/${HOST}/100`)
  // A host in the address is what the read needs, so the page paints the post itself
  // rather than the form that asks which instance to read from.
  await expect(page.getByText('Hello from the fixture.')).toBeVisible()
})

test('a bare handle stays on this instance instead of falling home', async ({ page }) => {
  await stubApi(page)
  await stubPublicApi(page)
  await signedIn(page)
  await page.goto('/@ada')
  await hydrated(page)

  // A bare handle is a name here, and the signed-in reader resolves it through their own
  // instance: what must not happen is the home timeline arriving instead of a profile.
  await expect(page.getByText('Ada').first()).toBeVisible()
  await expect(page).toHaveURL(/\/@ada$/)
})
