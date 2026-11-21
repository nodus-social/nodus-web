//! The metadata rows: the editor writes them, the profile reads them back.
//!
//! The rows ride on the account document: the credential read answers them for the editor,
//! and the profile screen paints the reader's own rows from the account it already holds.
//! The instance is therefore stubbed twice — the account read and the credential read — and
//! the credential write answers with the rows it kept. A save that lands is visible after a
//! reload only because those reads answer the rows the write stored, which is what these
//! stubs model.

import { expect, test } from '@playwright/test'

import { CORS, account, hydrated, signedIn, stubApi } from '../fixtures.mjs'

const VERIFY = '**/api/v1/accounts/verify_credentials'
const ACCOUNT = /\/api\/v1\/accounts\/[^/]+$/
const CREDENTIALS = '**/api/v1/accounts/update_credentials'

/** The rows the stub instance holds, in wire shape. */
function row(name, value, verifiedAt = null) {
  return { name, value, verified_at: verifiedAt }
}

/**
 * Serves the account read and the credential read from `rows`, and the credential write by
 * storing its body.
 *
 * The profile screen reads the rows off the account document it loads for the page, not off
 * a separate metadata call, so the account read is the one that has to carry them; the
 * credential read is what seeds the editor. `ACCOUNT` is registered first because Playwright
 * consults handlers newest-first: the literal `VERIFY` route added after it matches that same
 * URL and keeps its own answer, rather than being shadowed by the id pattern.
 *
 * The write arrives as JSON, not as a form: the rows are a nested list, which the form
 * encoding of this endpoint cannot spell, so the client sends the whole update as JSON.
 * `postDataJSON` is therefore the only reader that sees them — a `URLSearchParams` over
 * `postData()` would find an empty form and store nothing.
 */
async function credentialStub(page, rows) {
  const stored = { rows: [...rows], writes: [] }
  await page.route(ACCOUNT, (route) =>
    route.fulfill({ headers: CORS, json: { ...account, fields: stored.rows } }),
  )
  await page.route(VERIFY, (route) =>
    route.fulfill({ headers: CORS, json: { ...account, fields: stored.rows } }),
  )
  await page.route(CREDENTIALS, (route) => {
    if (route.request().method() !== 'PATCH') return route.fallback()
    const body = route.request().postDataJSON()
    stored.writes.push(body)
    stored.rows = body.fields.map((field) => row(field.name, field.value))
    return route.fulfill({
      headers: CORS,
      json: { ...account, display_name: body.display_name, note: body.note, fields: stored.rows },
    })
  })
  return stored
}

test('two rows are added, saved, and still on the profile after a reload', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  const stub = await credentialStub(page, [])
  await signedIn(page)
  await page.goto('/settings/profile')
  await hydrated(page)

  await page.getByRole('button', { name: 'Add a row' }).click()
  await page.getByRole('button', { name: 'Add a row' }).click()
  await page.getByLabel('Label').nth(0).fill('Blog')
  await page.getByLabel('Value').nth(0).fill('https://example.social/@ada')
  await page.getByLabel('Label').nth(1).fill('Project')
  await page.getByLabel('Value').nth(1).fill('nodus')

  await page.getByRole('button', { name: 'Save profile' }).click()
  await expect(page.getByText('Saved.')).toBeVisible()

  // The body the bridge sent, not a form: name and value per row, in row order. The SDK
  // sends the list under both names — `fields`, which nodus reads, and `fields_attributes`,
  // which upstream Mastodon reads — so both are pinned, and to the same rows in the same
  // order: a body where the two disagree would set a different table on each server.
  const sent = [
    { name: 'Blog', value: 'https://example.social/@ada' },
    { name: 'Project', value: 'nodus' },
  ]
  expect(stub.writes).toEqual([
    {
      display_name: account.display_name,
      note: account.note,
      fields: sent,
      fields_attributes: sent,
    },
  ])

  await page.reload()
  await hydrated(page)
  await expect(page.getByLabel('Label').nth(0)).toHaveValue('Blog')
  await expect(page.getByLabel('Value').nth(1)).toHaveValue('nodus')

  await page.goto(`/accounts/${account.id}`)
  await hydrated(page)
  const rows = page.locator('dl')
  await expect(rows).toContainText('Blog')
  await expect(rows).toContainText('Project')
  await expect(rows).toContainText('nodus')
  await expect(rows.getByRole('link', { name: 'https://example.social/@ada' })).toHaveAttribute(
    'rel',
    'me',
  )
})

test('the add control stops at six rows', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  await credentialStub(page, [])
  await signedIn(page)
  await page.goto('/settings/profile')
  await hydrated(page)

  const add = page.getByRole('button', { name: 'Add a row' })
  for (let index = 0; index < 6; index += 1) await add.click()
  await expect(add).toBeDisabled()
  await expect(page.getByLabel('Label')).toHaveCount(6)

  await page.getByRole('button', { name: 'Remove' }).first().click()
  await expect(add).toBeEnabled()
})

test('the instance’s complaint lands on the row it names', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  await page.route(VERIFY, (route) =>
    route.fulfill({ headers: CORS, json: { ...account, fields: [] } }),
  )
  // The client mirrors the instance's duplicate rule for rows it can see, so a server
  // complaint about rows that pass that rule is what this stub stands for: an instance
  // whose names collapse where the client's do not. What is under test is not the rule but
  // where the sentence goes.
  const message = 'profile fields 1 and 2 are both named “Blog”'
  await page.route(CREDENTIALS, (route) =>
    route.fulfill({ status: 422, headers: CORS, json: { error: message } }),
  )
  await signedIn(page)
  await page.goto('/settings/profile')
  await hydrated(page)

  await page.getByRole('button', { name: 'Add a row' }).click()
  await page.getByRole('button', { name: 'Add a row' }).click()
  await page.getByLabel('Label').nth(0).fill('Blog')
  await page.getByLabel('Value').nth(0).fill('https://example.social/@ada')
  await page.getByLabel('Label').nth(1).fill('Blog.')
  await page.getByLabel('Value').nth(1).fill('https://example.social/@blog')

  await page.getByRole('button', { name: 'Save profile' }).click()

  // Row two is the one the sentence counts as row 2, so it carries it; row one does not.
  await expect(page.getByRole('alert')).toHaveCount(2)
  await expect(page.getByRole('alert').nth(1)).toHaveText(message)
  await expect(page.getByRole('alert').nth(0)).toHaveText('')
})

test('the profile shows each pair, links only what is a link, and marks the verified one', async ({
  page,
}) => {
  await stubApi(page, { notifications: [] })
  await credentialStub(page, [
    row('Blog', 'https://example.social/@ada', '2024-01-01T00:00:00.000Z'),
    row('Project', 'nodus'),
  ])
  await signedIn(page)
  await page.goto(`/accounts/${account.id}`)
  await hydrated(page)

  const rows = page.locator('dl')
  await expect(rows.getByRole('link', { name: 'https://example.social/@ada' })).toHaveAttribute(
    'rel',
    'me',
  )
  // A value that is not a link is text, and carries no mark: the instance never checked it.
  await expect(rows.getByRole('link')).toHaveCount(1)
  await expect(rows).toContainText('nodus')
  await expect(rows.getByRole('img', { name: 'Verified link' })).toHaveCount(1)
  await expect(rows.getByRole('img', { name: 'Verified link' })).toBeVisible()
})

test('a profile with no rows draws no metadata block', async ({ page }) => {
  await stubApi(page, { notifications: [] })
  await credentialStub(page, [])
  await signedIn(page)
  await page.goto(`/accounts/${account.id}`)
  await hydrated(page)

  await expect(page.locator('dl')).toHaveCount(0)
})
