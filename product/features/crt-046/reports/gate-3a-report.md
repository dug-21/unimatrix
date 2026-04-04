# Gate 3a Report: crt-046

> Gate: 3a (Component Design Review)
> Date: 2026-04-04
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | WARN | `emit_behavioral_edges` deviates from arch/spec-mandated `enqueue_analytics` path; direct `write_pool_server()` used instead — no ADR covers this departure |
| Specification coverage | PASS | All FRs and NFRs have corresponding pseudocode |
| Risk coverage | PASS | All 15 active risks map to test scenarios |
| Interface consistency | PASS | Shared types consistent across pseudocode files |
| Critical Check 1 — memoisation gate placement (Resolution 2) | PASS | Memoisation early-return is explicitly placed AFTER step 8b in pseudocode |
| Critical Check 2 — parse_failure_count placement (Resolution 1) | PASS | Correctly specified as top-level field outside CycleReviewRecord |
| Critical Check 3 — write_graph_edge contract table (pattern #4041) | PASS | Contract table leads `emit_behavioral_edges` pseudocode |
| Critical Check 4 — INSERT OR IGNORE throughout | PASS | No INSERT OR REPLACE anywhere in pseudocode |
| Critical Check 5 — self-pair exclusion before dedup (Resolution 4) | PASS | `filter(|(a, b)| a != b)` appears before dedup in build_coaccess_pairs |
| Critical Check 6 — two-level briefing guard (ADR-004, Resolution 3) | PASS | Level 1 (feature absent OR goal empty) and Level 2 (embedding None) both present in correct order |
| Critical Check 7 — blend_cluster_entries is pure | PASS | No store access; caller supplies pre-fetched, pre-scored entries |
| Critical Check 8 — schema v22 cascade (9 touchpoints) | WARN | All 9 touchpoints addressed in pseudocode; AC-17 check cannot be run pre-implementation; one comment-only item in migration tests |
| Critical Check 9 — store.get_by_ids() existence | PASS | `get_by_ids()` does not exist; pseudocode correctly falls back to individual `store.get(id)` calls |
| Critical Check 10 — graph_edges write path contradiction | FAIL | Architecture and Spec mandate `enqueue_analytics`; pseudocode uses `write_pool_server()` directly; no ADR covers this; IMPLEMENTATION-BRIEF flags this as requiring human resolution |
| Critical Check 11 — test plan non-negotiable gate tests | PASS | All 8 non-negotiable tests present with correct assertions |
| Critical Check 12 — PAIR_CAP enforced at enumeration time | PASS | Cap enforced by halting at 200, not truncating after full generation |
| Critical Check 13 — InferenceConfig fields read from config | PASS | All three fields read from `InferenceConfig` at call time; not constants |
| Knowledge stewardship — pseudocode agent | PASS | Queried entries present; deviation documented |
| Knowledge stewardship — test plan agent | PASS | Queried entries present; stored entry with reason documented |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: WARN

**Evidence**: The pseudocode components align well with the Architecture:
- `behavioral_signals.rs` is a new module in `services/` per ADR-001.
- `goal_clusters.rs` implements the three new store methods with correct signatures.
- `context_cycle_review` step 8b placement is after step 8a and before step 11 (audit).
- `context_briefing` handler adds blending per Component 4 architecture.
- `InferenceConfig` new fields are added at `infra/config.rs`.
- Nine cascade touchpoints are all enumerated in store-v22 pseudocode.

**Issue (WARN)**: `emit_behavioral_edges` uses `write_pool_server()` directly (a private helper `write_graph_edge`) rather than `enqueue_analytics(AnalyticsWrite::GraphEdge)`. This deviates from ARCHITECTURE.md §Component 1 ("enqueue `AnalyticsWrite::GraphEdge` for both directions"), §Technology Decisions ("Analytics drain for graph edges"), and SPEC FR-06/FR-07 ("via `store.enqueue_analytics`"). See Critical Check 10 for the FAIL entry on this topic.

---

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**:
- FR-01 to FR-15 (edge emission + goal cluster): covered by `cycle-review-step-8b.md` and `behavioral-signals.md`.
- FR-16 to FR-23 (briefing blending): covered by `briefing-blending.md`.
- NFR-01 (idempotency): INSERT OR IGNORE throughout, confirmed in store-v22 and behavioral-signals pseudocode.
- NFR-02 (cold-start correctness): Two-level guard in briefing-blending.md; all four cold-start paths enumerated.
- NFR-03 (briefing latency): In-process O(100×384) cosine scan, NULL fast-path before DB calls.
- NFR-04 (pair cap safety): `PAIR_CAP = 200` constant; cap enforced at enumeration time.
- NFR-05 (no spawn_blocking for sqlx): All three new store methods are `async fn`, explicitly noted.
- NFR-06 (file size): New logic extracted to `behavioral_signals.rs`.
- NFR-07 (schema migration): Additive DDL only; `CREATE TABLE IF NOT EXISTS`.
- FR-03 (`parse_failure_count` surface): Correctly specified as top-level field in `cycle-review-step-8b.md`.
- SPEC Constraint 12 (`context_search` exclusion): Not modified; blending in `IndexBriefingService` only.
- SPEC Constraint 14 (`InferenceConfig` fields): `w_goal_cluster_conf`, `w_goal_boost`, `goal_cluster_similarity_threshold` all added to `InferenceConfig`.

No scope additions found (no unrequested features in pseudocode).

---

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**: All 15 active risks (R-01 through R-16 excluding resolved R-13) are mapped to test scenarios in the test plans:

| Risk | Test Plan Coverage |
|------|-------------------|
| R-01 | AC-15 integration test (cycle-review-step-8b.md) |
| R-02 | `test_emit_behavioral_edges_unique_conflict_not_counted` (behavioral-signals.md) |
| R-03 | Drain flush protocol in OVERVIEW.md; bootstrap_only=false code-inspection test |
| R-04 | AC-13 integration test (cycle-review-step-8b.md) |
| R-05 | AC-12 migration test + AC-17 grep check (store-v22.md) |
| R-06 | Structural verification + duplicate returns false test (behavioral-signals.md) |
| R-07 | AC-11 recency cap 101-row boundary (store-v22.md + briefing-blending.md) |
| R-08 | AC-16 unit + guard-B unit (briefing-blending.md) |
| R-09 | AC-14 integration + `test_build_coaccess_pairs_cap_enforced_at_200` (behavioral-signals.md) |
| R-10 | `test_emit_behavioral_edges_new_pair_emits_both_directions` (behavioral-signals.md) |
| R-11 | AC-08, AC-09, below-threshold and feature=None cold-start paths (briefing-blending.md) |
| R-12 | AC-10 (deprecated + quarantined) (briefing-blending.md) |
| R-14 | Code review check (store-v22.md) |
| R-15 | `test_create_tables_goal_clusters_schema` + migration column count (store-v22.md) |
| R-16 | `outcome_to_weight` table-driven test (behavioral-signals.md) |

---

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**:
- `GoalClusterRow` struct is defined once in store-v22.md and referenced consistently across briefing-blending.md and behavioral-signals.md.
- `InferenceConfig` new fields are defined in store-v22.md (OVERVIEW.md §Shared Types) and correctly consumed in briefing-blending.md using `config.w_goal_cluster_conf`, `config.w_goal_boost`, `config.goal_cluster_similarity_threshold`.
- Function signatures across files are consistent: `collect_coaccess_entry_ids`, `build_coaccess_pairs`, `emit_behavioral_edges`, `populate_goal_cluster`, `blend_cluster_entries` in behavioral-signals.md match their call sites in cycle-review-step-8b.md and briefing-blending.md.
- `ObservationRow`, `IndexEntry`, `EntryRecord` sourced from existing crates consistently.
- Naming collision (`IndexEntry.confidence` vs `EntryRecord.confidence`) explicitly documented in OVERVIEW.md, behavioral-signals.md, and briefing-blending.md with correct handling.

---

### Critical Check 1: Memoisation Gate Placement (Resolution 2)

**Status**: PASS

**Evidence**: `cycle-review-step-8b.md` explicitly documents the WRONG architecture prose and provides the correct implementation flow. The revised control flow places `[step 2.5* if memoised: format and return]` AFTER `[step 8b behavioral_signals::run_step_8b()]`. The pseudocode states: "The existing memoisation early-return block... MUST remain where it is — it is NOT moved. Step 8b is inserted AFTER step 8a... but BEFORE step 11." Then correctly states the implementation approach: record `memo_result` but do NOT return early at step 2.5; continue to step 8b; only after step 8b check if `memo_hit` and return. This satisfies Resolution 2 and FR-09 (step 8b always runs).

---

### Critical Check 2: parse_failure_count Placement (Resolution 1)

**Status**: PASS

**Evidence**: `cycle-review-step-8b.md` §parse_failure_count in JSON Response explicitly states: "`parse_failure_count: u32` is a TOP-LEVEL field in the `context_cycle_review` JSON response, OUTSIDE the serialized `CycleReviewRecord`." The pseudocode documents that `CycleReviewRecord` is NOT modified and no `SUMMARY_SCHEMA_VERSION` bump is required. The `run_step_8b` function returns `u32` which the handler includes as a top-level field.

---

### Critical Check 3: write_graph_edge Contract Table (pattern #4041)

**Status**: PASS

**Evidence**: `behavioral-signals.md` opens with "## write_graph_edge Return Contract (pattern #4041) — MUST READ FIRST" followed immediately by the three-case table:

| Return | Meaning | Counter action |
|--------|---------|----------------|
| Ok(true) | New row inserted | Increment edges_enqueued |
| Ok(false) | UNIQUE conflict | Do NOT increment |
| Err(_) | SQL failure | Log warn!, do NOT increment |

The `emit_behavioral_edges` algorithm repeats this table inline and keys counter increments exclusively off `Ok(true)`.

---

### Critical Check 4: INSERT OR IGNORE Throughout

**Status**: PASS

**Evidence**: No `INSERT OR REPLACE` found anywhere in pseudocode. `insert_goal_cluster` uses `INSERT OR IGNORE`. `write_graph_edge` uses `INSERT OR IGNORE INTO graph_edges`. Both verify `rows_affected()` to distinguish new row vs. conflict.

---

### Critical Check 5: Self-Pair Exclusion Before Dedup (Resolution 4)

**Status**: PASS

**Evidence**: `behavioral-signals.md` `build_coaccess_pairs` algorithm step 4.b.ii: "If `a == b`: skip (Resolution 4 — self-pair exclusion, DN-3)." This appears in the inner pair-enumeration loop BEFORE the deduplication step (step 4.b.iv `seen.contains`). The note explicitly states: "Note on step 4.b.ii: `filter(|(a, b)| a != b)` — self-pairs excluded BEFORE dedup." The test plan includes `test_build_coaccess_pairs_self_pairs_excluded` verifying this.

---

### Critical Check 6: Two-Level Briefing Guard (ADR-004, Resolution 3)

**Status**: PASS

**Evidence**: `briefing-blending.md` §Two-Level Guard contains both levels explicitly:

Level 1 — fires before any DB call:
```
let should_blend = feature_for_blending.is_some()
    && !feature_for_blending.unwrap().is_empty()
    && !current_goal.is_empty();
```
When `should_blend` is false: pure-semantic path, no DB calls.

Level 2 — fires after `get_cycle_start_goal_embedding`:
```
match goal_embedding_opt {
    None => cold-start (pure semantic)
    Some(goal_embedding) => proceed to cluster query
}
```

The complete integration pseudocode shows both checks in correct sequence: Level 1 before the `if should_blend { ... }` block (no DB call at all for Level 1 fail), Level 2 inside the block before the cluster query. I-04 (empty `current_goal`) is handled by `!current_goal.is_empty()` in Level 1, confirmed by test `test_briefing_guard_a_empty_goal_skips_embedding_lookup`.

---

### Critical Check 7: blend_cluster_entries is Pure

**Status**: PASS

**Evidence**: `behavioral-signals.md` §blend_cluster_entries explicitly states: "PURE FUNCTION — no store access, no async. Takes pre-fetched, pre-scored cluster entries." The function signature takes `Vec<IndexEntry>` and `Vec<(IndexEntry, f32)>` — no `SqlxStore` parameter. The `cluster_score` values in `cluster_entries_with_scores` are computed by the caller in the briefing handler (`briefing-blending.md` step 4).

---

### Critical Check 8: Schema v22 Cascade — 9 Touchpoints

**Status**: WARN

**Evidence**: All 9 touchpoints are explicitly enumerated in store-v22.md:

| # | Touchpoint | Status in Pseudocode |
|---|------------|---------------------|
| 1 | migration.rs `if current_version < 22` block | Present with full DDL |
| 2 | db.rs `create_tables_if_needed` DDL | Present, noted as byte-identical |
| 3 | db.rs schema_version INSERT bump to 22 | Present |
| 4 | db.rs test rename `_21` → `_22` | Present |
| 5 | sqlite_parity.rs goal_clusters tests (7 columns) | Present (`test_create_tables_goal_clusters_exists` + `test_create_tables_goal_clusters_schema`) |
| 6 | sqlite_parity.rs `test_schema_version_is_N` updated to 22 | Present |
| 7 | server.rs both `assert_eq!(version, 21)` → 22 | Present |
| 8 | Migration test rename `_is_21` → `_is_at_least_21` with `>= 21` | Present |
| 9 | Migration test column-count assertions referencing old total | Present as grep instruction |

**Issue (WARN)**: AC-17 (`grep -r 'schema_version.*== 21' crates/`) cannot be run pre-implementation, but the grep check is correctly specified in test-plan/store-v22.md as a Gate 3a blocking item. The existing codebase currently has one match: `crates/unimatrix-store/tests/migration_v19_v20.rs` which is a comment-only reference ("Assert: schema_version == 21 (v19→v20→v21 migration chain runs in full)."). This is a comment in a test about the v19→v21 migration chain, not a hard-coded `assert_eq!` that would fail at runtime. The AC-17 grep pattern `schema_version.*== 21` would still match this comment. The delivery agent must either: (a) confirm this is the only remaining match and it is a comment-only false positive exempt from the check, or (b) update the comment to say `>= 21` for correctness. Flag for delivery agent to resolve.

---

### Critical Check 9: store.get_by_ids() Existence

**Status**: PASS

**Evidence**: `get_by_ids()` does not exist in the codebase (confirmed: `grep -r "get_by_ids" crates/` returns zero results). The briefing-blending.md pseudocode correctly falls back to individual `store.get(id)` calls: "There is no get_by_ids bulk method yet; fetch individually." The OVERVIEW.md notes `AnalyticsWrite::GraphEdge` as "NOT used in step 8b emit path; see behavioral-signals.md" which is consistent. This is a graceful fallback with no design gap.

---

### Critical Check 10: graph_edges Write Path Contradiction

**Status**: FAIL

**Evidence and Issue**: A material contradiction exists between source documents and pseudocode on the graph_edges write path:

**Source documents specify `enqueue_analytics`**:
- ARCHITECTURE.md §Component 1: "enqueue `AnalyticsWrite::GraphEdge` for both directions of each pair with `relation_type = "Informs"`, `source = "behavioral"`, `bootstrap_only = false`"
- ARCHITECTURE.md §Technology Decisions: "Analytics drain for graph edges: `AnalyticsWrite::GraphEdge` with `INSERT OR IGNORE`. Idempotency, fire-and-forget, consistent with NLI and co-access paths. See ADR-002."
- SPECIFICATION.md FR-06: "the system shall enqueue **both** directed edges... via `store.enqueue_analytics`"
- SPECIFICATION.md FR-07: "Edge enqueueing shall be fire-and-forget via `enqueue_analytics`."
- IMPLEMENTATION-BRIEF.md Resolved Decisions: "`graph_edges` write path: `enqueue_analytics(AnalyticsWrite::GraphEdge)` with `INSERT OR IGNORE`. Fire-and-forget drain."

**Pseudocode uses direct write_pool_server()**:
- `behavioral-signals.md` §write_graph_edge: "Execute directly on `store.write_pool_server()` (not analytics drain — must return bool)"
- The pseudocode agent's stated justification: needed to implement the `write_graph_edge` return contract (pattern #4041) which requires `rows_affected()` feedback. The analytics drain is fire-and-forget and cannot return `bool`.

**Assessment**: This is a genuine design tension. The return contract requirement (pattern #4041) is real — `enqueue_analytics` cannot provide `rows_affected()` feedback. However, the deviation contradicts three source documents and changes both the behavior (no drain, immediate write) and the observability characteristics (no drain backpressure metrics, no shed counter). The IMPLEMENTATION-BRIEF.md explicitly flags this as an Open Question for delivery agent (OQ-3) and instructs: "If there's a contradiction requiring human resolution, flag as SCOPE FAIL." However, the pseudocode agent resolved it unilaterally in the pseudocode itself without generating an ADR.

The test plans are internally consistent with the pseudocode choice (they correctly note no drain flush is needed for graph_edges assertions). But I-02 in the RISK-TEST-STRATEGY says "integration tests must flush the drain" — this has been superseded in the test plans but the RISK-TEST-STRATEGY itself was not updated.

**Fix Required**: Either:
1. Generate an ADR (ADR-006) documenting the decision to use direct write for graph edges and confirming that `enqueue_analytics` cannot satisfy the write_graph_edge return contract. Update ARCHITECTURE.md and SPEC prose footnotes to reference ADR-006. This requires human approval.
2. Alternatively: if the human approves that the return contract can be relaxed (no counter accuracy needed), revert to `enqueue_analytics` and remove the `edges_enqueued` counter or make it approximate.

This is REWORKABLE (not SCOPE FAIL) because the technical resolution path exists and either option above can be executed in the current session without architectural restructuring.

---

### Critical Check 11: Test Plan Non-Negotiable Gate Tests

**Status**: PASS

**Evidence**: All 8 non-negotiable gate tests are present:

| Required Test | Where Specified | Status |
|---------------|----------------|--------|
| AC-13: parse_failure_count in response | cycle-review-step-8b.md §AC-13 | PRESENT — seeds malformed row, asserts `parse_failure_count >= 1` in JSON payload |
| AC-15: force=false still runs step 8b | cycle-review-step-8b.md §AC-15 | PRESENT — calls twice, asserts graph_edges count identical after second call |
| AC-11: recency cap 101-row boundary | briefing-blending.md §recency cap | PRESENT — seeds 101 rows, oldest has best cosine, asserts its IDs absent from output |
| AC-17: grep shell check | store-v22.md §AC-17 | PRESENT — `grep -r 'schema_version.*== 21' crates/` must return zero matches |
| R-02-contract: UNIQUE conflict not incremented | behavioral-signals.md §R-02-contract | PRESENT — pre-inserts NLI edge, asserts edges_enqueued == 0 |
| I-02: drain flush before graph_edges assertions | test-plan/OVERVIEW.md §drain flush | PRESENT — flush protocol documented; NOTE: since emit uses write_pool_server directly, drain flush is noted as unnecessary (consistent with pseudocode choice, but this conflicts with RISK-TEST-STRATEGY I-02 if Check 10 is reverted) |
| E-02: self-pair exclusion | behavioral-signals.md §test_build_coaccess_pairs_self_pairs_excluded | PRESENT — all same ID → empty pairs |
| I-04: empty current_goal → cold-start before DB call | briefing-blending.md §Guard A empty goal | PRESENT — asserts `get_cycle_start_goal_embedding` NOT called |

**Secondary note on drain flush**: The test plans are consistent with the direct-write pseudocode choice (no drain flush needed for graph_edges). If Check 10 is resolved by reverting to `enqueue_analytics`, the test plans must be updated to add drain flush steps. This is a forward dependency, not a current failure.

---

### Critical Check 12: PAIR_CAP Enforced at Enumeration Time

**Status**: PASS

**Evidence**: `behavioral-signals.md` `build_coaccess_pairs` algorithm step 4.b.vii:
```
If pairs.len() == PAIR_CAP:
    cap_hit = true; return (pairs, cap_hit).
```
The comment explicitly states: "(Cap enforced at enumeration time — not post-hoc truncation. NFR-04.)" and "Note on cap: when pairs.len() == PAIR_CAP, return immediately. The inner loops do NOT continue generating pairs that would then be discarded." The unit test `test_build_coaccess_pairs_cap_enforced_at_200` asserts pairs length == exactly 200 (not 300 then truncated).

---

### Critical Check 13: InferenceConfig Fields Read from Config

**Status**: PASS

**Evidence**: Three new `InferenceConfig` fields are defined in store-v22.md with `#[serde(default)]` and default functions. All three are accessed in `briefing-blending.md` via `config.goal_cluster_similarity_threshold`, `config.w_goal_cluster_conf`, `config.w_goal_boost` where `config` is `Arc<InferenceConfig>`. The OVERVIEW.md explicitly states: "These are read at call time from `Arc<InferenceConfig>` — they are NOT constants in `behavioral_signals.rs`."

---

### Knowledge Stewardship — Pseudocode Agent (crt-046-agent-1-pseudocode-report.md)

**Status**: PASS

**Evidence**: Report contains `## Knowledge Stewardship` section with:
- `Queried:` entries from `context_briefing` (15 entries returned) including #4041 (write_graph_edge contract), #4108 (behavioral co-access pair pattern), ADR entries.
- `Queried:` entries from `context_search` for behavioral signal patterns.
- Deviations documented, including the `write_graph_edge` direct write decision (pattern #4041 application).
- No `Stored:` entry present but this is a read-only agent per gate rules — `Queried:` entries are the required evidence.

---

### Knowledge Stewardship — Test Plan Agent (crt-046-agent-2-testplan-report.md)

**Status**: PASS

**Evidence**: Report contains `## Knowledge Stewardship` section with:
- `Queried:` entries from `context_briefing` (entries #4114, #4108, #3004, ADRs #4110, #4111, #4115).
- `Queried:` from `context_search` (analytics drain, entry #4114).
- `Stored:` entry present: "nothing novel to store — the test patterns in this plan follow established conventions from entries #4114, #4108, #3004. The only new pattern is the v21 fixture creation approach (programmatic DDL seeding), which is a standard migration test technique already documented in #3894. No novel technique emerged." Reason is present and specific.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Check 10: graph_edges write path — `write_pool_server()` used in pseudocode instead of `enqueue_analytics()` as specified in ARCHITECTURE.md, SPEC FR-06/FR-07, and IMPLEMENTATION-BRIEF resolved decisions. No ADR covers this departure. Human resolution required. | uni-architect (or human) | Either: (A) generate ADR-006 approving direct `write_pool_server()` for behavioral graph edges (because `enqueue_analytics` cannot satisfy the write_graph_edge return contract), update ARCHITECTURE.md and SPEC footnotes to reference ADR-006, and update RISK-TEST-STRATEGY I-02 to note drain flush is not needed for behavioral edges; OR (B) remove `edges_enqueued` counter accuracy requirement, revert to `enqueue_analytics`, and accept approximate counting. If (B), also update test plans to add drain flush steps for all graph_edges assertions. |

---

## Warnings (Non-Blocking)

1. **Check 8 / AC-17 pre-existing comment match**: `crates/unimatrix-store/tests/migration_v19_v20.rs` contains a comment "Assert: schema_version == 21 (v19→v20→v21 migration chain runs in full)." This will match `grep -r 'schema_version.*== 21' crates/`. The delivery agent should update this comment to "schema_version >= 21" for correctness and to ensure AC-17 returns zero matches.

2. **RISK-TEST-STRATEGY I-02 vs pseudocode**: RISK-TEST-STRATEGY says "integration tests querying graph_edges must force a drain flush." Test plans have updated this guidance to say drain flush is not needed (because direct write is used). If Check 10 is resolved by keeping direct write (Option A), the RISK-TEST-STRATEGY I-02 text should be updated. If resolved by reverting to `enqueue_analytics` (Option B), the test plans must add drain flush steps.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the gate failure pattern (pseudocode departing from source-document-specified write path without a new ADR) is a recurring validation finding already captured in the project's validation patterns. The specific check (write_graph_edge return contract vs drain fire-and-forget) is feature-specific and belongs in the gate report, not as a lesson-learned pattern. No novel systemic pattern emerged from this gate that would benefit future features beyond what entry #4041 already documents.

---

## Gate 3a Iteration 1 Recheck

> Date: 2026-04-04
> Result: REWORKABLE FAIL

### Re-check Scope

Iteration 1 was tasked with re-validating Critical Check 10 (graph_edges write path contradiction) after ADR-006 was generated at `product/features/crt-046/architecture/ADR-006-behavioral-graph-edge-direct-write.md` and ARCHITECTURE.md was updated. Also checked whether the two prior WARNs (Check 8 / AC-17 pre-existing comment, and RISK-TEST-STRATEGY I-02) were affected.

---

### Item 1 — ADR-006 Existence and Content

**Status**: PASS

ADR-006 exists at `product/features/crt-046/architecture/ADR-006-behavioral-graph-edge-direct-write.md`. Content is complete and correct:
- Documents the structural incompatibility: `enqueue_analytics` is fire-and-forget, returns `()`, cannot provide `rows_affected()` feedback required by pattern #4041.
- Evaluates Option A (direct `write_pool_server()`) and Option B (revert to drain, approximate counter).
- Documents why Option B was eliminated (pattern #4041 is non-negotiable — it was introduced specifically to prevent the crt-040 Gate 3a regression class).
- Cites precedent: ADR-003 (entry #3000) and entry #3883 for structural-write pattern.
- Decision section is unambiguous: `enqueue_analytics(AnalyticsWrite::GraphEdge)` is NOT used for behavioral edge emission.
- Consequence section explicitly addresses integration test drain flush behaviour (I-02): drain flush not needed for step 8b behavioral edge assertions.
- Stored in Unimatrix as entry #4124.

---

### Item 2 — ARCHITECTURE.md Reflects ADR-006

**Status**: WARN (partial — two sections correctly updated, two residual stale references remain)

**Correctly updated sections**:
- §Technology Decisions (lines 237–245): explicitly states `emit_behavioral_edges` uses `write_pool_server()` directly, explains why `enqueue_analytics` cannot satisfy pattern #4041, references ADR-006.
- §Integration Surface (lines 306–309): `graph_edges.source` row correctly annotates `NOT enqueue_analytics — ADR-006`. `write_graph_edge` return contract table is present.
- Component Interactions diagram (line 208): `write_graph_edge(store, ...) × 2N [direct write_pool_server(); NOT analytics drain — ADR-006]` is correct.

**Residual stale references** (introduced by incomplete cleanup when ADR-006 was added):

| Location | Stale text | Issue |
|----------|------------|-------|
| Component 1 description, lines 38–43 | `emit_behavioral_edges(store, pairs, weight) — enqueue AnalyticsWrite::GraphEdge for both directions` | Should say: writes via `write_graph_edge` (direct `write_pool_server()`); see ADR-006 |
| Integration Points "Existing code consumed" table, line 276 | `store.enqueue_analytics(GraphEdge)` → `behavioral edge emission` | `enqueue_analytics` is NOT used for behavioral edges per ADR-006; this row should be removed or the usage column corrected |
| Same table, line 282 | `AnalyticsWrite::GraphEdge` → `fire-and-forget edge enqueue` | Same issue; this enum variant is no longer used for behavioral edge emission |

Additionally, OQ-3 (lines 393–402) remains open despite being definitively resolved by ADR-006. It should be closed with a note that ADR-006 is the resolution. This is a documentation debt, not a logical contradiction, since it is an open-questions section.

**Assessment**: The stale Component 1 description is a logical contradiction within ARCHITECTURE.md itself — the authoritative §Technology Decisions section says "NOT analytics drain" while the Component 1 description still says "enqueue `AnalyticsWrite::GraphEdge`". A delivery agent reading only Component 1 would implement the wrong write path. This qualifies as a WARN rather than FAIL because ADR-006 and §Technology Decisions unambiguously resolve the question, but it is a rework item.

---

### Item 3 — Pseudocode Consistency with ADR-006

**Status**: PASS

`product/features/crt-046/pseudocode/behavioral-signals.md` is fully consistent with ADR-006 throughout:
- Module-private helper `write_graph_edge` explicitly states: "Execute directly on `store.write_pool_server()` (not analytics drain — must return bool)".
- `emit_behavioral_edges` repeat-includes the `write_graph_edge` return contract table.
- The drain flush note correctly states: "emit_behavioral_edges uses `write_graph_edge` (direct write_pool_server), not `enqueue_analytics`, so the drain is NOT involved here. Tests querying `graph_edges` immediately after `emit_behavioral_edges` do NOT need a drain flush."
- No use of `enqueue_analytics` anywhere in behavioral-signals.md.

---

### Item 4 — write_graph_edge Return Contract Table in Pseudocode

**Status**: PASS (confirmed from prior run, re-confirmed)

The contract table appears twice in `behavioral-signals.md`: once as a module-level preamble (§write_graph_edge Return Contract — MUST READ FIRST) and once inline in `emit_behavioral_edges`. This satisfies pattern #4041. Counter increments key off `Ok(true)` only in both locations.

---

### Item 5 — Prior Warning Re-check

**Check 8 / AC-17 pre-existing comment match** (WARN from prior run):
**Status**: WARN — unchanged, still present. The comment in `crates/unimatrix-store/tests/migration_v19_v20.rs` referencing `schema_version == 21` still exists in the codebase. No changes to that file were made as part of the ADR-006 work (as expected — this warning is unrelated to Check 10). Delivery agent must still address it.

**RISK-TEST-STRATEGY I-02 update**:
**Status**: WARN — unchanged, still present. The RISK-TEST-STRATEGY document still says "integration tests querying graph_edges must force a drain flush." ADR-006 Consequences section explicitly documents that I-02's drain flush requirement does not apply to behavioral edge assertions. However, the RISK-TEST-STRATEGY itself was not updated to reflect this. The test plans are already consistent with the direct-write approach. Delivery agent should add a footnote to RISK-TEST-STRATEGY I-02 referencing ADR-006.

---

### Updated Gate Result

The original FAIL (Check 10) is resolved by ADR-006, but the architecture cleanup is incomplete: three stale `enqueue_analytics`/`AnalyticsWrite::GraphEdge` references remain in ARCHITECTURE.md Component 1 description and the Integration Points table. The Component 1 description contradiction (says "enqueue `AnalyticsWrite::GraphEdge`" while §Technology Decisions says the opposite) is a delivery risk — a developer reading Component 1 could implement the wrong path.

| Check | Status | Notes |
|-------|--------|-------|
| ADR-006 exists and is complete | PASS | Clear rationale, Option A/B analysis, pattern #4041 citation, Unimatrix #4124 |
| ARCHITECTURE.md §Technology Decisions + §Integration Surface | PASS | Correctly updated to reference ADR-006 |
| ARCHITECTURE.md Component 1 description | WARN | Still says "enqueue `AnalyticsWrite::GraphEdge`" — stale; contradicts §Technology Decisions |
| ARCHITECTURE.md Integration Points table | WARN | Two rows still list `enqueue_analytics`/`AnalyticsWrite::GraphEdge` for behavioral edges |
| Pseudocode (behavioral-signals.md) | PASS | Fully consistent with ADR-006; no `enqueue_analytics` usage |
| Return contract table present in pseudocode | PASS | Present twice (preamble + inline) |
| Prior WARN: AC-17 comment | WARN | Unchanged; still delivery agent's responsibility |
| Prior WARN: RISK-TEST-STRATEGY I-02 | WARN | Unchanged; ADR-006 resolves the intent but the strategy doc not updated |

**Overall Result: REWORKABLE FAIL**

The core FAIL (Check 10 — no ADR covering the write path) is resolved. However, the incomplete ARCHITECTURE.md cleanup introduces a new document-internal contradiction at Component 1 that must be fixed before Gate 3b.

### Rework Required (Iteration 1)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| ARCHITECTURE.md Component 1 `emit_behavioral_edges` description still says "enqueue `AnalyticsWrite::GraphEdge`" | uni-architect | Update lines 38–43 to describe direct `write_graph_edge` (write_pool_server), not analytics drain. Reference ADR-006. |
| ARCHITECTURE.md Integration Points "Existing code consumed" table still lists `store.enqueue_analytics(GraphEdge)` and `AnalyticsWrite::GraphEdge` for behavioral edge emission | uni-architect | Remove or correct these two rows; `enqueue_analytics` is not consumed for behavioral edges per ADR-006 |
| ARCHITECTURE.md OQ-3 still open | uni-architect | Close OQ-3 with note: "Resolved by ADR-006 — direct `write_pool_server()` is used; `enqueue_analytics` is not suitable. See §Technology Decisions." |
| RISK-TEST-STRATEGY I-02 drain flush requirement not updated | uni-architect or test-plan agent | Add footnote: "For behavioral graph edge assertions (step 8b), drain flush is not required — edges are written via direct write_pool_server(); see ADR-006." |

### Knowledge Stewardship (Iteration 1)

- Stored: nothing novel to store -- incomplete architecture document cleanup after ADR addition (stale prose in component description vs. correctly updated technology decisions section) is a known validation pattern. The specific finding is feature-local. No new systemic pattern beyond existing lesson-learned entries.

---

## Gate 3a Iteration 2 Recheck (Final)

> Date: 2026-04-04
> Result: PASS

### Re-check Scope

Iteration 2 re-validates the four rework items from iteration 1 plus the carried-over AC-17 pre-existing comment warning.

---

### Item 1 — ARCHITECTURE.md Component 1 description: no stale `enqueue_analytics` / `AnalyticsWrite::GraphEdge` for behavioral emission

**Status**: PASS

Component 1 §emit_behavioral_edges (lines 38–44) now reads:

> "writes both directed edges for each pair directly via the module-private `write_graph_edge` helper...Does NOT use `enqueue_analytics` — the analytics drain is fire-and-forget and cannot satisfy the `write_graph_edge` return contract (ADR-006, Unimatrix #4124)."

No reference to `enqueue_analytics` or `AnalyticsWrite::GraphEdge` as the behavioral emission path anywhere in Component 1.

---

### Item 2 — ARCHITECTURE.md Integration Points table: stale rows removed, correct direct write row present

**Status**: PASS

The "Existing code consumed" table (line 279) now contains one row for this path:

| Component | Usage in crt-046 |
|-----------|-----------------|
| `store.write_pool_server()` | behavioral edge emission via `write_graph_edge` (direct write — NOT analytics drain; ADR-006) |

The two stale rows that previously listed `store.enqueue_analytics(GraphEdge)` and `AnalyticsWrite::GraphEdge` for behavioral edge emission are gone. The Integration Surface table (line 308) retains the correct annotation: "written via `write_graph_edge` helper (NOT `enqueue_analytics` — ADR-006)".

---

### Item 3 — ARCHITECTURE.md OQ-3: closed/resolved

**Status**: PASS

OQ-3 (lines 395–402) reads:

> "RESOLVED by ADR-006 (Unimatrix #4124). Behavioral graph edge writes use `write_pool_server()` directly via the `write_graph_edge` helper. `enqueue_analytics(AnalyticsWrite::GraphEdge)` is NOT used for step 8b emission... See §Technology Decisions and ADR-006 for full rationale."

The open question is definitively closed.

---

### Item 4 — RISK-TEST-STRATEGY I-02: ADR-006 clarification present

**Status**: PASS

RISK-TEST-STRATEGY I-02 (line 275) now contains the clarification paragraph:

> "ADR-006 clarification: Behavioral graph edge writes (`emit_behavioral_edges` / `write_graph_edge`) use `write_pool_server()` directly — not the analytics drain (see ADR-006). Drain flush is therefore NOT required before asserting behavioral `graph_edges` rows in step 8b integration tests (AC-01 extension, AC-15, R-02 contract test). The drain flush requirement in this I-02 entry applies only to tests that assert `graph_edges` rows written by other analytics-drain paths (NLI, co-access) in the same test body."

The strategy document is now consistent with ADR-006 and the pseudocode.

---

### Carried-over Warning: AC-17 pre-existing comment in migration_v19_v20.rs

**Status**: NOTE (not a gate failure)

`crates/unimatrix-store/tests/migration_v19_v20.rs` line 469 contains:

```
// Assert: schema_version == 21 (v19→v20→v21 migration chain runs in full).
```

This file is pre-existing code — it is not a crt-046 pseudocode or test-plan artifact and was not authored by any crt-046 agent. Gate 3a validates design artifacts only; pre-existing code is outside scope. The delivery agent remains responsible for updating this comment (e.g., to `// Assert: schema_version >= 21`) before closing Gate 3b so that AC-17's `grep -r 'schema_version.*== 21' crates/` returns zero matches. This is a delivery-time task, not a design-time failure.

---

### Iteration 2 Summary

| Item | Status | Notes |
|------|--------|-------|
| ARCHITECTURE.md Component 1: no stale `enqueue_analytics` for behavioral emission | PASS | Direct `write_graph_edge` / `write_pool_server()` described; ADR-006 cited |
| ARCHITECTURE.md Integration Points table: stale rows removed | PASS | Single correct row present; two stale rows gone |
| ARCHITECTURE.md OQ-3: closed | PASS | Explicitly resolved with ADR-006 reference |
| RISK-TEST-STRATEGY I-02: ADR-006 clarification present | PASS | Drain-flush scope correctly scoped to non-behavioral drain paths |
| AC-17 pre-existing comment (migration_v19_v20.rs) | NOTE | Pre-existing file; delivery agent must update at implementation time |

All rework items from iteration 1 are resolved. No new issues found.

**Overall Result: PASS**

### Knowledge Stewardship (Iteration 2)

- Stored: nothing novel to store -- this iteration confirmed clean resolution of the architecture document stale-reference pattern. No new systemic finding beyond what was already captured in iteration 0 and 1 reports.
