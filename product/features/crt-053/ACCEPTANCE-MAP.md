# crt-053 Acceptance Criteria Map

Source: SPECIFICATION.md AC-01..AC-05 (which fully cover SCOPE.md's three Validation items + the
SR-02/SR-05 fixture requirements). All criteria are **behavior-based** — assert seed
inclusion/exclusion and ranking/presence outcomes over specific entry IDs, never penalty constants
(C-04, crt-013 #703). **No eval-harness metric gate** (P@5/MRR/soft-GT) is permitted as acceptance
(SR-01, NFR-05; #500 / soft-GT trap). Verification surface: the nan-018 fixture corpus (with the
ass-073 positive-edge revision) **or** the Python integration suite over raw `entries` JSON (OQ-1).

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|---------------------|---------------------|--------|
| AC-01 | Seed filter excludes deprecated/superseded from the expander seed set. Given a pool with one Active + one Deprecated entry, both with positive out-edges: an entry reachable **only** via the deprecated seed is NOT injected; an entry reachable via the active seed IS injected. (SCOPE Validation #1, SR-05) | test | Behavior assertion on the injected candidate set with `ppr_expander_enabled = true`: assert active-only neighbor present, deprecated-only neighbor absent. **Must include a differential control arm (R-04): with the filter removed / deprecated seed forced active, the deprecated-only neighbor MUST reappear** — proving absence is filter-caused, not unreachable. | PENDING |
| AC-02 | Bit-for-bit unchanged when expander is off. With `ppr_expander_enabled = false`, search results (entries, order, scores) are identical to the pre-crt-053 baseline for the same fixture/query set. (SCOPE Validation #2, C-02, FR-07) | test | Baseline-equivalence assertion with flag off; existing default-off tests pass untouched, no new injection occurs. | PENDING |
| AC-03 | HNSW ranking unchanged. Existing search/penalty tests pass untouched; deprecated entries still appear in Flexible and are still penalized (ranked below a comparable active on the penalized path). (SCOPE Validation #3, C-01, C-03, FR-06) | test | Existing search/penalty suite passes with no modification; positive presence-of-deprecated-in-Flexible assertion holds. | PENDING |
| AC-04 | Terminal-active heads survive the filter. Given a superseded chain whose terminal active head is 6b-injected, that head remains in the seed set and its out-edge neighbors are still eligible for expansion. (SR-02, FR-03) | test | Behavior assertion that the 6b terminal-active head anchors the walk — its active-only neighbor is injected. Proves the filter retains, not just drops. | PENDING |
| AC-05 | Supersession false-positive guard. Given Deprecated A superseded_by Active B (both with positive out-edges), the BFS expands from B's path and NOT from A's path. (SCOPE Validation #1 / SR-05; supersession variant of AC-01) | test | Behavior assertion: neighbor reachable only via A absent; neighbor reachable via B present. **Differential control arm required (R-04)** as in AC-01. | PENDING |

## Anti-Acceptance (forbidden criterion)

| ID | Forbidden Assertion | Reason | Enforcement |
|----|---------------------|--------|-------------|
| ANTI-AC-01 | No test may assert deprecated entries are *absent* from Flexible (search) results. | Contradicts the two-mode design (Flexible = penalize-but-keep-visible; ADR-001 #481, C-03, Locked Decision 1). | grep — review crt-053 test files confirm no deprecated-absence-in-Flexible assertion exists (R-06). |

## Boundary / Trip-Wire Checks (gate review, not feature ACs)

| ID | Check | Verification Method | Verification Detail | Status |
|----|-------|---------------------|---------------------|--------|
| GATE-01 | Diff touches exactly one production file (`services/search.rs`) and exactly the `seed_ids` build inside the enabled branch. (C-01, FR-08, R-03) | manual | Diff review: no new symbol, no `find_terminal_active` on injected entries, no `penalty_map` mutation, no new config flag, no edge-write change. | PENDING |
| GATE-02 | Existing `graph_expand` write-only negative tests remain UNCHANGED (the #4495 vnc-018 trip-wire). | grep | Confirm no inversion/edit of existing graph_expand relation-type negative tests. | PENDING |
| GATE-03 | Quarantine gate at `search.rs:950` is unchanged; a Quarantined entry is still excluded by enforcement (not only by the new seed predicate). (R-11) | manual | Diff review of `:950` shows no edit; quarantine enforcement test intact. | PENDING |
| GATE-04 | No eval-harness metric gate (P@5/MRR/soft-GT) is used as acceptance. (SR-01, NFR-05, R-01) | grep | Inspection of crt-053 test files confirms zero eval-harness scoring-path pass/fail gates. | PENDING |
| GATE-05 | Predicate is the typed `Status::Active` enum comparison, not a string compare. (FR-02, R-12) | grep | Source review confirms `e.status == Status::Active`; at least one non-Deprecated non-Active status (Proposed/Quarantined) excluded, proving `== Active` not `!= Deprecated`. | PENDING |

## Coverage Trace

| Scope Validation Item | Acceptance Criteria |
|-----------------------|---------------------|
| #1 — seed filter excludes deprecated/superseded from seed set | AC-01, AC-05 (+ AC-04 retention complement) |
| #2 — bit-for-bit unchanged when expander off | AC-02 |
| #3 — HNSW ranking unchanged | AC-03 |
| Anti — no deprecated-absence-in-Flexible test | ANTI-AC-01 |
