# Test Plan — Seam, Round-Trip & UDS-Stamp (GATE-BLOCKING)

Source: ACCEPTANCE-MAP "Cross-Feature / Seam Verification" (gate-blocking per ADR-007), ADR-007 §1, ADR-002 §7, ADR-003. Risks: R-07, R-01, R-23, R-19. These tests gate the feature; the seam-survival test runs **before any vnc-030 server-work validation**.

Files: `packages/unimatrix/test/hook-client/seam-survival.test.js` (NEW), `packages/unimatrix/test/hook-client/uds-stamp-regression.test.js` (NEW), cargo listener-layer round-trip integration (extend listener tests), JS subagent-canary fixtures (in `state-canary.test.js`, cross-referenced here).

---

## 1. Post-rebase interception-seam survival (R-07 / FR-28 / ADR-007 §1) — GATE 1

**Purpose**: The seam FR-01..03 hang on. vnc-027 (MERGED) narrowed the install matcher and introduced a null no-send sentinel. If a later vnc-027 follow-up drifts either anchor, `context_cycle` routes through the sentinel, the tracker is never written, and the whole mechanism is silently inert while cycles-module unit tests still pass. This test is the regression tripwire. **Anchors pinned to real `file:line`**: matcher `lib/merge-settings.js:49` (`PRETOOLUSE_CYCLE_MATCHER = "context_cycle|mcp__unimatrix__context_cycle"`); validation→frame `build-request-tools.js:314`; null sentinel `build-request-tools.js:326`; short-circuit `index.js:366`; transport selection `index.js:410`.

Drives the **rebased `index.js` pipeline** (spawn-level via `freshProject()`/stub-server, the index.test.js idiom), not the cycles module in isolation.

### test_seam_cycle_start_yields_stamped_frame_and_writes_tracker
- **Arrange**: fresh project (`.git` root); a PreToolUse hook input for `mcp__unimatrix__context_cycle` with valid `{type:"start", topic:"vnc-030"}`; stub server capturing the FNF body.
- **Act**: run the entry.
- **Assert**:
  - tracker file `cycles/{sanitizeSessionKey(sid)}.json` created (`cycles.writeCycle` reached) with `topic="vnc-030"`;
  - a CYCLE_START `RecordEvent` frame was sent (`request !== null`, NOT the `:326` null sentinel);
  - the frame's `ImplantEvent.cycle_stamp.topic === "vnc-030"` (decoration stamped it), NOT the sentinel;
  - exit 0.

### test_seam_noncycle_pretooluse_yields_sentinel_no_side_effects
- **Arrange**: fresh project; a PreToolUse hook input for a NON-`context_cycle` tool (e.g. `Bash`).
- **Act**: run the entry.
- **Assert**:
  - `buildCycleEventOrFallthrough` returned the null sentinel (`:326`), `index.js:366` returns exit 0;
  - NO tracker file touched (no create/read/write/delete under `cycles/`);
  - `state.bumpStampMiss` NOT called (no `stamp_miss` increment; `health.json` unchanged or absent);
  - NO network call (stub server saw zero requests).

### test_seam_cli_validation_gate_rejects_invalid_params_no_tracker (ties R-10)
- Invalid `validateCycleParams` (e.g. missing topic) → no CYCLE_* frame, no tracker file (FR-01 invalid-params-no-file).

**Coverage requirement**: both branch points pinned to real `file:line`; the test is rebase-order-sensitive and is the R-07 mitigation. Runs in the suite **before** server-work validation.

---

## 2. End-to-end round-trip at all 3 server read sites (#3486 / R-01 / FR-13) — GATE 3

**Purpose**: #3486 class — field present but not consumed at one of N construction/read sites. Three record sites: single `~listener.rs:719`, second single `~:861`, batch `~:1042`. A shared `apply_stamp_to_row` helper (ADR-003 mandate) collapses them; the AC must STILL assert each independently. Field-exists-on-struct is insufficient.

cargo listener-layer integration (extend `uds/listener.rs` integration tests); where the stamp is constructible through the public MCP surface, also assert via infra-001 `test_lifecycle.py`.

### test_stamp_read_single_site_a_records_declared (~:719)
- Arrange: a stamped `ImplantEvent` (`cycle_stamp{topic:"vnc-030", phase:"delivery"}`) through the first single-record path; empty/contradicting registry.
- Assert: row `topic_signal == "vnc-030"`, `topic_source == "declared"`, `phase == "delivery"`; `registry.apply_stamp` set `FeatureSource::Declared`; `record_topic_signal` tally NOT incremented; `enrich_topic_signal` skipped.

### test_stamp_read_single_site_b_records_declared (~:861)
- Same assertions through the second single site — asserted **independently**, not assumed from site A.

### test_stamp_read_batch_site_records_n_declared (~:1042)
- Arrange: a `RecordEvents` batch of N stamped events.
- Assert: **N** rows, each `topic_source == "declared"` — catches the batch-site-forgotten case specifically (R-06).

### test_unstamped_frame_takes_legacy_chain_all_sites (negative)
- An unstamped (Rust-hook-shaped) frame through all three sites → `cycle_stamp: None` → legacy chain (extraction/fill/vote), `topic_source` per the heuristic write path, NOT `declared`.

**Coverage requirement**: one assertion **per site** that the stamp was read AND applied; the shared helper is exercised by all three; batch yields N declared rows.

---

## 3. UDS-path stamp regression (AC-10 / FR-29 / R-23 / ADR-002 §7) — GATE 4

**Purpose**: Discharge vnc-027's post-merge obligation owed to #699. Decoration mutates the in-memory `request` upstream of `selectTransport` (`index.js:410`), so the stamp *should* be transport-agnostic — but a serialization divergence between `transport-uds.encodeFrame` (`transport-uds.js:55-62`) and the HTTP body could drop/alter `cycle_stamp` only over UDS, slipping past HTTP-only round-trip tests.

File: `uds-stamp-regression.test.js` (NEW). The **byte-compare** portion opens no socket → **unguarded** (Windows keeps coverage, per #4832). Any portion driving the live UDS daemon → `{ skip: IS_WINDOWS }`.

### test_uds_encodeframe_carries_cycle_stamp (offline, UNGUARDED)
- Arrange: tracker present; one stamped FNF `RecordEvent`; drive `index.js` `runFireAndForget` with `config.mode = "uds"`.
- Act: capture the bytes from `transport-uds.encodeFrame`.
- Assert: the decoded JSON payload contains `cycle_stamp{topic, phase?}` matching the tracker.

### test_uds_http_cycle_stamp_byte_equivalent (offline, UNGUARDED)
- Arrange: identical stamped input.
- Assert: the `cycle_stamp` payload from `transport-uds.encodeFrame` is **byte-equivalent** to the `transport-http.post` body's `cycle_stamp` (decoration strictly upstream of the `index.js:410` fork → both `JSON.stringify` the same object).

### test_uds_replayed_queue_frame_carries_stamp (offline, UNGUARDED)
- Arrange: a stamped frame enqueued post-decoration (ADR-002 §2.5), then replayed over UDS.
- Assert: the replayed frame carries the same stamp (the queue stores the decorated `request`).

### test_uds_live_roundtrip_stamp_recorded (live daemon, `{ skip: IS_WINDOWS }`)
- Extend `parity-layer2-uds.test.js`: a stamped event over the real UDS socket lands a `topic_source='declared'` row (folds the UDS seam into the 3-site round-trip end-to-end).

**Coverage requirement**: the stamp is proven over UDS, not just HTTP; seam pinned at `transport-uds.encodeFrame`; offline byte-compare unguarded, live-socket guarded.

---

## 4. Subagent-gated canary fixture set (AC-06 / R-19 / FR-09) — GATE 5

**Purpose**: the canary is a zero-tolerance invariant (`stamp_miss == 0`), not a rate signal. Full fixture detail in `state-canary.md`; the gate-blocking quartet is restated here because ACCEPTANCE-MAP lists it as seam verification.

Four binding fixtures (depth semantics simulated via the FNF decoration miss branch):
1. **depth-0 never-declare → no increment** — a top-level event with no tracker is structural noise; `bumpStampMiss` NOT called; `stamp_miss` stays 0.
2. **depth≥1 subagent, inherited root tracker present → no increment** — `cycles/{root}.json` exists; the subagent event finds it and stamps; no increment.
3. **depth≥1 subagent carrying a non-inherited id while root tracker exists → exactly one increment** — the inheritance-drift signal; `bumpStampMiss` called once; `stamp_miss == 1`.
4. **depth>1 grandchild id with no tracker → lands in `stamp_miss`, not silent loss** — forward-compat (ADR-006 §5); asserts silent loss is impossible.

Plus the healthy end-to-end: **single declared session with a depth-1 subagent → `stamp_miss == 0`** (ships either OQ-E branch).

**Coverage requirement**: positive + negative + forward-compat; no 0.20 threshold, no `fnf_record_send_count` denominator, no `anyOtherCycleFile` rule, no baseline. Test-module doc comment pins **claude 2.1.167** and states the OQ-E Branch-A/B disposition (test-time invariant ships either way).

---

## Gate ordering (Stage 3c)

1. infra-001 `-m smoke` (mandatory minimum).
2. **Seam-survival (§1)** — before any server-work validation.
3. Wire serde + 3-site round-trip (§2) + UDS stamp (§3).
4. Canary set (§4).
5. Remainder of component suites.
