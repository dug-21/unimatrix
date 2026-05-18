## ADR-002: Confidence Floor Failure Posture — Fail Entire Call

### Context

The confidence floor check (`source_entry.confidence >= edge_confidence_floor`) is the primary
security mechanism against zero-confidence throwaway entries piggybacking on high-confidence
ADRs via PPR mass flow. The SCOPE.md §Security model describes this as: "if any edge in the
`edges` vec fails this check, the entire `context_store`/`context_correct` call fails — no
entry is written, no edges are written."

For `context_store` (new entries), the source entry does not exist before the insert. A new
entry's confidence is computed after insert from the Bayesian prior. The prior returns ~0.5
for zero-vote entries, which is above any reasonable floor (default 0.1). This means the
confidence check is vacuously true for brand-new entries. The threat model is specifically:
a corrected entry inheriting a correction chain from a formerly-confident entry now at low
confidence, then asserting edges to inflate its apparent graph position.

For `context_correct`, the corrected (new) entry is returned by `correct_result.corrected_entry`
and has a confidence value computed as part of the correction. This is the meaningful check site.

Two options were considered:
- Option A: Skip the confidence check entirely for `context_store` (new entries always meet the
  floor), apply only in `context_correct`.
- Option B: Apply the check post-insert for both tools, treating the newly-inserted entry's
  computed confidence as the value to check.

### Decision

Apply the confidence floor check post-insert in both `context_store` and `context_correct`,
using the newly inserted/corrected entry's computed confidence value. If the check fails:
- The entry write itself is NOT rolled back (rolling back after a successful insert is not
  supported in the existing non-transactional handler pipeline).
- The edges are NOT written.
- An error is returned to the caller indicating the confidence floor was not met.

Rationale for post-insert posture: for `context_store`, the initial confidence is always
above the default floor (0.1) due to the Bayesian prior, so the check never fails in practice
for new entries. For `context_correct`, the corrected entry may have a low confidence if
the correction chain is long or helpfulness votes are negative, making the check meaningful.
Applying it uniformly post-insert simplifies the implementation (one code path, not two).

The default floor of 0.1 is configurable via `StoreConfig.edge_confidence_floor`. Operators
can raise it for stricter graphs or lower it to 0.0 to disable the check.

**Idempotency clarification**: The confidence floor posture applies to validation failures.
INSERT OR IGNORE idempotency (re-asserting an existing edge triplet) is NOT swallowed as a
confidence failure — it is a normal success case.

### Consequences

Easier: uniform code path for both tools. The floor check is a simple scalar comparison after
insert. The default 0.1 floor is permissive enough to never fire for normal agent behavior.
Configuration allows tightening without code changes.

Harder: for `context_store`, the entry is written even when confidence check fails — the caller
receives an error but the entry was created. This is a narrow inconsistency: the entry exists
in the DB but has no declared edges. Agents should be aware this is the failure mode. Alternative
(pre-insert check using pre-computed confidence) would require eager confidence computation
before the write path, which is more complex and changes the current write pipeline ordering.

Related: ADR-001 (validation-first for type/self-ref), ADR-003 (partial-write posture).
