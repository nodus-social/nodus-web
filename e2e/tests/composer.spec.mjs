//! The inline composer's poll: off until asked for, refused where the instance would
//! refuse it, and sent in the shape `POST /api/v1/statuses` reads.
//!
//! The bounds here are the server's, not the test's invention: `validated_poll` in
//! `nodus-backend/crates/api/src/statuses.rs` accepts two to four non-empty choices of at
//! most fifty characters, and an expiry between five minutes and seven days.

import { expect, test } from '@playwright/test'

import { account, otherAccount, signedIn, status, stubApi } from '../fixtures.mjs'

const CORS = { 'access-control-allow-origin': '*' }

/** A status carrying a poll, as the instance paints one. */
function pollStatus(overrides = {}) {
  return status({
    content: '<p>Which way?</p>',
    poll: {
      id: '900',
      expires_at: '2026-11-14T12:00:00.000Z',
      expired: false,
      multiple: false,
      votes_count: 0,
      voters_count: 0,
      voted: false,
      own_votes: [],
      options: [
        { title: 'Yes', votes_count: 0 },
        { title: 'No', votes_count: 0 },
      ],
      emojis: [],
    },
    ...overrides,
  })
}

async function openPoll(page) {
  await page.getByRole('button', { name: 'Add a poll' }).click()
  await expect(page.getByLabel('Poll option 1')).toBeVisible()
}

async function write(page, text) {
  const body = page.locator('main .ProseMirror')
  await body.click()
  await page.keyboard.type(text)
  await expect(body).toContainText(text)
}

/** Records every publish request, and answers it with `answer`. */
async function watchPosts(page, answer) {
  const bodies = []
  await page.route('**/api/v1/statuses', async (route) => {
    bodies.push(route.request().postData() ?? '')
    await route.fulfill({ status: 200, headers: CORS, json: answer })
  })
  return bodies
}

test('a post without a poll sends no poll fields and opens no poll editor', async ({ page }) => {
  const plain = status({ content: '<p>No poll here.</p>' })
  const bodies = []
  await stubApi(page, { statuses: [plain] })
  await signedIn(page)
  await page.goto('/')

  // The path of least resistance: the editor is closed, and only its button is visible.
  await expect(page.getByRole('button', { name: 'Add a poll' })).toBeVisible()
  await expect(page.getByLabel('Poll option 1')).toHaveCount(0)

  await page.route('**/api/v1/statuses', async (route) => {
    bodies.push(route.request().postData() ?? '')
    await route.fulfill({ status: 200, headers: CORS, json: plain })
  })

  await write(page, 'Just text')
  await page.getByRole('button', { name: 'Post', exact: true }).click()

  await expect(page.getByRole('status')).toContainText('Post published as')
  expect(bodies).toHaveLength(1)
  expect(decodeURIComponent(bodies[0])).not.toContain('poll[')
})

test('the poll editor refuses blank and overlong choices beside each field', async ({ page }) => {
  await stubApi(page, { statuses: [status()] })
  await signedIn(page)
  await page.goto('/')

  await openPoll(page)
  // Two empty rows are exactly what a new poll has, and the reason is under each one.
  await expect(page.getByText('A choice cannot be empty.')).toHaveCount(2)
  await expect(page.getByText('A poll needs at least two filled choices.')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Post', exact: true })).toBeDisabled()

  await page.getByLabel('Poll option 1').fill('Yes')
  // One answer is still not a question with a choice.
  await expect(page.getByText('A poll needs at least two filled choices.')).toBeVisible()
  await expect(page.getByText('A choice cannot be empty.')).toHaveCount(1)

  await page.getByLabel('Poll option 2').fill('No')
  await expect(page.getByText('A choice cannot be empty.')).toHaveCount(0)
  await expect(page.getByText('A poll needs at least two filled choices.')).toHaveCount(0)
  // A poll is content on its own: no text is needed once both choices are filled.
  await expect(page.getByRole('button', { name: 'Post', exact: true })).toBeEnabled()

  await page.getByLabel('Poll option 1').fill('x'.repeat(51))
  await expect(page.getByText('At most 50 characters per choice.')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Post', exact: true })).toBeDisabled()

  // And it goes back to sendable when the choice is within the server's limit.
  await page.getByLabel('Poll option 1').fill('Maybe')
  await expect(page.getByText('At most 50 characters per choice.')).toHaveCount(0)
  await expect(page.getByRole('button', { name: 'Post', exact: true })).toBeEnabled()
})

test('the poll editor adds up to four choices and removes down to two', async ({ page }) => {
  await stubApi(page, { statuses: [status()] })
  await signedIn(page)
  await page.goto('/')
  await openPoll(page)

  const add = page.getByRole('button', { name: 'Add option' })
  await add.click()
  await expect(page.getByLabel('Poll option 3')).toBeVisible()
  await add.click()
  await expect(page.getByLabel('Poll option 4')).toBeVisible()
  await expect(add).toBeDisabled()
  await expect(page.getByText('A poll holds at most four choices.')).toBeVisible()

  const removeAt = (choice) =>
    page.getByLabel(`Poll option ${choice}`).locator('..').getByRole('button', { name: '×' })
  await expect(removeAt(2)).toBeEnabled()
  await removeAt(4).click()
  await expect(page.getByLabel('Poll option 4')).toHaveCount(0)
  await removeAt(3).click()
  await expect(page.getByLabel('Poll option 3')).toHaveCount(0)
  await expect(removeAt(2)).toBeDisabled()
})

test('posting a poll sends choices, an accepted duration, and the several-answers rule', async ({
  page,
}) => {
  const statuses = []
  const posted = pollStatus({ account: otherAccount })
  await stubApi(page, { statuses })
  await signedIn(page)
  await page.goto('/')

  const bodies = await watchPosts(page, posted)

  await openPoll(page)
  await page.getByLabel('Poll option 1').fill('Yes')
  await page.getByLabel('Poll option 2').fill('No')
  await page.getByLabel('How long the poll stays open').selectOption({ label: '1 hour' })
  await page.getByLabel('Several answers').check()

  // The reply to the publish is the status the refreshed timeline will read.
  statuses.push(posted)
  await page.getByRole('button', { name: 'Post', exact: true }).click()

  await expect(page.getByRole('status')).toContainText('Post published as')
  expect(bodies).toHaveLength(1)
  const sent = new URLSearchParams(bodies[0])
  expect(sent.getAll('poll[options][]')).toEqual(['Yes', 'No'])
  // One hour, inside the server's five-minute to seven-day window.
  expect(sent.get('poll[expires_in]')).toBe('3600')
  expect(sent.get('poll[multiple]')).toBe('true')
  expect(sent.get('poll[hide_totals]') ?? 'false').toBe('false')

  // The poll is in the timeline now, with its choices and a vote the reader may cast.
  await expect(page.locator('main')).toContainText('Which way?')
  await expect(page.getByRole('button', { name: /Yes/ })).toBeVisible()
  await expect(page.getByRole('button', { name: /No/ })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Vote' })).toBeEnabled()
})

test('the author of a poll is not offered a vote on it', async ({ page }) => {
  await stubApi(page, { statuses: [pollStatus({ account })] })
  await signedIn(page)
  await page.goto('/')

  await expect(page.getByRole('button', { name: /Yes/ })).toBeDisabled()
  await expect(page.getByRole('button', { name: /No/ })).toBeDisabled()
  // The affordance stays in the layout, quiet: the instance would take the vote, but the
  // author does not get to answer their own question here.
  await expect(page.getByRole('button', { name: 'Vote' })).toBeDisabled()
})

test('poll choices do not count toward the status character limit', async ({ page }) => {
  await stubApi(page, { statuses: [status()] })
  await signedIn(page)
  await page.goto('/')

  await write(page, 'a'.repeat(480))
  await expect(page.getByText('480/500')).toBeVisible()

  await openPoll(page)
  await page.getByLabel('Poll option 1').fill('y'.repeat(50))
  await page.getByLabel('Poll option 2').fill('n'.repeat(50))

  // The server counts only the status text; fifty characters of choice twice changed
  // nothing about the body's count or its sendability.
  await expect(page.getByText('480/500')).toBeVisible()
  await expect(page.getByRole('button', { name: 'Post', exact: true })).toBeEnabled()
})
