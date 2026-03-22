## ADR-002: `apply_nli_sort` Removal

### Context

`apply_nli_sort` is a `pub(crate)` function in `search.rs` introduced by crt-023. It takes
candidates and a parallel `Vec<NliScores>`, applies the entailment sort key
(`nli_scores.entailment * status_penalty`), and returns a sorted, truncated
`Vec<(EntryRecord, f64)>`. It was extracted from `try_nli_rerank` specifically for unit
testability.

crt-024 replaces the two-step pipeline (NLI sort in Step 7, co-access re-sort in Step 8)
with a single fused scoring pass. This removes the need for a sorting step at Step 7 entirely.
NLI scoring produces `Vec<NliScores>` indexed parallel to candidates; those scores are one
input to the single-pass fused formula.

Two options:
- **A (retain as helper)**: Keep `apply_nli_sort` as a helper called from the new
  single-pass scorer. It would extract the entailment value and apply it to the fused formula.
  Advantage: existing unit tests continue to compile with minimal change.
- **B (remove)**: Remove `apply_nli_sort`. NLI score extraction happens inline in the
  single-pass loop. Test coverage migrates to new single-pass tests.

The motivation for retaining `apply_nli_sort` was testability of the sort kernel in isolation.
After crt-024, there is no sort kernel — the score is computed per-candidate in a loop, not
sorted at the NLI step. Retaining `apply_nli_sort` as a "helper" would mean it either performs
no sort (making the name misleading) or performs a sort that the single-pass scorer then ignores
(wasteful and confusing). The function's contract is inseparable from the sort-then-discard
pattern being eliminated.

The scope explicitly states (Constraint 7): "If the single-pass approach eliminates the need
for it as a separate function, its test coverage must be migrated to the new single-pass test
coverage."

### Decision

`apply_nli_sort` is removed. Its behavior is fully replaced by the single-pass fused scorer.

The NLI entailment value for each candidate is accessed as `nli_scores[i].entailment` (as `f64`
cast from `f32`) within the scoring loop. No intermediate sort or truncation occurs at the NLI
step — the single sort happens after the fused score is computed for all candidates.

`try_nli_rerank` changes function contract: instead of returning
`Option<Vec<(EntryRecord, f64)>>` (sorted, truncated), it returns
`Option<Vec<NliScores>>` — the raw scores indexed parallel to the input candidates.
The caller (the scoring loop in `SearchService.search()`) receives these scores as input.
If NLI fails, `try_nli_rerank` returns `None` and the scorer uses `0.0` for the NLI term
(NLI re-normalization applies).

**Test migration**: All unit tests for `apply_nli_sort` (testing sort key semantics, tiebreak
behavior, length mismatch handling) are replaced by unit tests for the new fused scorer that
verify:
- NLI dominant ranking (AC-11 regression test)
- NLI-absent re-normalization
- Co-access normalization
- Utility normalization (including negative delta)
- Status penalty as multiplier
- Deterministic tiebreak behavior on equal scores

The crt-023 ADR-002 decision (entry #2701) established NLI entailment as the primary sort
signal. crt-024 does not contradict this — it makes NLI dominance structurally enforced via
weight coefficient rather than sort-key ordering, which is a stronger guarantee.

### Consequences

Easier:
- Single conceptual location for all ranking logic — no "what step 7 does gets undone by step 8" confusion.
- NLI entailment is now a permanent member of the scoring formula, not just a sort key.
- `try_nli_rerank` becomes simpler: it computes scores and returns them; it does not sort.
- The `apply_nli_sort` name is no longer available to future agents as a misunderstood "sort NLI results" affordance.

Harder:
- The unit tests for `apply_nli_sort` added in crt-023 must be explicitly deleted and
  replaced — not just updated. Implementer must not leave orphaned tests that call a
  function that no longer exists.
- `try_nli_rerank`'s return type changes; any call site that pattern-matches on
  `Option<Vec<(EntryRecord, f64)>>` must be updated.
- Loss of the isolated sort kernel means sort-correctness tests are now coupled to the
  fused formula. Tiebreak behavior tests must be written against `compute_fused_score`.
