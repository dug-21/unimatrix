#!/bin/bash
# #767 first-boot EMBEDDING-READINESS smoke (load-bearing — the real deliverable
# of the #767 fix; lesson #5130: a deploy posture carried in compose must be
# verified by a REAL run against the SHIPPED volume topology, not static review).
#
# THE BUG THIS GATE CATCHES (#767): the image bakes NO model (pattern #4658), so
# first boot must DOWNLOAD the ONNX models into /shared. docker-compose.yml had
# drifted to mounting `/shared:ro`, so the download `create_dir_all` hit EROFS,
# embed retried (10s/20s/40s backoff) and landed permanently EmbedFailed —
# `context_store`/`context_search` could not embed. The container was still
# "up" and `/health` (compile-time constants) still returned 200 the whole time.
#
# THIS GATE ASSERTS EMBED-READINESS, NOT LIVENESS. A `/health` probe or a
# "container is up" check PASSES on the broken state — that is exactly how #767
# slipped. So this gate does a REAL `context_store` followed by a REAL
# `context_search` over the per-slug MCP route. `context_search` MUST embed the
# query; if the model never loaded the call returns an embed error instead of a
# result. Exercising the embed path (not the liveness path) is the whole point.
#
# COMPOSE-DECLARED /shared MODE (load-bearing): the gate mounts /shared in the
# mode that docker-compose.yml declares (parsed at runtime, NOT hardcoded). The
# fix makes that :rw; if a future edit drifts it back to `:ro` on a virgin
# volume, this gate boots read-only, the download fails, and the embed round
# trip fails the gate — re-closing the #767 blind spot rather than testing a
# writable mount the shipped compose does not use.
#
# BACKOFF-AWARE WAIT: the embed retry monitor (embed_handle.rs:184-189) masks
# Failed as NotReady until MAX_RETRIES, with 10s/20s/40s backoff (~70s+). The
# readiness wait runs PAST that full window so a slow-but-healthy first download
# does not flake the gate.
#
# The runtime image is distroless (no shell, no coreutils). ALL filesystem
# inspection of the volumes is done via a `busybox` sidecar; never `docker exec`
# into the distroless container.
#
# Usage:   product/test/infra-001/scripts/docker-embed-readiness-smoke.sh
# Env:     IMAGE   pre-built image tag to test instead of building (optional)
#          KEEP    set to 1 to keep the container/volumes for inspection
#          READY_TIMEOUT_SECS  override the embed-readiness deadline (default 180)
#
# Exit contract (identical shape to docker-http-posture-smoke.sh so the SAME
# release-gate-lib.sh run_smoke_gate spine consumes it):
#   0 = ran + passed (terminal marker printed)
#   1 = ran + failed (fail())
#   3 = self-SKIPPED, LOUD: Docker OR network unavailable (deferred step, never a
#       silent pass — a silent skip would recreate the #767 blind spot)
#   4 = IMAGE= prebuilt tag could neither be pulled nor found locally (#795)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
SLUG="arch-research"
CNAME="uni-767-embed-$$"
DVOL="uni-767-data-$$"
SVOL="uni-767-shared-$$"
PORT=18444
READY_TIMEOUT_SECS="${READY_TIMEOUT_SECS:-180}"

log() { printf '[767-embed-smoke] %s\n' "$*"; }
fail() { printf '[767-embed-smoke] FAIL: %s\n' "$*" >&2; exit 1; }
# LOUD skip: emit on stderr AND exit 3 so the release gate flags a deferred step
# rather than a false-green. Never `exit 0` on a skip.
skip() {
  printf '[767-embed-smoke] SKIP: %s\n' "$*" >&2
  printf '[767-embed-smoke] This smoke MUST run in a Docker + network capable CI job (deferred step).\n' >&2
  exit 3
}

cleanup() {
  if [ "${KEEP:-0}" != "1" ]; then
    docker rm -f "$CNAME" >/dev/null 2>&1 || true
    docker volume rm "$DVOL" "$SVOL" >/dev/null 2>&1 || true
  fi
  [ -n "${TMP:-}" ] && rm -rf "$TMP"
}
trap cleanup EXIT

# busybox sidecar with the SHARED volume mounted read-only — for distroless
# filesystem inspection of the downloaded model files.
shared() { docker run --rm -v "$SVOL:/shared:ro" busybox "$@"; }
# busybox sidecar with the DATA volume mounted read-only — for token/cert pull.
data_ro() { docker run --rm -v "$DVOL:/data:ro" busybox "$@"; }

# -- Parse the COMPOSE-DECLARED /shared mount mode (load-bearing) ------------
# The gate mounts /shared in whatever mode the shipped compose declares so a
# future `:ro`-without-prepopulate drift FAILS here. Default (no flag) = :rw.
COMPOSE_FILE="$REPO_ROOT/docker-compose.yml"
[ -f "$COMPOSE_FILE" ] || fail "docker-compose.yml not found at $COMPOSE_FILE"
SHARED_LINE="$(grep -E '^[[:space:]]*-[[:space:]]*unimatrix-shared:/shared' "$COMPOSE_FILE" | head -1 || true)"
[ -n "$SHARED_LINE" ] || fail "could not find the unimatrix-shared:/shared mount in docker-compose.yml"
if printf '%s' "$SHARED_LINE" | grep -qE ':/shared:ro([[:space:]]|$)'; then
  SHARED_MODE=":ro"
else
  SHARED_MODE=""   # :rw default (compose declares no mode flag)
fi
log "compose-declared /shared mount mode: ${SHARED_MODE:-:rw (default)}"

# -- Preflight: Docker must be available (LOUD skip) ------------------------
if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then
  skip "Docker not available in this environment."
fi
# -- Preflight: network must be reachable (the model downloads from HuggingFace).
# A network-unavailable run is a LOUD skip, never a silent pass.
if ! curl -sSf -m 15 -o /dev/null https://huggingface.co 2>/dev/null; then
  skip "HuggingFace (https://huggingface.co) unreachable — first-boot model download cannot run."
fi

# -- Build (or reuse) the production image ----------------------------------
if [ -n "${IMAGE:-}" ]; then
  log "using prebuilt image: $IMAGE"
  docker pull "$IMAGE" \
    || docker image inspect "$IMAGE" >/dev/null 2>&1 \
    || { printf '[767-embed-smoke] FAIL: could not pull %s (confirm the tag was pushed and network is healthy)\n' "$IMAGE" >&2; exit 4; }
else
  IMAGE="unimatrix:767-embed-smoke"
  log "building image $IMAGE from $REPO_ROOT/Dockerfile (this is slow) ..."
  docker build -t "$IMAGE" "$REPO_ROOT" >/dev/null || fail "docker build failed"
fi

docker volume create "$DVOL" >/dev/null
docker volume create "$SVOL" >/dev/null

# -- Register the slug so [[projects]] routes /v1/<slug>/mcp ----------------
log "registering slug '$SLUG' ..."
docker run --rm -v "$DVOL:/data" "$IMAGE" --project-dir /data project register "$SLUG" \
  || fail "project register $SLUG failed"

# -- Boot: BOTH volumes mounted; /shared in the COMPOSE-DECLARED mode -------
# This is the topology the shipped compose produces. On a virgin :ro /shared
# (the #767 regression) the first-boot download fails and embed never readies.
log "boot: docker run with unimatrix-data:/data and unimatrix-shared:/shared${SHARED_MODE} ..."
docker run -d --name "$CNAME" \
  -v "$DVOL:/data" \
  -v "$SVOL:/shared${SHARED_MODE}" \
  -e UNIMATRIX_HTTP_ENABLED="true" \
  -e UNIMATRIX_PUBLIC_URL="https://localhost:${PORT}" \
  -p "${PORT}:8443" \
  "$IMAGE" >/dev/null

# GATE 1: HTTP listener binds (so we can drive the MCP round trip at all).
deadline=$(( $(date +%s) + 90 ))
while ! docker logs "$CNAME" 2>&1 | grep -q "HTTP transport active"; do
  [ "$(date +%s)" -gt "$deadline" ] && fail "HTTP listener never became active"
  sleep 2
done
log "HTTP transport active. PASS gate 1 (listener up — NOTE: this is liveness, NOT the embed assertion)"

# -- Pull token + cert from the data volume for a cert-pinned, authed POST --
HASH_DIR="$(data_ro sh -c 'ls -d /data/.unimatrix/*/ 2>/dev/null | while read d; do [ -f "$d/token" ] && echo "$d"; done | head -1')"
HASH_DIR="${HASH_DIR%/}"
[ -n "$HASH_DIR" ] || fail "could not locate the path-hash data dir (no token found)"
TMP="$(mktemp -d)"
data_ro cat "$HASH_DIR/token" > "$TMP/token"
data_ro cat "$HASH_DIR/tls/cert.pem" > "$TMP/cert.pem"
TOKEN="$(tr -d '\r\n' < "$TMP/token")"
[ -n "$TOKEN" ] || fail "empty bearer token"
[ -s "$TMP/cert.pem" ] || fail "empty TLS cert"

MCP_URL="https://localhost:${PORT}/v1/${SLUG}/mcp"

# mcp_call BODY  -> performs ONE JSON-RPC POST, echoes the response body.
# Streamable-HTTP returns either application/json or an SSE event stream; the
# JSON-RPC payload is on the (last) `data:` line for SSE, or the whole body for
# JSON. We normalise both to the JSON object on stdout.
mcp_call() {
  local body="$1" extra_hdr="${2:-}"
  local raw
  raw=$(curl -sS --cacert "$TMP/cert.pem" \
    -X POST "$MCP_URL" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    ${extra_hdr:+-H "$extra_hdr"} \
    --data-binary "$body") || return 1
  # SSE: take the last non-empty `data:` line; else pass the body through.
  if printf '%s' "$raw" | grep -q '^data:'; then
    printf '%s' "$raw" | sed -n 's/^data: \{0,1\}//p' | grep -v '^$' | tail -1
  else
    printf '%s' "$raw"
  fi
}

# -- MCP handshake: initialize -> capture Mcp-Session-Id -> initialized ------
INIT_BODY='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"767-embed-smoke","version":"0"}}}'
# Need the response HEADERS to read Mcp-Session-Id, so do this call with -D.
log "MCP initialize handshake on $MCP_URL ..."
curl -sS --cacert "$TMP/cert.pem" -D "$TMP/init.hdr" -o "$TMP/init.body" \
  -X POST "$MCP_URL" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  --data-binary "$INIT_BODY" || fail "MCP initialize request failed"
SESSION_ID="$(grep -i '^mcp-session-id:' "$TMP/init.hdr" | head -1 | sed 's/^[Mm]cp-[Ss]ession-[Ii]d:[[:space:]]*//' | tr -d '\r\n')"
[ -n "$SESSION_ID" ] || fail "MCP initialize did not return an Mcp-Session-Id header"
log "MCP session established: ${SESSION_ID}"
SESSION_HDR="Mcp-Session-Id: ${SESSION_ID}"

# notifications/initialized completes the handshake.
mcp_call '{"jsonrpc":"2.0","method":"notifications/initialized"}' "$SESSION_HDR" >/dev/null || true

# -- THE EMBED ASSERTION: context_store then context_search ------------------
# Both go through the embed path. We wait PAST the full retry/backoff window so
# a slow-but-healthy first download does not flake. An embed failure surfaces as
# a tool error (isError / "embed" in the message) rather than a stored/found
# entry — that is the #767 EmbedFailed symptom, and it FAILS the gate.
STORE_BODY='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"context_store","arguments":{"content":"767 embed-readiness smoke probe: first-boot embedding must be live.","topic":"smoke-767","category":"convention","title":"767-embed-probe"}}}'
SEARCH_BODY='{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"context_search","arguments":{"query":"first-boot embedding readiness smoke probe","k":3}}}'

log "asserting embed-readiness via context_store -> context_search round trip"
log "  (waiting up to ${READY_TIMEOUT_SECS}s — PAST the 10s/20s/40s embed retry backoff window)"
deadline=$(( $(date +%s) + READY_TIMEOUT_SECS ))
last=""
while :; do
  store_resp="$(mcp_call "$STORE_BODY" "$SESSION_HDR" || true)"
  search_resp="$(mcp_call "$SEARCH_BODY" "$SESSION_HDR" || true)"
  last="store=${store_resp} || search=${search_resp}"

  # Embed-not-ready/-failed surfaces as a JSON-RPC error or an isError tool
  # result mentioning embed. Treat those as "still warming" until the deadline.
  if printf '%s\n%s' "$store_resp" "$search_resp" | grep -qiE 'embed (model )?(not ?ready|failed)|embednotready|embedfailed|model .*not.*(ready|load)'; then
    : # embed still warming up — keep waiting
  elif printf '%s' "$search_resp" | grep -q '"result"' \
       && ! printf '%s' "$search_resp" | grep -qi '"isError":[[:space:]]*true'; then
    # context_search returned a real (non-error) result => the query was embedded
    # => the embed path is LIVE. This is the embed assertion, not liveness.
    log "context_search returned a non-error result => embed path is LIVE. PASS embed assertion"
    break
  fi
  if [ "$(date +%s)" -gt "$deadline" ]; then
    log "last response: ${last}"
    fail "embed never became ready within ${READY_TIMEOUT_SECS}s => context_search could not embed (the #767 EmbedFailed symptom). Check /shared is writable on first boot."
  fi
  sleep 5
done

# -- Sidecar-inspect: the model files actually landed on /shared, non-empty --
# Distroless: inspect via busybox, never `docker exec`.
MODEL_OK="$(shared sh -c 'f=$(find /shared/models -name model.onnx 2>/dev/null | head -1); t=$(find /shared/models -name tokenizer.json 2>/dev/null | head -1); [ -s "$f" ] && [ -s "$t" ] && echo OK || echo MISSING')"
[ "$MODEL_OK" = "OK" ] || fail "model.onnx and/or tokenizer.json missing or empty under /shared/models after boot"
log "model.onnx + tokenizer.json present and non-empty under /shared/models. PASS model-file check"

log "ALL GATES PASSED — first boot downloaded the model into /shared and the embed path is live (context_store/context_search round trip succeeded)."
