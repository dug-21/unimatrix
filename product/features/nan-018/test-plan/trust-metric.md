# Test Plan — Trust metric class (`eval/runner/trust.rs`, `ExpectedAssertions`, `TrustOutcome`)

**Component**: `eval/runner/trust.rs` (`evaluate_trust`), `eval/scenarios/types.rs` (`ScenarioRecord.assertions: Option<ExpectedAssertions>`), `eval/corpus/assertions.rs` (property types).
**Wave**: 1. **Primary risks**: R-11 (vacuous-pass, High) — **the single most likely correctness bug in the feature**.

## Unit test expectations — property truth tables (R-11, do not soften) — AC-02/03

Every property type is tested on a **complete truth table**, not just its happy path. The asymmetric absent-cases are asserted **explicitly**.

### rank-below `(A, B)` — AC-03 (R-11 asymmetry)
- `test_rank_below_both_present_a_after_b_pass`: A & B present, `rank(A) > rank(B)` ⇒ **pass**.
- `test_rank_below_both_present_a_before_b_fail`: A & B present, `rank(A) < rank(B)` ⇒ **fail**.
- `test_rank_below_a_absent_pass`: **A absent ⇒ pass** (vacuously satisfied — A can't be too high if absent).
- `test_rank_below_b_absent_fail`: **B absent (A present) ⇒ FAIL** — the entry that should rank higher is missing. **This is the load-bearing asymmetry test; it must exist and assert FAIL.**
- `test_rank_below_both_absent_pass`: both absent ⇒ A-absent dominates ⇒ **pass** (assert the chosen documented rule).

### redirect-to-head — AC-05
- `test_redirect_to_head_present_above_members_pass`: chain head present in top-k AND every present superseded member ranks strictly below head ⇒ **pass**.
- `test_redirect_to_head_head_absent_fail`: head absent ⇒ **fail** (not a vacuous pass).
- `test_redirect_to_head_member_outranks_head_fail`: a superseded member outranks the head ⇒ **fail**.
- `test_redirect_to_head_no_valid_head_defined_failure`: chain whose terminal is itself deprecated (dead-end) → `find_terminal_active` yields no head → **defined failure, not panic** (edge case).

### absence (forbidden-set) — AC-02
- `test_absence_forbidden_not_in_topk_pass`: `forbidden ∩ top_k == ∅` ⇒ **pass**.
- `test_absence_forbidden_present_fail`: any forbidden in top_k ⇒ **fail**.
- `test_absence_empty_result_set_pass`: empty result set ⇒ forbidden trivially absent ⇒ **pass** (edge case — flagged as vacuous; AC-14 non-vacuous test must use a *non-empty* set, see corpus-fixtures).

## Concrete behaviors
- `evaluate_trust(entries, assertions, alias_map) -> TrustOutcome { absence_pass, rank_pass, violations }`. Assert `violations` carries a **human-legible string naming the violated anchor** for each failure (so the report and AC-14 condition-2 inspection can count *evaluated* assertions).
- A naive "either absent ⇒ pass" implementation must be caught: the B-absent-FAIL test is the sentinel.
- `Assertion` enum is a **class** (SR-06): Wave-1 ships exactly the variants needed; assert a match arm exists per shipped variant, no speculative variants.

## Serde / round-trip (#3557, #3557 dual-direction)
- `test_scenario_record_assertions_roundtrip_nontrivial`: a `ScenarioRecord` with a **non-null** `assertions` (e.g. a populated `rank_below`) survives serialize→deserialize.
- `test_scenario_record_assertions_none_absent_from_jsonl` (consumer, types.rs) and producer-side null handling per the nan-009 dual-direction pattern (#3557) — both directions, since `assertions` is additive and separate from `expected`.

## Edge cases
- k larger than corpus size: every entry returned ⇒ absence assertions become **strict** (assert).
- Empty result set: absence trivially passes; rank-below both-absent per chosen rule.
