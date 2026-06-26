# C5' — HTTPS smoke gate (`scripts/cloud-cycle-lib.sh`, `cloud_cycle_gates`)

**Extended in place**, cumulative. ADR-005. Writes a dimension-keyed bundle
`{run_token, dimension_bundle:{...}}` to `$HTTPS_VECTOR_OUT` instead of `{run_token,
metric_vector}`; rides the existing `run_smoke_gate` discriminator. NO new spawn/cert/credstore/
bundle/transport path (R-16 fork-smell guard — append-only `fail()`/exit-1 contract).

## Purpose

The nan-021 gate drives the cycle through the bridge, fires /observe hooks, runs the barrier,
reviews, and emits the MetricVector. nan-022 widens the emitted out-file to the full dimension
bundle: it assembles (a) the MCP-bridge-surface captures from C2' (retrieval, proactive,
analytics MetricVector) and (b) the /observe-surface captures it owns (behavioral topic_signals,
precompact restored payload, isolation probe) into ONE `dimension_bundle`.

## Consumed verbatim

`cycle_observe_size`, `cycle_durability_barrier`, `assert_bridge_carried_traffic`,
`assert_json_only_accept_fails_framing`, the projectHash read-back, the C1 standup (Gates 1-7),
`_fire_observe_hooks` (the #5298 11-frame sequence — the SHARED driving contract), the stub seam
(`SMOKE_CYCLE_CMD`/`SMOKE_REVIEW_VECTOR`), `_dump_bridge_err`, `PARITY_PHASE`.

## Change 1 — capture the /observe-surface dimensions the shell owns

After `_fire_observe_hooks` + the barrier (existing), add captures for the dimensions whose wire
surface is `/observe` or whose DB read happens container-side:

```
# BEHAVIORAL (D2): read DISTINCT topic_signal from the per-slug observations table, container-
#   side, AFTER the barrier (R-04). Use the busybox `vol` sidecar / a pinned read path that
#   matches how Gate 7 reads the volume — NOT a host-path read (the store is inside the volume).
#   Emit topic_signals as a JSON array. DERIVED, never seeded.

# ISOLATION (D6): extend the posture smoke's per-slug Gates 1-4 framing — write to slug A,
#   attempt a read from slug B, check on-disk landing. Emit booleans:
#     slug_a_writes_visible_to_b, landed_only_in_a.

# PRECOMPACT (D5): drive the PreCompact /observe frame; capture the server-restored payload.
#   Determine measurability (OQ-B): emit {restored_payload:{...}|null, measurable:bool,
#   host_side_gap:str|null}. NEVER a silent drop / vacuous pass — name the gap (R-08).
```

These ride the pinned `/observe` route (D5/behavioral) and the existing volume-read posture
(D2/D6) — NO new transport/cert code.

## Change 2 — assemble + emit the dimension bundle (replaces the metric_vector emit)

The final emit step (existing "gate 8 — emit MetricVector(HTTPS)+RUN_TOKEN") is generalized to
assemble the full bundle from the C2' driver fragment + the shell-owned captures:

```
# The driver ($REVIEW_OUT / $DRIVE_OUT) carries: metric_vector, retrieval, proactive,
#   informs_edges, phase_signal (the MCP-bridge-surface fragment).
# The shell carries: topic_signals (D2), isolation booleans (D6), precompact payload (D5).
# Compose ONE dimension_bundle and write {run_token, dimension_bundle:{...}}:

RUN_TOKEN="$RUN_TOKEN" OUT="$HTTPS_VECTOR_OUT" node -e '
  const fs=require("fs");
  const drv = JSON.parse(fs.readFileSync(process.argv[1],"utf8"));   // C2 fragment
  const shell = JSON.parse(fs.readFileSync(process.argv[2],"utf8")); // {topic_signals, isolation, precompact}
  const bundle = {
    retrieval:  drv.retrieval,
    behavioral: { topic_signals: shell.topic_signals },
    analytics:  { metric_vector: drv.metric_vector, informs_edges: drv.informs_edges||[], phase_signal: drv.phase_signal||{} },
    proactive:  drv.proactive,
    precompact: shell.precompact,        // {restored_payload, measurable, host_side_gap}
    isolation:  shell.isolation,         // {slug_a_writes_visible_to_b, landed_only_in_a}
  };
  // never-empty guard: every key present + non-empty (except precompact null w/ measurable=false).
  // a missing/empty bridge-surface capture (metric_vector/retrieval/proactive) => exit 1 (R-09).
  fs.writeFileSync(process.env.OUT, JSON.stringify({run_token:process.env.RUN_TOKEN, dimension_bundle:bundle})+"\n");
' "$REVIEW_OUT" "$SHELL_CAPTURES" || { _dump_bridge_err "$BRIDGE_ERR"; fail "failed to emit dimension bundle out-file (empty/short capture?)"; }
[ -s "$HTTPS_VECTOR_OUT" ] || fail "dimension bundle out-file empty after emit"
```

The Python `load_https_bundle` (K5) re-validates the bundle on ingest (required keys, token,
null rules) — the shell's never-empty check is the first line; the Python guard is the binding
one (R-09 contract-tested both sides).

## Change 3 — extend the bridge-carried-traffic assertion to the new calls

`assert_bridge_carried_traffic` (verbatim) already proves SSE + Mcp-Session-Id replay on the
cycle calls. The new retrieval/briefing `tools/call`s ride the SAME bridge session, so the
existing assertion covers them; no new witness logic is required, but the assertion's
`sent_session_id` replay must hold across the ADDED calls too (the integration check — the
bridge actually carried the new calls, not a direct POST).

## Stub seam (R-12 / off-Docker logic test)

The `SMOKE_CYCLE_CMD` stub path is extended so C5's logic-test can synthesize the FULL bundle
(not just a MetricVector) and exercise the emit/never-empty/out-file control flow without Docker:
`SMOKE_REVIEW_VECTOR` is generalized (or a new `SMOKE_BUNDLE` env) to carry a contract-shaped
`dimension_bundle`. The stub proves the bundle assembly + never-empty guard pre-tag (#5258).

## Exit-code / false-green discipline (C-7 / FR-18)

- Skip-when-Docker-absent HARD-fails by the distinct exit code (verbatim `run_smoke_gate` truth
  table: 0 pass / 3 skip->HARD-FAIL / 4 unacq / 1 broke). The bundle emit failures fold into the
  existing `fail()`/exit-1 contract (append-only — nan-019 ADR-001).
- An anchored run-marker tied to `$RUN_TOKEN` is asserted present (proving this-run traffic) —
  the bundle's `run_token` IS that marker (R-12).

## Error handling

- Every new capture failure folds into the EXISTING `fail()` (exit 1) — append-only contract.
- A missing/empty bridge-surface or shell-surface capture -> exit 1 (never an empty bundle
  written). The Python ingest re-checks (defense-in-depth).
- The barrier timeout / non-growing store remain HARD fails (verbatim).

## Key test scenarios (hints)

- The emitted out-file is `{run_token, dimension_bundle:{...}}` with EVERY capture_key present
  (R-09 sc.4 live; stub-seam off-Docker).
- A missing/empty non-D5 capture -> exit 1, never an empty-key bundle (R-09 / R-03).
- Behavioral topic_signals read AFTER the barrier, container-side, derived (R-04 live).
- Isolation booleans emitted; precompact carries measurable/host_side_gap (R-08).
- The new retrieval/briefing calls rode the bridge (SSE + session-id replay still asserted).
- No new spawn/cert/transport path (R-16 review-flag; diff additive within the existing functions).
- Stub seam synthesizes a full bundle and exercises emit + never-empty pre-tag (#5258 / R-10).
