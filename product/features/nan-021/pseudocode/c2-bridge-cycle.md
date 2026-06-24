# C2 — Bridge-driven cycle (shell→node)

**Extends:** a NEW gate function `cloud_cycle_gates()` appended to
`scripts/docker-http-posture-smoke.sh` AFTER Gate 7 / before the terminal marker. **Reuses as-is**
(no new transport/cert/credstore code): `lib/hook-client/mcp-bridge.js`, `cert-pin.js`, `credstore.js`,
`bundle.js`, `lib/init.js`. Any net-new spawn/cert/credstore/bundle path here is a FORK SMELL (R-10).
This is the HTTPS leg of the pytest-orchestrator seam (ADR-001): pytest `subprocess.run`s the smoke,
which runs C1 then C2, then writes `MetricVector(HTTPS)` to a sandbox out-file.

## Purpose

Spawn `node mcp-bridge.js <projectHash>` as the LOCAL MCP server, drive
`context_cycle(start) → Bash + tool calls → context_cycle(stop)` over **stdio JSON-RPC to the bridge**
(NOT a direct `mcp_url` POST), fire live hooks → pinned `POST /v1/{slug}/observe`, run the symmetric
durability barrier, then `context_cycle_review(feature)` over HTTPS → emit `MetricVector(HTTPS)` to a
`$SANDBOX` file carrying the run-correlation token. Satisfies FR-2/FR-3/FR-9/FR-10/NFR-7, AC-02/AC-04.

## Inputs (passed by the pytest orchestrator via env/argv)

- `MANIFEST_PATH` — path to the JSON-serialized WORKLOAD manifest (C4 single source of truth).
- `RUN_TOKEN` — fresh per-run correlation token (= manifest `session_id` or a run id) to stamp output.
- `HTTPS_VECTOR_OUT` — fresh `$SANDBOX` path for `MetricVector(HTTPS)` (asserted absent at start by pytest).
- (from C1) `projectHash`, `$SLUG`, `$TMP/cert.pem`, `$TMP/token`, store DIR, port `18443`.

## Function: cloud_cycle_gates()

```
cloud_cycle_gates():
    # ---- 1. read projectHash BACK (OQ1/R-11 — never recompute, no hashing primitive) ----
    projectHash = basename(single dir under "$SANDBOX/home/.unimatrix/")
    if (zero or >1 dirs) -> fail "projectHash read-back ambiguous"   # init.js contract changed → loud
    assert credstore file "$SANDBOX/home/.unimatrix/$projectHash/remote.json" present, mode 0600

    # ---- 2. spawn the bridge LAST, drive IMMEDIATELY (NFR-7 idle-window minimization, R-05) ----
    BRIDGE_ERR = "$SANDBOX/bridge.stderr"            # capture-first (ADR-005/#5266)
    spawn:  HOME="$SANDBOX/home" node <lib>/mcp-bridge.js "$projectHash"   2> "$BRIDGE_ERR"
            (stdin/stdout are the stdio JSON-RPC channel we drive)
    # READINESS GATE 8: send `initialize`; block until the bridge's `initialize` reply is observed
    #   AND Mcp-Session-Id is captured (visible in BRIDGE_ERR/log). Event-driven, NOT a sleep.
    capture Mcp-Session-Id from the initialize round-trip
    # >>> NO interposed fixed wait between session-id capture and the first tool call <<<

    # ---- 3. drive the cycle THROUGH the bridge over stdio JSON-RPC (D-2, FR-2) ----
    load WORKLOAD from MANIFEST_PATH
    send context_cycle(start) via bridge stdio   (cycle_type=start, topic=feature_cycle, session=session_id)
    for call in WORKLOAD.tool_calls (in order):
        if call is the load-bearing Bash call:
            issue a REAL Bash tool call whose observed content carries the feature-ID token (FR-3)
        else: issue call.name with call.args via the bridge
        if call.observe:
            fire the live PostToolUse hook → pinned POST https://localhost:18443/v1/$SLUG/observe
                using --cacert "$TMP/cert.pem" and Bearer "$TOKEN"
                with the SAME stable session_id (FR-4/R-09)
    send context_cycle(stop) via bridge stdio

    # ---- 4. self-heal reliance (R-05/NFR-7) — DO NOT re-implement reconnection ----
    # A mid-cycle SESSION_NOT_FOUND (-32099) is handled by the SHIPPED single-flight keep_alive
    # self-heal (#830/#5280). The fixture adds NO retry/reconnect logic. If self-heal EXHAUSTS:
    #   -> hard fail with BRIDGE_ERR tail dumped; NEVER a silently dropped observe / short vector.

    # ---- 5. SYMMETRIC durability barrier (FR-10/ADR-006) BEFORE review ----
    durability_barrier(leg="HTTPS",
                       expected = WORKLOAD.expected_observe_count,
                       store_dir = per-slug store DIR)   # shared predicate, ~10s cap, sleep 1, DIR incl -wal
    # on timeout: fail "observes not durable: observed=<n> expected=<m>" (+ BRIDGE_ERR tail) — never empty compare

    # ---- 6. cycle_review over HTTPS → MetricVector(HTTPS) ----
    resp = context_cycle_review(feature_cycle) driven THROUGH the bridge over stdio JSON-RPC
    mv_https_json = JSON text of resp (the MetricVector inside RetrospectiveReport.metrics)

    # ---- 7. emit MetricVector(HTTPS) + RUN_TOKEN to the sandbox out-file (R-03 correlation) ----
    write {"run_token": RUN_TOKEN, "metric_vector": mv_https_json} to "$HTTPS_VECTOR_OUT"
    # bridge framing assertions captured for FR-9 (see below) are emitted to the smoke log / out-file
```

NOTE: `durability_barrier` is the C4-owned helper. Because it must be IDENTICAL on both legs and the
predicate (DIR-size / observe-count poll) is pure data, the shell leg calls the SAME logic — either by
shelling a tiny `python -m harness.parity_workload barrier ...` entrypoint over the manifest, or by the
smoke implementing the DIR-poll with the SAME bound/predicate constants exported by C4. Pseudocode-phase
gap flagged: Stage 3b must pick ONE so the predicate is single-sourced, not hand-duplicated (SR-05).

## Bridge-carried-the-traffic assertion (FR-9 / AC-02 / R-04)

The gate must PROVE the bridge carried it (not just a 200/204):
```
assert mcp-bridge.js process was spawned (it is the MCP server we drove over stdio)
assert Mcp-Session-Id was captured on `initialize` AND replayed byte-stable on a later call (from BRIDGE_ERR/log)
assert an SSE (text/event-stream) response was parsed (rmcp forces SSE, #5129) — Accept sent
       "application/json, text/event-stream"; a JSON-only Accept is a NEGATIVE control that must FAIL framing
assert ZERO direct cycle-`mcp_url` POSTs were issued by the fixture (bridge not bypassed, D-2)
```

## Stub seam (R-12 — pre-merge stub-drive)

Mirror the existing `SMOKE_*_CMD` pattern: gate behind the sourceable guard; optionally an
`SMOKE_CYCLE_CMD` seam so C5's logic-test can drive `cloud_cycle_gates`'s control flow (marker emit,
exit codes, out-file write) with a synthetic vector BEFORE the live tag run.

## Error handling

- projectHash read-back ambiguous → fail loud (init.js contract drift surfaced).
- bridge spawn/init failure → tail-dump `BRIDGE_ERR`, hard fail (R-13).
- self-heal exhausts (404) → hard fail with bridge stderr — treated as a FIXTURE defect, not a product bug.
- barrier timeout → hard fail "observes not durable" with observed-vs-expected — never empty compare (R-06).
- all token-free children capture stderr to `$SANDBOX`; `emit_bundle` (C1) stays the suppressed exception.

## Key test scenarios (hints for tester)

- AC-02: bridge spawned + driven over stdio; session-id replay + SSE parse asserted; zero direct
  `mcp_url` POST; JSON-only-Accept negative control FAILS framing (R-04).
- NFR-7/R-05: first tool call follows session-id capture with NO interposed fixed wait; a mid-cycle
  eviction is survived by the shipped self-heal OR hard-fails with captured cause (no silent truncation).
- FR-3: the Bash call's observed content carries the parseable feature-ID token (real derivation input).
- R-11: `projectHash` read back from the credstore dir; NO hashing primitive invoked in the gate.
- FR-10: barrier runs before review with the SAME bound/predicate as the UDS leg.
