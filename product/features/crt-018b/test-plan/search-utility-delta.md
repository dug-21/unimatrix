# Component Test Plan: search-utility-delta

**Source**: `crates/unimatrix-server/src/services/search.rs` (modified)
**Risk coverage**: R-01 (Critical), R-02 (Critical), R-05 (High), R-06 (High), R-07 (High), R-14 (Medium)

---

## Unit Test Expectations

All tests in `#[cfg(test)] mod tests` within `services/search.rs` (cumulative extension of existing test module). Extend tests in `services/confidence.rs` test module for the snapshot pattern precedent.

### AC-03 / AC-04 / AC-16 — utility_delta Pure Function

**Test**: `test_utility_delta_effective`
- Call `utility_delta(Some(EffectivenessCategory::Effective))`
- Assert return value `== 0.05_f64` (exactly, as UTILITY_BOOST constant)

**Test**: `test_utility_delta_settled`
- Call `utility_delta(Some(EffectivenessCategory::Settled))`
- Assert return value `== 0.01_f64` (exactly, as SETTLED_BOOST constant)

**Test**: `test_utility_delta_ineffective`
- Call `utility_delta(Some(EffectivenessCategory::Ineffective))`
- Assert return value `== -0.05_f64` (exactly, as -UTILITY_PENALTY)

**Test**: `test_utility_delta_noisy`
- Call `utility_delta(Some(EffectivenessCategory::Noisy))`
- Assert return value `== -0.05_f64`

**Test**: `test_utility_delta_unmatched_zero`
- Call `utility_delta(Some(EffectivenessCategory::Unmatched))`
- Assert return value `== 0.0_f64`

**Test**: `test_utility_delta_none_zero` (AC-06, R-07)
- Call `utility_delta(None)`
- Assert return value `== 0.0_f64`
- No panic, no default-to-penalty

**Test**: `test_utility_delta_noisy_equals_ineffective_penalty`
- Assert `utility_delta(Some(Noisy)) == utility_delta(Some(Ineffective))`
- Documents the intentional symmetry

### AC-05 / R-02 — Effective Outranks Near-Equal Ineffective

**Test**: `test_effective_outranks_ineffective_at_close_similarity`
- Input: two entries, confidence_weight = 0.15 (floor)
  - Entry A: sim = 0.75, conf = 0.60, category = Effective
  - Entry B: sim = 0.76, conf = 0.60, category = Ineffective
- Compute scores manually: A = (0.85*0.75 + 0.15*0.60 + 0.05) * 1.0, B = (0.85*0.76 + 0.15*0.60 - 0.05) * 1.0
- Assert score_A > score_B
- This is the concrete assertion from the SPECIFICATION FR-06 example

**Test**: `test_effective_outranks_ineffective_at_max_weight`
- Repeat with confidence_weight = 0.25 (ceiling)
- Assert same ordering holds at both spread extremes (R-14 unit-level check)

### R-05 — Utility Delta Inside Status Penalty Multiplication (ADR-003)

**Test**: `test_utility_delta_inside_deprecated_penalty`
- Entry: status = Deprecated, category = Effective
- Manual calculation: `(rerank_score(sim, conf, cw) + UTILITY_BOOST) * DEPRECATED_PENALTY`
- The final score must use the formula with delta inside the multiplication
- Assert computed score equals `(base + 0.05) * 0.7`, not `base * 0.7 + 0.05`
- Numerical difference: with sim=0.75, conf=0.60, cw=0.15 → base=0.7275
  - Correct: (0.7275 + 0.05) * 0.7 = 0.5443
  - Wrong:   0.7275 * 0.7 + 0.05 = 0.5593
  - Delta: 0.015 — assert difference from the wrong formula is non-negligible

**Test**: `test_utility_delta_inside_superseded_penalty`
- Entry: superseded, category = Noisy
- Assert `(base - 0.05) * 0.5` not `base * 0.5 - 0.05`

### R-02 — All Four rerank_score Call Sites Apply the Delta

The four call sites in `search.rs` (based on reading the current code) are:
1. Step 7 sort: `base_a = rerank_score(*sim_a, entry_a.confidence, confidence_weight) + prov_a` → delta added here alongside prov
2. Step 7 sort: `base_b = rerank_score(*sim_b, entry_b.confidence, confidence_weight) + prov_b` → symmetric
3. Step 8 co-access re-sort: `base_a = rerank_score(*sim_a, entry_a.confidence, confidence_weight)` → delta must be added here
4. Step 8 co-access re-sort: `base_b = rerank_score(*sim_b, entry_b.confidence, confidence_weight)` → symmetric
5. Step 11 final_score: `rerank_score(*sim, entry.confidence, confidence_weight) * penalty` → delta must appear here too

Each call site must include the utility delta. The tests below validate call site coverage by constructing a scenario where the delta changes the observable output at each stage.

**Test**: `test_all_four_rerank_sites_apply_delta_step7`
- Construct a search result where the Effective entry would lose in Step 7 without the delta
- Assert it wins after the delta is applied in Step 7
- (Uses a unit-level scoring test, not a full search pipeline test)

**Test**: `test_all_four_rerank_sites_apply_delta_step8`
- Construct a result set where the co-access re-sort would produce wrong ordering without delta
- Assert correct ordering after Step 8 re-sort with delta

**Code Review Checklist** (not an automated test, but required for Stage 3c):
- Count `rerank_score(` occurrences in `search.rs` (expected: exactly 5 per SPECIFICATION counting both comparators in each sort)
- For each occurrence, confirm `utility_delta(categories.get(&entry.id).copied())` is present in the same score expression

### R-06 — Generation Cache Shared Across rmcp Clones

**Test**: `test_cached_snapshot_shared_across_search_service_clones`
- Create `SearchService` instance S1
- Clone it: S2 = S1.clone()
- S1 and S2 should reference the same `Arc<Mutex<EffectivenessSnapshot>>`
- Update via the backing `EffectivenessStateHandle` (bump generation)
- Call the snapshot update logic on S1
- Assert S2 sees the new generation on its next snapshot check (because they share the `Arc`)
- This confirms `cached_snapshot` is `Arc<Mutex<EffectivenessSnapshot>>`, not a plain `EffectivenessSnapshot`

**Test**: `test_cached_snapshot_type_is_arc_mutex`
- Structural: assert that the field type is `Arc<Mutex<EffectivenessSnapshot>>`
- In practice: verify the clone behavior described above is possible (if it compiles with `Arc<Mutex<_>>`, it passes)

### R-01 — Lock Ordering (Read guard dropped before Mutex)

**Test**: `test_snapshot_read_guard_dropped_before_mutex_lock`
- This is the core lock-ordering safety test for ADR-001
- Simulates the snapshot pattern:
  ```rust
  let generation_seen = {
      let guard = effectiveness_state.read().unwrap_or_else(|e| e.into_inner());
      guard.generation
      // guard is dropped here — MUST happen before cached_snapshot.lock()
  };
  let mut snap = cached_snapshot.lock().unwrap_or_else(|e| e.into_inner());
  ```
- Verify that the read guard goes out of scope (block ends) before the mutex acquisition
- Test implementation: use `std::sync::Mutex::try_lock()` in a separate thread to confirm the mutex is free while the read guard is still held — then confirm it acquires after the block ends
- Use `#[tokio::test]` for the async context

### R-07 / AC-06 — Empty State Produces Zero Delta

**Test**: `test_search_with_empty_effectiveness_state_no_panic`
- Construct minimal search pipeline with empty `EffectivenessState`
- Assert all utility deltas are 0.0 (no panic, no default-to-penalty)
- Result ordering must match the pre-crt-018b formula exactly (same as confidence_weight * confidence + (1 - confidence_weight) * similarity)

### Flexible vs Strict Mode

**Test**: `test_utility_delta_not_applied_in_strict_mode`
- Construct a search with `RetrievalMode::Strict`
- Assert the scoring does not use utility_delta (Strict mode hard-filters to Active-only, delta is Flexible-only)
- Verify by comparing result with an equivalent Flexible-mode call: the Flexible call should differ when an entry has a non-zero delta

---

## Integration Test Expectations

The search ordering from the MCP interface is tested via AC-17 item 1 and item 4 in `test_lifecycle.py`. See OVERVIEW.md for the specific test plan.

**Observable assertion through MCP**: `context_search` response ordering changes after background tick writes classifications. Pre-tick: ordering is confidence/similarity only. Post-tick: Effective entries appear higher for equal-confidence results.

---

## Edge Cases

| Scenario | Expected | Test Type |
|----------|----------|-----------|
| Generation unchanged between calls | No HashMap clone; snapshot reused | Unit (generation cache skip) |
| Generation bumped by tick | Clone performed once; subsequent calls reuse | Unit |
| Entry IDs in categories but not in results (HNSW miss) | No delta applied for absent entry | Unit (already covered by None → 0.0) |
| Effective entry has no similarity match (score near 0) | Delta still applied; still near 0 but slightly higher | Unit |
| All entries Effective (uniform delta) | Relative ordering unchanged (uniform boost cancels) | Unit |
