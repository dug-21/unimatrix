# Component Test Plan: briefing-tiebreaker

**Source**: `crates/unimatrix-server/src/services/briefing.rs` (modified)
**Risk coverage**: R-06 (High), R-07 (High), R-09 (Medium)

---

## Unit Test Expectations

All tests in `#[cfg(test)] mod tests` within `services/briefing.rs` (extend existing test module if present, or create new inline module).

### AC-07 / R-09 — effectiveness_priority Pure Function

**Test**: `test_effectiveness_priority_effective`
- Call `effectiveness_priority(Some(EffectivenessCategory::Effective))`
- Assert return value `== 2_i32`

**Test**: `test_effectiveness_priority_settled`
- Call `effectiveness_priority(Some(EffectivenessCategory::Settled))`
- Assert return value `== 1_i32`

**Test**: `test_effectiveness_priority_unmatched`
- Call `effectiveness_priority(Some(EffectivenessCategory::Unmatched))`
- Assert return value `== 0_i32`

**Test**: `test_effectiveness_priority_none`
- Call `effectiveness_priority(None)`
- Assert return value `== 0_i32` (neutral, not negative — R-07 guard for briefing)

**Test**: `test_effectiveness_priority_ineffective`
- Call `effectiveness_priority(Some(EffectivenessCategory::Ineffective))`
- Assert return value `== -1_i32`

**Test**: `test_effectiveness_priority_noisy`
- Call `effectiveness_priority(Some(EffectivenessCategory::Noisy))`
- Assert return value `== -2_i32`

**Test**: `test_effectiveness_priority_noisy_lower_than_ineffective`
- Assert `effectiveness_priority(Some(Noisy)) < effectiveness_priority(Some(Ineffective))`
- Documents the canonical ordering: Noisy is the lowest priority in briefing

### AC-07 / R-09 — Injection History Sort: Confidence is Primary Key

The critical property is that confidence is primary and effectiveness is only the tiebreaker.

**Test**: `test_injection_sort_confidence_is_primary_key`
- Entry A: confidence = 0.90, category = Ineffective
- Entry B: confidence = 0.40, category = Effective
- Assert A ranks first in the sorted output (high confidence beats high effectiveness)
- This directly tests R-09: effectiveness must NOT be the primary sort key

**Test**: `test_injection_sort_effectiveness_is_tiebreaker`
- Entry A: confidence = 0.60, category = Ineffective (priority = -1)
- Entry B: confidence = 0.60, category = Effective (priority = 2)
- Assert B ranks first (equal confidence → effectiveness tiebreaker applies)
- This tests AC-07: the tiebreaker fires correctly when confidence is equal

**Test**: `test_injection_sort_equal_confidence_equal_effectiveness`
- Entry A: confidence = 0.60, category = Effective
- Entry B: confidence = 0.60, category = Effective
- Assert sort is stable (both have priority = 2, no preference)

**Test**: `test_injection_sort_three_entries_mixed`
- Entry A: confidence = 0.70, category = Effective
- Entry B: confidence = 0.80, category = Ineffective
- Entry C: confidence = 0.70, category = Ineffective
- Expected order: B (0.80, prio=-1), A (0.70, prio=2), C (0.70, prio=-1)
- Validates: higher confidence wins; equal confidence → Effective before Ineffective

### AC-08 — Convention Sort Tiebreaker

The convention sort has three keys: feature_tag first, then confidence, then effectiveness.

**Test**: `test_convention_sort_feature_tag_overrides_effectiveness`
- Entry A: feature_tag = Some("crt-018b"), confidence = 0.30, category = Ineffective
- Entry B: feature_tag = None, confidence = 0.90, category = Effective
- Assert A ranks first (feature_tag takes precedence over both confidence and effectiveness)

**Test**: `test_convention_sort_confidence_before_effectiveness_no_feature`
- Entry A: feature_tag = None, confidence = 0.90, category = Ineffective
- Entry B: feature_tag = None, confidence = 0.40, category = Effective
- Assert A ranks first (confidence is second key when no feature differentiation)

**Test**: `test_convention_sort_effectiveness_tiebreaker_no_feature`
- Entry A: feature_tag = None, confidence = 0.60, category = Ineffective
- Entry B: feature_tag = None, confidence = 0.60, category = Effective
- Assert B ranks first (effectiveness is tiebreaker when feature_tag and confidence are equal)

### ADR-004 — BriefingService Constructor Requires EffectivenessStateHandle

This is a compile-time constraint (the parameter is non-optional). The test plan's assertion is that the code compiles correctly. However, a documentation test confirms the constructor signature:

**Test**: `test_briefing_service_new_requires_handle`
- Construct `BriefingService::new(entry_store, search, gateway, semantic_k, effectiveness_state)` with a valid `EffectivenessStateHandle`
- Assert construction succeeds
- The fact that `Option<EffectivenessStateHandle>` is not accepted is guaranteed by the type system; this test confirms the parameter is present and the constructor works

### R-07 — Empty State in Briefing Produces Neutral Priority

**Test**: `test_briefing_with_empty_effectiveness_state_no_panic`
- Construct `BriefingService` with `EffectivenessStateHandle` pointing to empty `EffectivenessState`
- All `effectiveness_priority` lookups return 0 (neutral)
- Sort degrades to confidence-only (no panic, no wrong behavior)
- Assert output ordering is identical to a briefing without the effectiveness handle (pure confidence sort)

### R-06 — EffectivenessSnapshot Shared Across BriefingService Clones

**Test**: `test_briefing_service_clones_share_snapshot`
- Create `BriefingService` B1, clone to B2
- Update `EffectivenessStateHandle` with new categories
- Both B1 and B2's snapshot must see the updated generation on their next `assemble()` call
- This confirms the `cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>` is cloned as an Arc (shared pointer), not as a deep copy

---

## Integration Test Expectations

Integration test for AC-17 item 2 validates the briefing tiebreaker through the MCP interface:

**Observable assertion**: `context_briefing` response includes entries in order where Effective entries appear before Ineffective entries at equal confidence. This requires a fixture with multiple injection history entries at matching confidence values but different helpfulness histories.

Fixture construction approach:
1. Store two entries with identical semantic content (similar embeddings)
2. Vote entry A helpful 5 times (drives confidence up), vote B unhelpful 5 times
3. Manually equalize confidence by checking `context_get` for both entries
4. If confidence cannot be exactly equalized via MCP: assert the ordering direction is correct relative to the effectiveness category, acknowledging confidence may also differ

The integration test documents which signal (confidence or effectiveness) is driving the ordering and confirms at minimum that Effective entries do not appear below Ineffective entries when confidence is approximately equal.

---

## Edge Cases

| Scenario | Expected | Test Type |
|----------|----------|-----------|
| Briefing with zero injection history entries | No sort called; no panic | Unit |
| Briefing with single injection history entry | Sort returns same single entry | Unit |
| All entries have same confidence and same category | Sort is stable; no reordering | Unit |
| Briefing with categories dict containing entry IDs not in injection history | `categories.get(id)` returns None → priority 0 | Unit |
| feature_tag = Some("") (empty string) | Treated as present or absent per existing convention sort logic | Unit (defer to existing convention sort test) |
