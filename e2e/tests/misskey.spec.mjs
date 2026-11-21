// A signed-in reader whose instance is Misskey.
//
// The console's screens are one vocabulary over servers that do not share an API. A Misskey
// note carries a reaction map beside a private favourite, has no boost and no bookmark, and
// answers a different set of verbs — so the footer must draw the map it was given, offer the
// reaction controls the instance actually serves, and keep the star while dropping the
// buttons that would not work.
//
// The note is state here, not a fixed answer: reacting changes the tally and favouriting
// changes the flag, because the client reads the note back after each verb instead of
// guessing — a stub that echoed the same document would pass a client that never re-read.

import { expect, test } from '@playwright/test'
import { CORS, HOST, hydrated, signedIn, status, stubPublicApi } from '../fixtures.mjs'

/** A Misskey instance: a meta document it recognises, one note, and its verbs. */
async function stubMisskey(page) {
  const note = {
    id: 'n1',
    createdAt: '2026-01-01T00:00:00.000Z',
    text: 'Hello from a Misskey note.',
    user: { id: 'u1', username: 'ada', name: 'Ada' },
    reactions: { '❤️': 2, ':blob:': 1 },
    myReaction: null,
    isFavorited: false,
  }
  const sent = []

  const json = (value) => ({
    status: 200,
    headers: { 'content-type': 'application/json', ...CORS },
    body: JSON.stringify(value),
  })
  // Same guard as the shared stub: a request no route answered escapes this hostname and
  // arrives as a transport error three layers up, which names a symptom and not the URL.
  page.on('requestfailed', (request) => {
    const url = request.url()
    if (new URL(url).host !== HOST) return
    throw new Error(`request escaped the stub: ${request.method()} ${url}`)
  })

  // Everything this instance does not publish, answered before the named routes below. A
  // Mastodon-shaped `/api/v2/instance` here would make the session look like Mastodon.
  await page.route('**/api/**', (route) =>
    route.request().method() === 'OPTIONS'
      ? route.fulfill({ status: 204, headers: CORS })
      : route.fulfill({ headers: CORS, json: [] }),
  )
  // The only document the SDK's probe needs to call this server Misskey.
  await page.route('**/api/meta', (route) =>
    route.fulfill({
      headers: CORS,
      json: {
        name: 'Misskey',
        version: '2024.1.0',
        maxNoteTextLength: 3000,
        emojis: [],
      },
    }),
  )
  await page.route('**/api/notes/show', (route) => route.fulfill(json(note)))
  await page.route('**/api/notes/conversation', (route) => route.fulfill(json([])))
  await page.route('**/api/notes/children', (route) => route.fulfill(json([])))
  await page.route('**/api/notes/favorites/create', (route) => {
    sent.push({ verb: 'favourite', body: route.request().postDataJSON() })
    note.isFavorited = true
    return route.fulfill(json(note))
  })
  await page.route('**/api/notes/favorites/delete', (route) => {
    sent.push({ verb: 'unfavourite', body: route.request().postDataJSON() })
    note.isFavorited = false
    return route.fulfill(json(note))
  })
  await page.route('**/api/notes/reactions/create', (route) => {
    const body = route.request().postDataJSON()
    sent.push({ verb: 'react', body })
    note.reactions[body.reaction] = (note.reactions[body.reaction] ?? 0) + 1
    note.myReaction = body.reaction
    return route.fulfill(json({}))
  })
  await page.route('**/api/notes/reactions/delete', (route) => {
    sent.push({ verb: 'unreact', body: route.request().postDataJSON() })
    if (note.myReaction) {
      note.reactions[note.myReaction] -= 1
      if (note.reactions[note.myReaction] <= 0) delete note.reactions[note.myReaction]
      note.myReaction = null
    }
    return route.fulfill(json({}))
  })
  // Consults the preflight first and defers the rest, so a `POST` with a JSON body (every
  // verb above) is not answered with a 204 and never reaches its own handler.
  await page.route('**/api/**', (route) =>
    route.request().method() === 'OPTIONS'
      ? route.fulfill({ status: 204, headers: CORS })
      : route.fallback(),
  )

  return { note, sent }
}

test('a Misskey note draws its reaction map, takes a reaction, gives it back, and stars through its own favourite verb', async ({
  page,
}) => {
  const instance = await stubMisskey(page)
  // A thread link is read anonymously, from this app's own proxy, before any verb runs — so
  // the card the reader sees is drawn from a public read while the verbs below still go to
  // the instance the session is on.
  await stubPublicApi(page, {
    status: status({ id: 'n1', content: '<p>Hello from a Misskey note.</p>' }),
  })
  await signedIn(page)
  await page.goto(`/status/${HOST}/n1`)
  await hydrated(page)

  // The map the note reported, keyed the way Misskey spells each key: a Unicode emoji and a
  // custom shortcode, each with its own count.
  const heart = page.locator('[aria-label="❤️ reaction, 2"]')
  await expect(heart).toBeVisible()
  await expect(page.locator('[aria-label=":blob: reaction, 1"]')).toBeVisible()
  // A Misskey note has no boost, so the control is not drawn rather than drawn and refused.
  await expect(page.locator('button[title="Boost to your followers"]')).toHaveCount(0)
  await expect(page.locator('button[title="Save for later"]')).toHaveCount(0)

  // Reacting: the palette places a key, and the note comes back with it in the map.
  await page.locator('summary[aria-label="Add a reaction"]').click()
  await page.getByRole('button', { name: 'React with 👍' }).click()
  await expect(page.locator('[aria-label="👍 reaction, 1"]')).toBeVisible()
  expect(instance.sent[0]).toEqual({ verb: 'react', body: { noteId: 'n1', reaction: '👍' } })

  // The chip that is the reader's own is also the way to take it back: Misskey keeps one
  // reaction per account, so pressing it again cannot mean "place it again".
  await page.locator('[aria-label="👍 reaction, 1"]').click()
  await expect(page.locator('[aria-label="👍 reaction, 1"]')).toHaveCount(0)
  expect(instance.sent[1]).toEqual({ verb: 'unreact', body: { noteId: 'n1' } })

  // The star is Misskey's private favourite, not a reaction: it runs `favorites/create`, and
  // the answer it folds back in is a note read afterwards.
  await page.locator('button[title="Add to favourites"]').click()
  await expect(page.locator('button[title="Remove from favourites"]')).toBeVisible()
  expect(instance.sent[2]).toEqual({ verb: 'favourite', body: { noteId: 'n1' } })

  await page.locator('button[title="Remove from favourites"]').click()
  await expect(page.locator('button[title="Add to favourites"]')).toBeVisible()
  expect(instance.sent[3]).toEqual({ verb: 'unfavourite', body: { noteId: 'n1' } })
})
