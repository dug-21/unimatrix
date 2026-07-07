#!/usr/bin/env bash
# multi-tenant-isolation-smoke.sh — infra-003 (#853) standalone bidirectional
# multi-tenant HTTP cross-tenant isolation gate. Cumulative extension of the
# infra-001 posture-smoke harness (ADR-001): a SEPARATE top-level smoke with
# self-contained assertions (SR-12) — it does NOT graft onto posture-smoke's
# Gates 1–8 flow, so an upstream posture-smoke change surfaces here as an
# explicit failure, not a silent skip (R-13). No crates/ change.
#
# PROPERTY: an HTTP write addressed to a slug lands ONLY in that slug's per-slug
# store, across two served write surfaces (observe POST /v1/{slug}/observe and
# HTTP MCP-write POST /v1/{slug}/mcp), in BOTH directions between A=arch-research
# and B=isolation-b. Four distinctly-marked writes through the genuine production
# funnel (parse_project_key -> resolve_store -> dispatch); a genuine two-store
# content read asserts the full discrimination matrix per surface (each store
# holds only its own marker, present in own + absent from other), both directions.
#
# Components (pseudocode/OVERVIEW.md): C1 preflight, C2 registration+single
# restart+route-liveness PRECONDITION, C3 observe writes, C4 MCP-write probe
# (per-route own Mcp-Session-Id), C5 read-as-barrier positive control
# (retry-until-present), C6 cross-store negative + two-store read primitive,
# C7 verdict (bidirectional 2x2, positive-gates-negative, tri-state exit).
#
# TRI-STATE EXIT (C-12 / R-10 / #5180): GREEN=0 / RED=1 / INFRA=2 / SKIP=3.
#   RED dominates INFRA dominates GREEN. INFRA=2 is distinct from posture-smoke's
#   exit 4 ("IMAGE= prebuilt tag unavailable") so both can share one lane.
#   No non-GREEN outcome ever rounds to exit 0. Terminal run-marker
#   "[infra003-smoke] ALL GATES PASSED" is emitted ONLY on GREEN (verify-by-name,
#   matches release-gate-lib.sh:59  \[[a-z0-9-]+-smoke\] ALL GATES PASSED.*).
#
# Usage:   product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh
# Env:     IMAGE  pre-built image tag to test instead of building (optional)
#          KEEP   set to 1 to keep container/volume for inspection
#
# STUB SEAM (off-Docker gate-logic test — mirrors nan-020 SMOKE_STORE_SIZE_CMD,
# #5192/#5258): the external probes route through env-overridable argv so the
# verdict truth table runs WITHOUT Docker. In a real run every override is unset.
#   SMOKE_WRITE_CMD        : argv run as  CMD <surface> <slug> <marker>  for the
#                            C3/C4 write step (neutralizes curl/MCP off-Docker).
#   SMOKE_READ_MARKER_CMD  : argv run as  CMD <store_dir> <table> <predicate>  for
#                            the C5/C6 two-store read; prints a row-count (>=0) or
#                            the literal INFRA sentinel on stdout.
#   READ_DEADLINE_SECS     : C5 read-as-barrier deadline (default 10; mirrors the
#                            ~10s store-grow wait in docker-http-posture-smoke.sh).
#   READ_POLL_SLEEP        : C5 poll interval (default 1).
#   RUN                    : per-run nonce; tests pin it for deterministic markers.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"

# -- Script-global shared types/variables (OVERVIEW "Shared Types") -------------
SLUG_A="arch-research"      # existing constant — NOT a re-typed allowlist literal
SLUG_B="isolation-b"        # neutral test-scoped literal (ADR-004 / R-11)
PORT="${PORT:-18443}"
CNAME="uni-infra003-smoke-$$"
VOL="uni-infra003-data-$$"
TAG="infra003-smoke"
INFRA_SENTINEL="INFRA"
READ_DEADLINE_SECS="${READ_DEADLINE_SECS:-10}"
READ_POLL_SLEEP="${READ_POLL_SLEEP:-1}"
# C-WB warmup/readiness barrier deadline (infra-004 / ADR-001 #5349). #767-derived:
# docker-embed-readiness-smoke.sh READY_TIMEOUT_SECS=180, ~2.5x over the ~70s
# (10s/20s/40s) embed retry/backoff floor under a real cold HuggingFace download
# (measured cold MCP-ready ~5s — huge margin). env-overridable for arm/slow runners.
WARMUP_DEADLINE_SECS="${WARMUP_DEADLINE_SECS:-180}"

# Per-route MCP session ids — DISTINCT variables; A's session is NEVER used on
# B's route (R-17/C-13). There is no shared session variable that could be crossed.
# Assigned in isolation-probe-lib.sh (sourced below); declared here for set -u safety.
# shellcheck disable=SC2034
SID_A=""
# shellcheck disable=SC2034
SID_B=""

# -- Exit contract helpers (C7 / C-12) -----------------------------------------
log()        { printf '[%s] %s\n' "$TAG" "$*"; }
fail()       { printf '[%s] FAIL: %s\n'  "$TAG" "$*" >&2; exit 1; }   # RED
infra_fail() { printf '[%s] INFRA: %s\n' "$TAG" "$*" >&2; exit 2; }   # INFRA (distinct)

cleanup() {
  if [ "${KEEP:-0}" != "1" ]; then
    docker rm -f -v "$CNAME" >/dev/null 2>&1 || true
    docker volume rm "$VOL" >/dev/null 2>&1 || true
  fi
  [ -n "${TMP:-}" ] && rm -rf "$TMP"
  return 0
}

# Run a command in a busybox sidecar with the data volume mounted READ-ONLY at
# /data (C-02 distroless: never `docker exec`; a read cannot mutate the property
# it measures — Security). Mirrors posture-smoke :47.
vol() { docker run --rm -v "$VOL:/data:ro" busybox "$@"; }

# store_size(dir): WAL-robust liveness signal (posture-smoke :55). C2 boot/liveness
# waits ONLY — NEVER the durability barrier (that is C5's read-as-barrier; ADR-002).
store_size() { vol du -s "$1" | awk '{print $1}'; }

# =====================================================================
# C1 — Read-dependency preflight (docker / sqlite3 / busybox / curl / node)
# =====================================================================
preflight() {
  # Docker absent -> SKIP exit 3 (deferred CI step; matches posture-smoke).
  if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then
    echo "[$TAG] SKIP: Docker not available in this environment." >&2
    echo "[$TAG] This gate MUST run in a Docker-capable CI job (deferred step)." >&2
    exit 3
  fi
  # sqlite3 absent -> INFRA (content-read engine for BOTH surfaces; provision like node).
  command -v sqlite3 >/dev/null 2>&1 \
    || infra_fail "sqlite3 not provisioned on the host — mis-provisioned lane (provision like node); absence is INFRA, never an empty-pass"
  # busybox image absent and unpullable -> INFRA (no read-only vol sidecar possible).
  docker image inspect busybox >/dev/null 2>&1 || docker pull busybox >/dev/null 2>&1 \
    || infra_fail "busybox image unavailable — the vol read-only sidecar cannot mount the data volume (INFRA, never an empty-pass)"
  # curl + node are existing infra-001 idioms; missing -> INFRA here, not an opaque later failure.
  local tool
  for tool in curl node; do
    command -v "$tool" >/dev/null 2>&1 \
      || infra_fail "$tool not available — required for the cert-pinned write / JSON shaping path (INFRA)"
  done
  log "preflight OK: docker, sqlite3, busybox, curl, node present."
}

# =====================================================================
# C2 — Boot, two-slug registration, single restart, route-liveness PRECONDITION
# =====================================================================
wait_for_http_active() {
  local deadline; deadline=$(( $(date +%s) + 90 ))
  while :; do
    if docker logs "$CNAME" 2>&1 | grep -q "HTTP transport active"; then return 0; fi
    if docker logs "$CNAME" 2>&1 | grep -q "set .http. enabled"; then
      infra_fail "daemon logged the HTTP-disabled hint => booted HTTP-OFF"
    fi
    [ "$(date +%s)" -gt "$deadline" ] && infra_fail "HTTP listener never became active (boot failed)"
    sleep 2
  done
}

discover_hash_dir() {
  local d
  d="$(vol sh -c 'ls -d /data/.unimatrix/*/ 2>/dev/null | while read d; do [ -f "$d/token" ] && echo "$d"; done | head -1')"
  d="${d%/}"
  [ -n "$d" ] || infra_fail "could not locate the path-hash data dir (no token found)"
  HASH_DIR="$d"
}

setup_container() {
  if [ -n "${IMAGE:-}" ]; then
    log "using prebuilt image: $IMAGE"
    docker pull "$IMAGE" >/dev/null 2>&1 \
      || docker image inspect "$IMAGE" >/dev/null 2>&1 \
      || infra_fail "could not pull prebuilt IMAGE $IMAGE (confirm the tag was pushed / network healthy)"
  else
    IMAGE="unimatrix:infra003-smoke"
    log "building image $IMAGE from $REPO_ROOT/Dockerfile (slow) ..."
    docker build -t "$IMAGE" "$REPO_ROOT" >/dev/null || infra_fail "docker build failed"
  fi
  docker image inspect "$IMAGE" --format '{{json .Config.Env}}' \
    | grep -q 'UNIMATRIX_HTTP_ENABLED=true' \
    || infra_fail "image ENV is missing UNIMATRIX_HTTP_ENABLED=true"
  docker volume create "$VOL" >/dev/null

  # Boot 1: clean docker run, NO -e UNIMATRIX_HTTP_ENABLED (image ENV must enable).
  docker run -d --name "$CNAME" -v "$VOL:/data" \
    -e UNIMATRIX_PUBLIC_URL="https://localhost:${PORT}" \
    -p "${PORT}:8443" "$IMAGE" >/dev/null
  wait_for_http_active
  discover_hash_dir
  log "boot 1 OK; path-hash data dir: $HASH_DIR"
}

register_both_and_restart() {
  # Both slugs registered BEFORE the one restart (routing read once at boot, #5079).
  # Literals come from the SLUG_A/SLUG_B globals — NOT re-typed ADR-004 regex copies.
  local slug
  for slug in "$SLUG_A" "$SLUG_B"; do
    docker run --rm -v "$VOL:/data" "$IMAGE" --project-dir /data project register "$slug" \
      || infra_fail "project register $slug failed"
  done
  docker restart "$CNAME" >/dev/null
  wait_for_http_active
  log "registered $SLUG_A + $SLUG_B before a single restart; HTTP re-active."
}

assert_routes_live() {
  TMP="$(mktemp -d)"
  vol cat "$HASH_DIR/token"        > "$TMP/token"
  vol cat "$HASH_DIR/tls/cert.pem" > "$TMP/cert.pem"
  TOKEN="$(tr -d '\r\n' < "$TMP/token")"
  [ -n "$TOKEN" ]        || infra_fail "empty bearer token"
  [ -s "$TMP/cert.pem" ] || infra_fail "empty TLS cert"

  SLUG_DIR_A="/data/.unimatrix/${SLUG_A}"
  SLUG_DIR_B="/data/.unimatrix/${SLUG_B}"
  # Per-slug store dbs must EXIST before trusting any cell: a missing db at read
  # time is INFRA, never a phantom 0-row (R-07).
  local db
  for db in "$SLUG_DIR_A/unimatrix.db" "$SLUG_DIR_B/unimatrix.db"; do
    vol test -f "$db" || infra_fail "per-slug store $db missing post-restart (registration/route never built — INFRA)"
  done

  # Probe all FOUR routes non-404 WITHOUT writing a marker (C-06): a benign GET.
  # Any non-404 proves the route exists; only 404 means "route absent". Liveness
  # is a PRECONDITION only — non-404 != isolated.
  local route code
  for route in "/v1/${SLUG_A}/observe" "/v1/${SLUG_B}/observe" "/v1/${SLUG_A}/mcp" "/v1/${SLUG_B}/mcp"; do
    code="$(curl -sS --cacert "$TMP/cert.pem" -H "Authorization: Bearer $TOKEN" \
              -o /dev/null -w '%{http_code}' "https://localhost:${PORT}${route}" 2>/dev/null || true)"
    [ "$code" = "404" ] \
      && infra_fail "route $route is 404 after restart — slug never built a route (unregistered-B trap); INFRA, not an isolation pass"
    log "route $route is non-404 ($code) — PRECONDITION only (non-404 != isolated)."
  done
  log "all 4 routes non-404 (PRECONDITION only). PASS C2"
}

# === C3 observe + C4 MCP-write probes (sourced lib, keeps this file <=500 lines) =
# isolation-probe-lib.sh DEFINES observe_write / mcp_write / mcp_handshake /
# parse_sse_jsonrpc on source (mirrors how docker-http-posture-smoke.sh sources
# cloud-cycle-lib.sh). They run in this gate's scope (log/infra_fail + the C2 vars).
# shellcheck source=product/test/infra-001/scripts/isolation-probe-lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/isolation-probe-lib.sh"

# =====================================================================
# C6 — two-store read primitive (shared by C5 positive + C6 negative)
# =====================================================================
# read_marker(store_dir, table, predicate) -> row-count (>=0) on stdout, or the
# INFRA sentinel. PURE-RETURN (never exits): callers raise infra_fail on INFRA so
# the classification is not swallowed by a command-substitution subshell.
read_marker() {
  local store_dir="$1" table="$2" predicate="$3"
  if [ -n "${SMOKE_READ_MARKER_CMD:-}" ]; then
    # shellcheck disable=SC2086
    $SMOKE_READ_MARKER_CMD "$store_dir" "$table" "$predicate"
    return
  fi
  local slug_db="${store_dir}/unimatrix.db"
  local tmp="${TMP}/read.$$.$RANDOM.db"
  # Main db mandatory: a missing main db = INFRA, never a 0-row cell (R-07).
  if ! vol cat "$slug_db" > "$tmp" 2>/dev/null || [ ! -s "$tmp" ]; then
    rm -f "$tmp"
    printf '%s' "$INFRA_SENTINEL"; return
  fi
  # Sidecars mandatory for the DURABLE post-barrier view (R-04): a single-file copy
  # reads a PRE-checkpoint false-empty snapshot. An absent (already-checkpointed)
  # sidecar is fine ONLY because the main db is present.
  vol cat "${slug_db}-wal" > "${tmp}-wal" 2>/dev/null || rm -f "${tmp}-wal"
  vol cat "${slug_db}-shm" > "${tmp}-shm" 2>/dev/null || rm -f "${tmp}-shm"
  if ! command -v sqlite3 >/dev/null 2>&1; then
    rm -f "$tmp" "${tmp}-wal" "${tmp}-shm"
    printf '%s' "$INFRA_SENTINEL"; return
  fi
  # Predicate carries the marker literal, which is [a-z0-9-] only (R-12) — no LIKE
  # wildcard / quote can alter it.
  local count
  count="$(sqlite3 -json "$tmp" "SELECT count(*) AS n FROM ${table} WHERE ${predicate};" 2>/dev/null \
    | node -e 'let b="";process.stdin.on("data",c=>b+=c).on("end",()=>{let r=[];try{r=JSON.parse(b||"[]")}catch{};process.stdout.write(String((r[0]&&r[0].n)||0))})' 2>/dev/null || true)"
  rm -f "$tmp" "${tmp}-wal" "${tmp}-shm"
  case "$count" in ''|*[!0-9]*) count=0 ;; esac
  printf '%s' "$count"
}

# query_for(surface, marker) -> sets QF_TABLE / QF_PREDICATE.
query_for() {
  local surface="$1" marker="$2"
  if [ "$surface" = "observe" ]; then
    QF_TABLE="observations"
    QF_PREDICATE="topic_signal = '${marker}'"
  else
    QF_TABLE="entries"
    QF_PREDICATE="content LIKE '%${marker}%' OR topic = '${marker}'"   # AC-07 canonical
  fi
}

# =====================================================================
# C5 — per-cell write + read-as-barrier positive control (retry-until-present)
# =====================================================================
# write_then_barrier(surface, slug, store_dir, marker) -> sets WTB ∈ {PRESENT, INFRA}
write_then_barrier() {
  local surface="$1" slug="$2" store_dir="$3" marker="$4"
  if [ "$surface" = "observe" ]; then observe_write "$slug" "$marker"; else mcp_write "$slug" "$marker"; fi
  query_for "$surface" "$marker"
  local n deadline
  deadline=$(( $(date +%s) + READ_DEADLINE_SECS ))
  while :; do
    n="$(read_marker "$store_dir" "$QF_TABLE" "$QF_PREDICATE")"
    if [ "$n" = "$INFRA_SENTINEL" ]; then
      infra_fail "read-as-barrier $slug/$surface: store read failed (missing db / dep) — INFRA"
    fi
    if [ "$n" -ge 1 ] 2>/dev/null; then
      log "positive control $slug/$surface PRESENT (marker durable). marker=$marker"
      WTB="PRESENT"; return
    fi
    if [ "$(date +%s)" -gt "$deadline" ]; then
      # Own marker never appeared -> durability not established. INFRA, NEVER RED,
      # never a vacuous pass (ADR-002 §4 / AC-10 / R-05 sc.3).
      log "positive control $slug/$surface timed out — own marker absent at deadline => INFRA (durability/infra failure, not an isolation RED)."
      WTB="INFRA"; return
    fi
    sleep "$READ_POLL_SLEEP"
  done
}

run_cells() {
  # Strictly sequential per store, each write immediately followed by its own
  # barrier read — no shared aggregate `store_size` barrier (C-08 / ADR-002 §1).
  write_then_barrier observe "$SLUG_A" "$SLUG_DIR_A" "$M_OBS_A"; POS_OBS_A="$WTB"
  write_then_barrier observe "$SLUG_B" "$SLUG_DIR_B" "$M_OBS_B"; POS_OBS_B="$WTB"
  write_then_barrier mcp     "$SLUG_A" "$SLUG_DIR_A" "$M_MCP_A"; POS_MCP_A="$WTB"
  write_then_barrier mcp     "$SLUG_B" "$SLUG_DIR_B" "$M_MCP_B"; POS_MCP_B="$WTB"
}

# =====================================================================
# C6 — cross-store negative cells (positive-gates-negative)
# =====================================================================
# negative_cell(own_pos, store_dir, surface, foreign_marker) -> sets NEG ∈ {ABSENT, RED, SKIPPED}
negative_cell() {
  local own_pos="$1" store_dir="$2" surface="$3" foreign_marker="$4"
  query_for "$surface" "$foreign_marker"
  local n
  n="$(read_marker "$store_dir" "$QF_TABLE" "$QF_PREDICATE")"
  if [ "$n" = "$INFRA_SENTINEL" ]; then
    infra_fail "negative read $store_dir/$surface: store read failed — INFRA"
  fi
  if [ "$n" -ge 1 ] 2>/dev/null; then
    # Real leak — RED, INDEPENDENT of own_pos (a mis-route is RED even when the
    # own-store positive timed out INFRA; ADR-002 §4-5 / R-05 sc.4).
    log "CROSS-STORE LEAK: foreign marker $foreign_marker present in $store_dir"
    NEG="RED"; return
  fi
  # foreign absent:
  if [ "$own_pos" = "PRESENT" ]; then
    NEG="ABSENT"      # clean; eligible to gate a GREEN
  else
    NEG="SKIPPED"     # own positive INFRA — no GREEN claim (no vacuous pass)
  fi
}

run_negatives() {
  # NEG_X = "does store X hold the FOREIGN marker for that surface?"
  # NEG_* are consumed by verdict() via ${!cell} indirect expansion (SC2034 false positive).
  # shellcheck disable=SC2034
  {
    negative_cell "$POS_OBS_A" "$SLUG_DIR_A" observe "$M_OBS_B"; NEG_OBS_A="$NEG"   # B's obs marker in A?
    negative_cell "$POS_OBS_B" "$SLUG_DIR_B" observe "$M_OBS_A"; NEG_OBS_B="$NEG"   # A's obs marker in B?
    negative_cell "$POS_MCP_A" "$SLUG_DIR_A" mcp     "$M_MCP_B"; NEG_MCP_A="$NEG"   # B's mcp marker in A?
    negative_cell "$POS_MCP_B" "$SLUG_DIR_B" mcp     "$M_MCP_A"; NEG_MCP_B="$NEG"   # A's mcp marker in B?
  }
}

# =====================================================================
# Markers (build) + load-bearing non-substring self-check
# =====================================================================
derive_markers() {
  # _default_nonce + MARKER_FID_TOKEN (isolation-probe-lib.sh, #859): the b36 nonce is
  # PII-safe (letter-joined); the fixed all-digit token makes looks_like_feature_id TRUE
  # so observe topic_signal persists. Prefix preserved before the token. Both filters threaded.
  RUN="${RUN:-$(_default_nonce)}"
  case "$RUN" in *[!a-z0-9-]*) infra_fail "RUN nonce '$RUN' not [a-z0-9-]";; esac
  M_OBS_A="infra003-obs-a-${MARKER_FID_TOKEN}-${RUN}"   # A observe -> observations.topic_signal
  M_OBS_B="infra003-obs-b-${MARKER_FID_TOKEN}-${RUN}"   # B observe -> observations.topic_signal
  M_MCP_A="infra003-mcp-a-${MARKER_FID_TOKEN}-${RUN}"   # A MCP    -> entries.content (+ topic)
  M_MCP_B="infra003-mcp-b-${MARKER_FID_TOKEN}-${RUN}"   # B MCP    -> entries.content (+ topic)
  SLUG_DIR_A="${SLUG_DIR_A:-/data/.unimatrix/${SLUG_A}}"
  SLUG_DIR_B="${SLUG_DIR_B:-/data/.unimatrix/${SLUG_B}}"
}

assert_markers_distinct() {
  local markers=("$M_OBS_A" "$M_OBS_B" "$M_MCP_A" "$M_MCP_B")
  local m i j
  for m in "${markers[@]}"; do
    case "$m" in *[!a-z0-9-]*) infra_fail "marker '$m' contains a non-[a-z0-9-] char (R-12)";; esac
  done
  # PII-shape regression canary (#859) — AFTER the R-12 charset guard (its ERE
  # reduction assumes [a-z0-9-] input). Catches a future nonce-derivation regression
  # before the MCP context_store content-scan would reject the marker (-32006).
  for m in "${markers[@]}"; do
    assert_marker_pii_safe "$m"
    assert_marker_feature_id_shaped "$m"   # #859: observe topic_signal needs the feature-id shape (else NULL)
  done
  # Pairwise non-substring (load-bearing: the MCP read uses LIKE '%marker%', R-18).
  for i in "${!markers[@]}"; do
    for j in "${!markers[@]}"; do
      [ "$i" = "$j" ] && continue
      case "${markers[$j]}" in
        *"${markers[$i]}"*) infra_fail "marker '${markers[$i]}' is a substring of '${markers[$j]}' (R-18) — non-substring invariant broken";;
      esac
    done
  done
  log "four markers are charset-safe and pairwise non-substring. PASS"
}

# =====================================================================
# C7 — verdict gate (bidirectional 2x2, positive-gates-negative, tri-state exit)
# =====================================================================
verdict() {
  local cell red=0 infra=0
  # 1. RED DOMINATES — any cross-store leak is a hard isolation failure,
  #    independent of the positive outcomes (mis-route is RED even when its own
  #    positive timed out INFRA). ADR-002 §4-5 / R-05 sc.4.
  for cell in NEG_OBS_A NEG_OBS_B NEG_MCP_A NEG_MCP_B; do
    if [ "${!cell}" = "RED" ]; then red=1; log "RED cell: $cell — cross-store marker present"; fi
  done
  [ "$red" = "1" ] && fail "ISOLATION BROKEN — cross-store marker present (see cells above)"

  # 2. INFRA DOMINATES GREEN — any unestablished own positive forbids a pass.
  for cell in POS_OBS_A POS_OBS_B POS_MCP_A POS_MCP_B; do
    if [ "${!cell}" = "INFRA" ]; then infra=1; log "INFRA: own positive $cell never reached PRESENT"; fi
  done
  [ "$infra" = "1" ] \
    && infra_fail "durability/precondition not established for >=1 direction — INFRA (not an isolation pass, not a RED)"

  # 3. GREEN — every positive PRESENT in its own store AND every cross-cell ABSENT.
  log "observe: A has obs-a not obs-b; B has obs-b not obs-a => observe GREEN"
  log "mcp: A has mcp-a not mcp-b; B has mcp-b not mcp-a => mcp GREEN"
  log "point-in-time proof only: advances N3 (#5161), does not close it (N5/#788 lane unwired)."
  log "ALL GATES PASSED — bidirectional 2x2 isolation holds on both surfaces."
  exit 0
}

# Sourceable orchestration entry (the off-Docker gate-logic test sources this file
# and calls run_isolation_matrix against the SMOKE_*_CMD stubs — single source of
# truth, #5192). Assumes C2 already established the live container in a real run.
run_isolation_matrix() {
  derive_markers
  assert_markers_distinct
  run_cells
  run_negatives
  verdict
}

# =====================================================================
# C-WB — bounded warmup / readiness barrier (infra-004 / ADR-001 #5349)
# =====================================================================
# Inserted between assert_routes_live (C2 route-liveness) and run_isolation_matrix
# (C3/C4 load-bearing writes): confirm the EMBEDDING path is warm via an MCP
# context_store write + read-back round trip — the ONLY served write that exercises
# the model (store_ops.rs:131-133 get_adapter; observe is a fire-and-forget SQL
# insert with zero embed dependency, so it is the WRONG readiness proxy). Bounded by
# the #767-derived WARMUP_DEADLINE_SECS. A healthy run — incl. the cold first-boot
# HuggingFace download — proceeds; a not-ready state past the deadline => infra_fail
# (exit 2), NEVER RED/GREEN. No new mechanism: reuses write_then_barrier (already
# SMOKE_*_CMD stub-seam compatible) on a widened deadline.
warmup_barrier() {
  # 1. Establish RUN via the SAME _default_nonce seam derive_markers uses (#859).
  RUN="${RUN:-$(_default_nonce)}"
  case "$RUN" in *[!a-z0-9-]*) infra_fail "RUN nonce '$RUN' not [a-z0-9-]";; esac

  # 2. Build the throwaway warmup marker (charset + PII-shape + feature-id-shape, #859).
  local warmup_marker="infra003-warmup-${MARKER_FID_TOKEN}-${RUN}"
  case "$warmup_marker" in *[!a-z0-9-]*) infra_fail "warmup marker '$warmup_marker' not [a-z0-9-]";; esac
  assert_marker_pii_safe "$warmup_marker"
  assert_marker_feature_id_shaped "$warmup_marker"

  # 3. Load-bearing non-substring assertion vs the four cell markers (R-02).
  derive_markers   # idempotent: sets M_OBS_A/M_OBS_B/M_MCP_A/M_MCP_B from the SAME RUN
  local cell
  for cell in "$M_OBS_A" "$M_OBS_B" "$M_MCP_A" "$M_MCP_B"; do
    case "$cell" in *"$warmup_marker"*)
      infra_fail "warmup marker '$warmup_marker' collides (substring) with cell marker '$cell' (R-02) — non-substring invariant broken";; esac
    case "$warmup_marker" in *"$cell"*)
      infra_fail "warmup marker '$warmup_marker' collides (substring) with cell marker '$cell' (R-02) — non-substring invariant broken";; esac
  done
  log "warmup marker is charset-safe and pairwise non-substring of the four cell markers. PASS"

  # 4. One throwaway MCP context_store round trip on the LONGER #767-derived deadline.
  #    MCP is the only embedding-exercising surface (resolves OQ-WB-1: warm MCP, not
  #    observe). Reuse write_then_barrier (mcp_write + entries read-back); widen the
  #    poll deadline for this call only, then restore the matrix bound below.
  local saved_read_deadline="$READ_DEADLINE_SECS"
  READ_DEADLINE_SECS="$WARMUP_DEADLINE_SECS"
  log "warmup barrier: MCP context_store round trip to $SLUG_A (bound ${WARMUP_DEADLINE_SECS}s, #767-derived) ..."
  write_then_barrier mcp "$SLUG_A" "$SLUG_DIR_A" "$warmup_marker"   # sets WTB in {PRESENT, INFRA}
  READ_DEADLINE_SECS="$saved_read_deadline"

  # 5. CONSUME the PRESENT signal to gate proceed-to-matrix (R-01 funnel).
  case "$WTB" in   # WTB in {PRESENT, INFRA}
    PRESENT) log "warmup PRESENT — embedding path warm + store $SLUG_A MCP write durable; proceed to matrix." ;;
    *)  # INFRA (deadline timeout). A store-read failure already infra_fail'd inside write_then_barrier.
      infra_fail "warmup barrier: MCP context_store round trip not durable within ${WARMUP_DEADLINE_SECS}s => INFRA (embedding model not loaded / store not durable). NOT a RED, NOT a GREEN."
      ;;
  esac
}

# === Sourced-guard (#5192) ====================================================
# When SOURCED (the gate-logic test sources to drive run_isolation_matrix /
# component fns against stubs), stop here — do NOT run preflight / boot / verdict.
# When EXECUTED directly (the real smoke), $0 == BASH_SOURCE and main runs.
if [ "${BASH_SOURCE[0]}" != "${0}" ]; then
  return 0 2>/dev/null || true
fi

# =====================================================================
# main (executed directly only)
# =====================================================================
trap cleanup EXIT
preflight                    # C1
setup_container              # C2 boot
register_both_and_restart    # C2 registration + single restart
assert_routes_live           # C2 route-liveness PRECONDITION
warmup_barrier               # C-WB model-load + durable-write readiness gate (PRESENT|INFRA)
run_isolation_matrix         # C3/C4 writes -> C5 barrier -> C6 negatives -> C7 verdict (exits)
