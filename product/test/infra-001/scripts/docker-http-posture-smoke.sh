#!/bin/bash
# #783 docker-build + boot-into-HTTP smoke (load-bearing — lesson #4582:
# Dockerfile correctness needs a REAL build, not static review).
#
# Proves the SHIPPED image boots HTTP-serving by DEFAULT (the regression: it
# previously booted with the binary default http.enabled=false because
# UNIMATRIX_HTTP_ENABLED lived only in docker-compose.yml, which the release
# does not ship — so a clean `docker run` misrouted writes to the path-hash
# store, #783) and that a write through the per-slug HTTP route lands in the
# per-slug store `/data/.unimatrix/<slug>/unimatrix.db`, NOT the hash-dir store.
#
# The runtime image is distroless (no shell, no coreutils). ALL filesystem
# inspection of the data volume is therefore done via a `busybox` sidecar that
# mounts the same volume; never `docker exec` into the distroless container.
#
# Usage:   product/test/infra-001/scripts/docker-http-posture-smoke.sh
# Env:     IMAGE   pre-built image tag to test instead of building (optional)
#          KEEP    set to 1 to keep the container/volume for inspection
#
# Requires Docker. Exits 3 (with a clear SKIP reason) when Docker is
# unavailable so CI flags it as a deferred step rather than false-green.
# In IMAGE= mode, exits 4 when the prebuilt tag can neither be pulled nor found
# locally (tag not pushed / network unhealthy) — distinct from fail()'s exit 1.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
SLUG="arch-research"
CNAME="uni-783-smoke-$$"
VOL="uni-783-data-$$"
PORT=18443

log() { printf '[783-smoke] %s\n' "$*"; }
fail() { printf '[783-smoke] FAIL: %s\n' "$*" >&2; exit 1; }

cleanup() {
  if [ "${KEEP:-0}" != "1" ]; then
    docker rm -f "$CNAME" >/dev/null 2>&1 || true
    docker volume rm "$VOL" >/dev/null 2>&1 || true
  fi
  [ -n "${TMP:-}" ] && rm -rf "$TMP"
}
trap cleanup EXIT

# Run a command in a busybox sidecar with the data volume mounted read-only at
# /data (default) — used for all distroless-volume filesystem inspection.
vol() { docker run --rm -v "$VOL:/data:ro" busybox "$@"; }

# WAL-robust, non-decreasing size signal over a store DIRECTORY (AC-05/ADR-005).
# The main unimatrix.db file size is NOT monotone on one small committed write:
# under WAL autocheckpoint (~1000 pages, ADR #329) the write can sit in -wal and
# not enlarge the main .db until checkpoint. `du -s` over the store dir counts
# unimatrix.db + -wal + -shm, giving a signal that grows on a real write. All
# sampling is read-only via vol() — never `docker exec` into the distroless image.
store_size() { vol du -s "$1" | awk '{print $1}'; }

# -- Preflight: Docker must be available -----------------------------------
if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then
  echo "[783-smoke] SKIP: Docker not available in this environment." >&2
  echo "[783-smoke] This smoke MUST run in a Docker-capable CI job (deferred step)." >&2
  exit 3
fi

# -- Build (or reuse) the production image ---------------------------------
# Precondition for the IMAGE= mode: the caller must set/activate IMAGE; in that
# mode this script PULLS the tag (or falls back to a present local image) rather
# than building — the first docker op below is `inspect`, which never pulls, so a
# cross-runner cache miss would otherwise false-fail the gate (#795).
if [ -n "${IMAGE:-}" ]; then
  log "using prebuilt image: $IMAGE"
  # Pull-preferring with local fallback: on a fresh hosted runner this really
  # pulls the pushed bytes (ADR-002); on a dev box with a locally-built tag (not
  # in any registry) it falls back to the present image; only when genuinely
  # unavailable do we exit 4 (distinct from fail()'s exit 1). The `|| ... || {}`
  # chain is intentionally not routed through fail() and must run all arms before
  # set -e/pipefail trips.
  docker pull "$IMAGE" \
    || docker image inspect "$IMAGE" >/dev/null 2>&1 \
    || { printf '[783-smoke] FAIL: could not pull %s (confirm the tag was pushed and network is healthy)\n' "$IMAGE" >&2; exit 4; }
else
  IMAGE="unimatrix:783-smoke"
  log "building image $IMAGE from $REPO_ROOT/Dockerfile (this is slow) ..."
  docker build -t "$IMAGE" "$REPO_ROOT" >/dev/null || fail "docker build failed"
fi

# Sanity: the runtime image must carry the baked posture env.
docker image inspect "$IMAGE" --format '{{json .Config.Env}}' \
  | grep -q 'UNIMATRIX_HTTP_ENABLED=true' \
  || fail "image ENV is missing UNIMATRIX_HTTP_ENABLED=true"

docker volume create "$VOL" >/dev/null

# -- Boot 1: clean `docker run`, NO -e UNIMATRIX_HTTP_ENABLED ---------------
# The image ENV alone must enable HTTP. PUBLIC_URL is pinned only to avoid the
# loud placeholder warning; it is NOT required to boot (public_url.rs degrades).
log "boot 1: clean docker run (no UNIMATRIX_HTTP_ENABLED override) ..."
docker run -d --name "$CNAME" \
  -v "$VOL:/data" \
  -e UNIMATRIX_PUBLIC_URL="https://localhost:${PORT}" \
  -p "${PORT}:8443" \
  "$IMAGE" >/dev/null

# GATE 1: the HTTP listener binds by default. Proven by the daemon log line
# "HTTP transport active" — if the image booted HTTP-off (the #783 regression)
# this line never appears (we'd instead see the "set [http] enabled" hint).
deadline=$(( $(date +%s) + 90 ))
while :; do
  if docker logs "$CNAME" 2>&1 | grep -q "HTTP transport active"; then
    break
  fi
  if docker logs "$CNAME" 2>&1 | grep -q "set .http. enabled"; then
    fail "daemon logged the HTTP-disabled hint => image booted HTTP-OFF (the #783 regression)"
  fi
  [ "$(date +%s)" -gt "$deadline" ] && fail "HTTP listener never became active => image booted HTTP-OFF (the #783 regression)"
  sleep 2
done
log "daemon logged 'HTTP transport active' => image boots HTTP-ON by default. PASS gate 1"

# Discover the path-hash data dir (sibling of the per-slug dirs).
HASH_DIR="$(vol sh -c 'ls -d /data/.unimatrix/*/ 2>/dev/null | while read d; do [ -f "$d/token" ] && echo "$d"; done | head -1')"
HASH_DIR="${HASH_DIR%/}"
[ -n "$HASH_DIR" ] || fail "could not locate the path-hash data dir (no token found)"
log "path-hash data dir: $HASH_DIR"

# -- Register the slug, then restart so [[projects]] is applied -------------
log "registering slug '$SLUG' ..."
docker run --rm -v "$VOL:/data" "$IMAGE" --project-dir /data project register "$SLUG" \
  || fail "project register $SLUG failed"
docker restart "$CNAME" >/dev/null

# GATE: guard must NOT abort boot (HTTP is on) — listener becomes active again.
deadline=$(( $(date +%s) + 90 ))
while ! docker logs "$CNAME" 2>&1 | grep -q "HTTP transport active"; do
  [ "$(date +%s)" -gt "$deadline" ] && fail "listener did not re-activate after restart (guard may have aborted boot, or HTTP off)"
  sleep 2
done

# -- AC-05 BEFORE sample: after register+restart, BEFORE the per-slug POST ---
# WAL-robust dir-size of both stores so the post-write deltas are comparable.
SLUG_DIR="/data/.unimatrix/${SLUG}"
SLUG_BEFORE="$(store_size "$SLUG_DIR")"
HASH_BEFORE="$(store_size "$HASH_DIR")"

# -- Write through the per-slug HTTPS route /v1/<slug>/observe --------------
# Pull token + cert out of the volume (busybox sidecar) for a cert-pinned,
# bearer-auth POST. Token/cert live under the path-hash dir, NOT /data root.
TMP="$(mktemp -d)"
vol cat "$HASH_DIR/token" > "$TMP/token"
vol cat "$HASH_DIR/tls/cert.pem" > "$TMP/cert.pem"
TOKEN="$(tr -d '\r\n' < "$TMP/token")"
[ -n "$TOKEN" ] || fail "empty bearer token"
[ -s "$TMP/cert.pem" ] || fail "empty TLS cert"

OBSERVE_URL="https://localhost:${PORT}/v1/${SLUG}/observe"
log "POST SessionRegister -> $OBSERVE_URL (per-slug funnel) ..."
code=$(curl -sS --cacert "$TMP/cert.pem" -o /dev/null -w '%{http_code}' \
  -X POST "$OBSERVE_URL" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"type":"SessionRegister","session_id":"s-783","cwd":"/x","agent_role":null,"feature":null}' \
  ) || fail "curl to per-slug observe route failed"
# 204 = accepted by the slug router; 404 would mean the slug is NOT routed
# (HTTP off / not registered) — the bug symptom.
[ "$code" = "204" ] || fail "per-slug observe returned HTTP $code (expected 204) => slug not routed over HTTP"
log "per-slug observe route returned 204 => slug IS routed over HTTP. PASS gate 2"

# -- Assert the per-slug store exists (register created a real persistent db)
SLUG_DB="/data/.unimatrix/${SLUG}/unimatrix.db"
vol test -f "$SLUG_DB" || fail "per-slug store $SLUG_DB missing"
log "per-slug store present at $SLUG_DB. PASS gate 3"

# -- AC-05 AFTER sample: after the 204 is confirmed -------------------------
SLUG_AFTER="$(store_size "$SLUG_DIR")"
HASH_AFTER="$(store_size "$HASH_DIR")"

# GATE 4 (AC-05): the per-slug write must have GROWN the per-slug store and the
# hash store must be UNCHANGED — pinning the literal #783 mis-route symptom
# (slug dir empty, hash dir populated) even if the route returns 204 via some
# future different mechanism. Both via the existing fail() (exit 1).
[ "$SLUG_AFTER" -gt "$SLUG_BEFORE" ] \
  || fail "per-slug store did not grow after the write (before=$SLUG_BEFORE after=$SLUG_AFTER) => write did not land in the per-slug store"
[ "$HASH_AFTER" -eq "$HASH_BEFORE" ] \
  || fail "hash store changed after a per-slug write (before=$HASH_BEFORE after=$HASH_AFTER) => write mis-routed to the hash dir (the #783 symptom)"
log "per-slug store grew ($SLUG_BEFORE -> $SLUG_AFTER) and hash store unchanged ($HASH_BEFORE) => write landed correctly. PASS gate 4 (AC-05)"

log "ALL GATES PASSED — clean image boots HTTP-on and routes the registered slug over HTTPS."
