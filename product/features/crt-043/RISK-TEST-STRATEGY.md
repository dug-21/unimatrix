# Risk-Based Test Strategy: crt-043

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | INSERT/UPDATE race: embed UPDATE executes before INSERT commits → silent NULL goal_embedding | High | Med | Critical |
| R-02 | bincode config divergence: encode uses `standard()`, decode uses a different config → silent garbage floats | High | Low | High |
| R-03 | Missing write site: one of the four observation write sites omits the phase capture or bind | High | Med | High |
| R-04 | Phase captured inside spawn_blocking closure instead of before it → race with set_current_phase | High | Low | High |
| R-05 | v20→v21 migration partial application: one column added, schema_version bumped before second ALTER | Med | Low | High |
| R-06 | Migration idempotency broken: re-running on a v21 database alters or errors on already-present columns | Med | Low | Med |
| R-07 | embed task blocks tokio executor: embedding computation not routed through ml_inference_pool | High | Low | High |
| R-08 | cycle_events row absent at UPDATE time (residual race): goal_embedding permanently NULL with no log | Med | Med | Med |
| R-09 | Goal embedding spawned on empty/absent goal: no-op spawn wastes resources; no-warn on true empty | Low | Low | Low |
| R-10 | Embed service unavailable path: warn not emitted, or cycle start blocked awaiting embed | High | Low | High |
| R-11 | decode_goal_embedding missing or mismatched: Group 6/7 read sites have no decode API to call | Med | High | High |
| R-12 | context_cycle MCP response text changed: crt-043 accidentally mutates the response string | Low | Low | Low |
| R-13 | (topic_signal, phase) composite index deferred beyond delivery: Group 6 queries full-scan observations | Med | Med | Med |

---

## Risk-to-Scenario Mapping

### R-01: INSERT/UPDATE Race — Silent NULL goal_embedding
**Severity**: High  
**Likelihood**: Med  
**Impact**: The cycle_events start row permanently has NULL goal_embedding despite a valid goal being supplied. H1 goal-clustering silently has no data for that cycle. No error is surfaced to the caller.

**Test Scenarios**:
1. Integration test: call `handle_cycle_event(CycleLifecycle::Start, goal="test goal")` with an operational embed service. Await both spawned tasks (using JoinHandle or a small sleep in test). Assert `SELECT goal_embedding FROM cycle_events WHERE topic = ? AND event_type = 'cycle_start'` returns a non-NULL blob.
2. Stress test: fire 20 concurrent CycleStart handle_cycle_event calls each with a distinct cycle_id and non-empty goal. After all tasks settle, assert that all 20 goal_embedding columns are non-NULL — verifying the ordering holds under concurrent load.
3. Code-review assertion: confirm that the embed spawn is registered in `tokio::spawn` strictly after the INSERT spawn within `handle_cycle_event`, with no conditional path that could reorder them.

**Coverage Requirement**: At least one integration test must await task completion and verify non-NULL blob in the DB. The concurrent scenario (scenario 2) is recommended but may be marked slow-test. The residual race (R-08) is a separate, acknowledged degradation path.

---

### R-02: bincode Config Divergence — Silent Garbage Floats
**Severity**: High  
**Likelihood**: Low  
**Impact**: A read site using a mismatched bincode config (e.g., `legacy()` vs `standard()`) deserializes the blob into a wrong-length or corrupt Vec<f32> with no decode error. H1 cosine similarity produces nonsense. This failure is invisible until Group 6 produces wrong results.

**Test Scenarios**:
1. Round-trip unit test: `encode_goal_embedding(vec.clone())` → bytes, then `decode_goal_embedding(&bytes)` → decoded. Assert `decoded == vec` (exact float equality, since no lossy transform). This test must live in `unimatrix-store` alongside the helpers.
2. Negative test: pass a raw byte slice of wrong length to `decode_goal_embedding`. Assert a `DecodeError` is returned (not a panic or silent truncation).
3. Cross-call test: encode via the public helper, decode by calling `bincode::serde::decode_from_slice` with `config::standard()` directly. Assert they produce identical results — verifying the helper is a thin wrapper, not an abstraction with a different config.

**Coverage Requirement**: Round-trip test is mandatory (AC-14). Negative/malformed-blob test is required to validate decode error surface.

---

### R-03: Missing Write Site — Phase Not Captured at One of Four Sites
**Severity**: High  
**Likelihood**: Med  
**Impact**: Observations from that event type always have NULL phase even when a phase is active, corrupting H3 phase-stratification for the affected event type. Silent data loss — no error, no warning.

**Test Scenarios**:
1. Per-site unit test for RecordEvent path: insert an observation with an active phase on the session; read back the row; assert phase matches.
2. Per-site unit test for rework-candidate path: same pattern.
3. Per-site unit test for RecordEvents batch path: call `insert_observations_batch` with multiple events under an active phase; read back all rows; assert all have non-NULL phase.
4. Per-site unit test for ContextSearch path: trigger a ContextSearch observation under an active phase; verify phase on the written row.
5. Code-review checklist: confirm all four write sites in `dispatch_request` have the pre-capture line before `spawn_blocking`.

**Coverage Requirement**: All four write sites must have at least one test that produces a row with a non-NULL phase value. Tests must read back from the DB (not just assert the struct field).

---

### R-04: Phase Capture Inside spawn_blocking — Race with set_current_phase
**Severity**: High  
**Likelihood**: Low  
**Impact**: If phase is read inside the `spawn_blocking` closure rather than before it, a concurrent `set_current_phase` call between closure scheduling and execution can produce a phase value from a different event, or NULL when one was expected. Subtle, non-deterministic data corruption.

**Test Scenarios**:
1. Code-review assertion: in `dispatch_request`, confirm the pattern `let phase = session_registry.get_state(...).and_then(|s| s.current_phase.clone())` appears before `spawn_blocking(move || { ... })` at every write site. The closure captures `phase` by move, not `session_registry`.
2. Timing test: set phase to "design", immediately call insert_observation, then change phase to "delivery". Read back the row. Assert phase is "design" (the value at capture time, not at write time).

**Coverage Requirement**: Timing test (scenario 2) is mandatory to validate the pre-capture contract. Code review is a secondary gate.

---

### R-05: Migration Partial Application — One Column Added, Version Bumped
**Severity**: Med  
**Likelihood**: Low  
**Impact**: Database has `goal_embedding` but not `phase` (or vice versa), yet schema_version = 21. On re-open, the v21 block is skipped entirely. The missing column causes runtime errors on any observation INSERT attempt.

**Test Scenarios**:
1. Integration test: open a real v20 database through `Store::open()`. Assert both `goal_embedding` on `cycle_events` and `phase` on `observations` appear via `pragma_table_info`. Assert schema_version = 21 in the counters table.
2. Atomicity test: use a v20 database where `goal_embedding` already exists (simulate partial apply by adding the column manually). Call `Store::open()`. Assert the migration adds `phase`, does not error on the pre-existing `goal_embedding`, and schema_version = 21.
3. Failure injection (if feasible): force the second ALTER to fail mid-transaction and verify schema_version remains 20 and `goal_embedding` is absent (full rollback). This may require test infrastructure to inject SQL errors.

**Coverage Requirement**: Scenario 1 (real v20 fixture) is mandatory (AC-01, AC-07, FR-M-04, entry #378 lesson). Scenario 2 covers partial-apply recovery via pragma_table_info idempotency.

---

### R-06: Migration Idempotency Broken — Re-run on v21 Alters or Errors
**Severity**: Med  
**Likelihood**: Low  
**Impact**: Running migration on an already-v21 database produces a SQLite error (`duplicate column name`) or incorrectly bumps the schema version counter. Server startup fails for upgraded databases.

**Test Scenarios**:
1. Idempotency test: open a v21 database through `Store::open()` a second time. Assert no error is returned, schema_version remains 21, and both columns still exist. (AC-11)
2. Fresh-schema test: create a new database from scratch at v21 (empty store). Assert it initializes to v21 with both columns present without error.

**Coverage Requirement**: Scenario 1 is mandatory (AC-11).

---

### R-07: Embed Task Blocks Tokio Executor
**Severity**: High  
**Likelihood**: Low  
**Impact**: ONNX embedding computation on a tokio thread starves the async runtime. Under concurrent MCP load, all handlers become unresponsive. Entry #771 documents this exact failure pattern for blocking operations on the tokio runtime.

**Test Scenarios**:
1. Code-review assertion: confirm `adapter.embed_entry()` dispatches through `ml_inference_pool` (rayon pool via `spawn_with_timeout`), not directly on a tokio async task or via `tokio::task::spawn_blocking` calling into the tokio runtime.
2. Integration test: call cycle start with goal under a test embed service. Assert the MCP handler returns in < 5ms (NFR-01). The embedding completes asynchronously after the response.

**Coverage Requirement**: Code review is the primary gate. Response latency test (NFR-01) provides runtime evidence.

---

### R-08: Residual Race — UPDATE Executes Before INSERT, goal_embedding Permanently NULL
**Severity**: Med  
**Likelihood**: Med  
**Impact**: For a small fraction of cycle starts (dependent on tokio scheduler behavior under load), the goal embedding is permanently lost. The cycle completes normally; no error is visible. H1 coverage is sparse and non-deterministic. Referenced in ADR-002 as accepted degradation.

**Test Scenarios**:
1. Degradation acceptance test: confirm that `update_cycle_start_goal_embedding` on a non-existent cycle_id returns Ok(()) (zero rows affected is not an error). Assert goal_embedding is NULL on a subsequent read of that (non-existent) row — and no panic occurs.
2. Warn-path test for future enhancement: if a retry mechanism is added, verify the retry fires once and logs appropriately. (Out of scope for crt-043 — document as known gap.)

**Coverage Requirement**: Scenario 1 validates the graceful no-op contract. The race itself is accepted per ADR-002 — no test is required to force the race condition.

---

### R-09: Goal Embedding Spawned on Empty/Absent Goal
**Severity**: Low  
**Likelihood**: Low  
**Impact**: Unnecessary tokio spawn and embed work for empty goal. If a warning is erroneously emitted on empty goal, log noise is introduced (spec FR-B-09 states no warn on absent goal — warn is only for embed service unavailability).

**Test Scenarios**:
1. Empty-string test: call cycle start with `goal = ""`. Assert no embed task is spawned (verify by stub embed handle receiving zero calls). Assert no `tracing::warn!` is emitted. Assert goal_embedding is NULL.
2. Absent-goal test: call cycle start with no goal parameter. Same assertions.

**Coverage Requirement**: Both sub-cases are mandatory (AC-04b).

---

### R-10: Embed Service Unavailable — Warn Not Emitted or Cycle Blocked
**Severity**: High  
**Likelihood**: Low  
**Impact**: Either the cycle start call blocks waiting for embed (violating fire-and-forget contract and NFR-01) or the warn is swallowed, making embed failures invisible in logs. Historical entry #735 shows silent failure in fire-and-forget paths is a recurring pattern.

**Test Scenarios**:
1. Unavailable-service test: configure a stub embed service that returns `EmbedNotReady`. Call cycle start with a non-empty goal. Assert the call returns without blocking. Assert a `tracing::warn!` with the expected message is captured. Assert goal_embedding is NULL on the row. (AC-04a)
2. Embed-error test: configure a stub embed service that returns an error during `embed_entry()`. Same assertions as scenario 1.
3. Latency test: with a slow embed stub (50ms artificial delay), assert `handle_cycle_event` returns in < 5ms (NFR-01) — the delay must not be on the hot path.

**Coverage Requirement**: Scenarios 1 and 2 are mandatory (AC-04a). Scenario 3 validates the fire-and-forget timing contract.

---

### R-11: decode_goal_embedding Missing or Mismatched
**Severity**: Med  
**Likelihood**: High  
**Impact**: Group 6 agents implementing H1 goal-clustering find no decode API. They independently implement decoding using raw bytes or a different bincode config, producing format divergence (SR-02). Without a tested decode helper in the same PR, the codebase pattern is undefined.

**Test Scenarios**:
1. Presence test: `decode_goal_embedding` exists in `unimatrix-store` in the same module as `encode_goal_embedding`. Compilation is sufficient.
2. Round-trip test (same as R-02 scenario 1): encode a known Vec<f32>, decode via `decode_goal_embedding`, assert equality.
3. Pub(crate) scope test: confirm the helper is not accidentally re-exported as a public API. Code review assertion.

**Coverage Requirement**: Round-trip test is mandatory (AC-14). Existence is verified by compilation.

---

### R-12: context_cycle MCP Response Text Changed
**Severity**: Low  
**Likelihood**: Low  
**Impact**: Downstream agents or tests that parse the cycle start response string break silently. Scope explicitly forbids this (AC-06, FR-B-07).

**Test Scenarios**:
1. Response-text test: call `context_cycle(type=start, goal="x")` through the MCP layer in a test. Assert the returned string matches the pre-crt-043 expected text byte-for-byte.

**Coverage Requirement**: One test is sufficient (AC-06).

---

### R-13: Composite Index Decision Deferred Beyond Delivery
**Severity**: Med  
**Likelihood**: Med  
**Impact**: Group 6 S6/S7 signal queries scan the full observations table filtered by `(topic_signal, phase)`. At current data volumes this is acceptable; at scale (tens of thousands of observation rows) it becomes a latency bottleneck. SR-06 explicitly identifies this as a delivery-time decision, not a Group 6 deferral.

**Test Scenarios**:
1. Decision gate: before the PR is opened, the delivery agent must produce a written evaluation (in the PR description or a delivery note) stating whether the composite index was added or not, with justification. If added, the migration test (R-05 scenario 1) must verify the index exists via `sqlite_master`.
2. If added: verify the index covers both columns in the correct order (`topic_signal` first, then `phase`) via `pragma index_info`.

**Coverage Requirement**: Written decision required (FR-C-07). Index presence test required if the index is added.

---

## Integration Risks

**INSERT-before-UPDATE ordering (R-01, R-08)** — The two fire-and-forget spawns inside `handle_cycle_event` have a best-effort but non-guaranteed ordering under the multi-threaded tokio runtime. The architecture relies on rayon CPU work providing natural delay before the UPDATE executes. This is the highest-integration-risk point in the feature: the INSERT and UPDATE are in different async tasks with no explicit synchronization primitive between them.

**EmbedServiceHandle accessibility in UDS listener** — ADR-002 confirms the handle is accessible via signature extension. Any refactor that changes `dispatch_request`'s parameter list without updating `handle_cycle_event` breaks this path silently (the embed task spawns but embed_service is cloned from a stale Arc).

**spawn_blocking pool contention (entry #735)** — The `update_cycle_start_goal_embedding` UPDATE acquires the Store connection inside a background task. If other fire-and-forget work at cycle start holds the Store mutex simultaneously, this UPDATE queues behind them. NFR-03 requires this not add an independent Store acquisition; the delivery agent must confirm the sequencing at implementation time.

**Phase capture timing at all four write sites** — The pre-`spawn_blocking` capture pattern is established (col-024 ADR-004, entry #3374) but must be replicated at four independent sites. Each is an independent integration point that can be missed. The missing-site risk (R-03) is the highest-likelihood integration failure.

---

## Edge Cases

- **goal = ""** (empty string, not null): must not spawn embed task. Distinct from `goal = None`.
- **goal = " "** (whitespace only): behavior is unspecified in scope. Delivery agent must decide: treat as non-empty (spawn task, embed whitespace) or trim-then-check. Document the decision.
- **Cycle stop event received before embed task completes**: the cycle_events stop row is written; the goal_embedding UPDATE for the start row runs after the stop row exists. No ordering constraint between them — the UPDATE targets `event_type = 'cycle_start'` specifically.
- **Multiple concurrent CycleStart events for the same cycle_id**: `update_cycle_start_goal_embedding` uses `WHERE topic = ? AND event_type = 'cycle_start'` — if multiple start rows exist (a data anomaly), the UPDATE affects all of them. Not expected but should be documented as a known edge.
- **Phase set to empty string via `set_current_phase("")`**: if possible, this stores `phase = ''` (empty string), not NULL. Group 6 queries using `WHERE phase = 'design'` would miss it. Deliver should verify `set_current_phase` rejects or normalizes empty strings, or document the distinction.
- **Session registry miss**: `get_state(session_id)` returns None when the session is unknown. Phase must be NULL (not panic). Verify None propagation through the `and_then` chain.
- **v20 database with pre-existing `goal_embedding` column** (partial migration artifact): pragma_table_info check must skip that ALTER and proceed to add `phase`. Test via scenario 2 in R-05.
- **384-dimension vs future 768-dimension embeddings**: bincode blobs are distinguishable by byte length. `decode_goal_embedding` must not assume a fixed dimension. Round-trip test (R-02) should use the actual embed pipeline's output dimension, not a hand-crafted 384-float vector.

---

## Security Risks

**Untrusted input surface**: The `goal` parameter on `context_cycle(type=start)` is the only new untrusted input path. It is a free-text string from the MCP tool caller.

- **Injection via goal text**: `goal` is passed to `EmbedServiceHandle` for embedding (ONNX inference on a Vec<f32>), not to any SQL query. The embedding pipeline accepts arbitrary Unicode text; there is no SQL injection surface here. The `update_cycle_start_goal_embedding` method takes the serialized bytes (not the goal text) as its argument — no goal text reaches a SQL bind parameter.
- **goal_embedding blob as attack surface**: the blob is written by crt-043 (trusted server code) and read by Group 6 (future trusted code). No external actor writes directly to this column. Risk is low.
- **phase value injection**: `phase` is stored as-is from `SessionState.current_phase`, which is set by the authenticated MCP `context_cycle` tool caller. It is bound as a SQL parameter (not interpolated), so SQL injection is not possible. The column has no allowlist — a caller can store arbitrary strings — but this is a data quality risk (SR-05), not a security risk.
- **Blast radius if embed service is compromised**: the embed service is an internal rayon pool running local ONNX inference. There is no network boundary. A compromised embed service could return malformed Vec<f32> values — but `encode_goal_embedding` would serialize them faithfully, and `decode_goal_embedding` would deserialize them without validation. H1 goal-clustering would produce nonsense results. Mitigation: validate decoded vector dimension matches the expected model output dimension at read time (Group 6 responsibility, documented here).

**No new network surface, no new external dependencies, no HTTP client introduced** (AC-05, FR-B-11). Security risk is confined to the internal embed pipeline and SQL parameter binding.

---

## Failure Modes

**Embed service unavailable at cycle start**: embedding task calls `get_adapter()`, receives `EmbedNotReady`, emits `tracing::warn!`, exits. `goal_embedding` is NULL. Cycle start response is unaffected. Recovery: if the embed service becomes available before the next cycle start, subsequent cycles get embeddings. No retry for the affected cycle.

**Embed computation error (ONNX failure)**: embedding task catches the error from `adapter.embed_entry()`, emits `tracing::warn!`, exits. Same outcome as unavailable case.

**`encode_goal_embedding` failure**: bincode encode fails (should be unreachable for a valid Vec<f32> with `standard()` config, but the Result is propagated). Emit `tracing::warn!`. No UPDATE issued. `goal_embedding` is NULL.

**`update_cycle_start_goal_embedding` store error**: DB write error (connection failure, locked DB). Emit `tracing::warn!` with cycle_id. Row stays with NULL `goal_embedding`. Server continues normally.

**Observation write with session state miss**: `get_state(session_id)` returns None. Phase captured as None. Row written with `phase = NULL`. No error, no warning. This is the expected cold-start path (Workflow 5).

**Migration failure (partial)**: one ALTER TABLE fails. The outer transaction in `migrate_if_needed` rolls back. Schema version remains 20. `Store::open()` returns an error. Server fails to start. On next restart, the full v21 block re-runs from scratch — the pragma_table_info checks handle any partially-applied column.

**v21 database opened by old binary**: old binary expects schema_version <= 20; it will either fail the version check or encounter unknown columns. This is expected and acceptable (NFR-05). No crt-043 action needed.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: INSERT/UPDATE race — UPDATE may precede INSERT | R-01, R-08 | ADR-002 chooses Option 1: embed spawn fired from `handle_cycle_event` after INSERT spawn. Residual race accepted; NULL degradation documented. |
| SR-02: bincode format divergence — no decode path for read sites | R-02, R-11 | ADR-001 specifies exact API (`encode_to_vec` / `decode_from_slice`, `config::standard()`). Paired helpers mandated in same PR. Round-trip test required. |
| SR-03: fire-and-forget pool saturation from additional Store acquisition | R-07 | NFR-03 prohibits independent Store mutex acquisition. Delivery agent must sequence the embed UPDATE with existing cycle-start fire-and-forget work. |
| SR-04: two ADD COLUMN in one migration — partial apply risk | R-05 | ADR-003 confirms both statements share the outer transaction from `migrate_if_needed`. Rollback is atomic. Integration test against real v20 DB required. |
| SR-05: phase allowlist absent — case-variant values corrupt Group 6 queries | — | Accepted at write time per FR-C-06, C-05. Canonical values documented. Group 6 must apply `LOWER()` at query time. No crt-043 action beyond documentation. |
| SR-06: composite index deferred — full-table scan on observations for Group 6 | R-13 | FR-C-07 mandates delivery-agent decision before PR open. Cannot be deferred to Group 6. |
| SR-07: EmbedServiceHandle not accessible in UDS listener | — | Resolved. ADR-002 confirms the handle is in scope at all three `handle_cycle_event` call sites in `dispatch_request`. Signature extension only. |
| SR-08: NULL goal_embedding for pre-v21 rows — downstream consumers may not handle NULL | — | Accepted cold-start degradation (NFR-04). Group 6/7 spec must handle NULL. No crt-043 action. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 3 scenarios — integration await test + concurrent test + code review |
| High | 6 (R-02, R-03, R-04, R-07, R-10, R-11) | 13 scenarios across the six risks |
| Med | 5 (R-05, R-06, R-08, R-11, R-13) | 8 scenarios — migration fixture, idempotency, degradation, index decision |
| Low | 2 (R-09, R-12) | 3 scenarios — empty goal, response text |

**Non-negotiable tests** (must exist before gate-3b):
- Round-trip encode→decode test for `encode_goal_embedding` / `decode_goal_embedding` (R-02, AC-14)
- Integration test: real v20 database through `Store::open()` → both columns present, schema_version = 21 (R-05, AC-01/AC-07, FR-M-04)
- Phase written for all four observation write sites, read back from DB (R-03, AC-09/AC-10)
- Embed-service-unavailable path: warn emitted, cycle start not blocked, goal_embedding NULL (R-10, AC-04a)
- Empty/absent goal: no task spawned, no warn emitted (R-09, AC-04b)
- Migration idempotency: Store::open() on v21 completes without error (R-06, AC-11)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #3579 (Gate 3b test omission = mandatory rework), #2758 (grep non-negotiable tests before accepting coverage reports), #2577 (boundary tests must ship same pass as implementation). These reinforce the non-negotiable test list above.
- Queried: `/uni-knowledge-search` for "risk pattern SQLite migration embedding serialization" — found #4065 (pair every new SQLite embedding blob with encode/decode helpers, same PR). This directly validates R-11 and the ADR-001 decision.
- Queried: `/uni-knowledge-search` for "tokio spawn fire-and-forget race condition" — found #735 (spawn_blocking pool saturation from unbatched fire-and-forget writes), #771 (blocking store.lock_conn on tokio causes hangs), #1673 (fire-and-forget supervisor pattern). Entry #735 directly informs R-10 and NFR-03; entry #771 informs R-07.
- Stored: nothing novel to store — the fire-and-forget INSERT/UPDATE race pattern (R-01/R-08) is already captured in ADR-002 and the existing pattern library is sufficient. The bincode helper discipline is already stored as entry #4065.
