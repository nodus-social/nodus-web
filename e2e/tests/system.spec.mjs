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

  test('following a log keeps reading it and stops when the drawer closes', async ({ page }) => {
    const calls = await consoleAs(page, {
      logs: [
        { timestamp: '2026-05-01T10:00:01Z', level: 'info', target: 'nodus_server', message: 'started' },
      ],
    })
    await open(page, '/admin/system')

    await page.getByRole('button', { name: 'Logs' }).first().click()
    await page.getByRole('button', { name: 'Follow', exact: true }).click()
    await expect(page.getByRole('button', { name: 'Following' })).toBeVisible()

    const reads = () => calls.filter((call) => call.url.includes('/topology/api/logs')).length
    await expect.poll(reads, { timeout: 15_000 }).toBeGreaterThan(1)

    await page.getByRole('button', { name: 'Close' }).click()
    await expect(page.getByRole('button', { name: 'Following' })).toHaveCount(0)
    const settled = reads()
    await page.waitForTimeout(5_000)
    expect(reads()).toBe(settled)
  })

  test('following a log shows the lines that arrive after the first read', async ({ page }) => {
    const calls = await consoleAs(page, {
      logs: [
        { timestamp: '2026-05-01T10:00:01Z', level: 'info', target: 'nodus_server', message: 'started' },
      ],
      logStream: [
        { timestamp: '2026-05-01T10:00:09Z', level: 'info', target: 'nodus_server', message: 'the queue drained' },
      ],
    })
    await open(page, '/admin/system')

    await page.getByRole('button', { name: 'Logs' }).first().click()
    await expect(page.getByText('started')).toBeVisible()
    await page.getByRole('button', { name: 'Follow', exact: true }).click()

    // The line the first read cannot have carried: it arrived on the stream.
    await expect(page.getByText('the queue drained')).toBeVisible()
    expect(calls.filter((call) => call.url.includes('/logs/stream')).length).toBeGreaterThan(0)

    const drawerReads = () => calls.filter((call) => /\/logs(\?|$)/.test(call.url)).length
    const reads = drawerReads()
    await page.waitForTimeout(3_000)
    // Still there after the old two-second poll would have read again: the stream held it.
    await expect(page.getByText('the queue drained')).toBeVisible()
    expect(drawerReads()).toBe(reads)
  })

  test('a worker can be asked to run its pass now', async ({ page }) => {
    const calls = await consoleAs(page, {
      topology: [
        {
          name: 'api',
          transport: 'unix',
          endpoint: '/var/lib/nodus/run/api.sock',
          status: 'healthy',
          version: '0.1.0',
          uptime_seconds: 600,
          cpu_usage_percent: 1,
          rss_bytes: 1024,
          queue_depth: null,
          latency_ms: 3,
          last_seen_at: null,
          last_error: null,
          version_mismatch: false,
        },
        {
          name: 'prune',
          transport: 'unix',
          endpoint: '/var/lib/nodus/run/prune.sock',
          status: 'healthy',
          version: '0.1.0',
          uptime_seconds: 600,
          cpu_usage_percent: 1,
          rss_bytes: 1024,
          queue_depth: 4,
          latency_ms: 3,
          last_seen_at: null,
          last_error: null,
          version_mismatch: false,
        },
      ],
    })
    await open(page, '/admin/system')

    // The API answers requests and has nothing to run early, so only a worker offers this.
    await expect(page.getByRole('button', { name: 'Do it now' })).toHaveCount(1)
    await page.getByRole('button', { name: 'Do it now' }).click()

    await expect(page.getByText(/prune took the request: a pass is queued/)).toBeVisible()
    const asked = calls.filter((call) => call.url.endsWith('/topology/prune/trigger'))
    expect(asked).toHaveLength(1)
    expect(JSON.parse(asked[0].body)).toEqual({ work: 'prune' })
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
