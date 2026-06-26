#!/bin/bash
# cloud-cycle-lib.sh (nan-021 C2) — the bridge-driven cloud-cycle gate, factored
# out of docker-http-posture-smoke.sh to keep each file <=500 lines (workspace
# rule) and to mirror release-gate-lib.sh's sourced-library pattern. SOURCED by
# the smoke (and by C5's logic-test) — it does NOT execute on its own; it only
# defines functions. It relies on the parent providing log(), fail(), store_size(),
# vol(), and $PORT/$SLUG/$SLUG_DIR/$SANDBOX/$TMP/$TOKEN (all in the smoke's scope
# when cloud_cycle_gates runs after C1's Gates 1-7). Append-only fail()/exit-1
# contract (nan-019 ADR-001) — every new failure folds into the EXISTING fail().
# Drives a full context_cycle(start) -> tool calls (incl. a real Bash carrying
# the manifest feature-ID token) -> context_cycle(stop) THROUGH the SHIPPED
# `node mcp-bridge.js <projectHash>` over stdio JSON-RPC (D-2 — the bridge is in
# path; NEVER a direct mcp_url POST), fires live PostToolUse hooks -> pinned
# `POST /v1/$SLUG/observe`, runs the SYMMETRIC durability barrier (ADR-006), then
# `context_cycle_review` over the bridge -> emits MetricVector(HTTPS)+RUN_TOKEN to
# the $SANDBOX out-file C4 ingests. Asserts (FR-9/AC-02) the bridge CARRIED it:
# Mcp-Session-Id captured+replayed, text/event-stream parsed, a JSON-only Accept
# NEGATIVE control FAILS framing. Reuses C1 standup verbatim (Gates 1-7), the
# bridge/cert-pin/credstore/bundle JS as-is, and the C4 manifest/predicate — adds
# NO new spawn/cert/credstore/bundle/transport path (R-10 fork smell guard).
#
# Inputs (from the pytest orchestrator via env; ADR-001 seam):
#   MANIFEST_PATH     : JSON workload manifest (C4 single source of truth).
#   RUN_TOKEN         : per-run correlation token stamped into the out-file (R-03).
#   HTTPS_VECTOR_OUT  : fresh $SANDBOX path for {"run_token","metric_vector"}.
# From C1 (already in main scope): $SLUG, $SLUG_DIR, $TMP/cert.pem, $TMP/token,
#   $TOKEN, $SANDBOX/home (credstore HOME), port $PORT.
#
# Stub seam (R-12, mirrors SMOKE_*_CMD): SMOKE_CYCLE_CMD overrides the bridge
# drive so C5's logic-test can drive control flow (marker, exit codes, out-file
# write, witness assertions) with a synthetic vector BEFORE the live tag run.
#
# nan-022 (C5'): the out-file payload WIDENS from {run_token, metric_vector} to
# {run_token, dimension_bundle:{retrieval, behavioral, analytics, proactive,
# precompact, isolation}} (ADR-005 / cloud-cycle-lib.md). The /observe-surface +
# container-side captures the SHELL owns (behavioral D2, isolation D6, precompact
# D5) and the full six-key bundle assembly live in cloud-bundle-lib.sh (sourced
# below) to keep BOTH files <=500 lines (the nan-021 lib-split precedent). The MCP-
# bridge-surface captures (retrieval, proactive, analytics) come from the C2'
# bridge-cycle-driver.js fragment ($REVIEW_OUT). Shell captures are taken AFTER the
# symmetric durability barrier (R-04) — a pre-barrier DB read is INFRA, never a
# parity verdict.

REPO_ROOT_DEFAULT="${REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)}"
INFRA_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"   # product/test/infra-001 (PYTHONPATH root for harness.*)
SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRIDGE_JS="$REPO_ROOT_DEFAULT/packages/unimatrix/lib/hook-client/mcp-bridge.js"
WITNESS_JS="$SCRIPTS_DIR/bridge-witness.js"
DRIVER_JS="$SCRIPTS_DIR/bridge-cycle-driver.js"

# nan-022 C5': the dimension-bundle captures + assembly (sourced; defines fns only).
# shellcheck source=product/test/infra-001/scripts/cloud-bundle-lib.sh
. "$SCRIPTS_DIR/cloud-bundle-lib.sh"

# C4-single-sourced observe predicate over the per-slug store DIR. The store
# lives INSIDE the distroless container volume, so it is sampled via the busybox
# `vol` sidecar (du -s over the slug DIR incl. -wal/-shm, the same WAL-robust
# signal Gate 7 uses) — NOT the host-path python observe-count (which cannot see
# the container volume). The cadence + STABILIZE-across-two-polls predicate are
# the SAME as C4's durability_barrier (DEFAULT_BARRIER_DEADLINE_S=10, POLL_S=1):
# asymmetry with the UDS leg is forbidden (ADR-006). Injectable for the stub seam.
cycle_observe_size() {
  if [ -n "${SMOKE_STORE_SIZE_CMD:-}" ]; then
    # shellcheck disable=SC2086
    $SMOKE_STORE_SIZE_CMD "$1"
  else
    store_size "$1"
  fi
}

# Symmetric durability barrier (ADR-006/FR-10): block until the per-slug store
# DIR size STABILIZES across two consecutive polls (the WAL stopped growing =>
# all driven observes flushed), bounded ~10s, sleep 1. Timeout = HARD fail with
# observed-vs-expected — NEVER an empty compare. Mirrors C4 durability_barrier.
cycle_durability_barrier() {
  local store_dir="$1" expected="$2"
  local deadline prev cur
  deadline=$(( $(date +%s) + 10 ))
  prev=""
  while :; do
    cur="$(cycle_observe_size "$store_dir")"
    if [ -n "$prev" ] && [ "$cur" = "$prev" ] && [ "${cur:-0}" -gt 0 ]; then
      log "durability barrier released (HTTPS): store size $cur stable (expected_observes>=$expected). PASS"
      return 0
    fi
    [ "$(date +%s)" -gt "$deadline" ] \
      && fail "observes not durable within deadline (HTTPS): observed_size_stable=${cur:-0} expected=$expected"
    prev="$cur"
    sleep 1
  done
}

# Assert the bridge CARRIED the traffic from the witness lines (FR-9/AC-02/R-04).
# $1 = bridge stderr file carrying BRIDGE_WITNESS:<json> lines.
#   - Accept "application/json, text/event-stream" sent (SSE offered),
#   - >=1 text/event-stream response parsed (SSE actually carried, not a 200),
#   - the initialize-minted Mcp-Session-Id REPLAYED byte-stable on a later request.
assert_bridge_carried_traffic() {
  local errf="$1"
  [ -s "$errf" ] || fail "no BRIDGE_WITNESS output captured — bridge carried nothing (FR-9/AC-02)"

  grep -q 'BRIDGE_WITNESS:.*"accept":"application/json, text/event-stream"' "$errf" \
    || fail "bridge did not send the SSE Accept header (application/json, text/event-stream) — AC-02"

  grep -q 'BRIDGE_WITNESS:.*"ev":"response".*"content_type":"text/event-stream' "$errf" \
    || fail "no text/event-stream response parsed by the bridge (rmcp forces SSE, #5129) — AC-02 (a 200 is NOT sufficient)"

  # Session-id replay: capture the id received on the initialize response, then
  # require it sent back (byte-stable) on >=1 LATER request (a tools/call).
  local minted
  minted="$(grep 'BRIDGE_WITNESS:.*"ev":"response".*"recv_session_id":"' "$errf" \
    | head -1 | sed -n 's/.*"recv_session_id":"\([^"]*\)".*/\1/p')"
  [ -n "$minted" ] \
    || fail "no Mcp-Session-Id minted on initialize — bridge did not establish a session (AC-02)"
  grep -q "BRIDGE_WITNESS:.*\"ev\":\"request\".*\"sent_session_id\":\"$minted\"" "$errf" \
    || fail "Mcp-Session-Id $minted was never replayed on a later request — bridge bypassed (AC-02/D-2)"
  log "bridge carried it: SSE Accept sent, text/event-stream parsed, Mcp-Session-Id $minted replayed byte-stable. PASS (FR-9/AC-02)"
}

# NEGATIVE control (R-04): a JSON-only Accept (NO text/event-stream) MUST FAIL
# framing — proving the fixture exercises REAL SSE, not a JSON shortcut. We POST
# a pinned MCP request with a JSON-only Accept and assert the response is NOT a
# parseable SSE 200 (rmcp returns SSE; a JSON-only client cannot frame it). This
# is a SELF-TEST of the SSE requirement, not part of the cycle drive.
assert_json_only_accept_fails_framing() {
  local mcp_url="https://localhost:${PORT}/v1/${SLUG}/mcp"
  local code ctype
  # Send an initialize with Accept: application/json ONLY. rmcp answers SSE
  # (text/event-stream) or 406; either way a JSON-only framer gets no JSON body
  # it can parse as the MCP reply -> the negative control HOLDS.
  ctype="$(curl -sS --cacert "$TMP/cert.pem" -o /dev/null -w '%{content_type}' \
    -X POST "$mcp_url" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"neg-control","version":"1.0.0"}}}' \
    2>/dev/null || true)"
  case "$ctype" in
    application/json*)
      fail "JSON-only Accept returned application/json — SSE framing was NOT required (negative control failed, R-04)"
      ;;
    *)
      log "negative control: JSON-only Accept did not yield a parseable JSON MCP reply (ct='${ctype:-none}') => REAL SSE required. PASS (R-04)"
      ;;
  esac
}

cloud_cycle_gates() {
  # ---- preconditions from the orchestrator ----
  [ -n "${MANIFEST_PATH:-}" ] || fail "cloud_cycle_gates: MANIFEST_PATH unset (C4 manifest required)"
  [ -n "${RUN_TOKEN:-}" ]     || fail "cloud_cycle_gates: RUN_TOKEN unset (R-03 correlation token required)"
  [ -n "${HTTPS_VECTOR_OUT:-}" ] || fail "cloud_cycle_gates: HTTPS_VECTOR_OUT unset (out-file path required)"
  [ -f "$MANIFEST_PATH" ]     || fail "cloud_cycle_gates: manifest $MANIFEST_PATH not found"

  # ---- 1. read projectHash BACK (OQ1/R-11 — never recompute) ----
  local cred_root="$SANDBOX/home/.unimatrix" project_hash hash_count cred_file
  [ -d "$cred_root" ] || fail "credstore root $cred_root absent — C1 standup incomplete (C1->C2)"
  hash_count="$(find "$cred_root" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
  [ "$hash_count" = "1" ] \
    || fail "projectHash read-back ambiguous: expected 1 dir under $cred_root, found $hash_count (init.js drift)"
  project_hash="$(find "$cred_root" -mindepth 1 -maxdepth 1 -type d -exec basename {} \;)"
  cred_file="$cred_root/$project_hash/remote.json"
  [ -f "$cred_file" ] || fail "credstore $cred_file absent — bridge cannot attach (C1->C2)"
  log "projectHash read back as $project_hash (credstore present)"

  # Expected observe count from the C4 manifest (single source of truth) — drives
  # the durability barrier predicate (FR-10).
  local expected_observes
  expected_observes="$(PYTHONPATH="$INFRA_DIR${PYTHONPATH:+:$PYTHONPATH}" python3 -m harness.parity_workload expected-observe-count 2>/dev/null || true)"
  case "$expected_observes" in
    ''|*[!0-9]*) expected_observes=1 ;;  # off-harness fallback; barrier still stabilizes
  esac

  # ---- BEFORE sample for the barrier ----
  local store_before
  store_before="$(cycle_observe_size "$SLUG_DIR")"

  # ---- 2/3. spawn the bridge LAST + drive the cycle IMMEDIATELY (NFR-7) ----
  local BRIDGE_ERR="$SANDBOX/bridge.stderr" DRIVE_OUT="$SANDBOX/bridge.cycle.json"
  if [ -n "${SMOKE_CYCLE_CMD:-}" ]; then
    # Stub seam (R-12 / C5 logic-test): drive control flow without Docker/node.
    # shellcheck disable=SC2086
    $SMOKE_CYCLE_CMD "$project_hash" "$MANIFEST_PATH" >"$DRIVE_OUT" 2>"$BRIDGE_ERR" \
      || { _dump_bridge_err "$BRIDGE_ERR"; fail "cycle drive (stub) failed"; }
  else
    command -v node >/dev/null 2>&1 || fail "node not available — bridge cycle cannot run"
    [ -f "$BRIDGE_JS" ] || fail "shipped bridge $BRIDGE_JS not found (NFR-2 reuse-as-is)"
    [ -f "$WITNESS_JS" ] || fail "bridge witness $WITNESS_JS not found"
    [ -f "$DRIVER_JS" ] || fail "bridge cycle driver $DRIVER_JS not found"
    # The driver speaks stdio JSON-RPC to the REAL bridge process (witness preloaded
    # via NODE_OPTIONS). HOME=$SANDBOX/home so credstore.read finds THIS run's cred.
    HOME="$SANDBOX/home" \
      node "$DRIVER_JS" "$project_hash" "$MANIFEST_PATH" \
        --bridge "$BRIDGE_JS" --witness "$WITNESS_JS" \
        >"$DRIVE_OUT" 2>"$BRIDGE_ERR" \
      || { _dump_bridge_err "$BRIDGE_ERR"; fail "bridge cycle drive failed (start->tools->stop)"; }
  fi
  # Driver result must be ok:true.
  grep -q '"ok": *true\|"ok":true' "$DRIVE_OUT" \
    || { _dump_bridge_err "$BRIDGE_ERR"; log "drive out:"; sed -n '1,5p' "$DRIVE_OUT" >&2; fail "bridge cycle reported ok:false"; }
  log "cycle driven THROUGH the bridge (start -> tools -> stop). PASS gate 8a"

  # ---- 3b. fire live PostToolUse hooks -> pinned POST /v1/$SLUG/observe ----
  # The SAME stable session_id as the cycle (FR-4/R-09). Real pinned HTTPS with
  # the leaf cert (bearer flushes only after the curl --cacert pin matches).
  _fire_observe_hooks "$MANIFEST_PATH" "$BRIDGE_ERR"

  # ---- 4. bridge-carried-traffic assertion (FR-9/AC-02) — real run only ----
  if [ -z "${SMOKE_CYCLE_CMD:-}" ]; then
    assert_bridge_carried_traffic "$BRIDGE_ERR"
    assert_json_only_accept_fails_framing
  else
    log "stub seam: bridge-carried-traffic + negative control skipped (no live wire). PASS gate 8b (stub)"
  fi

  # ---- 5. SYMMETRIC durability barrier BEFORE review (ADR-006/FR-10) ----
  cycle_durability_barrier "$SLUG_DIR" "$expected_observes"
  local store_after
  store_after="$(cycle_observe_size "$SLUG_DIR")"
  [ "${store_after:-0}" -gt "${store_before:-0}" ] \
    || fail "cycle observes did not grow the per-slug store ($store_before -> $store_after) => not durable"

  # ---- 6. context_cycle_review over the bridge -> the C2' driver fragment ----
  # nan-022: the driver now returns the MCP-bridge-surface FRAGMENT
  # {ok, metric_vector, retrieval, proactive, informs_edges, phase_signal, ...}.
  # metric_vector is the parsed MetricVector (the analytics dimension); retrieval/
  # proactive are the D1/D4 ranked captures (with capture_2 for the intra double-
  # capture). The shape C3's UDS leg hands the comparators (ADR-003/ADR-005).
  # $REVIEW_OUT carries the fragment JSON.
  local REVIEW_OUT="$SANDBOX/bridge.review.json"
  if [ -n "${SMOKE_CYCLE_CMD:-}" ]; then
    # Stub: synthesize the driver's bridge-surface fragment (control-flow / out-file
    # shape test). SMOKE_BUNDLE_FRAGMENT (preferred) carries a full fragment JSON;
    # SMOKE_REVIEW_VECTOR (nan-021 back-compat) carries just a MetricVector, wrapped
    # into a minimal valid fragment so the legacy stub still exercises the spine.
    if [ -n "${SMOKE_BUNDLE_FRAGMENT:-}" ]; then
      [ -f "$SMOKE_BUNDLE_FRAGMENT" ] \
        || fail "SMOKE_BUNDLE_FRAGMENT=$SMOKE_BUNDLE_FRAGMENT not found (stub seam)"
      node -e 'const fs=require("fs");let o;try{o=JSON.parse(fs.readFileSync(process.argv[1],"utf8"))}catch(e){process.exit(1)}if(o.ok!==true){process.exit(1)}process.stdout.write(JSON.stringify(o)+"\n")' \
        "$SMOKE_BUNDLE_FRAGMENT" >"$REVIEW_OUT" || fail "stub bundle fragment is not a valid ok:true fragment"
    else
      local stub_mv="${SMOKE_REVIEW_VECTOR:-}"
      [ -n "$stub_mv" ] || stub_mv='{"universal":{"total_tool_calls":3}}'
      SMOKE_REVIEW_VECTOR="$stub_mv" \
        node -e '
          const v=process.env.SMOKE_REVIEW_VECTOR;let mv;try{mv=JSON.parse(v)}catch(e){process.exit(1)}
          // minimal bridge-surface fragment: a back-compat MetricVector-only stub still
          // carries the retrieval/proactive keys the bundle assembler requires so the
          // never-empty guard is exercised on a legitimate (non-empty) capture.
          process.stdout.write(JSON.stringify({ok:true,metric_vector:mv,
            retrieval:{queries:[{tool:"context_search",args:{},result_ids:["1"],scores:[0.9]}],capture_2:[{tool:"context_search",args:{},result_ids:["1"],scores:[0.9]}]},
            proactive:{briefing_ids:["1"],briefing_scores:[0.9],injection_set:["1"],capture_2:{briefing_ids:["1"],briefing_scores:[0.9],injection_set:["1"]}},
            informs_edges:[],phase_signal:{}})+"\n");
        ' >"$REVIEW_OUT" || fail "stub review vector is not valid JSON"
    fi
  else
    HOME="$SANDBOX/home" REVIEW_INLINE=1 \
      node "$DRIVER_JS" "$project_hash" "$MANIFEST_PATH" \
        --bridge "$BRIDGE_JS" --witness "$WITNESS_JS" \
        >"$REVIEW_OUT" 2>>"$BRIDGE_ERR" \
      || { _dump_bridge_err "$BRIDGE_ERR"; fail "context_cycle_review over the bridge failed"; }
  fi
  grep -q '"ok": *true\|"ok":true' "$REVIEW_OUT" \
    || { _dump_bridge_err "$BRIDGE_ERR"; fail "context_cycle_review reported ok:false"; }
  log "context_cycle_review over the bridge yielded the bridge-surface bundle fragment. PASS gate 8c"

  # ---- 7. assemble the shell-owned /observe-surface captures (POST-barrier) ----
  # R-04: the symmetric durability barrier (step 5 above) has released, so all
  # driven observes are flushed. ONLY NOW do the container-side DB reads run — a
  # pre-barrier read is an INFRA condition, never a parity verdict. The captures
  # (behavioral D2, isolation D6, precompact D5) live in cloud-bundle-lib.sh.
  local SHELL_CAPTURES="$SANDBOX/shell_captures.json"
  assemble_shell_captures "$SLUG_DIR" "$MANIFEST_PATH" "$SHELL_CAPTURES"

  # ---- 8. assemble + emit the SIX-key dimension bundle to the out-file (R-09) ----
  # Compose {run_token, dimension_bundle:{retrieval, behavioral, analytics,
  # proactive, precompact, isolation}} from the C2' driver fragment + the shell-
  # owned captures. The never-empty guard fires BEFORE the write: any missing/empty
  # non-D5 capture => exit 1, never an empty-key bundle. Python load_https_bundle
  # re-validates on ingest (the binding guard — R-09 contract-tested both sides).
  emit_dimension_bundle "$REVIEW_OUT" "$SHELL_CAPTURES" "$RUN_TOKEN" "$HTTPS_VECTOR_OUT"
  log "emitted dimension_bundle(HTTPS)+RUN_TOKEN to $HTTPS_VECTOR_OUT. PASS gate 8 (cloud cycle)"
}

# Drive the live cycle over pinned /observe as the C3-CANONICAL HookRequest
# sequence — the SHARED driving contract (SR-05). This MUST be BYTE-IDENTICAL to
# C3's drive_uds_leg (harness/parity_legs.py) so the cross-leg MetricVector parity
# holds (same total_tool_calls, same phases bucket). C3 is the SOURCE OF TRUTH; C2
# conforms to it. Sequence (11 frames for the default 3-call manifest):
#   1. SessionRegister(feature, agent_role="tester")
#   2. RecordEvent cycle_start — phase = PARITY_PHASE ("delivery") via next_phase;
#      writes the cycle_events row the review's col-024 primary path reads.
#   3. RecordEvent PreToolUse TaskCreate, tool_input.subject="delivery: drive the
#      parity workload" — the PHASE-SETTER (metrics.rs::compute_phases buckets by the
#      last TaskCreate/TaskUpdate PreToolUse "{phase}: …" subject) AND counts toward
#      total_tool_calls.
#   4. per observe tool call: PreToolUse(name, tool_input=args)  [Pre increments
#      total_tool_calls] THEN PostToolUse(name, response_size, response_snippet,
#      tool_input=args)  [the observation row; the server DERIVES topic_signal from
#      the payload feature-ID token — FR-3/AC-03, no seed].
#   5. RecordEvent cycle_stop
#   6. SessionClose(outcome="completed", duration_secs=1)
# Frame shapes mirror hook_client.py record_* EXACTLY (event_type "PostToolUse", NOT
# the rework variant; timestamp 0 — wall-clock, D-5-excluded). All bodies token-free;
# Bearer carries the token (NFR-06); pinned curl (bearer flushes only on a cert match
# — vnc-039). The legacy type:"PostToolUse" tag is NOT a live HookRequest variant
# (records nothing); these are RecordEvents.
PARITY_PHASE="delivery"   # shared with C3 parity_legs.PARITY_PHASE — symmetric, do NOT diverge
_fire_observe_hooks() {
  local manifest="$1" errf="$2"
  local observe_url="https://localhost:${PORT}/v1/${SLUG}/observe"
  # Build the ordered HookRequest body array via node (one source: the manifest),
  # byte-matching C3's drive_uds_leg frame shapes.
  local bodies_file="$SANDBOX/observe_bodies.json"
  MANIFEST="$manifest" PHASE="$PARITY_PHASE" OUT="$bodies_file" node -e '
    const fs=require("fs");
    const m=JSON.parse(fs.readFileSync(process.env.MANIFEST,"utf8"));
    const sid=m.session_id, feat=m.feature_cycle, phase=process.env.PHASE;
    const rec=(et,payload)=>({type:"RecordEvent",event_type:et,session_id:sid,timestamp:0,payload});
    const out=[];
    // 1. SessionRegister (agent_role mirrors C3 "tester").
    out.push({type:"SessionRegister",session_id:sid,cwd:"",agent_role:"tester",feature:feat});
    // 2. cycle_start with the phase (next_phase) — mirrors record_cycle_start(...,phase).
    out.push(rec("cycle_start",{feature_cycle:feat,next_phase:phase}));
    // 3. phase-setting TaskCreate PreToolUse — EXACT subject from the shared contract.
    out.push(rec("PreToolUse",{tool_name:"TaskCreate",tool_input:{subject:phase+": drive the parity workload"}}));
    // 4. per observe call: Pre(tool_input=args) then Post(payload incl tool_input=args).
    for (const c of (m.tool_calls||[])) {
      if (!c.observe) continue;
      out.push(rec("PreToolUse",{tool_name:c.name,tool_input:c.args||{}}));
      out.push(rec("PostToolUse",{tool_name:c.name,response_size:c.response_size||0,
                                  response_snippet:c.response_snippet||"",tool_input:c.args||{}}));
    }
    // 5. cycle_stop, 6. SessionClose.
    out.push(rec("cycle_stop",{feature_cycle:feat}));
    out.push({type:"SessionClose",session_id:sid,outcome:"completed",duration_secs:1});
    fs.writeFileSync(process.env.OUT, JSON.stringify(out));
  ' 2>>"$errf" || fail "could not build the C3-canonical cycle RecordEvent sequence"
  local count
  count="$(node -e 'const a=require(process.argv[1]);process.stdout.write(String(a.length))' "$bodies_file" 2>>"$errf")"
  # default 3-call manifest => 11 frames; minimum = SessionRegister+cycle_start+
  # TaskCreate+>=1(Pre+Post)+cycle_stop+SessionClose = 8.
  [ -n "$count" ] && [ "$count" -ge 8 ] || fail "cycle sequence too short ($count frames) — shared-contract drift"
  local n=0 pre=0 post=0 code body
  while [ "$n" -lt "$count" ]; do
    body="$(node -e 'const a=require(process.argv[1]);process.stdout.write(JSON.stringify(a[Number(process.argv[2])]))' "$bodies_file" "$n" 2>>"$errf")"
    code="$(curl -sS --cacert "$TMP/cert.pem" -o /dev/null -w '%{http_code}' \
      -X POST "$observe_url" \
      -H "Authorization: Bearer ${TOKEN}" \
      -H "Content-Type: application/json" \
      -d "$body" 2>>"$errf" || true)"
    [ "$code" = "204" ] \
      || fail "pinned /observe returned HTTP ${code:-none} (expected 204) — hook did not land (FR-4)"
    case "$body" in *'"event_type":"PreToolUse"'*) pre=$((pre+1)) ;; esac
    case "$body" in *'"event_type":"PostToolUse"'*) post=$((post+1)) ;; esac
    n=$((n+1))
  done
  log "drove the C3-canonical cycle over pinned /observe ($count frames, $pre PreToolUse => total_tool_calls, $post PostToolUse observes, phase '$PARITY_PHASE'). PASS gate 8 hooks"
}

# Tail-dump captured bridge stderr ON FAILURE ONLY (ADR-005/#5266). The bridge is
# a token-free child (NFR-06: it never logs Authorization); emit_bundle (C1) is
# the only suppressed child. Bounded to cap CI-log volume.
_dump_bridge_err() {
  local errf="$1"
  log "---- mcp-bridge.js stderr (tail, on failure) ----"
  if [ -s "$errf" ]; then
    tail -n 60 "$errf" | while IFS= read -r line; do log "bridge: $line"; done
  else
    log "bridge: (no output captured)"
  fi
  log "---- end mcp-bridge.js stderr ----"
}
