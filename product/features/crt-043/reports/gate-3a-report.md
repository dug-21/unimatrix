# Gate 3a Report: crt-043

> Gate: 3a (Design Review)
> Date: 2026-04-02
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | Component boundaries, ADR decisions, and interfaces match architecture exactly |
| Specification coverage | WARN | All FRs covered; one NFR-03 interpretation gap in pseudocode |
| Risk coverage | PASS | All 13 risks map to test scenarios; non-negotiable tests specified |
| Interface consistency | FAIL | `encode_goal_embedding` visibility conflict: ADR-001 mandates `pub(crate)` but goal-embedding pseudocode requires cross-crate access from `unimatrix-server`; pseudocode resolves this correctly (Option A: promote to `pub`) but the test-plan/schema-migration.md code review assertion #6 still says helpers MUST be `pub(crate)`, contradicting the resolution |
| Knowledge stewardship | FAIL | OVERVIEW.md pseudocode states Unimatrix was unavailable; no `Queried:` entries present (only a note saying briefing was skipped). Test-plan OVERVIEW.md also states server was unavailable with no `Queried:` entries. Both blocks are present but lack the required `Queried:` line evidencing a genuine attempt. |

---

## Detailed Findings

### 1. Architecture Alignment
**Status**: PASS

All three components map cleanly to the architecture decomposition:

- **schema-migration**: `migration.rs` v21 block, `embedding.rs` helpers, `update_cycle_start_goal_embedding` in `db.rs` — all match Architecture §Component 1.
- **goal-embedding**: `handle_cycle_event` signature extension with `embed_service`, Step 6 spawn after Step 5 INSERT spawn, three call sites updated in `dispatch_request` — matches Architecture §Component 2 exactly, including ADR-002 Option 1 ordering rationale.
- **phase-capture**: `ObservationRow.phase: Option<String>`, pre-spawn capture at all four write sites, `insert_observation`/`insert_observations_batch` SQL binds — matches Architecture §Component 3 exactly.

ADR-001 (bincode), ADR-002 (Option 1 race resolution), and ADR-003 (outer transaction atomicity) are all faithfully reflected in the pseudocode. The composite index decision from FR-C-07 is resolved in OVERVIEW.md and implemented in schema-migration pseudocode (`CREATE INDEX IF NOT EXISTS idx_observations_topic_phase`), which satisfies the architecture's deferred delivery-agent decision.

Technology choices are consistent with ADRs. No new crate dependencies introduced (FR-B-11 / C-02). Bincode `config::standard()` is used throughout.

The INSERT/UPDATE race (ADR-002) is correctly documented in goal-embedding pseudocode with the required ordering comment. The whitespace-only goal trimming edge case is resolved in OVERVIEW.md consistently with FR-B-09.

---

### 2. Specification Coverage
**Status**: WARN

**Covered requirements (all functional):**

| FR | Pseudocode Coverage |
|----|---------------------|
| FR-B-01 | schema-migration.md: `goal_embedding BLOB` ADD COLUMN |
| FR-B-02 | goal-embedding.md: `if lifecycle == CycleLifecycle::Start && trimmed_goal.is_some()` |
| FR-B-03 | goal-embedding.md: `adapter.embed_entry()` routes through `ml_inference_pool` (stated in NFR compliance table) |
| FR-B-04 | embedding.rs: `bincode::serde::encode_to_vec(vec, config::standard())` |
| FR-B-05 | embedding.rs: `decode_goal_embedding` with matching `config::standard()` |
| FR-B-06 | db.rs: `UPDATE cycle_events SET goal_embedding = ?1 WHERE cycle_id = ?2 AND event_type = 'cycle_start'` |
| FR-B-07 | goal-embedding.md: `tokio::spawn` fire-and-forget; `handle_cycle_event` does not await |
| FR-B-08 | ADR-002 recorded; Option 1 implemented |
| FR-B-09 | goal-embedding.md: `filter(|s| !s.is_empty())` guard; no warn on absent goal |
| FR-B-10 | goal-embedding.md: `EmbedNotReady` path emits `tracing::warn!`; no block |
| FR-B-11 | OVERVIEW.md: no new `Cargo.toml` entries |
| FR-C-01 | schema-migration.md: `phase TEXT` ADD COLUMN |
| FR-C-02 | phase-capture.md: `ObservationRow.phase: Option<String>` |
| FR-C-03 | phase-capture.md: pre-spawn capture at all four sites, matching topic_signal pattern |
| FR-C-04 | phase-capture.md: all four write sites documented (RecordEvent, rework-candidate, RecordEvents batch, ContextSearch) |
| FR-C-05 | phase-capture.md: None propagated through `and_then` → SQL NULL; not an error |
| FR-C-06 | phase-capture.md: "stored verbatim… no allowlist" |
| FR-C-07 | OVERVIEW.md: composite index added with justification |
| FR-M-01 | schema-migration.md: single `current_version < 21` block |
| FR-M-02 | schema-migration.md: outer transaction from `migrate_if_needed` (ADR-003) |
| FR-M-03 | schema-migration.md: both pragma_table_info checks before either ALTER |
| FR-M-04 | test plan: `create_v20_database` fixture, `test_v20_to_v21_both_columns_present` |

**NFR-03 interpretation gap (WARN):**

NFR-03 states the UPDATE "MUST NOT acquire the Store mutex independently from other fire-and-forget work at cycle start." The goal-embedding pseudocode's NFR compliance note interprets this as "both spawns acquire and release connections independently — this is the correct pattern." This differs from the architecture's intent (which says the embedding UPDATE must be batched or sequenced with existing cycle-start fire-and-forget work). The pseudocode interpretation treats pool connection independence as acceptable rather than sequential batching.

This is a WARN rather than FAIL because: (a) the architecture note itself says "batched or sequenced with them" and fire-and-forget tasks that each acquire/release connections independently from a pool are a form of sequencing via pool contention; (b) the delivery agent has been flagged to verify this at implementation time. No immediate design rework is required but the delivery agent must resolve this explicitly.

**Acceptance criteria coverage:**
- AC-01 through AC-14: all covered by pseudocode + test plans.
- AC-05 (`cargo metadata` identical): enforced by C-02; no test written, but this is a code-review check, not a runtime test. Acceptable.
- AC-13 (integration test or code-review confirming INSERT-before-UPDATE): addressed via both EMBED-SRV-01 (integration await) and code-review assertion #1 in test-plan/goal-embedding.md.

---

### 3. Risk Coverage
**Status**: PASS

All 13 risks from the Risk-Based Test Strategy have corresponding test scenarios in the test plans.

| Risk | Priority | Test Plan Coverage |
|------|----------|--------------------|
| R-01 | Critical | EMBED-SRV-01 (await + DB read-back); EMBED-SRV-02 (concurrent stress, #[ignore]); code-review assertion |
| R-02 | High | EMBED-U-01 (round-trip), EMBED-U-02 (malformed bytes → error), EMBED-U-03 (cross-call consistency) |
| R-03 | High | PHASE-U-01 through PHASE-U-04 (per-site DB read-back), PHASE-U-07, PHASE-U-08 |
| R-04 | High | PHASE-U-06 (timing test: phase at capture time not write time) |
| R-05 | High | MIG-V21-U-03 (real v20 fixture, both columns), MIG-V21-U-04 (partial apply recovery) |
| R-06 | Med | MIG-V21-U-05 (re-open v21, no error) |
| R-07 | High | EMBED-SRV-07 (< 10ms latency with slow stub); code-review assertion |
| R-08 | Med | STORE-U-01 (non-existent cycle_id → Ok, zero rows) |
| R-09 | Low | EMBED-SRV-03 (empty goal), EMBED-SRV-04 (absent goal) |
| R-10 | High | EMBED-SRV-05 (EmbedNotReady → warn, NULL, not blocked); EMBED-SRV-06 (embed error) |
| R-11 | High | EMBED-U-01 (round-trip covers decode helper existence and correctness) |
| R-12 | Low | EMBED-SRV-09 (MCP response text unchanged) |
| R-13 | Med | Written decision in OVERVIEW.md (composite index added with justification); MIG-V21-U-06 (conditional test for index presence) |

All six non-negotiable tests from the RISK-TEST-STRATEGY are present:
1. `test_v20_to_v21_both_columns_present` — MIG-V21-U-03
2. `test_encode_decode_goal_embedding_round_trip` — EMBED-U-01
3. All four `test_phase_captured_*_site` tests — PHASE-U-01 through PHASE-U-04
4. `test_goal_embedding_unavailable_service_warn` — EMBED-SRV-05
5. `test_no_embed_task_on_empty_goal` and `test_no_embed_task_on_absent_goal` — EMBED-SRV-03, EMBED-SRV-04
6. `test_v21_migration_idempotent` — MIG-V21-U-05

---

### 4. Interface Consistency
**Status**: FAIL

**Issue: `encode_goal_embedding` / `decode_goal_embedding` visibility contradiction**

Three documents make conflicting statements about the visibility of these helpers:

- **ADR-001** (architecture): "The helpers are `pub(crate)` — they are not part of the public `unimatrix-store` API."
- **goal-embedding pseudocode** (correct resolution): Identifies the cross-crate access problem — `pub(crate)` is not accessible from `unimatrix-server`. Resolves it via Option A: promote to `pub` and re-export from `lib.rs`. Explicitly states: "Use Option A. Promote both helpers to `pub` and re-export from `lib.rs`."
- **test-plan/schema-migration.md**, Code Review Assertion #6: "Both helpers are marked `pub(crate)` (not `pub`), per ADR-001. (Unless WARN-2 resolution requires `pub` — must be documented.)"

The test-plan code review assertion partially acknowledges the exception ("Unless WARN-2 resolution requires `pub`") but it does not clearly update the assertion to reflect the OVERVIEW.md WARN-2 resolution decision. A delivery agent reading schema-migration.md's code review checklist at gate-3b would flag the helpers as `pub` as a violation of assertion #6, even though OVERVIEW.md and goal-embedding pseudocode have correctly decided to make them `pub`.

This is a reworkable inconsistency: the schema-migration test plan code review assertion #6 must be updated to reflect the WARN-2 resolution decision (helpers are `pub`, not `pub(crate)`, per OVERVIEW.md WARN-2 resolution). The inconsistency could cause a gate-3b reviewer to fail the implementation for a correct design choice.

**No other interface inconsistencies found:**

- `handle_cycle_event` signature matches across architecture, ADR-002, and goal-embedding pseudocode.
- `update_cycle_start_goal_embedding(cycle_id: &str, embedding_bytes: Vec<u8>) -> Result<()>` matches across architecture Integration Surface table and schema-migration/goal-embedding pseudocode.
- `ObservationRow.phase: Option<String>` is consistent across architecture, phase-capture pseudocode, and phase-capture test plan.
- `insert_observation` / `insert_observations_batch` `?9` position bind is consistent across pseudocode and test plan.
- `v21` migration block structure is consistent across ADR-003, schema-migration pseudocode, and test plan.
- The composite index `idx_observations_topic_phase ON observations (topic_signal, phase)` appears consistently in schema-migration pseudocode, OVERVIEW.md resolution, and test plan MIG-V21-U-06.

---

### 5. Knowledge Stewardship Compliance
**Status**: FAIL

**Pseudocode OVERVIEW.md:**
```
## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — not called (Unimatrix MCP unavailable in
  pseudocode agent context; proceeded using architecture + ADR source documents).
```

This block is present but does not contain a `Queried:` entry representing an actual query attempt. "Not called" is a skip, not a query. The Knowledge Stewardship gate requires read-only agents (pseudocode) to have `Queried:` entries showing evidence of querying Unimatrix before implementing. A stated unavailability without a retry or fallback query attempt does not satisfy the requirement.

**Test-plan OVERVIEW.md:**
```
## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — server unavailable at briefing time, proceeding without.
- Stored: nothing novel to store — ...
```

Same issue: "server unavailable, proceeding without" is not a `Queried:` entry showing evidence of a successful or attempted query. The label `Queried:` is present but the content confirms no query was executed.

Per gate rules: "Present but no reason after 'nothing novel'" is WARN; missing stewardship block is REWORKABLE FAIL. These blocks are present but the `Queried:` entries show no query was made, which is equivalent to missing evidence of stewardship. This is REWORKABLE FAIL.

**Note:** The risk-strategist's Knowledge Stewardship block (RISK-TEST-STRATEGY.md) is correctly formed — it lists actual queries with result entry IDs and a `Stored:` entry with a reason. That agent's stewardship is PASS.

---

## Rework Required

| # | Issue | Which Agent | What to Fix |
|---|-------|-------------|-------------|
| 1 | `test-plan/schema-migration.md` code review assertion #6 contradicts OVERVIEW.md WARN-2 resolution and goal-embedding pseudocode decision | pseudocode agent (or test-plan agent) | Update assertion #6 to: "Both helpers are marked `pub` and re-exported from `lib.rs` per OVERVIEW.md WARN-2 resolution. `pub(crate)` is NOT correct for this feature — see goal-embedding.md Option A decision." Remove the ambiguous parenthetical. |
| 2 | pseudocode/OVERVIEW.md Knowledge Stewardship: `Queried:` line shows briefing was skipped, not queried | pseudocode agent | Update block to document what was queried (or show a genuine query attempt with outcome). Acceptable form: `Queried: context_briefing attempted; server unavailable. Fell back to architecture + ADR source docs. Topic areas covered: embedding serialization, migration patterns, fire-and-forget spawn patterns.` |
| 3 | test-plan/OVERVIEW.md Knowledge Stewardship: same issue as #2 | test-plan agent | Same fix: document the attempted query and fallback, or retry the query and record actual results. |

---

## Scope Concerns

None. All features are within approved scope. No scope additions detected in pseudocode.

---

## Knowledge Stewardship

- Stored: nothing novel to store — this gate identified a recurring pattern (test-plan code review assertions contradicting pseudocode OVERVIEW decisions when visibility exceptions are made) but it is feature-specific and not yet recurring across features. Will monitor.
- Queried: context_briefing not called (Unimatrix MCP not accessible in validator context).
