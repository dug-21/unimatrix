# Test Plan: effectiveness-engine

Component: `crates/unimatrix-engine/src/effectiveness.rs`
Test location: `#[cfg(test)] mod tests` within the same file

## Unit Test Expectations

All tests are pure function tests with no I/O. Arrange test data as struct literals, call the function, assert the result.

### classify_entry Tests

**E-01: Noisy > Ineffective priority** (R-01)
- Input: entry_id=1, trust_source="auto", helpful_count=0, unhelpful_count=0, injection_count=5, success_count=0, rework_count=0, abandoned_count=5, topic_has_sessions=true, noisy_trust_sources=&["auto"]
- Assert: category == EffectivenessCategory::Noisy (not Ineffective, even though >= 3 injections and 0% success)

**E-02: Ineffective > Unmatched priority** (R-01)
- Input: entry_id=2, trust_source="agent", helpful_count=1, injection_count=4, success_count=0, rework_count=0, abandoned_count=4, topic_has_sessions=true
- Assert: category == EffectivenessCategory::Ineffective (not Unmatched; it has injections)

**E-03: Unmatched > Settled priority** (R-01)
- Input: entry_id=3, trust_source="human", injection_count=0, success_count=0, rework_count=0, abandoned_count=0, topic_has_sessions=true
- Assert: category == EffectivenessCategory::Unmatched (topic is active, zero injections)

**E-04: Boundary at INEFFECTIVE_MIN_INJECTIONS with exactly 30% success** (R-01)
- Input: injection_count=3, success_count=1, rework_count=0, abandoned_count=2 (success_rate = 1/3 = 33.3%)
- Assert: category != Ineffective (33.3% >= 30%)
- Input: injection_count=10, success_count=2, rework_count=1, abandoned_count=7 (utility = (2*1.0 + 1*0.5)/10 = 0.25)
- Assert: category == Ineffective (25% < 30%, >= 3 injections)

**E-05: Default to Effective** (R-01)
- Input: trust_source="human", helpful_count=5, injection_count=5, success_count=4, rework_count=1, abandoned_count=0, topic_has_sessions=true
- Assert: category == EffectivenessCategory::Effective

**E-06: NULL/empty topic mapped to "(unattributed)"** (R-02)
- Input: topic="" (empty string), all other fields valid
- Assert: result.topic == "(unattributed)"
- Input: topic value that is already "(unattributed)"
- Assert: result.topic == "(unattributed)" (no double-wrapping)

### utility_score Tests

**E-14: Zero denominator** (R-05)
- Input: utility_score(0, 0, 0)
- Assert: returns 0.0 (not NaN, not panic)

**E-15: Pure success** (R-05)
- Input: utility_score(10, 0, 0)
- Assert: returns 1.0 (10 * 1.0 / 10)

**E-16: Mixed outcomes** (R-05)
- Input: utility_score(3, 4, 3)
- Assert: returns (3*1.0 + 4*0.5 + 3*0.0) / 10 = 0.5

**E-16b: Large values no overflow**
- Input: utility_score(1_000_000, 1_000_000, 1_000_000)
- Assert: returns 0.5 (uses f64 internally, no u32 overflow in weighted sum)

### build_calibration_buckets Tests

**E-07: Confidence = 0.0 in first bucket** (R-04)
- Input: rows = &[(0.0, true)]
- Assert: bucket[0] (range [0.0, 0.1)) has entry_count=1, actual_success_rate=1.0

**E-08: Confidence = 0.1 in second bucket** (R-04)
- Input: rows = &[(0.1, false)]
- Assert: bucket[1] (range [0.1, 0.2)) has entry_count=1

**E-09: Confidence = 0.9 in last bucket** (R-04)
- Input: rows = &[(0.9, true)]
- Assert: bucket[9] (range [0.9, 1.0]) has entry_count=1

**E-10: Confidence = 1.0 in last bucket** (R-04)
- Input: rows = &[(1.0, true)]
- Assert: bucket[9] has entry_count=1 (inclusive upper bound)

**E-11: Confidence = 0.09999999 in first bucket** (R-04)
- Input: rows = &[(0.09999999, false)]
- Assert: bucket[0] has entry_count=1

**E-12: Confidence = 0.5 in sixth bucket** (R-04)
- Input: rows = &[(0.5, true)]
- Assert: bucket[5] (range [0.5, 0.6)) has entry_count=1

**E-13: Empty calibration data** (R-04)
- Input: rows = &[]
- Assert: returns 10 buckets, all with entry_count=0, actual_success_rate=0.0

### Settled Classification Tests

**E-17: Inactive topic + success injection = Settled** (R-09)
- Input: injection_count=2, success_count=1, rework_count=1, abandoned_count=0, topic_has_sessions=false
- Assert: category == Settled

**E-18: Inactive topic + no success injection = NOT Settled** (R-09)
- Input: injection_count=2, success_count=0, rework_count=1, abandoned_count=1, topic_has_sessions=false
- Assert: category != Settled (falls through to Effective or other based on criteria)

**E-19: Inactive topic + zero injections** (R-09)
- Input: injection_count=0, success_count=0, rework_count=0, abandoned_count=0, topic_has_sessions=false
- Assert: category == Settled OR Unmatched depending on design (zero injections + inactive topic; specification says Settled requires "at least one historical injection with success outcome" so this should NOT be Settled)

### NOISY_TRUST_SOURCES Tests

**E-20: Matching trust_source** (R-10)
- Input: trust_source="auto", noisy_trust_sources=&["auto"]
- Assert: if other Noisy criteria met, category == Noisy

**E-21: Non-matching trust_source** (R-10)
- Input: trust_source="agent", noisy_trust_sources=&["auto"], helpful_count=0, injection_count>0
- Assert: category != Noisy

### aggregate_by_source Tests

**E-22: Zero-injection trust source** (R-13)
- Input: entries with trust_source="human", all classified as Unmatched (zero injections)
- Assert: aggregate_utility == 0.0 (not NaN)

**E-23: Mixed trust sources**
- Input: 3 entries from "auto" (2 Noisy, 1 Effective), 2 from "human" (1 Effective, 1 Settled)
- Assert: auto.noisy_count == 2, auto.effective_count == 1, human.effective_count == 1, human.settled_count == 1

**E-24: Empty entries list** (R-13)
- Input: entries = &[]
- Assert: returns empty Vec (no sources)

### build_report Tests

**E-25: Top 10 ineffective cap** (AC-12)
- Input: 15 entries all classified Ineffective
- Assert: top_ineffective.len() == 10, sorted by injection_count descending

**E-26: All noisy entries listed** (AC-12)
- Input: 20 entries all classified Noisy
- Assert: noisy_entries.len() == 20 (no cap)

**E-27: Top 10 unmatched cap**
- Input: 15 entries all classified Unmatched
- Assert: unmatched_entries.len() == 10

**E-28: Empty data produces valid report** (AC-14)
- Input: classifications = vec![], calibration_rows = &[], data_window with session_count=0
- Assert: by_category has all five categories with count 0, calibration has 10 empty buckets, top_ineffective/noisy_entries/unmatched_entries all empty

## Edge Cases

- Entry with all zero counts (no injections, no outcomes, no helpful/unhelpful)
- Entry with only rework outcomes (utility = 0.5, all sessions are rework)
- Entry with exactly INEFFECTIVE_MIN_INJECTIONS (boundary)
- Confidence values at every 0.1 boundary
- Title with pipe character `|` (stored verbatim, formatting is server-layer concern)
