// The side banners are the page's outer columns, not a fixed strip beside one.
//
// The layout owns the reading column at its 72rem measure and gives every leftover pixel
// at the window edge to the pictures. These tests pin both halves of that: the measured
// gutter/banner/centre widths, and a screenshot at each desktop width so a change to the
// grid is visible even when the numbers still look plausible.

import { expect, test } from '@playwright/test'

import { ACCOUNT_ID, CORS, HOST, hydrated, signedIn, stubApi } from '../fixtures.mjs'

/** One picture drawn deterministically, so a screenshot diff is the layout, not the bits. */
const BANNER_SVG = `
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 1600">
  <rect width="1200" height="1600" fill="#1f2937"/>
  <path d="M0 200 L1200 0 L1200 1600 L0 1600 Z" fill="#0f766e"/>
  <path d="M0 900 L1200 700 L1200 1600 L0 1600 Z" fill="#134e4a"/>
  <circle cx="600" cy="800" r="260" fill="#f59e0b" opacity="0.85"/>
</svg>`

/** Seed both side banners through the same per-account store the client reads at boot. */
async function bannersOnBothSides(page) {
  await stubApi(page)
  await signedIn(page)
  await page.route('https://cdn.example/banner.svg', (route) =>
    route.fulfill({ contentType: 'image/svg+xml', body: BANNER_SVG }),
  )
  // Registered after `stubApi`, so it wins over the fixture's empty metadata answer.
  // The client refreshes the style after boot; a local-only style would be painted once
  // and then overwritten by the empty server answer.
  await page.route('**/api/v1/nodus/account/metadata**', (route) =>
    route.fulfill({
      headers: CORS,
      json: {
        metadata: {
          'nodus-left-image': {
            slots: [{ url: 'https://cdn.example/banner.svg', alt: 'Left banner' }],
          },
          'nodus-right-image': {
            slots: [{ url: 'https://cdn.example/banner.svg', alt: 'Right banner' }],
          },
        },
      },
    }),
  )
  await page.addInitScript(
    ([key]) => {
      localStorage.setItem(
        key,
        JSON.stringify({
          left: {
            slots: [{ url: 'https://cdn.example/banner.svg', alt: 'Left banner' }],
          },
          right: {
            slots: [{ url: 'https://cdn.example/banner.svg', alt: 'Right banner' }],
          },
        }),
      )
    },
    [`nodus.session_style.${HOST}::${ACCOUNT_ID}`],
  )
  await page.goto('/home')
  await hydrated(page)
  await expect(page.locator('[data-session-banner="left"] img')).toBeVisible()
  await expect(page.locator('[data-session-banner="right"] img')).toBeVisible()
}

/** The numbers a reader would measure with a ruler: gutter, picture, and reading column. */
async function measured(page) {
  return page.evaluate(() => {
    const box = (selector) => {
      const element = document.querySelector(selector)
      if (!element) return null
      const { width, height } = element.getBoundingClientRect()
      return { width: Math.round(width), height: Math.round(height) }
    }
    const grid = document.querySelector('[data-session-banner="left"]')?.parentElement
    const centre = grid?.querySelector('main')?.parentElement
    const banner = (side) => document.querySelector(`[data-session-banner="${side}"]`)
    return {
      viewport: window.innerWidth,
      pageWidth: Math.round(document.documentElement.clientWidth),
      gutter: banner('left')?.getBoundingClientRect().width ?? 0,
      left: box('[data-session-banner="left"] img'),
      right: box('[data-session-banner="right"] img'),
      centre: box('main')?.width ?? 0,
      centreTrack: centre ? Math.round(centre.getBoundingClientRect().width) : 0,
      grid: grid ? Math.round(grid.getBoundingClientRect().width) : 0,
      columns: grid ? getComputedStyle(grid).gridTemplateColumns : '',
      display: banner('left') ? getComputedStyle(banner('left')).display : '',
      outerClass: banner('left')?.className ?? '',
    }
  })
}

test('the side banners fill the gutter at every desktop width', async ({ page }) => {
  test.skip(test.info().project.name !== 'desktop', 'the side banners only exist at xl and up')
  test.setTimeout(120_000)
  await bannersOnBothSides(page)

  const widths = [1400, 1536, 1920, 2560]
  const readings = []
  for (const width of widths) {
    await page.setViewportSize({ width, height: 900 })
    // The banners are in the grid, so a resize is the whole change; wait for the layout
    // to settle before measuring so a screenshot and a number describe the same paint.
    await page.waitForFunction((expected) => window.innerWidth === expected, width)
    await expect(page.locator('[data-session-banner="left"] img')).toBeVisible()
    readings.push(await measured(page))
    await expect(page).toHaveScreenshot(`session-banner-${width}.png`, {
      animations: 'disabled',
      caret: 'hide',
    })
  }

  // The reading column is the same at every width: the banners grow into the gutter, not
  // out of the page. Measured as the centre wrapper's `main`, which is what a reader reads.
  for (const reading of readings) {
    console.log(`reading ${JSON.stringify(reading)}`)
  }
  const centre = readings.map((reading) => reading.centre)
  expect(new Set(centre).size, `reading column changed width: ${JSON.stringify(readings)}`).toBe(1)
  expect(centre[0]).toBeGreaterThan(600)

  // Each banner fills its side up to the 43rem cap. Below the cap it is the raw gutter;
  // above it, the picture stops growing while staying pinned to the window edge. The page
  // box is a few pixels narrower than the viewport because the root reserves a scrollbar
  // gutter, so the gutter is measured from the grid, not from `window.innerWidth`.
  for (const reading of readings) {
    const rawGutter = (reading.grid - 1152) / 2
    const expected = Math.min(rawGutter, 688)
    expect(Math.abs(reading.gutter - expected)).toBeLessThanOrEqual(1)
    expect(Math.abs(reading.left.width - expected)).toBeLessThanOrEqual(1)
    expect(Math.abs(reading.right.width - expected)).toBeLessThanOrEqual(1)
    // The centre track is always the full measure, even at the narrowest desktop width.
    expect(reading.centreTrack).toBe(1152)
  }

  // The 2560 case is the one the cap exists for: the raw gutter is 704 a side, and the
  // picture is drawn 688 — still touching the window edge, not centred in a letterbox.
  expect(readings.at(-1).viewport).toBe(2560)
  expect(readings.at(-1).left.width).toBe(688)
  expect(readings.at(-1).right.width).toBe(688)

  // The proof for the report: the four measurements, logged in one line per viewport.
  for (const reading of readings) {
    console.log(
      `viewport ${reading.viewport}: gutter ${reading.gutter}px, ` +
        `banner ${reading.left.width}px, centre ${reading.centre}px ` +
        `(track ${reading.centreTrack}px)`,
    )
  }
})

test('below xl the side regions are not drawn', async ({ page }) => {
  test.skip(test.info().project.name !== 'desktop', 'this test sets its own viewport')
  await bannersOnBothSides(page)
  await page.setViewportSize({ width: 1200, height: 900 })

  // The same `hidden xl:flex` as before the grid: the pictures survive in the style, the
  // region simply has no track to draw into on a narrow window.
  await expect(page.locator('[data-session-banner="left"]')).toBeHidden()
  await expect(page.locator('[data-session-banner="right"]')).toBeHidden()
  const reading = await measured(page)
  expect(reading.display).toBe('none')
})

test('the grid holds at both edges of the desktop range', async ({ page }) => {
  test.skip(test.info().project.name !== 'desktop', 'this test sets its own viewport')
  await bannersOnBothSides(page)

  // 1280 is the first width with a side region, and the one where a greedy banner would
  // eat the reading column. The centre track stays 72rem and `main` keeps its width.
  await page.setViewportSize({ width: 1280, height: 900 })
  const narrow = await measured(page)
  expect(narrow.centreTrack).toBe(1152)
  expect(narrow.centre).toBe(944)
  expect(narrow.left.width).toBe(59)

  // Past 2560 the raw gutter keeps growing, but the picture is capped at 43rem and stays
  // against the window edge, so it does not become an ever-wider letterboxed band.
  await page.setViewportSize({ width: 2880, height: 900 })
  const wide = await measured(page)
  expect(wide.centreTrack).toBe(1152)
  expect(wide.centre).toBe(944)
  expect(wide.left.width).toBe(688)
  expect(wide.right.width).toBe(688)
})

test('a banner column does not swallow a click meant for the page', async ({ page }) => {
  test.skip(test.info().project.name !== 'desktop', 'the side banners only exist at xl and up')
  await bannersOnBothSides(page)
  await page.setViewportSize({ width: 1920, height: 900 })

  // A rail link sits next to the wider banner now. The column is a grid track, so a click
  // on the rail reaches the rail; the old fixed strip needed `pointer-events-none` for
  // this, and losing that discipline would turn the gutter into a dead zone.
  const home = page.locator('[data-rail="left"]').getByRole('link', { name: 'Home', exact: true }).first()
  await home.click()
  await expect(page).toHaveURL(/\/home$/)
})

test('the banner verbs still work in the wider column', async ({ page }) => {
  test.skip(test.info().project.name !== 'desktop', 'the side banners only exist at xl and up')
  await bannersOnBothSides(page)
  // The editor is a route: arriving paints the page with its arranging tools on.
  await page.goto('/settings/styling/editor')
  await hydrated(page)
  await page.setViewportSize({ width: 2560, height: 900 })
  await expect(page.locator('[data-style-slot-edit]').first()).toBeVisible()

  // The verbs sit in the picture's own corner, so they stay clickable at the cap.
  const edit = page.locator('[data-session-banner="left"] [data-style-slot-edit]').first()
  const handle = page.locator('[data-session-banner="left"] [data-style-picture-drag]').first()
  await expect(edit).toBeVisible()
  await expect(handle).toBeVisible()
  await edit.click()
  await expect(page.locator('[data-session-banner="left"] [data-style-slot-editor]')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Replace picture' }).first()).toBeVisible()
  await expect(page.locator('[data-style-slot-alt]').first()).toBeVisible()
})
