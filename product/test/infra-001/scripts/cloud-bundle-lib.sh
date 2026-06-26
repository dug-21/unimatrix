#!/bin/bash
# cloud-bundle-lib.sh (nan-022 C5') — the /observe-surface captures + the
# dimension-bundle assembly, factored out of cloud-cycle-lib.sh to keep BOTH
# files <=500 lines (workspace rule) and to mirror nan-021's lib-split precedent
# (cloud-cycle-lib.sh was itself factored out of docker-http-posture-smoke.sh).
# SOURCED by cloud-cycle-lib.sh (which the smoke + C5's logic-test source) — it
# does NOT execute on its own; it only DEFINES functions. It relies on the parent
# (the smoke / cloud_cycle_gates) providing log(), fail(), cycle_observe_size(),
# and $SANDBOX/$SLUG/$SLUG_DIR/$PORT/$TOKEN/$TMP/$RUN_TOKEN (all in scope when
# cloud_cycle_gates calls these AFTER the symmetric durability barrier — R-04).
#
# Scope (nan-022 ADR-005 / cloud-cycle-lib.md Change 1+2): the bridge driver
# (bridge-cycle-driver.js, C2') owns the MCP-bridge-surface captures (retrieval,
# proactive, analytics MetricVector + informs_edges + phase_signal). THIS lib owns
# the /observe-surface + container-side-DB-read captures the SHELL drives:
#   * BEHAVIORAL (D2): DISTINCT topic_signal rows, read container-side AFTER the
#     barrier (DERIVED, never seeded).
#   * PRECOMPACT (D5): server-restored payload + measurable/host_side_gap call-out
#     (NEVER a silent drop / vacuous pass — name the gap; ADR-006).
# It then ASSEMBLES the full five-key dimension_bundle from the C2' driver fragment
# + these shell-owned captures and writes {run_token, dimension_bundle:{...}} to
# $HTTPS_VECTOR_OUT (replacing the nan-021 {run_token, metric_vector} emit). The
# never-empty guard is the FIRST line; the Python load_https_bundle (K5) ingest is
# the binding one (R-09, contract-tested both sides). Append-only fail()/exit-1
# contract (nan-019 ADR-001) — every new capture failure folds into fail().
#
# NO new spawn/cert/credstore/bundle/transport path (R-16 fork-smell guard): the
# /observe captures ride the SAME pinned curl idiom _fire_observe_hooks uses; the
# container-side DB reads ride the SAME vol() busybox sidecar Gate 7 uses.

# --- stub seam (R-12 / off-Docker logic test) -------------------------------
# The shell-owned captures hit the live container (pinned /observe + busybox vol
# read). Off-Docker, the C5 logic-test injects synthesized captures so the bundle
# assembly + never-empty guard + barrier ordering are exercised WITHOUT Docker:
#   SMOKE_SHELL_CAPTURES : path to a JSON file carrying the pre-synthesized
#                          {topic_signals, precompact} shell fragment.
#                          When set, the capture_* helpers are SKIPPED and
#                          this file is used verbatim as $SHELL_CAPTURES (the same
#                          real-vs-stub split the rest of the C2 seam uses).
# This mirrors SMOKE_CYCLE_CMD / SMOKE_REVIEW_VECTOR: the stub proves control flow
# (emit/never-empty/out-file) pre-tag (#5258); the live captures run on the tag.

# capture_behavioral_topic_signals <store_dir> <out_file>
#   D2: read the DISTINCT topic_signal values from the per-slug observations table,
#   CONTAINER-SIDE via the busybox vol sidecar (the store lives inside the volume —
#   a host-path read cannot see it; matches how Gate 7 samples the volume). MUST be
#   called AFTER the durability barrier (R-04): a pre-barrier read is an INFRA-ERROR
#   condition (the WAL may not be flushed) — the CALLER enforces ordering, this fn
#   asserts the barrier-released store is non-empty. Emits a JSON array of strings.
#   DERIVED by the server from the payload feature-ID token, never seeded (FR-3).
capture_behavioral_topic_signals() {
  local store_dir="$1" out_file="$2"
  local slug_db="$store_dir/unimatrix.db"
  # The runtime image is distroless (no shell/sqlite IN it), so the per-slug db is
  # read through the busybox `vol` sidecar exactly as Gate 7 samples the volume: the
  # WHOLE file is `vol cat`-copied out to the hermetic sandbox, then the DISTINCT
  # server-derived topic_signal values are queried HOST-SIDE. The copy is a snapshot
  # AFTER the barrier (R-04) so the WAL is flushed and the rows are durable.
  #
  # NOTE (Stage-3c flag — see report): the host query uses `sqlite3`. That binary is
  # NOT guaranteed on the distroless image (we never exec it there) NOR on every
  # runner. The release lane MUST provision sqlite3 on the host (it already provisions
  # node). If absent, this is an INFRA-ERROR (a mis-provisioned lane), NEVER a silent
  # empty capture that empty-passes — same hard-fail discipline as node-absence in
  # docker-http-posture-smoke.sh. The off-Docker logic test drives this fn via the
  # SMOKE_SHELL_CAPTURES stub, so it does not depend on sqlite3 being present here.
  # WAL discipline (Stage-3c first-live-run fix — Stage-3c fix; see product/features/nan-022/testing/RISK-COVERAGE-REPORT.md / R-04): the per-slug store
  # runs in WAL mode and the durability barrier proves the bytes are DURABLE on disk
  # (it counts the `-wal`), but it does NOT checkpoint the WAL into the main db file.
  # Copying ONLY `unimatrix.db` therefore reads a PRE-CHECKPOINT snapshot — the freshly
  # observed rows live in `unimatrix.db-wal` and are invisible, yielding a FALSE empty
  # capture (the barrier said durable, the single-file copy defeated it). Copy the main
  # db AND its `-wal`/`-shm` sidecars so sqlite3 sees the post-barrier DURABLE view.
  local tmp_db="$SANDBOX/behavioral.db"
  if ! vol cat "$slug_db" > "$tmp_db" 2>/dev/null || [ ! -s "$tmp_db" ]; then
    fail "behavioral capture (D2): could not read per-slug store $slug_db from the volume (post-barrier) — INFRA"
  fi
  # Sidecars are best-effort: absent (already-checkpointed) is fine; present is required
  # for the durable view. A missing main db is the INFRA above; a missing WAL is not.
  vol cat "${slug_db}-wal" > "${tmp_db}-wal" 2>/dev/null || rm -f "${tmp_db}-wal"
  vol cat "${slug_db}-shm" > "${tmp_db}-shm" 2>/dev/null || rm -f "${tmp_db}-shm"
  command -v sqlite3 >/dev/null 2>&1 \
    || fail "behavioral capture (D2): sqlite3 not provisioned on the host — mis-provisioned lane (INFRA, never an empty-pass; provision sqlite3 like node)"
  local signals
  signals="$(sqlite3 -json "$tmp_db" \
    "SELECT DISTINCT topic_signal FROM observations WHERE topic_signal IS NOT NULL AND topic_signal != '' ORDER BY topic_signal;" 2>/dev/null \
    | node -e 'let b="";process.stdin.on("data",c=>b+=c).on("end",()=>{let rows=[];try{rows=JSON.parse(b||"[]")}catch{};process.stdout.write(JSON.stringify(rows.map(r=>r.topic_signal)))})' 2>/dev/null || true)"
  case "$signals" in
    '['*']') : ;;
    *) signals="" ;;
  esac
  if [ -z "$signals" ] || [ "$signals" = "[]" ]; then
    fail "behavioral capture (D2): no DISTINCT topic_signal rows after the barrier — empty capture is INFRA (R-09), never an empty-pass"
  fi
  printf '%s' "$signals" > "$out_file"
  log "behavioral (D2): captured DISTINCT topic_signals container-side after the barrier (derived). PASS"
}

# capture_precompact <manifest> <out_file>
#   D5: drive the PreCompact /observe frame and capture the server-restored payload.
#   Per ADR-006 / OQ-2 the harness cannot drive a live CC host, so PreCompact may be
#   "measured-where-drivable + documented host_side_gap" — emit
#   {restored_payload:{...}|null, measurable:bool, host_side_gap:str|null}. NEVER a
#   silent drop / vacuous pass: when not measurable, restored_payload is null AND
#   host_side_gap NAMES the gap (the ONLY legitimate null in the bundle). The K4
#   classifier honours measurable=False + a named gap as a documented exception,
#   never green-by-default.
capture_precompact() {
  local manifest="$1" out_file="$2"
  local observe_url="https://localhost:${PORT}/v1/${SLUG}/observe"
  local sid feat body code
  sid="$(node -e 'const m=require(process.argv[1]);process.stdout.write(String(m.session_id||""))' "$manifest" 2>/dev/null || true)"
  feat="$(node -e 'const m=require(process.argv[1]);process.stdout.write(String(m.feature_cycle||""))' "$manifest" 2>/dev/null || true)"
  [ -n "$sid" ] || fail "precompact capture (D5): manifest session_id missing — cannot drive PreCompact frame"
  # Fire the PreCompact RecordEvent over the pinned /observe route (same idiom as
  # _fire_observe_hooks — no new transport). The server's PreCompact handling is
  # the measurability question: the restored payload is host-side (CC) on a live
  # compaction. The harness drives the frame; whether the server returns a
  # restored payload it can read back determines measurability.
  body="$(node -e '
    const sid=process.argv[1], feat=process.argv[2];
    process.stdout.write(JSON.stringify({type:"RecordEvent",event_type:"PreCompact",session_id:sid,timestamp:0,payload:{feature_cycle:feat}}));
  ' "$sid" "$feat" 2>/dev/null)"
  code="$(curl -sS --cacert "$TMP/cert.pem" -o /dev/null -w '%{http_code}' \
    -X POST "$observe_url" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d "$body" 2>/dev/null || true)"
  # PreCompact restoration is a host-side (CC) component the harness cannot drive
  # (ADR-006 / OQ-2): the /observe frame lands (204) but the RESTORED payload is
  # produced by the live CC host on compaction, not by this harness. So we emit the
  # documented host-side gap — measurable=False, restored_payload=null, gap NAMED.
  # This is the human-signed documented exception for the flip session, stated
  # PLAINLY here, never rounded up to "fully measured" and never a silent drop.
  if [ "$code" != "204" ]; then
    fail "precompact capture (D5): pinned /observe PreCompact frame returned HTTP ${code:-none} (expected 204) — INFRA"
  fi
  printf '{"restored_payload":null,"measurable":false,"host_side_gap":%s}' \
    '"PreCompact restoration is produced by the live Claude-Code host on compaction; the harness drives the /observe frame but cannot drive a CC compaction to read back a restored payload (ADR-006/OQ-2). Documented host-side gap — not a vacuous pass."' \
    > "$out_file"
  log "precompact (D5): /observe PreCompact frame landed (204); restoration is a documented host-side gap (measurable=false, gap NAMED — never a vacuous pass). PASS"
}

# assemble_shell_captures <store_dir> <manifest> <out_file>
#   Compose the shell-owned fragment {topic_signals, precompact} the
#   bundle assembler reads. MUST run AFTER the durability barrier (R-04). When the
#   SMOKE_SHELL_CAPTURES stub is set (off-Docker logic test), use it verbatim.
assemble_shell_captures() {
  local store_dir="$1" manifest="$2" out_file="$3"
  if [ -n "${SMOKE_SHELL_CAPTURES:-}" ]; then
    [ -f "$SMOKE_SHELL_CAPTURES" ] \
      || fail "SMOKE_SHELL_CAPTURES=$SMOKE_SHELL_CAPTURES not found (stub seam)"
    cp "$SMOKE_SHELL_CAPTURES" "$out_file"
    log "stub seam: shell captures (topic_signals/precompact) supplied via SMOKE_SHELL_CAPTURES. PASS (stub)"
    return 0
  fi
  local b_out="$SANDBOX/cap.behavioral.json" p_out="$SANDBOX/cap.precompact.json"
  capture_behavioral_topic_signals "$store_dir" "$b_out"
  capture_precompact "$manifest" "$p_out"
  BEH="$b_out" PRE="$p_out" OUT="$out_file" node -e '
    const fs=require("fs");
    const topic_signals=JSON.parse(fs.readFileSync(process.env.BEH,"utf8"));
    const precompact=JSON.parse(fs.readFileSync(process.env.PRE,"utf8"));
    fs.writeFileSync(process.env.OUT, JSON.stringify({topic_signals, precompact})+"\n");
  ' || fail "could not compose the shell-owned capture fragment"
}

# emit_dimension_bundle <driver_fragment> <shell_captures> <run_token> <out_file>
#   Assemble the FIVE-key dimension_bundle from the C2' driver fragment (retrieval,
#   proactive, metric_vector, informs_edges, phase_signal) + the shell-owned
#   captures (topic_signals, precompact) and write
#   {run_token, dimension_bundle:{...}} to <out_file>. The never-empty guard runs
#   BEFORE the write: a missing/empty bridge-surface capture (retrieval / proactive
#   / metric_vector) or a missing/empty shell capture (behavioral) ->
#   exit 1, never an empty-key bundle (R-09 / R-03). Only precompact.restored_payload
#   may be null, and ONLY with measurable=false. The Python load_https_bundle (K5)
#   re-validates on ingest (the binding guard — contract-tested both sides).
emit_dimension_bundle() {
  local driver_fragment="$1" shell_captures="$2" run_token="$3" out_file="$4"
  RUN_TOKEN="$run_token" OUT="$out_file" node -e '
    const fs=require("fs");
    const drv = JSON.parse(fs.readFileSync(process.argv[1],"utf8"));   // C2 fragment
    const shell = JSON.parse(fs.readFileSync(process.argv[2],"utf8")); // {topic_signals, precompact}
    const isObj = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
    const nonEmptyArr = (v) => Array.isArray(v) && v.length > 0;
    // ---- bridge-surface never-empty guard (R-09/R-03): retrieval, proactive, analytics ----
    const retrieval = drv.retrieval;
    if (!isObj(retrieval) || !nonEmptyArr(retrieval.queries) || !("capture_2" in retrieval)) {
      process.stderr.write("retrieval capture missing/empty (queries[] required, capture_2 for intra) => INFRA, never empty-pass (R-09)\n");
      process.exit(1);
    }
    const proactive = drv.proactive;
    if (!isObj(proactive) || !("briefing_ids" in proactive) || !("capture_2" in proactive)) {
      process.stderr.write("proactive capture missing/empty (briefing_ids + capture_2 required) => INFRA (R-09)\n");
      process.exit(1);
    }
    const mv = drv.metric_vector;
    if (!isObj(mv) || Object.keys(mv).length === 0) {
      process.stderr.write("empty/short MetricVector — barrier released early? (R-06)\n");
      process.exit(1);
    }
    // ---- shell-surface never-empty guard: behavioral ----
    if (!nonEmptyArr(shell.topic_signals)) {
      process.stderr.write("behavioral topic_signals missing/empty => INFRA (R-09), never empty-pass\n");
      process.exit(1);
    }
    // ---- precompact: the ONLY null-eligible capture, and ONLY with measurable=false ----
    const pre = shell.precompact;
    if (!isObj(pre) || typeof pre.measurable !== "boolean") {
      process.stderr.write("precompact capture missing measurable flag => INFRA (R-09)\n");
      process.exit(1);
    }
    if (pre.restored_payload === null && pre.measurable !== false) {
      process.stderr.write("precompact restored_payload is null but measurable!=false — illegal null (R-08)\n");
      process.exit(1);
    }
    if (pre.measurable === false && (pre.host_side_gap === null || pre.host_side_gap === undefined || pre.host_side_gap === "")) {
      process.stderr.write("precompact measurable=false but host_side_gap not named — vacuous pass forbidden (R-08)\n");
      process.exit(1);
    }
    const bundle = {
      retrieval:  retrieval,
      behavioral: { topic_signals: shell.topic_signals },
      analytics:  { metric_vector: mv, informs_edges: drv.informs_edges || [], phase_signal: drv.phase_signal || {} },
      proactive:  proactive,
      precompact: pre,
    };
    fs.writeFileSync(process.env.OUT, JSON.stringify({run_token: process.env.RUN_TOKEN, dimension_bundle: bundle}) + "\n");
  ' "$driver_fragment" "$shell_captures" \
    || { _dump_bridge_err "${BRIDGE_ERR:-/dev/null}"; fail "failed to emit dimension bundle out-file (empty/short capture?)"; }
  [ -s "$out_file" ] || fail "dimension bundle out-file empty after emit"
}
