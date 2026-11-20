//! Tags: a hashtag is a place with a header, a timeline and a subscription of its own.
//!
//! The follow verb is the interesting part: the button shows what the instance answered,
//! never what was clicked, so these tests make the instance answer and then read the button.

import { expect, test } from '@playwright/test'

import { hydrated, signedIn, status, stubApi } from '../fixtures.mjs'

test('a hashtag chip in a post opens that tag', async ({ page }) => {
  await stubApi(page, {
    statuses: [
      status({
        content: '<p>Filed under rust.</p>',
        tags: [{ name: 'rust', url: 'https://stub.instance/tags/rust' }],
      }),
    ],
  })
  await signedIn(page)
  await page.goto('/')
  await hydrated(page)

  // The chip is the in-app link: a search would ask a different question, and this is a
  // place with a timeline of its own.
  await page.getByRole('link', { name: /#rust/ }).first().click()
  await expect(page).toHaveURL(/\/tags\/rust$/)
  await expect(page.getByRole('heading', { name: '#rust' })).toBeVisible()
  await expect(page.getByText('Filed under rust.')).toBeVisible()
})

test('following a tag shows the state the instance returned', async ({ page }) => {
  await stubApi(page, { statuses: [status({ content: '<p>Tagged rust.</p>' })] })
  await signedIn(page)
  await page.goto('/tags/rust')
  await hydrated(page)

  await page.getByRole('button', { name: 'Follow' }).click()
  await expect(page.getByRole('button', { name: 'Unfollow' })).toBeVisible()

  await page.getByRole('button', { name: 'Unfollow' }).click()
  await expect(page.getByRole('button', { name: 'Follow' })).toBeVisible()
})

test('a tag nobody has posted with says so instead of showing an empty list', async ({ page }) => {
  await stubApi(page, { statuses: [] })
  await signedIn(page)
  await page.goto('/tags/quiet')
  await hydrated(page)

  await expect(page.getByText('Nobody has posted with #quiet yet.')).toBeVisible()
})
