# C2 — Bridge-Driven Cycle (shell→node) — Test Plan

> **Component:** spawn `mcp-bridge.js <projectHash>`, drive `context_cycle(start) → Bash + tool calls →
> context_cycle(stop)` over stdio JSON-RPC; fire live hooks → pinned `POST /v1/{slug}/observe`; apply the
> durability barrier; HTTPS-side `context_cycle_review` → emit `MetricVector(HTTPS)` to a `$SANDBOX` file
> with a run-correlation token. **NEW `cloud_cycle_gates` gate fn** in the smoke; reuses
> `mcp-bridge.js`/`cert-pin.js`/`credstore.js`/`bundle.js`/`init.js` AS-IS (no new transport code).
> **ACs:** AC-02 (primary), AC-03 (derived attribution over the wire). **Risks:** R-04, R-05, R-07, R-09, R-13.

---

## Test Expectations

### AC-02 / FR-9 — the bridge CARRIED the traffic (R-04, Critical)

This is the headline coverage SR-02 flagged as un-smoked. A 200/204 is NOT sufficient.

- **`test_c2_bridge_process_spawned`**: assert `node mcp-bridge.js <projectHash>` is actually spawned (the
  process exists) and the cycle MCP traffic is driven over ITS stdio JSON-RPC.
- **`test_c2_no_direct_mcp_url_post`**: assert ZERO direct `mcp_url` POSTs are issued by the fixture for
  cycle tool calls — the bridge must not be bypassed (D-2). NOT UDS, NOT stdio-direct, NOT a direct
  `mcp_url` POST.
- **`test_c2_session_id_captured_and_replayed`**: assert `Mcp-Session-Id` was captured on `initialize` and
  replayed **byte-stable** on a later call (observable in bridge stderr/log).
- **`test_c2_sse_parsed_not_just_200`**: assert a `text/event-stream` response was PARSED (rmcp forces SSE,
  #5129) — not merely a 200. The MCP path sends `Accept: application/json, text/event-stream`.
- **`test_c2_json_only_accept_fails_framing`** (NEGATIVE CONTROL): a JSON-only `Accept` (no
  `text/event-stream`) MUST FAIL the framing — proves the fixture exercises REAL SSE, not a JSON shortcut.
- **`test_c2_pinned_flush_after_fingerprint_match`** (trust boundary, #4970): the bearer flushes ONLY after
  `verifyPeerFingerprint` matches the leaf DER fingerprint (`sha256:<hex>`) — a real pinned HTTPS handshake,
  not a "pin was configured" shape assertion. An unpinned / plain-HTTP `/observe` is forbidden (NFR-5).

### AC-02 / NFR-7 — idle-window minimization + shipped self-heal (R-05, Critical)

- **`test_c2_drive_immediately_no_interposed_wait`**: assert the first tool call follows session-id capture
  (readiness gate 8, event-driven) with NO interposed fixed wait — the captured session is not left idle
  long enough for rmcp `keep_alive` to evict it.
- **`test_c2_eviction_survived_by_shipped_self_heal`**: assert a mid-cycle eviction is survived by the
  SHIPPED single-flight self-heal (single-flight re-init on `SESSION_NOT_FOUND` -32099, #830/#5280). The
  fixture must NOT contain its own retry/reconnect logic (re-authoring shipped behavior = NFR-2 violation).
- **`test_c2_heal_exhausting_404_hard_fails_with_stderr`** (failure mode): a 404 that exhausts the self-heal
  surfaces as a HARD cycle failure with captured bridge stderr — never a silent dropped observe that shows
  up only as a short `MetricVector`. (This fixture doubles as the #830 self-heal regression guard — a flake
  HERE correctly SIGNALS a #830 regression; that coupling is intended, not a bug.)

### AC-03 — derived `topic_signal == feature`, no seed (R-07/R-09, over the wire)

The cycle's observations cross the REAL wire through the real chain — this is the derivation under test.

- **`test_c2_bash_carries_feature_id_token`**: the cycle issues a real `Bash` tool call whose observed
  content carries an explicit, load-bearing feature-ID token parseable by `extract_topic_signal` (FR-3) —
  assert the derivation has REAL input, not an accidental match.
- **`test_c2_topic_signal_equals_feature_exact`**: assert `topic_signal == feature` EXACTLY (string equal)
  for every driven HTTPS observation. A derived `unattributed` near-miss is a HARD fail (R-07 guard).
- **`test_c2_feature_is_valid_registry_feature`**: the workload `feature_cycle` is a VALID registry feature
  so `enrich_topic_signal_with_source` resolves the `declared` branch (not registry-fill/vote/unattributed);
  assert the slug/feature is registered before driving.

### R-09 — stable session identity on the HTTPS leg (#832 class)

- **`test_c2_single_stable_session_identity`**: assert ONE stable CC session identity is threaded through
  the cycle-declaration hook spawn AND all per-tool observe spawns on the HTTPS leg — the cycle-join holds
  (the #832 root cause was divergent session ids). The value comes from the C4 manifest (single driver),
  and is the SAME value the UDS leg (C3) uses.

### Durability barrier on the HTTPS leg (R-06 — shared helper owned by C4)

- **`test_c2_https_barrier_before_review`**: assert the C4 symmetric durability-barrier helper runs on the
  HTTPS leg AFTER `context_cycle(stop)` and BEFORE `context_cycle_review` — the SAME helper/predicate/
  deadline as the UDS leg (asymmetry forbidden). Predicate detail is tested in C4; C2 asserts it is INVOKED
  on this leg before the HTTPS review.

### R-13 — capture-first child stderr (the bridge is a token-free child)

- **`test_c2_bridge_init_container_capture_stderr`**: `mcp-bridge.js`, `init`, the container each write
  stderr to a `$SANDBOX` file, tail-dumped on failure only — never `2>/dev/null`.

---

## Output contract (the C2→C4 seam, R-03)

- **`test_c2_emits_https_vector_with_correlation_token`**: C2 emits `MetricVector(HTTPS)` (the JSON text of
  the HTTPS `context_cycle_review`) to a FRESH `$SANDBOX` file carrying the run-correlation token (the
  workload's stable session identity / run id). C4's comparator rejects any vector whose token ≠ this run
  (stale-file guard).

## Edge cases

- Mid-cycle session eviction (R-05) — covered above.
- `unattributed` near-miss (R-07) — hard fail, not a pass.
- Bearer logged via captured stderr — forbidden; `emit_bundle` is the only suppressed child (asserted in C1).

## Integration boundary

C2 consumes C1's live endpoint + credstore + read-back `projectHash`; it produces the
`MetricVector(HTTPS)` `$SANDBOX` file (token-correlated) that C4 ingests in the same pytest invocation.
