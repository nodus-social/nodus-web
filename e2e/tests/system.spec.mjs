import { expect, test } from '@playwright/test'

import { adminApi, granted, signedIn, stubApi } from '../fixtures.mjs'

const open = async (page, path) => {
  await page.goto(path)
  await page.waitForFunction(() => window.__nodus === true)
}

const consoleAs = async (page, options = {}) => {
  await stubApi(page)
  const calls = await adminApi(page, options)
  await signedIn(page)
  await granted(page)
  return calls
}

test.describe('system health', () => {
  test('every node is listed with the numbers it reported', async ({ page }) => {
    await consoleAs(page)
    await open(page, '/admin/system')

    await expect(page.getByRole('heading', { name: 'System health' })).toBeVisible()
    await expect(page.getByText('api', { exact: true })).toBeVisible()
    await expect(page.getByText('scan', { exact: true })).toBeVisible()
    await expect(page.getByText('Answering')).toBeVisible()
    await expect(page.getByText('No answer')).toBeVisible()
    // A node that did not answer keeps its last error rather than a zero.
    await expect(page.getByText(/connection refused/)).toBeVisible()
    await expect(page.getByText('3h 2m')).toBeVisible()
    await expect(page.getByText('24 MiB')).toBeVisible()
  })

  test('opening a node reads its log at the level asked for', async ({ page }) => {
    const calls = await consoleAs(page, {
      logs: [
        { timestamp: '2026-05-01T10:00:01Z', level: 'info', target: 'nodus_server', message: 'started' },
        { timestamp: '2026-05-01T10:00:02Z', level: 'warn', target: 'nodus_server', message: 'the queue is long' },
      ],
    })
    await open(page, '/admin/system')

    await page.getByRole('button', { name: 'Logs' }).first().click()
    await expect(page.getByText('api log')).toBeVisible()
    await expect(page.getByText('the queue is long')).toBeVisible()

    const read = calls.filter((call) => call.url.includes('/topology/api/logs'))
    expect(read.length).toBeGreaterThan(0)
    expect(read[0].url).toContain('min_level=info')
    expect(read[0].url).toContain('limit=200')

    await page.getByRole('button', { name: 'Warnings' }).click()
    await expect
      .poll(() => calls.filter((call) => call.url.includes('min_level=warn')).length)
      .toBeGreaterThan(0)
  })

  test('asking again reads the topology a second time', async ({ page }) => {
    const calls = await consoleAs(page)
    await open(page, '/admin/system')
    await expect(page.getByText('Answering')).toBeVisible()

    await page.getByRole('button', { name: 'Ask again' }).click()
    await expect
      .poll(() => calls.filter((call) => /\/admin\/topology$/.test(call.url)).length)
      .toBeGreaterThan(1)
  })
})
