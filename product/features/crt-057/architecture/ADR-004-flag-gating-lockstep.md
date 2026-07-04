## ADR-004: The Four-Site #4750 Seam Now Gates ONLY the Content-Opaque Fold Read; the Review-Purge Is Removed; Scoped Retrieval Is a Read-Only Snapshot, Not a Gated Destructive Side-Effect

Feature: crt-057 · GH #894 · Re-scoped by human 2026-07-04 after ass-091 (#898)
Reworked: the prior "flag gating preserves the purge/distill/attach four-site lockstep" decision is
superseded. There is no purge to gate, and no flag; the surviving success side-effect is the fold read.

### Context

`context_cycle_review` has four `result.is_ok()` success returns (purged-signals, cached-metrics,
memo-hit, full-pipeline). The #4750 pattern exists because a *success-only side-effect* inserted at
only the tail return silently skips the cache-hit and degraded paths (history #4585 — lockstep sites
drift silently). Historically three side-effects rode this seam in lockstep: the transcript purge
(`purge_cycle_transcripts`), the candidate distill (`distill_before_purge`), and the attach
(`attach_to_response_assembly`); source-assertion tests (`distill_handler.rs:651-726`) count each at
exactly 4× and assert an attach textually precedes each purge.

crt-057 changes what rides the seam. The **purge is removed** (ADR-001) — no destructive side-effect
remains. The **scoped `transcript` retrieval** is a read-only `snapshot()` (ADR-006), attached to the
response when the caller supplies the block. The **content-opaque fold read** (crt-054/055,
#5030/#5042 — `activity_snapshots_for_feature`, read-before-any-reclamation, lands durable integers on
`CycleReviewRecord`) is the one remaining true success side-effect.

### Decision

**The #4750 four-site lockstep now protects exactly one side-effect: the content-opaque fold read.**

1. **Fold read — the sole gated success side-effect, at all four returns.** It lands non-`force`-
   reproducible durable integers, so missing a site (especially the easy-to-miss memo-hit path,
   site 3) under-counts a cached re-review — the precise #4750 failure mode. It stays gated at all
   four success returns, in lockstep, exactly as before. This is the assertion that still matters.

2. **Purge — removed from all four sites.** Delete `self.purge_cycle_transcripts(&feature_cycle)` at
   `tools.rs:2379, 2558, 3328, 3451`. The source-assertion test that counts that string at exactly 4×
   is **deliberately updated** (to 0 in the handler, C-1/AC-12) with recorded rationale — the review no
   longer purges, so there is nothing to keep in lockstep. The attach-before-purge ordering assertion
   is likewise removed (no purge to order against).

3. **Scoped retrieval — read-only, not a destructive side-effect.** `distill_before_purge` (name
   vestigial) + `attach_to_response_assembly` still run at the four returns so the `transcript` block
   is honored on every path including memo-hit (OQ-3), but they are **response decoration, not
   state-mutating side-effects**: getting one wrong omits candidates from a response (caught
   behaviorally), it does not corrupt durable or buffer state. The `distill_before_purge(` /
   `attach_to_response_assembly(` counted strings are preserved (adding the `scope` argument keeps the
   prefix), so those source assertions can stand unchanged if kept; their real enforcement is the
   behavioral matrix. Because the source assertions cannot see that the `scope` argument is threaded at
   the memo-hit site, the **behavioral D-8 matrix carries explicit memo-hit rows**: memo-hit +
   `transcript` present → scoped candidates present, buffer intact; memo-hit + no `transcript` → no
   candidates, buffer intact.

### Consequences

Easier: the seam is simpler — one gated side-effect (the fold) instead of three; the destructive
lockstep hazard is gone with the purge; the fold read strictly benefits (the buffer survives the
review, so nothing it reads is lost sooner); reverting the scoped retrieval is mechanical (it mutates
no state).

Harder: the source-assertion test suite must be deliberately updated for the removed purge count (a
recorded, rationale-bearing change, not a silent one); correct `scope` threading at the four sites —
especially memo-hit — is not source-assertable and rests on the behavioral matrix, so the D-8 memo-hit
rows are a hard requirement; a future fifth success return must wire the fold read (the existing
fifth-return exhaustiveness gate catches a missing helper call) and honor the `transcript` block
(behavioral row needed).

Cross-refs: #4750 (four-site pattern), #4585 (lockstep drift), #4851 (one distill helper at four
returns), #5030/#5042 (content-opaque fold), ADR-001 (purge removed), ADR-002 (API surface),
ADR-006 (scoped-retrieval mechanism).
