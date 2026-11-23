//! The settings strip: a tab is drawn only when its capability is published.
//!
//! The strip is the one place where a wrong guess is visible before a request: a tab
//! drawn for a name the server does not publish leads to a page whose every read 404s.
//! These tests make the instance answer narrowly and assert the tab list, plus the
//! sentence a direct visit gets instead of a blank panel.

import { readFileSync } from 'node:fs'

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
})

test('a seeded answer paints before the probe, so no tab is drawn and then removed', async ({ page }) => {
  // A static deploy has no server island. A `trunk build` is served with one injected here,
  // the way the server paints it, and `/api/v1/meta` answers every name late: a strip that
  // waited for the probe would draw Muted first and the test would record it.
  const features = SERVED.filter((name) => name !== 'mutes' && name !== 'blocks')
  const island = JSON.stringify({
    capabilities: { contract: '0.1.0', provider: 'nodus', features, unknown: [] },
  })
  const html = readFileSync(new URL('../../dist/index.html', import.meta.url), 'utf8').replace(
    '<body>',
    `<body><script id="nodus-page" type="application/json">${island}</script>`,
  )
  await page.route('**/settings/account', (route) =>
    route.fulfill({ body: html, contentType: 'text/html' }),
  )
  await page.addInitScript(() => {
    window.__drewMuted = false
    new MutationObserver(() => {
      if (document.querySelector('a[role="tab"][href="/settings/muted"]')) {
        window.__drewMuted = true
      }
    }).observe(document, { childList: true, subtree: true })
  })
  await stubApi(page, { features: SERVED })
  await page.route('**/api/v1/meta', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 500))
    await route.fallback()
  })
  await signedIn(page)
  await page.goto('/settings/account')
  await hydrated(page)

  await expect(tab(page, 'muted')).toHaveCount(0)
  await expect(tab(page, 'blocked')).toHaveCount(0)
  await expect(tab(page, 'domain-blocks')).toBeVisible()
  expect(await page.evaluate(() => window.__drewMuted)).toBe(false)
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
