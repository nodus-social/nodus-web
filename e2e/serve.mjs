//! A static server for the built client, with the one behaviour a plain file server
//! lacks: an unknown path answers `index.html` instead of 404.
//!
//! That is what the deployment does (Caddy `try_files {path} /index.html`), and without
//! it every deep link — `/settings`, `/home`, `/status/{host}/{id}` — fails in the test
//! environment while working in production, which would make these tests lie.

import { createReadStream, existsSync, statSync } from 'node:fs'
import { createServer } from 'node:http'
import { extname, join, normalize } from 'node:path'

const root = new URL('../dist', import.meta.url).pathname
// The push worker is the one asset the SSR host serves itself, from
// `crates/web-server/assets/sw.js` (`service_worker` in src/main.rs). The worker test
// registers the real file, so this server answers `/sw.js` with it as JavaScript rather
// than falling back to index.html — an HTML body is refused by `register`.
const worker = new URL('../crates/web-server/assets/sw.js', import.meta.url).pathname
const port = Number(process.env.PORT ?? 4173)

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
}

createServer((request, response) => {
  const path = decodeURIComponent(new URL(request.url, 'http://localhost').pathname)
  const candidate = normalize(join(root, path))
  const inside = candidate.startsWith(root)
  const file = path === '/sw.js'
    ? worker
    : inside && existsSync(candidate) && statSync(candidate).isFile()
      ? candidate
      : null
  const target = file ?? join(root, 'index.html')

  response.writeHead(200, {
    'content-type': TYPES[extname(target)] ?? 'application/octet-stream',
    'cache-control': 'no-store',
  })
  createReadStream(target).pipe(response)
}).listen(port, '127.0.0.1', () => {
  console.log(`serving ${root} on http://127.0.0.1:${port}`)
})
