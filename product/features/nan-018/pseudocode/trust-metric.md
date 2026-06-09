# Component: Trust metric class — `trust-metric.md`

**Wave**: 1
**Location**: `crates/unimatrix-server/src/eval/runner/trust.rs` (new) +
`eval/scenarios/types.rs` (additive field). **ADR**: ADR-004 (#4898).
**Risks**: R-09, R-10, R-11 (vacuous-pass — the most likely correctness bug), R-12.

## Purpose

Evaluate property-based correctness assertions (redirect-to-head, absence, rank-below) in the
harness, per profile, per scenario — so trust rides A/B sweeps alongside P@5/MRR/cost (C-03).
A reusable metric *class*: an `Assertion` enum the evaluator matches over; Wave-1 ships exactly
the variants the three property types need (SR-06, no speculative types).

## Additive field on `ScenarioRecord` (`scenarios/types.rs`)

```
pub type EntryRef = String;            // corpus alias, e.g. "chainA.head"

pub struct ExpectedAssertions {
    pub redirect_to_head: Vec<EntryRef>,             // each: chain head must be at/above its queried member
    pub forbidden_absent: Vec<EntryRef>,             // each must be ABSENT from top-k
    pub rank_below: Vec<(EntryRef, EntryRef)>,       // (A, B): rank(A) strictly below rank(B)
}

pub struct ScenarioRecord {
    // ... existing ...
    pub expected: Option<Vec<u64>>,                  // literal-ID; LOG-SOURCED ONLY (unchanged)
    pub assertions: Option<ExpectedAssertions>,      // NEW; additive; primary-corpus property ground truth
}
```
`assertions` is kept separate from `expected` for backward wire-compat; log-sourced scenarios
never set it. The primary fixture corpus uses `assertions` and NEVER `expected` (C-04; loader-enforced,
see `corpus-loader.md`).

## Alias resolution boundary

Assertions are authored against aliases; the loader (`corpus-loader.md`) returns an
`AliasMap` (alias → resolved entry id) at load. `evaluate_trust` receives the map and resolves
each `EntryRef` to an id. A missing alias is a HARD ERROR upstream at load (R-10) — the evaluator
never silently treats an unresolvable alias as absent.

## `evaluate_trust` — the core evaluator

```
pub fn evaluate_trust(
    entries: &[ScoredEntry],            // the ranked result list for this profile/scenario
    assertions: &ExpectedAssertions,
    alias_map: &AliasMap,
) -> TrustOutcome {
    // Build a rank index: id -> 0-based rank in `entries` (lower = better).
    rank_of: Map<id, usize> = { entries[i].id : i  for i in 0..entries.len() }
    let present = |id| rank_of.contains_key(id)

    violations: Vec<String> = []
    absence_pass = true
    rank_pass = true

    // --- absence (forbidden_absent) : forbidden ∩ top_k == ∅ ---
    for fref in assertions.forbidden_absent:
        fid = alias_map.resolve(fref)               // hard-resolved; load guarantees existence
        if present(fid):
            absence_pass = false
            violations.push("absence: forbidden '{fref}' present at rank {rank_of[fid]}")

    // --- rank_below (A, B): rank(A) > rank(B), with ASYMMETRIC absent semantics (R-11) ---
    for (aref, bref) in assertions.rank_below:
        aid = alias_map.resolve(aref); bid = alias_map.resolve(bref)
        match (present(aid), present(bid)):
            (true, true)   => if rank_of[aid] <= rank_of[bid]:           // A must be strictly BELOW B
                                  rank_pass = false
                                  violations.push("rank_below: '{aref}'(rank {rank_of[aid]}) not below '{bref}'(rank {rank_of[bid]})")
            (false, _)     => { /* A absent ⇒ PASS (vacuously below) — no-op */ }
            (true, false)  => { rank_pass = false                          // *** B absent while A present ⇒ FAIL ***
                                violations.push("rank_below: '{bref}' absent but '{aref}' present (should-rank-higher missing)") }
            // (false, false): A-absent dominates ⇒ PASS (matches the first arm)

    // --- redirect_to_head : head present in top-k at rank <= each present superseded member ---
    for head_ref in assertions.redirect_to_head:
        head_id = alias_map.resolve(head_ref)        // alias of the chain's terminal-active head
        // The chain members for this head are resolvable via the loaded graph + find_terminal_active
        // semantics (graph.rs:547). The loader pre-computes, per redirect_to_head alias, the set of
        // superseded member ids whose terminal-active is head_id (see corpus-loader.md::head_members).
        members = alias_map.head_members(head_ref)   // superseded predecessors of this head
        if not present(head_id):
            rank_pass = false
            violations.push("redirect_to_head: head '{head_ref}' absent from results")
        else:
            for m in members where present(m):
                if rank_of[m] < rank_of[head_id]:    // a superseded member outranks the head
                    rank_pass = false
                    violations.push("redirect_to_head: superseded member (rank {rank_of[m]}) outranks head '{head_ref}'(rank {rank_of[head_id]})")

    TrustOutcome { absence_pass, rank_pass, violations }
}
```

### Operational semantics (do NOT soften — R-11)

| Property | Pass | Fail |
|----------|------|------|
| **absence** | forbidden ∩ top_k == ∅ | any forbidden present |
| **rank-below (A,B)** | both present & rank(A) > rank(B); **A absent** | **B absent while A present**; both present & rank(A) <= rank(B) |
| **redirect-to-head** | head present AND no present superseded member outranks it | head absent; superseded member outranks head |

- The asymmetric `rank_below` B-absent ⇒ FAIL case is the single most likely correctness bug.
  A naive "either absent ⇒ pass" inverts it. Assert it explicitly.
- `both absent` for rank-below ⇒ A-absent arm dominates ⇒ PASS (document the chosen rule).
- `redirect_to_head` with no valid head (dead-end chain, optional 5th shape) ⇒ defined FAIL
  (head absent path), never a panic.

## `Assertion` enum (the class — extensibility without touching call sites)

`ExpectedAssertions` is the on-disk shape; the evaluator internally treats each item as an
`Assertion` variant so future correctness properties (quarantine-absent, contradiction-suppressed)
slot in without changing the call site:

```
enum Assertion {
    Absence(EntryRef),
    RankBelow(EntryRef, EntryRef),
    RedirectToHead(EntryRef),
    // Wave-1 ships ONLY these three. No speculative variants (SR-06).
}
```

## `TrustOutcome` (Integration Surface)

```
pub struct TrustOutcome { pub absence_pass: bool, pub rank_pass: bool, pub violations: Vec<String> }
```
`absence_pass` aggregates all absence assertions; `rank_pass` aggregates rank-below + redirect-to-head;
`violations` carries human-readable per-violation strings for the report and for regression diffing.

## Call site (in `eval/runner/replay.rs` / `run_single_profile`)

```
// right after the result list `entries: Vec<ScoredEntry>` is built, per profile:
let trust = match &scenario.assertions {
    Some(a) => evaluate_trust(&entries, a, &alias_map),
    None    => TrustOutcome { absence_pass: true, rank_pass: true, violations: vec![] },  // no assertions ⇒ trivially pass
};
profile_result.trust = trust;   // see report-extensions.md for ProfileResult plumbing
```
Evaluated in the SAME pass as P@5/MRR/cost (C-03) so one run correlates all four families (AC-04).

## Data flow

- **Inputs**: `&[ScoredEntry]` (ranked results), `&ExpectedAssertions`, `&AliasMap`.
- **Output**: `TrustOutcome`, stored on `ProfileResult`.

## Error handling

- Missing/duplicate alias ⇒ caught at LOAD (`corpus-loader.md`), never reaches the evaluator.
- No assertions on a scenario ⇒ trivial pass (does not pollute regression counts).
- Empty result set ⇒ absence trivially passes; rank-below both-absent passes; redirect-to-head fails
  (head absent). All defined, no panic.

## Key test scenarios

- **rank-below truth table (R-11.1, AC-03)**: present/present rank(A)>rank(B) ⇒ pass;
  present/present rank(A)<rank(B) ⇒ fail; A-absent ⇒ pass; **B-absent ⇒ fail**; both-absent ⇒ pass.
- **redirect-to-head (R-11.2)**: head present + members below ⇒ pass; head absent ⇒ fail;
  superseded member outranks head ⇒ fail; no-valid-head (dead-end) ⇒ defined fail, no panic.
- **absence (R-11.3)**: forbidden ∩ top_k == ∅ ⇒ pass; any forbidden present ⇒ fail.
- **Empty result set**: defined verdicts per above.
- **Alias resolution stability (R-10)**: same logical assertion resolves to the same verdict across
  two loads with different id assignment.
- **No-assertions scenario**: trivial pass; not counted as a trust regression.
