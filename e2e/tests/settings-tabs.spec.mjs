//! The settings strip: a tab is drawn only when its capability is published.
//!
//! The strip is the one place where a wrong guess is visible before a request: a tab
//! drawn for a name the server does not publish leads to a page whose every read 404s.
//! These tests make the instance answer narrowly and assert the tab list, plus the
//! sentence a direct visit gets instead of a blank panel.

import { expect, test } from '@playwright/test'

import { FEATURES, hydrated, signedIn, stubApi } from '../fixtures.mjs'

/** The six sections this app gates on names the default stub does not publish. */
const GATED = ['muted', 'blocked', 'domain-blocks', 'sessions', 'two-factor', 'export']

/** Every name this app's settings strip knows how to read. */
const SERVED = [...FEATURES, 'mutes', 'blocks', 'domain_blocks', 'sessions', 'two_factor', 'exports']

/** Tabs whose gate is `None`: the shell's own forms, drawn on any server. */
const UNGATED = [
  'account',
  'profile',
  'appearance',
  'notifications',
  'preferences',
  'filters',
  'lists',
  'featured-tags',
  'instance',
]

function tab(page, slug) {
  return page.locator(`a[role="tab"][href="/settings/${slug}"]`)
}

test('a server that publishes every name draws every gated and ungated tab', async ({ page }) => {
  await stubApi(page, { features: SERVED })
  await signedIn(page)
  await page.goto('/settings/account')
  await hydrated(page)

  for (const slug of [...UNGATED, ...GATED]) {
    await expect(tab(page, slug)).toBeVisible()
  }
  await expect(page.locator('a[role="tab"]')).toHaveCount(UNGATED.length + GATED.length)
})

test('a server without mutes and blocks drops only those two tabs', async ({ page }) => {
  // This is the flicker check as well as the gate check: a tab drawn from the wrong
  // answer and removed when `/api/v1/meta` lands would pass a final-count assertion, so
  // the page records whether the hidden tab was ever in the document.
  await page.addInitScript(() => {
    window.__drewMutedBeforeAnswer = false
    new MutationObserver(() => {
      if (document.querySelector('a[role="tab"][href="/settings/muted"]')) {
        window.__drewMutedBeforeAnswer = true
      }
    }).observe(document, { childList: true, subtree: true })
  })
  const narrower = SERVED.filter((name) => name !== 'mutes' && name !== 'blocks')
  await stubApi(page, { features: narrower })
  await signedIn(page)
  await page.goto('/settings/account')
  await hydrated(page)

  await expect(tab(page, 'muted')).toHaveCount(0)
  await expect(tab(page, 'blocked')).toHaveCount(0)
  for (const slug of ['domain-blocks', 'sessions', 'two-factor', 'export', ...UNGATED]) {
    await expect(tab(page, slug)).toBeVisible()
  }
  expect(await page.evaluate(() => window.__drewMutedBeforeAnswer)).toBe(false)
})

test('a direct visit to a hidden section names the server, not a blank tab', async ({ page }) => {
  const narrower = SERVED.filter((name) => name !== 'mutes')
  await stubApi(page, { features: narrower })
  await signedIn(page)
  await page.goto('/settings/muted')
  await hydrated(page)

  await expect(page.getByText('This instance does not serve the mute list.')).toBeVisible()
  await expect(tab(page, 'muted')).toHaveCount(0)
  // The explanation is not an error page: the rest of settings still works.
  await expect(tab(page, 'account')).toBeVisible()
})
