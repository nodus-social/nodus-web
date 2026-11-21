//! The drive: one screen over the account's own files.
//!
//! Every response is a constant held by the test, and every write is recorded, so a
//! control that looks right but sends nothing fails here. The drive is small enough to
//! stub whole: a summary, a folder tree, and the rows of one folder.

import { expect, test } from '@playwright/test'

import { hydrated, signedIn, stubApi } from '../fixtures.mjs'

const CORS = {
  'access-control-allow-origin': '*',
  'access-control-allow-headers': '*',
  'access-control-allow-methods': 'GET, POST, PATCH, DELETE, OPTIONS',
}

/** One file, in the shape `GET /api/v1/drive/files` answers. */
function file({ id = 'f1', filename = 'notes.txt', folder_id = null, visibility = 'private', state = 'ready', size = 12 } = {}) {
  return {
    id,
    folder_id,
    filename,
    mime_type: 'text/plain',
    byte_size: size,
    sha256: '0'.repeat(64),
    visibility,
    processing_state: state,
    created_at: '2026-11-07T12:00:00.000Z',
  }
}

/**
 * A whole stub drive, and the calls it received.
 *
 * The state is the test's, not the browser's: an upload appends a row and adds bytes, a
 * delete removes both, exactly as the API would, so the meter's change is measured rather
 * than asserted against a constant.
 */
async function stubDrive(page, { files = [], folders = [], shareRefusal = null } = {}) {
  const state = { files: [...files], folders: [...folders], calls: [], shareRefusal }

  const tree = (parent = null) =>
    state.folders
      .filter((folder) => (folder.parent_id ?? null) === parent)
      .map((folder) => ({ ...folder, children: tree(folder.id) }))

  await page.route('**/api/v1/drive**', (route) => {
    const request = route.request()
    const method = request.method()
    if (method === 'OPTIONS') {
      return route.fulfill({ status: 204, headers: CORS })
    }
    const url = new URL(request.url())
    const path = url.pathname.replace(/^\/api\/v1\/drive/, '')
    const body = request.postData() ?? ''

    if (path === '' && method === 'GET') {
      return route.fulfill({
        headers: CORS,
        json: {
          used_bytes: state.files.reduce((sum, row) => sum + row.byte_size, 0),
          objects: state.files.length,
          folders: state.folders.length,
        },
      })
    }

    if (path === '/folders' && method === 'GET') {
      return route.fulfill({ headers: CORS, json: { folders: tree() } })
    }

    if (path === '/folders' && method === 'POST') {
      const parsed = JSON.parse(body)
      const folder = {
        id: `d${state.folders.length + 1}`,
        parent_id: parsed.parent_id ?? null,
        name: parsed.name,
        created_at: '2026-11-07T12:00:00.000Z',
        updated_at: '2026-11-07T12:00:00.000Z',
      }
      state.folders.push(folder)
      state.calls.push({ method, path, body: parsed })
      return route.fulfill({ status: 201, headers: CORS, json: folder })
    }

    const folderMatch = /^\/folders\/([^/]+)$/.exec(path)
    if (folderMatch) {
      const id = folderMatch[1]
      if (method === 'PATCH') {
        const parsed = JSON.parse(body)
        state.calls.push({ method, path, body: parsed })
        const folder = state.folders.find((row) => row.id === id)
        if (folder && parsed.name) folder.name = parsed.name
        return route.fulfill({ headers: CORS, json: folder })
      }
      if (method === 'DELETE') {
        state.calls.push({ method, path })
        const gone = new Set([id])
        let grew = true
        while (grew) {
          grew = false
          for (const folder of state.folders) {
            if (folder.parent_id && gone.has(folder.parent_id) && !gone.has(folder.id)) {
              gone.add(folder.id)
              grew = true
            }
          }
        }
        state.folders = state.folders.filter((folder) => !gone.has(folder.id))
        state.files = state.files.filter((row) => !gone.has(row.folder_id))
        return route.fulfill({ status: 204, headers: CORS })
      }
    }

    if (path === '/files' && method === 'GET') {
      const folder = url.searchParams.get('folder_id')
      return route.fulfill({
        headers: CORS,
        json: state.files.filter((row) => (folder ? row.folder_id === folder : row.folder_id === null)),
      })
    }

    if (path === '/files' && method === 'POST') {
      // The boundary is the browser's; the test only needs the filename it carried.
      const named = /filename="([^"]*)"/.exec(body)
      const uploaded = file({
        id: `u${state.files.length + 1}`,
        filename: named ? named[1] : 'upload.txt',
        size: body.length,
        state: 'pending',
      })
      state.files.push(uploaded)
      state.calls.push({ method, path })
      return route.fulfill({ status: 201, headers: CORS, json: uploaded })
    }

    const shareMatch = /^\/files\/([^/]+)\/share$/.exec(path)
    if (shareMatch && method === 'POST') {
      const id = shareMatch[1]
      const row = state.files.find((entry) => entry.id === id)
      const parsed = body ? JSON.parse(body) : null
      state.calls.push({ method, path, body: parsed })
      if (state.shareRefusal) {
        return route.fulfill({
          status: state.shareRefusal.status,
          headers: CORS,
          json: { error: state.shareRefusal.error },
        })
      }
      if (!row || row.visibility === 'private') {
        return route.fulfill({ status: 409, headers: CORS, json: { error: 'this file is private' } })
      }
      return route.fulfill({
        headers: CORS,
        json: {
          url: `${new URL(request.url()).origin}/media/share/${'a'.repeat(32)}`,
          expires_at: new Date(Date.now() + (parsed?.expires_in_seconds ?? 604800) * 1000).toISOString(),
        },
      })
    }

    const contentMatch = /^\/files\/([^/]+)\/content$/.exec(path)
    if (contentMatch) {
      return route.fulfill({
        headers: { ...CORS, 'content-disposition': 'attachment; filename="notes.txt"' },
        body: 'hello world\n',
      })
    }

    const fileMatch = /^\/files\/([^/]+)$/.exec(path)
    if (fileMatch) {
      const id = fileMatch[1]
      const row = state.files.find((entry) => entry.id === id)
      if (method === 'PATCH') {
        const parsed = JSON.parse(body)
        state.calls.push({ method, path, body: parsed })
        if (row) Object.assign(row, parsed)
        return route.fulfill({ headers: CORS, json: row })
      }
      if (method === 'DELETE') {
        state.calls.push({ method, path })
        state.files = state.files.filter((entry) => entry.id !== id)
        return route.fulfill({ status: 204, headers: CORS })
      }
    }

    return route.fulfill({ status: 404, headers: CORS, json: { error: 'not stubbed' } })
  })

  return state
}

async function open(page, options = {}) {
  await stubApi(page)
  const state = await stubDrive(page, options)
  await signedIn(page)
  await page.goto('/drive')
  await hydrated(page)
  return state
}

test('an empty drive says what it is for, and the meter is zero', async ({ page }) => {
  await open(page)

  await expect(page.getByTestId('drive-empty')).toBeVisible()
  await expect(page.getByTestId('drive-empty')).toContainText('It is where you keep files for this account')
  await expect(page.getByTestId('drive-used')).toContainText('0 B used')
  await expect(page.getByTestId('drive-used')).toContainText('0 files')
  // No quota is published, so no bar is drawn against one.
  await expect(page.locator('progress')).toHaveCount(0)
})

test('an upload adds a row and moves the used meter', async ({ page }) => {
  const state = await open(page)
  await expect(page.getByTestId('drive-used')).toContainText('0 B used')

  await page.getByLabel('Upload files').setInputFiles({
    name: 'hello.txt',
    mimeType: 'text/plain',
    buffer: Buffer.from('hello drive\n'),
  })

  await expect(page.getByTestId('drive-file')).toHaveCount(1)
  await expect(page.getByTestId('drive-file')).toContainText('hello.txt')
  await expect(page.getByTestId('drive-used')).not.toContainText('0 B used')
  await expect(page.getByTestId('processing-badge')).toContainText('still being made')
  expect(state.calls.some((call) => call.method === 'POST' && call.path === '/files')).toBe(true)
})

test('renaming a file sends its new name', async ({ page }) => {
  const state = await open(page, { files: [file()] })

  await page.getByLabel('Rename notes.txt').click()
  await page.getByLabel('File name').fill('renamed.txt')
  await page.getByRole('button', { name: 'Save', exact: true }).click()

  await expect(page.getByTestId('drive-file')).toContainText('renamed.txt')
  expect(state.calls).toContainEqual({
    method: 'PATCH',
    path: '/files/f1',
    body: { filename: 'renamed.txt' },
  })
})

test('moving a file picks a folder from a list', async ({ page }) => {
  const state = await open(page, { files: [file()] })

  await page.getByLabel('New folder').click()
  await page.getByLabel('New folder name').fill('Archive')
  await page.getByRole('button', { name: 'Create' }).click()
  await expect(page.getByTestId('folder-row')).toContainText('Archive')

  await page.getByLabel('Move notes.txt to a folder').selectOption({ label: 'Archive' })

  // The open folder is still the root, so the file has left the list it was in. The row
  // goes only once the move has been answered and the list refetched, so this is the
  // signal that the recorded call below is there: asserting the calls on its own races
  // the request leaving the browser.
  await expect(page.getByTestId('drive-file')).toHaveCount(0)
  expect(state.calls).toContainEqual({
    method: 'PATCH',
    path: '/files/f1',
    body: { folder_id: 'd1' },
  })
})

test('deleting asks first, and the confirmation names what goes', async ({ page }) => {
  const state = await open(page, { files: [file()] })

  await page.getByLabel('Delete notes.txt').click()
  await expect(page.getByRole('alert')).toContainText('The bytes go with it')
  expect(state.calls).toHaveLength(0)

  await page.getByTestId('confirm-file-delete').click()

  await expect(page.getByTestId('drive-file')).toHaveCount(0)
  expect(state.calls).toContainEqual({ method: 'DELETE', path: '/files/f1' })
})

test('visibility changes through the row control', async ({ page }) => {
  const state = await open(page, { files: [file()] })

  await page.getByLabel('Visibility of notes.txt').selectOption('public_federated')

  await expect(page.getByTestId('drive-file')).toContainText('Public')
  expect(state.calls).toContainEqual({
    method: 'PATCH',
    path: '/files/f1',
    body: { visibility: 'public_federated' },
  })
})

test('the sidebar deletes a folder only after naming its subtree', async ({ page }) => {
  const state = await open(page, {
    folders: [
      { id: 'd1', parent_id: null, name: 'Archive', created_at: '', updated_at: '' },
      { id: 'd2', parent_id: 'd1', name: '2025', created_at: '', updated_at: '' },
    ],
  })

  await page.getByLabel('Delete folder Archive').click()
  await expect(page.getByRole('alert')).toContainText('every folder and file under it')

  await page.getByTestId('confirm-folder-delete').click()

  await expect(page.getByTestId('folder-row')).toHaveCount(0)
  expect(state.calls).toContainEqual({ method: 'DELETE', path: '/folders/d1' })
})

test('a failing preview is said to have failed', async ({ page }) => {
  await open(page, { files: [file({ state: 'failed' })] })

  await expect(page.getByTestId('processing-badge')).toContainText('Preview failed')
})

test('downloading goes through the content endpoint', async ({ page }) => {
  const state = await open(page, { files: [file()] })

  const download = page.waitForEvent('download')
  await page.getByLabel('Download notes.txt').click()
  const saved = await download

  expect(saved.suggestedFilename()).toBe('notes.txt')
  expect(state.files).toHaveLength(1)
})

test('a link is asked for with one span and shown as the instance made it', async ({ page }) => {
  const state = await open(page, { files: [file({ visibility: 'unlisted_link' })] })

  await page.getByLabel('Copy a link notes.txt').click()
  await expect(page.getByTestId('share-dialog')).toBeVisible()
  // The dialog offers spans, not a free-form expiry; the default is seven days.
  await page.getByLabel('Link lifetime').selectOption({ label: '1 day' })
  await page.getByTestId('mint-share-link').click()

  // The request names the file, carries the span, and carries nothing else.
  const share = state.calls.find((call) => call.path === '/files/f1/share')
  expect(share.method).toBe('POST')
  expect(share.body).toEqual({ expires_in_seconds: 86400 })

  // The rendered link is the instance's own media route, which is the grant.
  await expect(page.getByLabel('Share link')).toHaveValue(/^https?:\/\/[^/]+\/media\/share\//)
  await expect(page.getByTestId('share-minted')).toContainText('It stops working on')
})

test('a private file says why there is no link to copy', async ({ page }) => {
  await open(page, { files: [file()] })

  await expect(page.getByTestId('share-private')).toHaveText('A private file has no audience outside you.')
  await expect(page.getByTestId('share-file')).toHaveCount(0)
})

test("a refusal is the instance's own sentence", async ({ page }) => {
  await open(page, {
    files: [file({ visibility: 'public_federated' })],
    shareRefusal: { status: 503, error: 'share links are not configured' },
  })

  await page.getByLabel('Copy a link notes.txt').click()
  await page.getByTestId('mint-share-link').click()

  await expect(page.getByTestId('share-refusal')).toHaveText('share links are not configured')
})
