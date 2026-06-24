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
  [ -n "${SANDBOX:-}" ] && rm -rf "$SANDBOX"   # nan-020: hermetic sandbox teardown
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

# === nan-020 Gate 5–7 STUB-DRIVABLE SEAM (R-01/R-07; #5192 indirection) ====
# The three new external commands and the Gate 7 store sampler are routed
# through these wrappers so the pre-merge gate-logic test can inject stubs
# (env-overridable) and drive the truth table + negative control WITHOUT
# Docker/node. In a real run every override is unset and the real commands run.
#
# Contract (mirrors nan-019 run_smoke_gate/SMOKE_CMD indirection):
#   SMOKE_EMIT_CMD        : argv run for Gate 5 bundle emit; blob on stdout.
#   SMOKE_INIT_CMD        : argv run for Gate 6 host init --bundle consume.
#   SMOKE_HOOK_CMD        : argv run for Gate 7 hook fire; observe code on stdout.
#   SMOKE_STORE_SIZE_CMD  : argv run (with the store dir appended) for the
#                           Gate 7 store sampler; integer size on stdout.
# Each default invokes the verified real command. Overrides are whitespace-split
# argv (test fixtures only — never set in CI / shipped invocation).

# emit_bundle -> stdout blob (stderr token-redacted echo intentionally dropped).
emit_bundle() {
  if [ -n "${SMOKE_EMIT_CMD:-}" ]; then
    # shellcheck disable=SC2086
    $SMOKE_EMIT_CMD 2>/dev/null
  else
    # #812: the emit container MUST receive UNIMATRIX_PUBLIC_URL — env does not cross
    # containers, so without it the server bakes the `<EDIT-ME>` placeholder into the
    # v:2 bundle's observe_url and the host `init --bundle` rejects it as an invalid
    # URL. Same scheme/host/port as the boot container (L290) and Gate 2 curl: one
    # consistent source of truth across boot / curl / emit (nan-020 ADR-002 #5256).
    docker run --rm -v "$VOL:/data" \
      -e UNIMATRIX_PUBLIC_URL="https://localhost:${PORT}" \
      "$IMAGE" --project-dir /data client-bundle "$SLUG" 2>/dev/null
  fi
}

# consume_bundle <blob> -> rc only (hermetic; HOME on the CHILD, never in-process).
# #812: the init child's stderr+stdout are NO LONGER discarded — they are captured
# to $SANDBOX/init.out (reaped by the cleanup() trap, hermetic). The file is the
# diagnostic surface the caller tail-dumps ON FAILURE ONLY (Gate 6 was undiagnosable
# by construction while this went to /dev/null). Safe to capture: init's stderr on a
# Ping failure is a connect/URL/fingerprint string and NEVER the token (NFR-06; the
# bearer is flushed to the request body only after the pin verifies, never to a
# stream). This captures ONLY the init child — emit_bundle's blob-bearing stderr
# (L78) stays dropped. Redirect to a FILE, never a pipe, so set -e/pipefail and the
# rc capture (#4873 class) are unaffected.
consume_bundle() {
  local blob="$1"
  if [ -n "${SMOKE_INIT_CMD:-}" ]; then
    # shellcheck disable=SC2086
    HOME="$SANDBOX/home" $SMOKE_INIT_CMD "$blob" "$SANDBOX/proj" >"$SANDBOX/init.out" 2>&1
  else
    HOME="$SANDBOX/home" \
      node "$REPO_ROOT/packages/unimatrix/bin/unimatrix.js" \
        init --bundle "$blob" --project-dir "$SANDBOX/proj" >"$SANDBOX/init.out" 2>&1
  fi
}

# fire_hook -> observe HTTP code on stdout (best-effort; client is fail-open).
# SAME isolated HOME as Gate 6 so the hook client reads THIS run's credstore.
fire_hook() {
  if [ -n "${SMOKE_HOOK_CMD:-}" ]; then
    # shellcheck disable=SC2086
    printf '%s' "$HOOK_EVENT_JSON" | HOME="$SANDBOX/home" $SMOKE_HOOK_CMD 2>/dev/null
  else
    # The repo-checkout hook client (NFR-4) reads observe_url from the bundle
    # store and POSTs verbatim. It is fail-open (always exit 0, zero stdout on
    # its own paths), so it surfaces no HTTP code here — the store delta below
    # is the load-bearing assertion. We emit nothing => observe_code stays "".
    printf '%s' "$HOOK_EVENT_JSON" \
      | HOME="$SANDBOX/home" \
        node "$REPO_ROOT/packages/unimatrix/lib/hook-client/index.js" SessionStart \
        >/dev/null 2>&1 || true
    printf ''
  fi
}

# gate7_store_size <dir> -> integer (injectable so the negative-control delta=0
# and positive-twin delta>0 are stub-drivable without a real per-slug store).
gate7_store_size() {
  if [ -n "${SMOKE_STORE_SIZE_CMD:-}" ]; then
    # shellcheck disable=SC2086
    $SMOKE_STORE_SIZE_CMD "$1"
  else
    store_size "$1"
  fi
}

# A minimal hook stdin event for ONE observable record (#818: SessionStart ->
# SessionRegister, the first event a freshly-bundled client fires, proven to
# persist on the per-slug route). Overridable by the stub harness. cwd is
# stamped at fire time (Gate 7) once $SANDBOX/proj exists.
HOOK_EVENT_JSON="${HOOK_EVENT_JSON:-}"

# bundle_attach_gates — Gates 5–7 as ONE coherent sourceable function (#5192:
# the smoke and the pre-merge gate-logic test exercise the SAME bytes). The
# script calls it after Gate 4; the test sources this file and calls it directly
# with the SMOKE_*_CMD stubs injected, driving the truth table + the hermeticity
# negative control WITHOUT Docker/node. Append-only after Gate 4. Every new
# failure folds into the EXISTING fail() (exit 1) with a distinct, attributable
# message (ADR-001) — no new exit codes 5/6/7.
bundle_attach_gates() {
  # ---- Host preflight: node must be present (defense-in-depth behind setup-node@v4) ----
  # node-absence is a mis-provisioned lane => hard-fail exit 1 (NOT exit 3 — that code is
  # Docker-only). The pinned setup-node@v4 (release.yml) is the acquisition path; this is
  # the backstop so a provisioning regression never silent-greens (ADR-001 / R-04 sc.2).
  if ! command -v node >/dev/null 2>&1; then
    fail "node not available — the documented init --bundle path cannot be exercised"
  fi

  # ---- GATE 5: emit the connection bundle from the booted image (Rust, in-container) ----
  # Capture STDOUT ONLY (the emitter drops stderr — the token-redacted echo MUST NOT be
  # folded into the blob or logged; R-05/security). Capture rc WITHOUT a pipe so
  # set -e/pipefail cannot swallow it (R-03/#4873 class).
  local BUNDLE emit_rc init_rc observe_code BUNDLE_BEFORE BUNDLE_AFTER
  set +e
  BUNDLE="$(emit_bundle)"
  emit_rc=$?
  set -e
  [ "$emit_rc" -eq 0 ] \
    || fail "client-bundle emit failed (rc=$emit_rc) — subcommand renamed/absent in shipped image?"

  # Blob shape validation at the boundary (R-05): non-empty AND correct prefix. Prefix test
  # (not substring); $BUNDLE quoted everywhere (blob may carry shell-significant chars).
  case "$BUNDLE" in
    unimatrix-bundle:*) : ;;
    *) fail "client-bundle produced no/invalid bundle blob" ;;
  esac
  log "client-bundle emitted a unimatrix-bundle: blob. PASS gate 5"

  # #812: emit-time guard — assert the decoded bundle's observe_url is well-formed
  # (rejects the `<EDIT-ME>` placeholder a PUBLIC_URL-less emit container would bake,
  # AND confirms it parses). Converts the entire bug class from an opaque Gate-6
  # "(invalid URL)" 30+ lines later into a precise emit-time failure. Folds into the
  # EXISTING fail()/exit-1 contract (ADR-001 #5249) — no new exit code. Decode mirrors
  # the client (bundle.js): base64url body of the `unimatrix-bundle:` blob -> JSON.
  # Prints ONLY observe_url (never the token-bearing blob/JSON) — token-safe (R-05).
  # Runs on the REAL emit path only: the SMOKE_EMIT_CMD stub seam injects synthetic
  # (non-base64url) blobs for the gate-logic truth table, which this guard would
  # spuriously reject — same real-vs-stub split the rest of the seam already uses.
  if [ -z "${SMOKE_EMIT_CMD:-}" ]; then
    local OBSERVE_URL
    OBSERVE_URL="$(printf '%s' "$BUNDLE" \
      | node -e 'let b="";process.stdin.on("data",c=>b+=c).on("end",()=>{try{const j=JSON.parse(Buffer.from(b.trim().slice("unimatrix-bundle:".length),"base64url").toString("utf8"));process.stdout.write(String(j.observe_url||""))}catch{process.exit(0)}})' 2>/dev/null)"
    case "$OBSERVE_URL" in
      *"<EDIT-ME>"*) fail "bundle carries placeholder URL — emit container missing UNIMATRIX_PUBLIC_URL" ;;
    esac
    if ! node -e 'new URL(process.argv[1])' "$OBSERVE_URL" >/dev/null 2>&1; then
      fail "bundle observe_url is not a parseable URL — emit container env malformed"
    fi
    log "bundle observe_url is well-formed (no placeholder, parseable). PASS gate 5 guard"
  fi

  # ---- HERMETIC SANDBOX (ADR-005): per-run, HOME-isolated, throwaway --project-dir ----
  # Established at the SHELL/PROCESS boundary. mktemp -d is already fresh; the explicit
  # rm -rf + mkdir is the clean-on-ENTRY guarantee for the crashed-prior-run case.
  SANDBOX="$(mktemp -d)" || fail "could not create hermetic sandbox (mktemp -d failed)"
  rm -rf "$SANDBOX/home" "$SANDBOX/proj"
  mkdir -p "$SANDBOX/home" "$SANDBOX/proj"
  log "hermetic sandbox at $SANDBOX (isolated HOME + throwaway --project-dir)"

  # ---- GATE 6: consume the bundle on the host, HERMETICALLY (JS, repo-checkout client) ----
  # Process-boundary isolation: HOME + --project-dir set on the SPAWNED CHILD only (the
  # harness never mutates its own HOME — Rust-2024 forbids in-process set_var; ADR-005).
  # repo-checkout client (NFR-4), NO --slug (retired on bundle path, init.js:353).
  set +e
  consume_bundle "$BUNDLE"
  init_rc=$?
  set -e
  if [ "$init_rc" -ne 0 ]; then
    # #812: surface the init child's captured streams (was /dev/null — undiagnosable
    # by construction). FAILURE PATH ONLY (happy path stays quiet, no stdout pollution).
    # Tail-bounded to cap CI-log volume. The dump is init's stream only — never the
    # blob (emit_bundle's token-bearing stderr is dropped at L78), so no token leak.
    log "---- init --bundle stderr/stdout (tail, on failure) ----"
    if [ -s "$SANDBOX/init.out" ]; then
      tail -n 50 "$SANDBOX/init.out" | while IFS= read -r line; do
        log "init: $line"
      done
    else
      log "init: (no output captured)"
    fi
    log "---- end init --bundle output ----"
    fail "init --bundle failed (rc=$init_rc) — bundle attach broken"
  fi
  log "init --bundle attached against the booted image (hermetic HOME). PASS gate 6"

  # ---- GATE 6 READINESS (nan-021 C1): credstore materialized, mode 0600, under the hermetic HOME ----
  # The C1->C2 boundary the bridge spawn waits on (ADR-002 #5294 "credstore present" gate; c1 test plan
  # test_c1_gate_remote_json_present_mode_0600). init's rc==0 alone does NOT prove the credstore file
  # actually landed at the expected path/mode — credstore.js can null-out on no-homedir. This gate makes
  # the standup VERIFIABLY produce the artifact the Wave-2 cloud_cycle_gates() reads back.
  #
  # projectHash is READ BACK by listing the single dir under $SANDBOX/home/.unimatrix/ — NEVER recomputed,
  # NO hashing primitive in this path (OQ1/R-11; init.js writes exactly one <projectHash>/ here, a 16-hex
  # SHA-256). If a future init.js change writes zero or >1 dirs, this read-back fails LOUD (contract drift
  # surfaced, not silently miscomputed). All inspection is host-side under the hermetic sandbox (R-14) —
  # no busybox/docker, no real ~/.unimatrix touched.
  #
  # REAL-PATH ONLY (same real-vs-stub split as the Gate-5 guard at L193): mode-0600 + single-projectHash
  # are properties of the REAL credstore.js write; the SMOKE_INIT_CMD stub drives the exit-code truth table
  # (it writes a fixed `stub-hash` dir at the umask default, not 0600) — asserting them on the stub would
  # spuriously fail the C5 logic-test happy path. The stub's credstore-absence is already covered at Gate 7
  # (STUB_INIT_WRITE_CRED=0 → "observe did not land"). On the live tag run this gate always runs.
  if [ -z "${SMOKE_INIT_CMD:-}" ]; then
    local cred_root="$SANDBOX/home/.unimatrix" project_hash hash_count cred_file cred_mode
    [ -d "$cred_root" ] \
      || fail "credstore root $cred_root absent after init --bundle — hermetic HOME not populated (R-14/C1->C2)"
    hash_count="$(find "$cred_root" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
    [ "$hash_count" = "1" ] \
      || fail "projectHash read-back ambiguous: expected exactly 1 dir under $cred_root, found $hash_count (init.js contract drift)"
    project_hash="$(find "$cred_root" -mindepth 1 -maxdepth 1 -type d -exec basename {} \;)"
    cred_file="$cred_root/$project_hash/remote.json"
    [ -f "$cred_file" ] \
      || fail "credstore $cred_file absent after init --bundle — bridge cannot attach (C1->C2 boundary)"
    # Mode 0600 is the trust boundary (credstore.js STORE_MODE 0o600): the bearer must not be world/group
    # readable. stat -c works on GNU coreutils (CI ubuntu lanes); -f %Lp is the BSD/macOS dev-box fallback.
    cred_mode="$(stat -c '%a' "$cred_file" 2>/dev/null || stat -f '%Lp' "$cred_file" 2>/dev/null)"
    [ "$cred_mode" = "600" ] \
      || fail "credstore $cred_file mode is $cred_mode, expected 600 (bearer must be owner-only — R-14)"
    log "credstore present at mode 0600 (projectHash $project_hash) before any bridge spawn. PASS gate 6 readiness"
  fi

  # ---- GATE 7: fire one hook event through the wired client; assert observe + store grow ----
  # Fresh BEFORE sample of the per-slug store, taken NOW (after Gates 1–4 already wrote), so
  # the delta attributable to THIS hook fire is isolated (R-07 sc.4 — fresh-write evidence).
  BUNDLE_BEFORE="$(gate7_store_size "$SLUG_DIR")"

  # Stamp the event cwd into the isolated proj tree at fire time (once $SANDBOX exists).
  # #818: probe is SessionStart (-> SessionRegister, build-request.js:50-57), NOT Stop.
  # SessionStart is the FIRST event a freshly-bundled client fires and is proven to
  # persist on this exact per-slug route (Gate 2 fires the identical SessionRegister
  # frame; Gate 4 asserts its store delta). A Stop -> SessionClose for a never-
  # registered id is a server no-op (no write) => the old probe could never grow the
  # store. Use a FRESH, unique id (s-818-bundle, never s-783) so this delta is solely
  # attributable to THIS fire (R-07 sc.4 fresh-write evidence).
  [ -n "$HOOK_EVENT_JSON" ] \
    || HOOK_EVENT_JSON="{\"hook_event_name\":\"SessionStart\",\"session_id\":\"s-818-bundle\",\"cwd\":\"$SANDBOX/proj\"}"

  # Fire ONE hook event through the SAME isolated HOME so the hook client reads THIS run's
  # credstore ($SANDBOX/home/.unimatrix/<hash>/remote.json), never the runner's real
  # ~/.unimatrix (R-07 sc.1). The client is fail-open (exit 0, zero stdout on its own
  # paths), so we DO NOT key the 204 on its exit code — the store delta below is the
  # LOAD-BEARING assertion. observe_code is best-effort: only the stub surfaces it.
  set +e
  observe_code="$(fire_hook)"
  set -e
  # Distinguish doc-drift from route change (R-02 / SR-09) when a code is observable:
  if [ -n "$observe_code" ] && [ "$observe_code" != "204" ]; then
    fail "documented bundle attach observe returned HTTP $observe_code (expected 204)"
  fi

  # Load-bearing, non-skip assertion (R-07 sc.4 / #4977): the per-slug store grew by
  # THIS run's write. A persistent delta of 0 = the attach silently no-opped => RED.
  # The ASSERTION stays mandatory; only the SAMPLING is a bounded deadline-poll
  # (#818): the server write is tokio::spawn fire-and-forget + WAL synchronous=NORMAL,
  # so it lands sub-second but is NOT synced before the 204. Re-sample the WHOLE slug
  # dir (must include -wal) until it grows, with a ~10s deadline (arm64-CI headroom,
  # well under the 90s boot waits above). On timeout, the EXISTING fail() (exit 1, same
  # message) fires — append-only exit contract (nan-019 ADR-001), no new exit code. This
  # mirrors the in-file boot-wait idiom (the GATE 1 / restart loops above).
  #
  # SCOPE BOUNDARY (#818): Gate 7 proves the bundle-configured client's LIFECYCLE-
  # REGISTER leg — transport + bundle config + project-hash + cert-pin + persistence of
  # a SessionRegister via /observe. It does NOT exercise the BEHAVIORAL-OBSERVE payload
  # (PostToolUse / transcript deltas) that self-learning (C11/C0) depends on: transcript
  # is never-persist by design, so it produces no knowledge-store `du` delta and cannot
  # be asserted this way. Behavioral-observe coverage lives in #814 (real-server round-
  # trip) and the existing server-logged PostToolUse path (vnc-039).
  deadline=$(( $(date +%s) + 10 ))
  while :; do
    BUNDLE_AFTER="$(gate7_store_size "$SLUG_DIR")"
    [ "$BUNDLE_AFTER" -gt "$BUNDLE_BEFORE" ] && break
    [ "$(date +%s)" -gt "$deadline" ] \
      && fail "bundle-path observe did not land in per-slug store"
    sleep 1
  done
  log "bundle-path observe landed (store $BUNDLE_BEFORE -> $BUNDLE_AFTER). PASS gate 7"
}
# === end nan-020 seam ======================================================

# === nan-021 C2: cloud cycle gate (sourced) ===============================
# The bridge-driven Gate 8 (cloud_cycle_gates) lives in cloud-cycle-lib.sh to
# keep this file <=500 lines (workspace rule). Sourced here so it runs in the
# smoke's scope (uses log/fail/store_size/vol + $SLUG/$SLUG_DIR/$TMP/$TOKEN).
# Like release-gate-lib.sh, it only DEFINES functions on source.
# shellcheck source=product/test/infra-001/scripts/cloud-cycle-lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/cloud-cycle-lib.sh"
# === end nan-021 C2 sourced seam ==========================================

# Sourced-guard (#5192): when this file is SOURCED (the pre-merge gate-logic
# test sources it to call bundle_attach_gates against stubs), stop here — do
# NOT run the Docker preflight / Gates 1–4 / terminal marker. When EXECUTED
# directly (the real smoke), $0 == BASH_SOURCE and the main flow below runs.
if [ "${BASH_SOURCE[0]}" != "${0}" ]; then
  return 0 2>/dev/null || true
fi

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

# Gates 5–7 (documented bundle attach) — defined in the seam section above as
# bundle_attach_gates(); reuses the SAME container/volume/slug/port/cert as Gates 1–4.
bundle_attach_gates

# Gate 8 (nan-021 C2 cloud cycle) — runs ONLY when the pytest orchestrator wired
# the C2 inputs (MANIFEST_PATH/RUN_TOKEN/HTTPS_VECTOR_OUT). The standalone #783
# smoke (no nan-021 env) skips it so its contract is unchanged (append-only).
if [ -n "${MANIFEST_PATH:-}" ] && [ -n "${RUN_TOKEN:-}" ] && [ -n "${HTTPS_VECTOR_OUT:-}" ]; then
  log "nan-021 C2 inputs present — driving the cloud cycle through the bridge (gate 8) ..."
  cloud_cycle_gates
else
  log "nan-021 C2 inputs absent — skipping gate 8 (standalone #783 posture smoke)."
fi

log "ALL GATES PASSED — clean image boots HTTP-on and routes the registered slug over HTTPS."
