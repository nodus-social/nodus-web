#!/usr/bin/env node
// Skip policy for the web e2e suite: every skipped row must be explained.
//
// `skip-allowlist.json` lists call sites, not rows: one `test.skip` is two rows
// across tablet and phone, one `test.fixme` is three, so the eight call sites
// in `tests/` produce the seventeen rows #42 counts.
//
// The JSON report (default test-results/results.json) is authoritative: a
// `test.skip` inside a `test.fixme` never runs, so --source-only is advisory.
// A --project or file-restricted run only enforces reported files; a full run
// also rejects stale entries. Non-zero `flaky` fails: a flake is not green.

import { readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const testsDir = join(here, 'tests')
const allowlistPath = join(here, 'skip-allowlist.json')
const defaultReportPath = join(here, 'test-results', 'results.json')

// The projects in playwright.config.mjs; "*" expands to all of them.
const PROJECTS = ['desktop', 'tablet', 'phone']
const KINDS = new Set(['skip', 'fixme'])

const sourceOnly = process.argv.includes('--source-only')
const reportArgument = process.argv.slice(2).find((argument) => !argument.startsWith('--'))

/** Paths from the allowlist and the report both become relative to tests/. */
function belowTests(file) {
  const posix = file.replaceAll('\\', '/')
  const marker = '/tests/'
  const at = posix.lastIndexOf(marker)
  if (at !== -1) return posix.slice(at + marker.length)
  if (posix.startsWith('tests/')) return posix.slice('tests/'.length)
  return posix
}

function loadAllowlist() {
  let document
  try {
    document = JSON.parse(readFileSync(allowlistPath, 'utf8'))
  } catch (error) {
    throw new Error(`cannot read ${allowlistPath}: ${error.message}`)
  }
  if (document.version !== 1) throw new Error(`${allowlistPath}: unsupported version ${document.version}`)
  const entries = document.entries ?? []
  const seen = new Set()
  for (const entry of entries) {
    for (const field of ['file', 'title', 'kind', 'projects', 'reason', 'issue', 'documented']) {
      if (!entry[field]) throw new Error(`${allowlistPath}: entry is missing ${field}: ${JSON.stringify(entry)}`)
    }
    if (!KINDS.has(entry.kind)) throw new Error(`${allowlistPath}: kind must be skip or fixme: ${JSON.stringify(entry)}`)
    if (!Array.isArray(entry.projects) || entry.projects.length === 0) {
      throw new Error(`${allowlistPath}: projects must be a non-empty array: ${JSON.stringify(entry)}`)
    }
    for (const project of entry.projects) {
      if (project !== '*' && !PROJECTS.includes(project)) {
        throw new Error(`${allowlistPath}: unknown project ${project} in ${JSON.stringify(entry)}`)
      }
    }
    const key = `${belowTests(entry.file)} :: ${entry.title}`
    if (seen.has(key)) throw new Error(`${allowlistPath}: duplicate entry for ${key}`)
    seen.add(key)
  }
  return entries
}

/** Expands call sites into the rows the report must contain. */
function expectedRows(entries) {
  const rows = new Map()
  for (const entry of entries) {
    const projects = entry.projects.includes('*') ? PROJECTS : entry.projects
    for (const project of projects) {
      rows.set(`${belowTests(entry.file)} :: ${entry.title} :: ${project}`, { entry, project })
    }
  }
  return rows
}

/** Returns every spec file in the report and every row the run skipped. */
function readReport(report) {
  const files = new Set()
  const rows = []
  const walk = (suite) => {
    for (const spec of suite.specs ?? []) {
      files.add(belowTests(spec.file))
      for (const test of spec.tests ?? []) {
        const annotation = (test.annotations ?? []).find((candidate) => KINDS.has(candidate.type))
        const skipped = test.status === 'skipped' || (test.results ?? []).some((result) => result.status === 'skipped')
        if (!skipped) continue
        rows.push({
          file: belowTests(spec.file), title: spec.title, project: test.projectName,
          kind: annotation?.type, reason: annotation?.description ?? '',
        })
      }
    }
    for (const child of suite.suites ?? []) walk(child)
  }
  for (const suite of report.suites ?? []) walk(suite)
  return { files, rows }
}

function checkReport(reportPath) {
  let report
  try {
    report = JSON.parse(readFileSync(reportPath, 'utf8'))
  } catch (error) {
    throw new Error(`cannot read the Playwright JSON report at ${reportPath}: ${error.message}`)
  }
  const expected = expectedRows(loadAllowlist())
  const { files, rows } = readReport(report)
  const problems = []
  const matched = new Set()

  for (const row of rows) {
    const key = `${row.file} :: ${row.title} :: ${row.project}`
    if (!row.kind) {
      problems.push(`skipped row has no skip/fixme annotation: ${key}`)
      continue
    }
    const hit = expected.get(key)
    if (!hit) {
      problems.push(`skip not allowlisted: ${key} (${row.kind}${row.reason ? `: ${row.reason}` : ''})`)
      continue
    }
    matched.add(key)
    if (row.kind !== hit.entry.kind) {
      problems.push(`kind drift: ${key} is ${row.kind}, the allowlist says ${hit.entry.kind}`)
    }
    if (row.reason && row.reason !== hit.entry.reason) {
      problems.push(`reason drift: ${key}\n  report:    ${row.reason}\n  allowlist: ${hit.entry.reason}`)
    }
  }

  for (const [key, hit] of expected) {
    if (matched.has(key)) continue
    // A partial run has no opinion on files it did not run; the full CI run
    // does, and that is where a stale entry fails.
    if (!files.has(belowTests(hit.entry.file))) continue
    problems.push(`stale allowlist entry: ${key} (${hit.entry.kind}) does not skip in this report`)
  }

  const flaky = report.stats?.flaky ?? 0
  if (flaky > 0) problems.push(`flaky: ${flaky} test(s) passed only after a retry; run with --retries=0`)

  const exercised = [...expected.values()].filter((hit) => files.has(belowTests(hit.entry.file))).length
  console.log(`skip policy: ${rows.length} skipped rows, ${exercised} allowlisted rows expected, ${files.size} spec files in the report`)
  reportProblems(problems)
}

/** Greps spec sources for test.skip/test.fixme call sites. Advisory only. */
function scanSource(file) {
  const lines = readFileSync(file, 'utf8').split('\n')
  const declarations = []
  // Titles are matched on one line; every spec in the suite writes them so.
  const declarationPattern = /^\s*test(?:\.(fixme|skip))?\(\s*(['"`])(.*?)\2/
  const callPattern = /test\.(skip|fixme)\(/g

  lines.forEach((line, index) => {
    const match = declarationPattern.exec(line)
    if (match) declarations.push({ line: index + 1, kind: match[1] ?? 'test', title: match[3] })
  })

  const sites = []
  lines.forEach((line, index) => {
    if (/^\s*(\/\/|\*)/.test(line)) return
    callPattern.lastIndex = 0
    let match
    while ((match = callPattern.exec(line))) {
      const lineNumber = index + 1
      const enclosing = [...declarations].reverse().find((declaration) => declaration.line <= lineNumber)
      if (!enclosing) {
        sites.push({ line: lineNumber, kind: match[1], title: '(unknown test)', dead: false })
        continue
      }
      // A call on the declaration line is the call site; a skip inside a
      // test.fixme body can never run, because fixme wins before the body.
      const dead = enclosing.kind === 'fixme' && enclosing.line !== lineNumber
      sites.push({ line: lineNumber, kind: match[1], title: enclosing.title, dead })
    }
  })
  return sites
}

function specFiles() {
  const found = []
  const walk = (directory) => {
    for (const name of readdirSync(directory)) {
      const path = join(directory, name)
      if (statSync(path).isDirectory()) walk(path)
      else if (name.endsWith('.spec.mjs')) found.push(path)
    }
  }
  walk(testsDir)
  return found.sort()
}

function checkSource() {
  const allowed = new Map()
  const problems = []
  for (const entry of loadAllowlist()) allowed.set(`${belowTests(entry.file)} :: ${entry.title}`, entry)

  const seen = new Set()
  let live = 0
  let dead = 0
  for (const path of specFiles()) {
    const file = relative(here, path).replaceAll('\\', '/')
    for (const site of scanSource(path)) {
      if (site.dead) {
        dead += 1
        console.warn(`skip policy: advisory: ${file}:${site.line} test.${site.kind} inside a test.fixme never runs`)
        continue
      }
      live += 1
      const key = `${belowTests(file)} :: ${site.title}`
      const entry = allowed.get(key)
      if (!entry) {
        problems.push(`source call site not allowlisted: ${file}:${site.line} test.${site.kind} "${site.title}"`)
        continue
      }
      seen.add(key)
      if (entry.kind !== site.kind) problems.push(`source kind drift: ${key} is test.${site.kind}, the allowlist says ${entry.kind}`)
    }
  }

  for (const [key, entry] of allowed) {
    if (!seen.has(key)) problems.push(`stale allowlist entry: ${key} (no live test.${entry.kind} call site in source)`)
  }

  console.log(`skip policy source scan: ${live} live call sites, ${dead} shadowed by a fixme (advisory), ${allowed.size} allowlisted`)
  reportProblems(problems)
}

function reportProblems(problems) {
  for (const problem of problems) console.error(`skip policy: ${problem}`)
  if (problems.length > 0) {
    process.exitCode = 1
    return
  }
  console.log('skip policy: ok')
}

try {
  if (sourceOnly) checkSource()
  else checkReport(reportArgument ?? defaultReportPath)
} catch (error) {
  console.error(`skip policy: ${error.message}`)
  process.exitCode = 1
}
