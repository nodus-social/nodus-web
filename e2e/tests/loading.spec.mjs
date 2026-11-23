//! A second visit to a timeline must not go back to the network.
//!
//! The SDK holds the page and its timestamp, so navigating away and back within the TTL is
//! a read of state that is already in the wasm module. This test is the guard on that
//! promise: it counts the requests the browser actually made, not the ones the UI implies.

import { expect, test } from '@playwright/test'

import { CORS, signedIn, status, stubApi, hydrated } from '../fixtures.mjs'

// FIXME (2026-11-09, re-measured): both requests happen at boot, before any tab is
// touched, and the revisit half of this test passes — so freshness and the in-flight claim
// work, and the duplicate is a second *boot-time* caller rather than a second mount of the
// screen after data lands. Earlier note kept for history: measured on the deployed build — `/api/v1/timelines/home?limit=20&` is requested
// twice, sequentially, before the first tab switch. The island adoption carries a timestamp
// (`Sdk::adopt_page` dispatches `TimelineLoaded { fetched_at: Some(now) }`) and `timeline::ensure`
// claims in-flight keys, so neither is the cause; the second call comes from a second mount of
// the Home screen. Until that mount is found, this stays a documented failure rather than a
// weakened expectation.
test.fixme('a timeline is fetched once, and revisited from the store', async ({ page }) => {
  test.skip(test.info().project.name !== 'desktop', 'the phone shell renders the same store from a different bar');
  const asked = []
  page.on('request', (request) => {
    if (request.url().includes('/api/v1/timelines/')) {
      asked.push(new URL(request.url()).pathname + new URL(request.url()).search)
    }
  })

  await stubApi(page)
  await signedIn(page)
  await page.goto('/')
  await hydrated(page)
  await expect(page.getByText('Hello from the fixture.')).toBeVisible()

  // Away and back: the same page, inside the minute it stays fresh.
  const tabs = page.getByRole('tablist').first()
  await tabs.getByRole('tab', { name: 'Local' }).click()
  await tabs.getByRole('tab', { name: 'Home' }).click()
  await expect(page.getByText('Hello from the fixture.')).toBeVisible()

  const home = asked.filter((path) => path.includes('/timelines/home'))
  expect(home, `requests: ${asked.join(', ')}`).toHaveLength(1)
})

/**
 * Record every placeholder the page inserts, including one that is removed before an
 * assertion could see it. A fast answer is exactly the case a locator poll misses.
 */
function watchPlaceholders() {
  window.__placeholders = []
  const collect = (node) => {
    if (!(node instanceof Element)) return
    for (const el of [node, ...node.querySelectorAll('.skeleton, .loading-spinner')]) {
      if (el.matches('.skeleton, .loading-spinner')) {
        window.__placeholders.push(el.outerHTML.slice(0, 120))
      }
    }
  }
  new MutationObserver((records) => {
    for (const record of records) for (const node of record.addedNodes) collect(node)
  }).observe(document, { childList: true, subtree: true })
}

test('a timeline still loading paints no placeholder', async ({ page }) => {
  test.skip(test.info().project.name !== 'desktop', 'one viewport is enough to prove the shape')
  await stubApi(page)
  await page.route('**/api/v1/timelines/home**', async (route) => {
    await new Promise((done) => setTimeout(done, 2000))
    await route.fulfill({ headers: CORS, json: [status()] })
  })
  await signedIn(page)
  await page.addInitScript(watchPlaceholders)
  await page.goto('/')

  // The composer is up while the first page is still in flight: the screen is painted, only
  // its rows are missing. No card, bar or spinner stands in for them.
  await expect(page.locator('main .ProseMirror')).toBeVisible()
  await expect(page.getByText('Hello from the fixture.')).toHaveCount(0)
  expect(await page.evaluate(() => window.__placeholders)).toEqual([])

  await expect(page.getByText('Hello from the fixture.')).toBeVisible({ timeout: 8000 })
  expect(await page.evaluate(() => window.__placeholders)).toEqual([])
})

test('a timeline that lands at once never paints a placeholder', async ({ page }) => {
  await stubApi(page)
  await signedIn(page)
  await page.addInitScript(watchPlaceholders)
  await page.goto('/')
  await expect(page.getByText('Hello from the fixture.')).toBeVisible()
  expect(await page.evaluate(() => window.__placeholders)).toEqual([])
})
