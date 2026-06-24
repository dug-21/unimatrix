# nan-021 Agent 4 — C2 Bridge-Driven Cycle (Stage 3b) Report

**Agent:** nan-021-agent-4-c2-bridge-cycle
**Component:** C2 — bridge-driven cycle (shell→node), Gate 8 `cloud_cycle_gates`
**Status:** COMPLETE — foreground-exercised GREEN end-to-end against the shipped image (exit 0)

## Files created / modified
- `product/test/infra-001/scripts/docker-http-posture-smoke.sh` (MODIFIED, +19 lines, purely additive): sources `cloud-cycle-lib.sh` (before the sourced-guard so the gate fn is defined whether executed or sourced — C5 stub-drive parity), and invokes `cloud_cycle_gates` after Gate 7 ONLY when the orchestrator wired `MANIFEST_PATH`/`RUN_TOKEN`/`HTTPS_VECTOR_OUT` (the standalone #783 smoke skips Gate 8 — contract unchanged). 494 lines.
- `product/test/infra-001/scripts/cloud-cycle-lib.sh` (NEW, 329 lines): the C2 gate, factored out to keep each file ≤500 lines (mirrors `release-gate-lib.sh`). Contains `cloud_cycle_gates`, the symmetric durability barrier, the bridge-carried-traffic + negative-control assertions, the C3-canonical `/observe` cycle drive, and capture-first stderr dump.
- `product/test/infra-001/scripts/bridge-cycle-driver.js` (NEW, 285 lines): spawns the REAL shipped `node mcp-bridge.js <projectHash>` (witness preloaded via NODE_OPTIONS), speaks newline-delimited JSON-RPC to its stdio (D-2 bridge-in-path), drives `context_cycle(start/stop)` for the AC-02 witness and `context_cycle_review(format=json,force)` for the MetricVector. No transport/cert/credstore/reconnect code (NFR-2).
- `product/test/infra-001/scripts/bridge-witness.js` (NEW, 81 lines): https.request preload that emits `BRIDGE_WITNESS:<json>` wire observations (Accept, sent/recv Mcp-Session-Id, status, content-type) without altering behavior or reading Authorization (NFR-06).

## Tests / exercise (foreground)
**Full live smoke incl. Gate 8 against `unimatrix:783-smoke` — exit 0, ALL GATES PASSED:**
- Gate 8a: cycle driven THROUGH the real bridge over stdio JSON-RPC — PASS
- Gate 8 hooks: 9 RecordEvents (SessionRegister + cycle_start + 3×(Pre+Post) + cycle_stop), 3 PostToolUse, stable session_id, pinned — PASS
- **FR-9/AC-02 bridge-carried-traffic: SSE Accept sent, text/event-stream PARSED, Mcp-Session-Id `c4588b66-…` REPLAYED byte-stable — PASS** (real wire, not a 200)
- Negative control (R-04): JSON-only Accept yields no parseable JSON MCP reply → real SSE required — PASS
- Symmetric durability barrier (ADR-006): released on store-size stabilization — PASS
- Gate 8c: `context_cycle_review` over the bridge → MetricVector with all 21 universal fields (`total_tool_calls:3`, `session_count:1`) — PASS
- Out-file: MetricVector(HTTPS)+RUN_TOKEN emitted; **C4 `load_https_vector` ingests it as a dict** (keys: computed_at/universal/phases/domain_metrics); stale-token guard fires on a wrong token — PASS

**Assertion teeth verified:** a witness log with no SSE response, or with session-id never replayed, correctly FAILS (no false-green).
**C5 stub seam (`SMOKE_CYCLE_CMD`) off-Docker:** control flow + barrier + out-file emit GREEN.
**No-seed static guard** (C4 `assert_no_seed_reachable`) PASS on all three new files. **Zero production-code diff** (only `product/test/infra-001/**` touched).

## Issues / findings (triaged)
Two pseudocode-phase assumptions were FALSIFIED by the live run and corrected in-fixture (not server bugs):
1. **MCP `context_cycle(start/stop)` does NOT feed the review.** The review's primary `cycle_events-first` (col-024) path reads rows written by the HOOK wire `cycle_start` RecordEvent, not the MCP cycle tool. The cycle is therefore driven as the C3-canonical `/observe` RecordEvent sequence (SessionRegister → cycle_start → Pre/Post → cycle_stop) — IDENTICAL to C3's UDS leg (hook_client.py record_* methods) so the legs are parity-by-construction. The bridge still carries `context_cycle(start/stop)` + the review (AC-02 witness holds on those real MCP calls).
2. **`Read`/`Bash`/`Grep` are Claude-Code host tools, not server MCP tools** (`tools/call "Read"` → -32602 tool not found). The manifest's tool_calls manifest ONLY as the hook RecordEvent sequence, never as MCP calls.

Both recorded as pattern #5295. No GH issue filed — these are fixture-design corrections, not product defects. The shipped bridge, cert-pin, credstore, self-heal, and attribution chain were exercised as-is and behaved correctly.

**One out-of-scope pre-existing nit (NOT mine, flagged not fixed):** `docker-http-posture-smoke.sh:210` `rm -rf "$SANDBOX/home"` triggers shellcheck SC2115 — in C1's `bundle_attach_gates`, predates nan-021.

## SR-05 cross-leg reconciliation (Wave-2 coordinator request)
C3 published `harness/parity_legs.py` (drive_uds_leg + PARITY_PHASE) as the shared driving contract. Read it directly (source of truth) and conformed C2's `_fire_observe_hooks` to emit the BYTE-IDENTICAL 11-frame sequence:
1. SessionRegister(agent_role="tester", feature) → 2. cycle_start(next_phase="delivery") → 3. **PreToolUse TaskCreate, subject="delivery: drive the parity workload"** (was MISSING — the phase-setter + total_tool_calls contributor) → 4. per call: PreToolUse(tool_input=args) + PostToolUse(response_size, response_snippet, tool_input=args) → 5. cycle_stop → 6. **SessionClose(outcome="completed", duration_secs=1)** (was MISSING).
Switched from the shipped build-request.js rework frame to C3's exact `event_type:"PostToolUse"` plain frame so the legs match byte-for-byte.

**Re-exercised live against `unimatrix:783-smoke`, exit 0 — HTTPS-leg MetricVector now MATCHES C3's reference:**
- `total_tool_calls == 4` ✓ (TaskCreate + Read + Bash + Grep PreToolUse)
- `phases == {"delivery": {tool_call_count: 4}}` ✓
- bridge-carried-traffic (SSE + Mcp-Session-Id replay), negative control, durability barrier, no-seed guard, C4 ingest, C5 stub seam — all still PASS.
- AC-03 derived attribution re-verified with C3's PostToolUse frame: `observations.topic_signal` (non-null) == `{nan-021}`, derived over the wire, no seed.

**No remaining divergence.** I did NOT modify any C3 file; C2 conforms to C3.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced ADR-002 (#5294), ADR-006 (#5291), ADR-005 (#5290), bridge pattern #5129/#5115; all applied.
- Stored: entry #5295 → corrected to #5298 "Byte-identical cross-leg RecordEvent sequence (SR-05) for cycle_review parity" (pattern); entry #5296 "Wire-witness pattern for SSE + Mcp-Session-Id replay" (pattern).
