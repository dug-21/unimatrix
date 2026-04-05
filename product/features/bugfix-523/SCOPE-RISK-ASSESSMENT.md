# Scope Risk Assessment — bugfix-523

## Summary

4-item hardening batch. Items are independent at the code level but share one test file (config.rs). The main scope-level risks are: Item 1 ADR compliance, Item 3 edit-scale in a large file, and tracing-test coverage gaps that have blocked gates in prior features.

---

## Risks

| Risk ID | Title | Severity | Likelihood | Mitigation |
|---------|-------|----------|------------|------------|
| SR-01 | Item 1 gate placement violates ADR-001 if inserted at wrong boundary | High | Low | Gate must land after Path C, before `get_provider()`. SCOPE.md is explicit but implementor must confirm via code read — ADR #4017 defines the structural invariant. |
| SR-02 | Item 3: 19-field edit in ~8000-line config.rs risks missed or double-applied fields | Med | Med | SCOPE.md enumerates all 19 fields with line numbers. Tester must run NaN AC for all 19 fields (AC-06 through AC-24), not just a sample. |
| SR-03 | Tracing-test ACs for Item 2 (warn→debug) historically cause Gate 3b failures | Med | High | Prior features (lesson #3935) deferred tracing-level ACs as "structural coverage" — gate rejected. AC-04/AC-05 distinguish expected vs. anomaly warn; the spec must make testability explicit or accept AC deferral risk. |
| SR-04 | Item 3 NaN tests ship without the `assert_validate_fails_with_field` helper pattern, causing silent passing | Med | Low | SCOPE.md mandates the helper (line 163) and names it. All 19 NaN tests must use it. Risk is low given explicit constraint but non-trivial in a batch of 19. |
| SR-05 | Item 4 sanitize guard added after capability check — wrong insertion order would allow unsanitized session_id to reach registry before abort | Med | Low | SCOPE.md specifies insertion point precisely (after capability check, before first `event.session_id` use). PR review must verify insertion position, not just presence. |
| SR-06 | Items 1 and 3 are both in `nli_detection_tick.rs`/`config.rs` — concurrent edits in same file if implementation is split across waves | Low | Med | If swarm splits Item 1+2 to one agent and Item 3 to another, config.rs will not conflict (different file), but `nli_detection_tick.rs` hosts Items 1 and 2 — assign both to the same implementation wave or agent. |

---

## Assumptions

- **SCOPE.md §Background/Item 1**: Assumes the current gate location (`~line 546`, after Path C, before `get_provider()`) is still accurate in HEAD. If `nli_detection_tick.rs` has been modified since crt-039 merged, line numbers may have shifted. Architect must verify before specifying exact insertion point.
- **SCOPE.md §Background/Item 3**: Lists 19 fields with line numbers (~997–1414). config.rs is ~8000 lines and was last touched by PR #516. Field list is assumed complete; any fields added after PR #516 are not covered.
- **SCOPE.md §Non-Goals**: Explicitly excludes `RetentionConfig`, `CoherenceConfig`. This is sound — scope is correctly bounded.

---

## Design Recommendations

- **SR-01**: Specification must reproduce the exact insertion site in terms of structural landmarks (Path C completion, `candidate_pairs` check) rather than line numbers alone, so the implementor can locate it even if line numbers have drifted.
- **SR-03**: Specification must explicitly state whether AC-04 requires a log-level assertion or only behavioral coverage (skip, no panic). Given lesson #3935, the tester will push back on any tracing-level AC at gate — resolve this in spec before delivery, not at gate.
- **SR-02 / SR-04**: Specification should include the complete field list as a checklist, not a count ("19 fields"). A count mismatch is the most likely source of a NaN test gap at Gate 3a.
- **SR-06**: Specification wave assignments should co-locate Items 1 and 2 (both touch `nli_detection_tick.rs`) to avoid merge conflict risk.

---

## Top 3 for Architect Attention

1. **SR-01 — ADR-001 gate placement**: The fix is correct in principle but the insertion boundary is load-bearing. ADR #4017 defines a structural invariant (Path A and Path C unconditional; Path B gated). Any insertion point that gates Path C as well as Path B violates the ADR and will corrupt Informs edge accumulation in production. The spec must express the insertion site in structural terms.

2. **SR-03 — Tracing-level AC testability**: Items 2 (warn→debug) and 1 (debug! on early return) both introduce log-level ACs. Lesson #3935 shows these are the most likely ACs to be deferred or fail at gate. The spec must decide upfront: full tracing assertion using `tracing-test`, or behavioral-only coverage with log level acknowledged untested.

3. **SR-02 — 19-field edit completeness**: A batch edit across 19 fields in an 8000-line file is the highest mechanical risk in this batch. The risk is not incorrectness per field — the pattern is simple — but omission. The spec's field checklist and AC-06 through AC-24 are the primary mitigations; the tester must treat this as a checklist verification, not a sampling exercise.
