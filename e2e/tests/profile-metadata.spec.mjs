//! The metadata rows: the editor writes them, the profile reads them back.
//!
//! The rows are not part of the account document, so the instance is stubbed twice: the
//! credential read answers with the rows as the instance stores them, and the credential
//! write answers with the rows it kept. A save that lands is visible after a reload only
//! because the read answers the rows the write stored, which is what these stubs model.

import { expect, test } from '@playwright/test'

import { CORS, account, hydrated, signedIn, stubApi } from '../fixtures.mjs'

const VERIFY = '**/api/v1/accounts/verify_credentials'
const CREDENTIALS = '**/api/v1/accounts/update_credentials'

/** The rows the stub instance holds, in wire shape. */
function row(name, value, verifiedAt = null) {
  return { name, value, verified_at: verifiedAt }
}

/**
 * Serves the credential read from `rows`, and the credential write by storing its body.
 *
 * The write arrives as JSON, not as a form: the rows are a nested list, which the form
 * encoding of this endpoint cannot spell, so the client sends the whole update as JSON.
 * `postDataJSON` is therefore the only reader that sees them — a `URLSearchParams` over
 * `postData()` would find an empty form and store nothing.
 */
async function credentialStub(page, rows) {
  const stored = { rows: [...rows], writes: [] }
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

  // The body the bridge sent, not a form: name and value per row, in row order.
  expect(stub.writes).toEqual([
    {
      display_name: account.display_name,
      note: account.note,
      fields: [
        { name: 'Blog', value: 'https://example.social/@ada' },
        { name: 'Project', value: 'nodus' },
      ],
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
