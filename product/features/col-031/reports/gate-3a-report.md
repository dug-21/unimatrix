# Gate 3a Report: col-031

> Gate: 3a (Design Review)
> Date: 2026-03-28
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 7 component pseudocode files conform to ARCHITECTURE.md component boundaries, interfaces, and ADRs |
| Specification coverage | PASS | All 11 FRs and 8 NFRs covered; no scope additions detected |
| Risk coverage (test plans) | PASS | All 14 risks from RISK-TEST-STRATEGY.md have named tests in test plans |
| Interface consistency | PASS | Shared types in OVERVIEW.md match all per-component pseudocode files |
| Rank formula correctness (item 1) | PASS | `1.0 - ((rank-1) as f32 / N as f32)` present; `1 - rank/N` form absent |
| use_fallback guard order (item 2) | PASS | Guard fires before `phase_affinity_score` call in pre-loop block |
| phase_affinity_score cold-start return (item 3) | PASS | Returns `1.0` when `use_fallback=true`; distinct from fused scoring path |
| Lock acquisition order comment (item 4) | PASS | Named comment present in background_tick.md Change 4 |
| CAST(json_each.value AS INTEGER) form (item 5) | PASS | Present in SELECT, JOIN predicate, and GROUP BY clauses |
| lookback_days bound as i64 (item 6) | PASS | `.bind(lookback_days as i64)` present |
| PhaseFreqTableHandle non-optional at 7 sites (item 7) | PASS | All 7 sites shown as required non-optional parameters |
| AC-16 one-line replay.rs change (item 8) | PASS | Exactly one line: `current_phase: record.context.phase.clone()` |
| Integration harness plan (item 10) | PASS | Present in test-plan/OVERVIEW.md with suite applicability table and 2 new tests |
| Knowledge stewardship — pseudocode agent | PASS | `Queried:` entries present; no `Stored:` needed (read-only agent) |
| Knowledge stewardship — test-plan agent | PASS | `Queried:` and `Stored:` entries present (#3691) |
| Knowledge stewardship — architect agent | PASS | `## Knowledge Stewardship` section added in rework; 5 ADRs (#3685-#3689) listed as `Stored:` entries |
| Knowledge stewardship — spec agent | PASS | `Queried:` entries present |
| Knowledge stewardship — risk agent | PASS | `Queried:` and `Stored: nothing novel` entries present |
| Test plan — 7 Critical/High risks named (item 9) | PASS | R-01 through R-07 all have named tests in OVERVIEW.md risk-to-test mapping |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**: Each pseudocode file maps to the component breakdown in ARCHITECTURE.md §2:

- `pseudocode/phase_freq_table.md` — `services/phase_freq_table.rs` (Architecture §1): struct shape, handle type, `new()`, `new_handle()`, `rebuild()`, `phase_affinity_score()` all match.
- `pseudocode/query_log_store_method.md` — Store method (Architecture §2): `PhaseFreqRow` fields (`phase: String, category: String, entry_id: u64, freq: i64`) match exactly.
- `pseudocode/search_scoring.md` — SearchService (Architecture §5): pre-loop snapshot pattern, `ServiceSearchParams.current_phase` field, non-optional `phase_freq_table` field — all match.
- `pseudocode/background_tick.md` — Background Tick (Architecture §4): three-function signature chain `spawn_background_tick` → `background_tick_loop` → `run_single_tick`; rebuild-after-TypedGraph call; retain-on-error semantics — all match.
- `pseudocode/service_layer.md` — ServiceLayer (Architecture §3): `with_rate_config` creates handle, accessor exposed, handle passed to `SearchService::new` — all match.
- `pseudocode/inference_config.md` — InferenceConfig (Architecture §6): `default_w_phase_explicit` raised to 0.05, `query_log_lookback_days: u32` with default 30, `[1, 3650]` range check in `validate()` — all match.
- `pseudocode/replay_fix.md` — Eval harness fix (Architecture §7): exactly one line in `replay.rs`, no changes to `extract.rs` or `output.rs` — matches architecture constraint.

Technology choices are consistent with ADRs. No deviations from approved architecture.

---

### Check 2: Specification Coverage

**Status**: PASS

**Evidence**:

All 11 functional requirements are present in pseudocode:

| FR | Covered by |
|----|------------|
| FR-01 (PhaseFreqTable struct) | `phase_freq_table.md` structs section |
| FR-02 (PhaseFreqTableHandle) | `phase_freq_table.md` type alias; poison recovery via `.unwrap_or_else` shown at all acquisition sites |
| FR-03 (Store query method) | `query_log_store_method.md` with exact SQL and `lookback_days as i64` binding |
| FR-04 (rebuild) | `phase_freq_table.md` rebuild() section |
| FR-05 (Rank normalization) | `phase_freq_table.md` line 201: `let score = 1.0_f32 - ((rank - 1) as f32 / n as f32)` |
| FR-06 (phase_affinity_score) | `phase_freq_table.md` phase_affinity_score() section; 1.0 on fallback, absent phase, absent entry |
| FR-07 (Background tick) | `background_tick.md` Change 4 |
| FR-08 (ServiceLayer wiring) | `service_layer.md` — all 7 changes |
| FR-09 (Fused scoring) | `search_scoring.md` Changes 5 and 6 |
| FR-10 (InferenceConfig) | `inference_config.md` all changes |
| FR-11 (Eval harness fix) | `replay_fix.md` — one-line change |

All 8 NFRs are addressed:

| NFR | Addressed by |
|-----|-------------|
| NFR-01 (≤500 lines) | `phase_freq_table.md` header notes max 500 lines; SQL in store crate |
| NFR-02 (lock-hold discipline) | `search_scoring.md` Change 5: guard drops before scoring loop |
| NFR-03 (lock acquisition order) | `background_tick.md` Change 4: explicit named comment |
| NFR-04 (cold-start score identity) | `search_scoring.md` test `test_scoring_score_identity_cold_start` |
| NFR-05 (AC-12/AC-16 hard constraint) | `replay_fix.md` notes non-separability; test plan OVERVIEW.md reiterates |
| NFR-06 (no PPR scaffolding) | No PPR internals appear in any pseudocode file |
| NFR-07 (no schema migrations) | No migration pseudocode present |
| NFR-08 (poison recovery) | `.unwrap_or_else(|e| e.into_inner())` shown at all lock sites |

No scope additions detected (no PPR internals, no schema changes, no diagnostic endpoints).

---

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**: OVERVIEW.md risk-to-test mapping table covers all 14 risks. For the 7 Critical/High risks explicitly required by item 9:

| Risk | Priority | Named Test |
|------|----------|-----------|
| R-01 | Critical | `test_run_single_tick_propagates_phase_freq_handle`; `cargo build --workspace`; grep audit |
| R-02 | Critical | `test_replay_forwards_current_phase_to_service_search_params`; eval output inspection |
| R-03 | High | `test_scoring_use_fallback_true_sets_phase_explicit_norm_zero`; `test_scoring_score_identity_cold_start` |
| R-04 | High | `test_phase_affinity_score_use_fallback_returns_one` (and 2 more absent-entry tests) |
| R-05 | High | `test_query_phase_freq_table_returns_correct_entry_id` (TestDb) |
| R-06 | High | `test_scoring_lock_released_before_scoring_loop`; code review |
| R-07 | High | `test_phase_affinity_score_single_entry_bucket_returns_one`; `test_rebuild_normalization_three_entry_bucket_exact_scores` |
| R-14 | High | `cargo build --workspace`; grep audit of 7 sites |

Each risk has at least one concrete named test.

---

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**: OVERVIEW.md defines shared types. Verification of usage across component files:

| Type | Defined in OVERVIEW.md | Per-component usage |
|------|----------------------|---------------------|
| `PhaseFreqTable { table: HashMap<(String,String), Vec<(u64,f32)>>, use_fallback: bool }` | Yes | `phase_freq_table.md` matches exactly |
| `PhaseFreqTableHandle = Arc<RwLock<PhaseFreqTable>>` | Yes | All 4 consuming components use this type |
| `PhaseFreqRow { phase: String, category: String, entry_id: u64, freq: i64 }` | Yes | `query_log_store_method.md` matches exactly |
| `ServiceSearchParams.current_phase: Option<String>` | Yes | `search_scoring.md`, `replay_fix.md` both reference this field correctly |
| `InferenceConfig { w_phase_explicit: f64 (0.05), query_log_lookback_days: u32 (30) }` | Yes | `inference_config.md`, `background_tick.md` both reference `inference_config.query_log_lookback_days` |

No contradictions found between component pseudocode files. Data flow in OVERVIEW.md matches the component interaction descriptions throughout.

---

### Check 5: Explicit Verification Items

#### Item 1: Rank formula

**Status**: PASS

`phase_freq_table.md` line 201:
```
let score = 1.0_f32 - ((rank - 1) as f32 / n as f32);
```
Single-entry (N=1, rank=1): `1.0 - (0/1) = 1.0`. Correct. The prohibited `1 - rank/N` form is absent.

The formula is also correctly shown in OVERVIEW.md data flow: `score = 1.0 - ((rank-1) / N)`.

#### Item 2: use_fallback guard order

**Status**: PASS

`search_scoring.md` Change 5 pre-loop block:
```
if guard.use_fallback {
    // GUARD FIRES: cold-start; do NOT call phase_affinity_score.
    None
    // guard drops here — lock released
} else {
    // Extract snapshot ...
```
The `use_fallback` guard fires before any call to `phase_affinity_score`. When `use_fallback = true`, `None` is returned and the lock is released — `phase_affinity_score` is never called on this path.

#### Item 3: phase_affinity_score cold-start return

**Status**: PASS

`phase_freq_table.md` `phase_affinity_score()` body:
```
if self.use_fallback {
    return 1.0;
}
```
This is the distinct PPR-contract path. The fused scoring path never calls this method when `use_fallback = true` (guarded in Change 5). The two behaviors are properly distinct.

#### Item 4: Lock acquisition order comment

**Status**: PASS

`background_tick.md` Change 4 contains:
```
// LOCK ACQUISITION ORDER in run_single_tick (SR-07, NFR-03):
//   1. EffectivenessStateHandle  -- acquired and released during maintenance_tick above
//   2. TypedGraphStateHandle     -- acquired and released in the block above this one
//   3. PhaseFreqTableHandle      -- acquired here (write, swap only)
```
Exact names match the required order from ARCHITECTURE.md §Cross-Cutting Concerns.

#### Item 5: CAST(json_each.value AS INTEGER)

**Status**: PASS

`query_log_store_method.md` SQL:
- Line 90: `CAST(je.value AS INTEGER)  AS entry_id` (SELECT clause)
- Line 94: `JOIN entries e ON CAST(je.value AS INTEGER) = e.id` (JOIN predicate)
- Line 98: `GROUP BY q.phase, e.category, CAST(je.value AS INTEGER)` (GROUP BY clause)

The CAST form is present in all three required locations.

#### Item 6: lookback_days bound as i64

**Status**: PASS

`query_log_store_method.md` line 104:
```
.bind(lookback_days as i64)
```
Matches the architecture requirement.

#### Item 7: PhaseFreqTableHandle non-optional at 7 sites

**Status**: PASS

All 7 sites shown in pseudocode with non-optional type:
1. `SearchService::new()` — `search_scoring.md` Change 4: required `phase_freq_table: PhaseFreqTableHandle` parameter, no `Option<>`
2. `run_single_tick` — `background_tick.md` Change 3: `phase_freq_table: &PhaseFreqTableHandle`, no `Option<>`
3. `background_tick_loop` — `background_tick.md` Change 2: `phase_freq_table: PhaseFreqTableHandle`, no `Option<>`
4. `spawn_background_tick` — `background_tick.md` Change 1: `phase_freq_table: PhaseFreqTableHandle`, no `Option<>`
5. `ServiceLayer.with_rate_config` — `service_layer.md` Change 5: `Arc::clone(&phase_freq_table)` passed, no `Option<>`
6. `ServiceLayer` struct field — `service_layer.md` Change 3: `phase_freq_table: PhaseFreqTableHandle`, no `Option<>`
7. Test helpers — `search_scoring.md` wiring site checklist lists `server.rs`, `shutdown.rs`, `test_support.rs`, `listener.rs`, `eval/profile/layer.rs` all receiving `current_phase: None` (the struct field update from ADR-005)

#### Item 8: AC-16 one-line replay.rs change

**Status**: PASS

`replay_fix.md` specifies the complete diff as exactly:
```diff
+        current_phase: record.context.phase.clone(),  // col-031: AC-16 — forward phase to scoring
```
No other lines changed. The pseudocode explicitly prohibits changes to `extract.rs` and `output.rs`.

#### Item 9: Test plans cover 7 Critical/High risks

**Status**: PASS

Covered in Check 3 above. All 7 Critical/High risks (R-01, R-02, R-03, R-04, R-05, R-06, R-07) plus R-14 (also High) have named tests in per-component test plans and in the OVERVIEW.md risk-to-test mapping.

#### Item 10: Integration harness plan

**Status**: PASS

`test-plan/OVERVIEW.md` contains:
- Suite applicability table (smoke, tools, lifecycle, edge_cases: YES; confidence, contradiction, security, protocol, volume, adaptation: NO with reasons)
- Two new integration tests identified for `suites/test_lifecycle.py`
- Gap analysis explaining why AC-08 does not need an infra-001 test
- Suite commands for Stage 3c

---

### Check 6: Knowledge Stewardship Compliance

**Status**: PASS (fixed in rework iteration 1)

**Evidence**:

| Agent | Report | Stewardship Block | Finding |
|-------|--------|-------------------|---------|
| col-031-agent-1-architect | `agents/col-031-agent-1-architect-report.md` | PRESENT (added in rework) | PASS — 5 ADRs listed as `Stored:` entries: #3685 (rank normalization), #3686 (time-based retention), #3687 (two cold-start contracts), #3688 (activate w_phase_explicit), #3689 (required handle threading) |
| col-031-agent-2-spec | `agents/col-031-agent-2-spec-report.md` | Present | PASS — `Queried:` entries listed (15 entries from context_briefing) |
| col-031-agent-3-risk | `agents/col-031-agent-3-risk-report.md` | Present | PASS — `Queried:` entries listed; `Stored: nothing novel` with reason |
| col-031-agent-1-pseudocode | `agents/col-031-agent-1-pseudocode-report.md` | Present | PASS — `Queried:` entries listed (read-only, no `Stored:` needed) |
| col-031-agent-2-testplan | `agents/col-031-agent-2-testplan-report.md` | Present | PASS — `Queried:` and `Stored:` entries (entry #3691) |
| col-031-vision-guardian | `agents/col-031-vision-guardian-report.md` | Present | PASS — `Queried:` and `Stored: nothing novel` with reason |

All 6 agent reports now have compliant `## Knowledge Stewardship` sections.

---

### Minor Finding: Test Name/Behavior Mismatch (WARN)

**Status**: WARN

In `test-plan/search_scoring.md`, the test `test_scoring_absent_entry_in_snapshot_norm_is_zero` has a name implying absent-entry returns `0.0`, but the test body note clarifies that absent entries in a populated non-fallback table return `1.0` (neutral) per AC-07 and the architecture. The test name is misleading relative to the expected behavior.

This does not block delivery — the note in the test plan corrects the intent — but the implementation agent should name the test `test_scoring_absent_entry_in_snapshot_returns_neutral` or similar to avoid confusion at Gate 3b.

---

## Rework Required

None.

---

## Rework History

| Iteration | Issue | Resolution |
|-----------|-------|------------|
| 1 | `## Knowledge Stewardship` section absent from architect report | Added in rework — 5 ADRs (#3685-#3689) listed as `Stored:` entries |

---

## Knowledge Stewardship

- Stored: nothing novel to store — stewardship omission in architect report is feature-specific; existing gate rules cover this pattern. No recurring cross-feature pattern identified.
