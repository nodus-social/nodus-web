#!/usr/bin/env bash
# Build the client bundle and the SSR shell, then publish both to the webserver container.
#
#   ./web/deploy/lxc/deploy.sh
#   HOST=root@hyper.luxuride.eu VMID=100 ./web/deploy/lxc/deploy.sh
#
# The bundle is built in the development container (trunk, node, wasm target) because the
# output is a wasm module: it has no libc to mismatch. The SSR shell is a native binary and
# is built in rust:1-bookworm, the same base as the container, so its glibc matches.
set -euo pipefail

# shellcheck disable=SC2029

HOST="${HOST:-root@hyper.luxuride.eu}"
VMID="${VMID:-100}"
UNIT="${UNIT:-nodus-ssr}"
PORT="${PORT:-8080}"
DOCROOT="${DOCROOT:-/srv/nodus}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CACHE_VOLUME="${CACHE_VOLUME:-nodus-cargo}"
BINARY="${ROOT}/target-bookworm/release/nodus-web-server"

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

# Every ssh call is fenced, and the fence is the point: `BatchMode` refuses to sit at a
# prompt nobody is at, `ConnectTimeout` bounds the handshake, and the keepalive pair notices
# a peer that has stopped answering instead of waiting out the kernel's own patience. A
# deploy that loses its host has to end; it must not hold the terminal open for as long as
# the network takes to give up.
SSH_OPTS=(
  -o BatchMode=yes
  -o ConnectTimeout=10
  -o ServerAliveInterval=5
  -o ServerAliveCountMax=3
)

remote() { ssh "${SSH_OPTS[@]}" "${HOST}" "$@"; }

# Reads the unit's status, and is the only thing that says the deploy worked. The call is
# capped twice over: the ssh flags above bound the connection, and `timeout --kill-after`
# bounds the call itself. That second cap is not belt-and-braces — bash defers SIGTERM
# while ssh is in the foreground, so a probe that stops getting bytes back leaves `timeout`
# waiting on a shell that will not die, and only SIGKILL ends it. Eight hours of
# `poll_schedule_timeout` is what that looked like the last time this was left unbounded.
probe_status() {
  local attempt output
  for attempt in 1 2 3; do
    if output=$(timeout -k 5 20 ssh "${SSH_OPTS[@]}" "${HOST}" \
        "pct exec ${VMID} -- systemctl --no-pager --lines=0 status ${UNIT}" 2>&1); then
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

echo "==> packing the bundle"
tar -C "${ROOT}/dist" -czf /tmp/nodus-dist.tgz .

echo "==> stopping ${UNIT}"
remote "pct exec ${VMID} -- systemctl stop ${UNIT}"

echo "==> pushing the SSR shell"
scp -q "${SSH_OPTS[@]}" "${BINARY}" "${HOST}:/tmp/nodus-ssr-new"
remote "pct push ${VMID} /tmp/nodus-ssr-new /usr/local/bin/nodus-server --perms 0755"

echo "==> publishing the bundle to ${DOCROOT}"
scp -q "${SSH_OPTS[@]}" /tmp/nodus-dist.tgz "${HOST}:/tmp/nodus-dist.tgz"
remote "pct push ${VMID} /tmp/nodus-dist.tgz /tmp/nodus-dist.tgz"
# Old hashed assets are dropped so the directory cannot grow without bound; index.html is
# written last by tar, so a page loaded mid-publish still has a matching asset set.
remote "pct exec ${VMID} -- bash -lc 'rm -rf ${DOCROOT}/* && tar -C ${DOCROOT} -xzf /tmp/nodus-dist.tgz && chown -R caddy:caddy ${DOCROOT} && rm -f /tmp/nodus-dist.tgz'"

echo "==> starting ${UNIT}"
remote "pct exec ${VMID} -- systemctl start ${UNIT}"

for _ in $(seq 1 15); do
  # Bounded like every other call: a health check that cannot answer is a retry, not a wait.
  if timeout -k 5 20 ssh "${SSH_OPTS[@]}" "${HOST}" \
      "pct exec ${VMID} -- curl -fsS --max-time 3 http://127.0.0.1:${PORT}/healthz" >/dev/null 2>&1; then
    echo "==> ready"
    break
  fi
  sleep 1
done

# Not `|| true`: a deploy that cannot read the unit's status has not verified anything, so
# it reports the command that failed and exits non-zero rather than claiming success.
probe_status || exit 1
echo "==> done"
