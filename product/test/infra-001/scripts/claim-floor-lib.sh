#!/bin/bash
# claim-floor-lib.sh (#915/#916) — the two personal-cloud claim-floor gates, factored
# out of docker-http-posture-smoke.sh to keep each file <=500 lines (workspace rule) and
# to mirror cloud-cycle-lib.sh's sourced-library pattern. SOURCED by the smoke (and by
# the pre-merge logic test) — it does NOT execute on its own; it only DEFINES functions.
# It relies on the parent providing log(), fail(), and $SANDBOX/$IMAGE/$TMP (all in the
# smoke's scope when these gates run, after C1's Gates 1-7). Append-only fail()/exit-1
# contract (nan-019 ADR-001) — every new failure folds into the EXISTING fail().
#
# GATE 9 — client-works (#916/C15): after bundle_attach_gates, drive ONE stateless
#   context_* tools/call (context_status) THROUGH the SHIPPED mcp-bridge.js using the
#   Gate-6 credstore (HOME=$SANDBOX/home, projectHash READ BACK — never recomputed). This
#   is the only green gate that proves the ATTACHED client performs a real context_* op
#   over the pinned-TLS bundle path (the proof previously lived only in the permanently-
#   red nan-021 parity_matrix lane). The bridge drive is wrapped in `timeout` (bounded).
#
# GATE 10 — compose boot (#915/C1): boot the LITERAL shipped docker-compose.yml (the
#   operator's first command, `docker compose up`) with the VERSION UNDER TEST pinned,
#   wait for the HTTPS listener, then a pinned-TLS /health probe on the shipped SAN/port.
#   Scoped HONESTLY as boot+serve proof (NOT volume-mode/posture-drift — /health alone is
#   not posture proof; Gates 1-4 own posture on the docker-run path).
#
# Stub seam (mirrors SMOKE_*_CMD): SMOKE_CLIENT_CALL_CMD overrides the single-call bridge
# drive so the pre-merge logic test drives Gate-9 control flow without node/Docker. Gate
# 10's live-only docker/curl steps are small single-purpose helpers the logic test
# OVERRIDES in its own harness (never in this shipped file), mirroring how the C5 logic
# test overrides _fire_observe_hooks / cycle_durability_barrier.

REPO_ROOT_DEFAULT="${REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_JS="$REPO_ROOT_DEFAULT/packages/unimatrix/lib/hook-client/mcp-bridge.js"
SINGLE_DRIVER_JS="$SCRIPTS_DIR/bridge-single-call-driver.js"

# Gate 9 knobs. context_status is stateless + embed-free (won't perturb the shared-lane
# gate-8 credstore/cycle nor eat the #767 embed-retry window). The bounded deadline
# (constraint 1) caps a bridge hang so it cannot eat the blocking lane's job timeout.
CLIENT_CALL_TOOL="${CLIENT_CALL_TOOL:-context_status}"
CLIENT_CALL_DEADLINE_S="${CLIENT_CALL_DEADLINE_S:-60}"

# Gate 10 shipped defaults (verified against docker-compose.yml: image service
# `unimatrix`, SAN cloud.example, TLS port 8443). Project-scoped so a mid-gate failure
# tears down exactly THIS run's stack (cleanup trap in the smoke).
COMPOSE_FILE="${COMPOSE_FILE:-$REPO_ROOT_DEFAULT/docker-compose.yml}"
COMPOSE_PROJECT="${COMPOSE_PROJECT:-uni-$$}"
COMPOSE_SAN="${COMPOSE_SAN:-cloud.example}"
COMPOSE_PORT="${COMPOSE_PORT:-8443}"
COMPOSE_LISTENER_DEADLINE_S="${COMPOSE_LISTENER_DEADLINE_S:-90}"

# ===========================================================================
# GATE 9 — client-works (#916/C15)
# ===========================================================================

# Tail-dump the captured single-call bridge stderr ON FAILURE ONLY. The bridge is a
# token-free child (NFR-06: never logs Authorization). Bounded to cap CI-log volume.
_dump_client_err() {
  local errf="$1"
  log "---- mcp-bridge.js (single-call) stderr (tail, on failure) ----"
  if [ -s "$errf" ]; then
    tail -n 40 "$errf" | while IFS= read -r line; do log "bridge: $line"; done
  else
    log "bridge: (no output captured)"
  fi
  log "---- end mcp-bridge.js stderr ----"
}

# Drive ONE context_* tools/call through the SHIPPED bridge, bounded by `timeout`
# (constraint 1). Stub seam: SMOKE_CLIENT_CALL_CMD overrides the drive for the logic test.
drive_client_call() {
  local project_hash="$1" out="$2" err="$3"
  if [ -n "${SMOKE_CLIENT_CALL_CMD:-}" ]; then
    # Stub seam (mirrors SMOKE_*_CMD): drive control flow without node/Docker. Bounded by
    # `timeout` exactly as the real path is (a stub hang is a real-path hang here too).
    # shellcheck disable=SC2086
    HOME="$SANDBOX/home" timeout "$CLIENT_CALL_DEADLINE_S" \
      $SMOKE_CLIENT_CALL_CMD "$project_hash" "$CLIENT_CALL_TOOL" >"$out" 2>"$err"
  else
    command -v node >/dev/null 2>&1 \
      || fail "client-works: node not available — the shipped bridge cannot be driven"
    [ -f "$BRIDGE_JS" ] \
      || fail "client-works: shipped bridge $BRIDGE_JS not found (reuse-as-is)"
    [ -f "$SINGLE_DRIVER_JS" ] \
      || fail "client-works: single-call driver $SINGLE_DRIVER_JS not found"
    # HOME=$SANDBOX/home so credstore.read finds THIS run's remote.json. `timeout` bounds
    # a bridge hang so it cannot eat the blocking lane's job timeout (constraint 1).
    HOME="$SANDBOX/home" timeout "$CLIENT_CALL_DEADLINE_S" \
      node "$SINGLE_DRIVER_JS" "$project_hash" "$CLIENT_CALL_TOOL" --bridge "$BRIDGE_JS" \
        >"$out" 2>"$err"
  fi
}

client_works_gate() {
  # ---- read the projectHash BACK from the Gate-6 credstore (OQ1/R-11 — never recompute) ----
  local cred_root="$SANDBOX/home/.unimatrix" project_hash hash_count cred_file
  [ -d "$cred_root" ] \
    || fail "client-works: credstore root $cred_root absent — bundle attach (Gate 6) incomplete"
  hash_count="$(find "$cred_root" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
  [ "$hash_count" = "1" ] \
    || fail "client-works: projectHash read-back ambiguous: expected 1 dir under $cred_root, found $hash_count (init.js contract drift)"
  project_hash="$(find "$cred_root" -mindepth 1 -maxdepth 1 -type d -exec basename {} \;)"
  cred_file="$cred_root/$project_hash/remote.json"
  [ -f "$cred_file" ] \
    || fail "client-works: credstore $cred_file absent — the bridge cannot attach"

  # ---- drive ONE stateless context_* op THROUGH the shipped bridge (bounded) ----
  local CALL_OUT="$SANDBOX/client_call.json" CALL_ERR="$SANDBOX/client_call.stderr" call_rc
  set +e
  drive_client_call "$project_hash" "$CALL_OUT" "$CALL_ERR"
  call_rc=$?
  set -e
  if [ "$call_rc" -eq 124 ]; then
    _dump_client_err "$CALL_ERR"
    fail "client-works: ${CLIENT_CALL_TOOL} over the bridge timed out after ${CLIENT_CALL_DEADLINE_S}s (bounded — a hang must not eat the blocking lane's job timeout)"
  fi
  [ "$call_rc" -eq 0 ] \
    || { _dump_client_err "$CALL_ERR"; fail "client-works: bridge single-call driver failed (rc=$call_rc) — the attached client cannot reach the server over the pinned-TLS bundle"; }
  grep -q '"ok": *true\|"ok":true' "$CALL_OUT" \
    || { _dump_client_err "$CALL_ERR"; fail "client-works: ${CLIENT_CALL_TOOL} via mcp-bridge.js did not return ok — no real context_* op over the pinned-TLS bundle (#916/C15)"; }
  log "client performed ${CLIENT_CALL_TOOL} via the SHIPPED mcp-bridge.js over the pinned-TLS bundle (projectHash $project_hash). PASS gate 9 (client-works, #916/C15)"
}

# ===========================================================================
# GATE 10 — compose boot (#915/C1)
# ===========================================================================
# Live-only helpers — single-purpose so the pre-merge logic test can OVERRIDE each in its
# OWN harness (the shipped bytes stay untouched). Their default bodies run the real
# docker/curl commands.

compose_plugin_present()   { docker compose version >/dev/null 2>&1; }
compose_do_up()            { UNIMATRIX_IMAGE="$IMAGE" docker compose -p "$COMPOSE_PROJECT" -f "$COMPOSE_FILE" up -d --pull never >/dev/null 2>&1; }
compose_service_cid()      { docker compose -p "$COMPOSE_PROJECT" -f "$COMPOSE_FILE" ps -q unimatrix 2>/dev/null; }
compose_expected_digest()  { docker image inspect "$IMAGE" --format '{{.Id}}' 2>/dev/null; }
compose_container_digest() { docker inspect "$1" --format '{{.Image}}' 2>/dev/null; }
compose_listener_active()  { docker logs "$1" 2>&1 | grep -q "HTTP transport active"; }
# Extract the served self-signed leaf from the compose data volume via a busybox sidecar
# (the runtime image is distroless — no shell, so never `docker exec`). Volume name is
# compose's `<project>_<volume>` for the shipped `unimatrix-data` volume.
compose_extract_cert()     { docker run --rm -v "${COMPOSE_PROJECT}_unimatrix-data:/data:ro" busybox sh -c 'f=$(ls /data/.unimatrix/*/tls/cert.pem 2>/dev/null | head -1); [ -n "$f" ] && cat "$f"' > "$1" 2>/dev/null; }
# Pinned TLS /health on the LITERAL shipped SAN+port. --resolve maps the shipped SAN to
# loopback so hostname verification holds against the SAN (a bare localhost curl would
# fail hostname verification); --cacert pins the container's leaf (constraint 3).
compose_health_code()      { curl -sS --cacert "$1" --resolve "${COMPOSE_SAN}:${COMPOSE_PORT}:127.0.0.1" -o /dev/null -w '%{http_code}' "https://${COMPOSE_SAN}:${COMPOSE_PORT}/health" 2>/dev/null; }
# Project-scoped teardown (`down -v`), called from the smoke's cleanup trap when the gate
# armed COMPOSE_UP=1 — so a mid-gate failure still removes the stack + its volumes.
compose_teardown()         { docker compose -p "$COMPOSE_PROJECT" -f "$COMPOSE_FILE" down -v >/dev/null 2>&1 || true; }

compose_boot_gate() {
  # Constraint 2: compose-plugin absence is a mis-provisioned lane => HARD fail (exit 1),
  # NEVER a self-skip. Mirrors the node-absence backstop in bundle_attach_gates.
  compose_plugin_present \
    || fail "compose boot: docker compose plugin absent — the operator's first command (docker compose up) cannot be exercised (mis-provisioned lane; never self-skip)"
  [ -f "$COMPOSE_FILE" ] \
    || fail "compose boot: compose file $COMPOSE_FILE not found — cannot exercise the shipped docker compose up"
  [ -n "${IMAGE:-}" ] \
    || fail "compose boot: IMAGE unset — the gate needs the version-under-test image"

  # Constraint 4: bind UNIMATRIX_IMAGE=$IMAGE + --pull never so compose boots the VERSION
  # UNDER TEST (locally the freshly-built tag, in CI the resolve_image digest), never the
  # ghcr :latest release image.
  compose_do_up || fail "compose boot: docker compose up failed (image under test: $IMAGE)"
  # shellcheck disable=SC2034  # read cross-file by the smoke's cleanup() trap
  COMPOSE_UP=1   # arm the cleanup-trap teardown (docker compose down -v)

  local cid
  cid="$(compose_service_cid)"
  [ -n "$cid" ] \
    || fail "compose boot: could not resolve the unimatrix service container id after up"

  # Constraint 4 guard: the digest compose ACTUALLY booted MUST equal $IMAGE's digest, so
  # a future refactor cannot silently re-introduce a :latest pull.
  local want_digest got_digest
  want_digest="$(compose_expected_digest || true)"
  got_digest="$(compose_container_digest "$cid" || true)"
  [ -n "$want_digest" ] \
    || fail "compose boot: could not resolve the image-under-test digest ($IMAGE)"
  [ "$want_digest" = "$got_digest" ] \
    || fail "compose boot: booted image digest '$got_digest' != version-under-test '$IMAGE' digest '$want_digest' — compose used the wrong image (:latest regression guard, #915)"

  # boot+serve proof: wait for the HTTPS listener log (the same signal Gate 1 waits on).
  local deadline
  deadline=$(( $(date +%s) + COMPOSE_LISTENER_DEADLINE_S ))
  while :; do
    compose_listener_active "$cid" && break
    [ "$(date +%s)" -gt "$deadline" ] \
      && fail "compose boot: HTTPS listener never became active within ${COMPOSE_LISTENER_DEADLINE_S}s"
    sleep 2
  done

  # Constraint 3: pinned TLS /health against the LITERAL shipped defaults.
  local cert="$TMP/compose-cert.pem" code
  compose_extract_cert "$cert"
  [ -s "$cert" ] \
    || fail "compose boot: could not extract the served TLS cert from the compose data volume"
  code="$(compose_health_code "$cert" || true)"
  [ "$code" = "200" ] \
    || fail "compose boot: pinned TLS /health on the shipped defaults (${COMPOSE_SAN}:${COMPOSE_PORT}) returned HTTP ${code:-none} (expected 200)"

  log "docker compose up booted a serving HTTPS instance on the shipped defaults (${COMPOSE_SAN}:${COMPOSE_PORT}), pinned /health 200, booted image digest matches the version under test. PASS gate 10 (compose boot, #915/C1)"
}
