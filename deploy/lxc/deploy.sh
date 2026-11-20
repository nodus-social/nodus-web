#!/usr/bin/env bash
# Build the client bundle and the SSR shell, then publish both to the webserver container.
#
#   ./web/deploy/lxc/deploy.sh
#   HOST=root@hyper.luxuride.eu VMID=100 ./web/deploy/lxc/deploy.sh
#
# The bundle is built in the development container (trunk, node, wasm target) because the
# output is a wasm module: it has no libc to mismatch. The SSR shell is a native binary and
# is built in rust:1-bookworm, the same base as the container, so its glibc matches.
#
# Safety: a deploy must never leave the site with the SSR unit stopped. Nothing is stopped:
# the binary is staged as `.new`, the bundle is unpacked beside the docroot and moved in with
# `mv -f` (per file, index.html last), and the unit is restarted exactly once at the end.
# A running process keeps its old inode across the rename, so the only gap is the restart
# itself. A trap on EXIT/INT/TERM restarts the unit on any exit path and fails the run if it
# is not active; a post-deploy probe of the public /home page fails the run unless the site
# answers 200 with the asset names this build just published.
set -euo pipefail

# shellcheck disable=SC2029

HOST="${HOST:-root@hyper.luxuride.eu}"
VMID="${VMID:-100}"
UNIT="${UNIT:-nodus-ssr}"
PORT="${PORT:-8080}"
DOCROOT="${DOCROOT:-/srv/nodus}"
SITE_URL="${SITE_URL:-https://nodus.luxuride.eu}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CACHE_VOLUME="${CACHE_VOLUME:-nodus-cargo}"
BINARY="${ROOT}/target-bookworm/release/nodus-web-server"

# Every ssh call is fenced, and the fence is the point: `BatchMode` refuses to sit at a
# prompt nobody is at, `ConnectTimeout` bounds the handshake, and the keepalive pair notices
# a peer that has stopped answering instead of waiting out the kernel's own patience. A
# deploy that loses its host has to end; it must not hold the terminal open for as long as
# the network takes to give up. `timeout --kill-after` caps each remote command as well:
# bash defers SIGTERM while ssh is in the foreground, so only SIGKILL ends a probe that
# stops getting bytes back. Eight hours of `poll_schedule_timeout` is what that looked like
# the last time this was left unbounded.
SSH_OPTS=(
  -o BatchMode=yes
  -o ConnectTimeout=10
  -o ServerAliveInterval=5
  -o ServerAliveCountMax=3
)

remote() { timeout -k 5 20 ssh "${SSH_OPTS[@]}" "${HOST}" "pct exec ${VMID} -- $*"; }
host_command() { timeout -k 5 20 ssh "${SSH_OPTS[@]}" "${HOST}" "$*"; }
push() { timeout -k 5 20 scp -q "${SSH_OPTS[@]}" "$1" "${HOST}:$2"; }

PUBLISH_LOCAL=""

# On any exit path the unit has to be active. The incoming exit status wins: a failed
# deploy stays failed even if the unit was rescued. SIGKILL cannot run a trap, which is why
# the script never stops the unit in the first place: there is no window in which a killed
# deploy leaves it stopped.
require_up() {
  local rc=$?
  local state
  trap - EXIT INT TERM
  set +e
  if [[ -n "${PUBLISH_LOCAL}" ]]; then rm -f "${PUBLISH_LOCAL}"; fi
  state=$(remote "systemctl is-active ${UNIT}" 2>/dev/null | tr -d '[:space:]')
  if [[ "${state}" != "active" ]]; then
    echo "!! ${UNIT} is ${state:-unknown}: restarting it (deploy interrupted)" >&2
    remote "systemctl restart ${UNIT}" >/dev/null 2>&1
    state=$(remote "systemctl is-active ${UNIT}" 2>/dev/null | tr -d '[:space:]')
  fi
  if [[ "${state}" != "active" ]]; then
    echo "!! FAILED to leave ${UNIT} active (state: ${state:-unknown})" >&2
    rc=1
  fi
  exit "${rc}"
}
trap require_up EXIT INT TERM

# Reads the unit's status, and is one of the things that says the deploy worked.
probe_status() {
  local attempt output
  for attempt in 1 2 3; do
    if output=$(remote "systemctl --no-pager --lines=0 status ${UNIT}" 2>&1); then
      printf '%s\n' "${output}"
      return 0
    fi
    echo "==> status probe ${attempt}/3 did not answer" >&2
    printf '%s\n' "${output}" >&2
    sleep 2
  done
  {
    echo "==> the unit's status could not be read, so this deploy cannot be verified" >&2
    printf 'command: timeout -k 5 20 ssh %s %s "pct exec %s -- systemctl --no-pager --lines=0 status %s"\n' \
      "${SSH_OPTS[*]}" "${HOST}" "${VMID}" "${UNIT}" >&2
    printf 'reason: %s\n' "${output}" >&2
  }
  return 1
}

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  echo "==> building the bundle in ${ROOT}"
  # trunk and the wasm target live in the development container; a host without them asks
  # the container to build, which keeps one copy of the toolchain.
  if command -v trunk >/dev/null 2>&1; then
    ( cd "${ROOT}" && node js/build.mjs && node js/css.mjs && trunk build --release )
  else
    devcontainer exec \
      --workspace-folder "$(cd "${ROOT}/.." && pwd)" \
      bash -lc 'cd web && node js/build.mjs && node js/css.mjs && trunk build --release'
  fi

  echo "==> building the SSR shell for bookworm"
  docker run --rm \
    -v "$(dirname "${ROOT}"):/src" \
    -v "${CACHE_VOLUME}:/cargo" \
    -w "/src/web" \
    -e CARGO_HOME=/cargo \
    rust:1-bookworm \
    cargo build --release --locked --bin nodus-web-server \
      --target-dir /src/web/target-bookworm
fi

[[ -x "${BINARY}" ]] || { echo "missing ${BINARY}" >&2; exit 1; }
[[ -f "${ROOT}/dist/index.html" ]] || { echo "missing ${ROOT}/dist/index.html" >&2; exit 1; }

# The publish helper runs inside the container, so its quoting is its own. It unpacks the
# bundle beside the docroot, verifies index.html landed, moves every entry into the docroot
# (index.html last), and only then drops files the new bundle does not contain. At no point
# is the docroot missing or empty, and a browser that read either index finds its assets.
PUBLISH_LOCAL=$(mktemp)
cat > "${PUBLISH_LOCAL}" <<'PUBLISH'
#!/usr/bin/env bash
set -euo pipefail
docroot="$1"
tarball="$2"
staging="${docroot}.new"

rm -rf "$staging"
mkdir -p "$staging"
tar -C "$staging" -xzf "$tarball"
test -f "$staging/index.html"
chown -R caddy:caddy "$staging"

find "$staging" -mindepth 1 -maxdepth 1 ! -name index.html | while IFS= read -r entry; do
  if [[ -d "$entry" && -d "$docroot/${entry##*/}" ]]; then
    # A directory cannot be renamed over an existing one; merge it instead, adding only
    # the files the new bundle has. What the old index named is still on disk.
    cp -a "$entry"/. "$docroot/${entry##*/}/"
    rm -rf "$entry"
  else
    mv -f "$entry" "$docroot"/
  fi
done
mv -f "$staging/index.html" "$docroot/index.html"
rm -rf "$staging"

# Old hashed assets are dropped so the directory cannot grow without bound, but only after
# the new index.html is in place: the page load that crossed the switch still finds both
# asset sets. Dotfiles (the trunk lock) are left alone, as the old publish left them.
cd "$docroot"
tar -tzf "$tarball" | sed 's#^\./##; s#/$##' | grep -v '^$' | sort -u > /tmp/nodus-new.list
find . -mindepth 1 ! -name '.*' -printf '%P\n' | while IFS= read -r p; do
  grep -qxF -- "$p" /tmp/nodus-new.list || rm -rf -- "$p"
done
rm -f /tmp/nodus-new.list "$tarball"
PUBLISH

echo "==> packing the bundle"
tar -C "${ROOT}/dist" -czf /tmp/nodus-dist.tgz .

echo "==> staging the SSR shell"
push "${BINARY}" /tmp/nodus-ssr-new
host_command "pct push ${VMID} /tmp/nodus-ssr-new /usr/local/bin/nodus-server.new --perms 0755"

echo "==> staging the bundle"
push /tmp/nodus-dist.tgz /tmp/nodus-dist.tgz
host_command "pct push ${VMID} /tmp/nodus-dist.tgz /tmp/nodus-dist.tgz"
push "${PUBLISH_LOCAL}" /tmp/nodus-publish.sh
host_command "pct push ${VMID} /tmp/nodus-publish.sh /tmp/nodus-publish.sh --perms 0755"

# Both staged files must exist before anything moves: if either push was short, the
# docroot and the binary are still the previous, working pair.
remote "test -f /usr/local/bin/nodus-server.new && test -f /tmp/nodus-dist.tgz && test -f /tmp/nodus-publish.sh"

echo "==> publishing the bundle to ${DOCROOT}"
remote "/tmp/nodus-publish.sh ${DOCROOT} /tmp/nodus-dist.tgz"
remote "rm -f /tmp/nodus-publish.sh"

echo "==> installing the SSR shell"
remote "mv -f /usr/local/bin/nodus-server.new /usr/local/bin/nodus-server"

echo "==> restarting ${UNIT}"
remote "systemctl restart ${UNIT}"

echo "==> waiting for ${UNIT}"
ready=0
for _ in $(seq 1 20); do
  # Bounded like every other call: a health check that cannot answer is a retry, not a wait.
  if remote "curl -fsS --max-time 3 http://127.0.0.1:${PORT}/healthz" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done
[[ "${ready}" == "1" ]] || echo "!! ${UNIT} did not answer /healthz; the public assertion decides" >&2

# The public page is what the operator sees, so it is the thing that must be checked. A
# deploy that leaves the unit stopped or the bundle half-published has to exit non-zero.
assert_served() {
  local code asset failures=0
  local -a served=() built=()
  local page="${SITE_URL}/home"

  if ! code=$(timeout -k 5 20 curl -sS -o /tmp/nodus-home.html -w '%{http_code}' --max-time 15 "${page}"); then
    echo "!! GET ${page} failed" >&2
    return 1
  fi
  if [[ "${code}" != "200" ]]; then
    echo "!! GET ${page} returned ${code} (expected 200)" >&2
    return 1
  fi

  mapfile -t served < <(grep -oE '[A-Za-z0-9_.-]+-[0-9a-f]{16}[A-Za-z0-9_.-]*\.(js|wasm|css)' /tmp/nodus-home.html | sort -u)
  mapfile -t built < <(grep -oE '[A-Za-z0-9_.-]+-[0-9a-f]{16}[A-Za-z0-9_.-]*\.(js|wasm|css)' "${ROOT}/dist/index.html" | sort -u)
  if [[ ${#built[@]} -eq 0 ]]; then
    echo "!! ${ROOT}/dist/index.html names no hashed assets, so nothing was verified" >&2
    return 1
  fi

  # Every asset the served page names must be one this build published.
  for asset in "${served[@]}"; do
    printf '%s\n' "${built[@]}" | grep -qxF -- "${asset}" || {
      echo "!! ${page} names ${asset}, which is not in ${ROOT}/dist" >&2
      failures=1
    }
  done
  # Every module this build published must be named by the served page. The stylesheet is
  # inlined by the SSR shell, so it is checked by URL below instead.
  for asset in "${built[@]}"; do
    case "${asset}" in *.css) continue ;; esac
    printf '%s\n' "${served[@]}" | grep -qxF -- "${asset}" || {
      echo "!! ${ROOT}/dist has ${asset}, but ${page} does not name it" >&2
      failures=1
    }
  done
  # The names matching is not enough if the bytes behind them are missing.
  for asset in "${built[@]}"; do
    code=$(timeout -k 5 20 curl -sS -o /dev/null -w '%{http_code}' --max-time 15 "${SITE_URL}/${asset}" 2>/dev/null) || code=000
    [[ "${code}" == "200" ]] || {
      echo "!! ${SITE_URL}/${asset} returned ${code} (expected 200)" >&2
      failures=1
    }
  done
  [[ "${failures}" == "0" ]]
}

# Not `|| true`: a deploy that cannot read the unit's status has not verified anything, so
# it reports the command that failed and exits non-zero rather than claiming success.
probe_status || exit 1
assert_served || exit 1
echo "==> ${UNIT} active, ${SITE_URL}/home 200, assets match ${ROOT}/dist"
echo "==> done"
