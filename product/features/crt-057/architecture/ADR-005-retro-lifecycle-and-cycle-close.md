## ADR-005: Retro Lifecycle — pr-review Phase, Close-Cycle-After-Merge, Retro-Post-Merge; Now Trivially Safe Because Both Review AND Close Are Non-Destructive (both protocols)

Feature: crt-057 · GH #894 · Expands D-7 · Prerequisite: ADR-001 (non-destructive review)
Applies to: `uni-delivery-protocol.md` AND `uni-bugfix-protocol.md`
Reworked 2026-07-04 after ass-091 (#898): the buffer-timing reconciliation is now trivial — with no
purge verb on the review, there is no reaper anywhere on the review→close→retro path.

### Context

Both delivery and bugfix protocols **close the feature cycle before the human merges**, so the merge
decision and any PR-review rework are never attributed to the cycle and are lost to the retro harvest:

- **Delivery** (`uni-delivery-protocol.md:516-521`): after presenting the PR it runs
  `context_cycle(phase-end, pr-review)` then `context_cycle(stop)` — the cycle stops at the end of the
  pr-review phase, before the human's merge decision. No `/uni-retro` step.
- **Bugfix** (`uni-bugfix-protocol.md:418-435`): `context_cycle(stop)` fires when the security review
  returns clean, before Phase 5 (Human Review & Merge). The cycle is already closed at merge time. No
  `/uni-retro` step.

The human requirement: a distinct pr-review / bug-review phase kept OPEN through the merge decision,
the cycle closed ONLY after merge, then `/uni-retro` post-merge — ordering **merge → close → retro** —
across BOTH protocols.

Under the prior boolean contract this reordering was load-bearing and risky: the retro extraction was
the SOLE purge trigger, so a close-side purge would have made retro-after-close extract from empty
buffers. That risk **evaporates** under the re-scoped design: ADR-001 removes the purge verb entirely,
so the review is non-destructive and there is nothing to sequence around.

### Decision

**Restructure the close-of-cycle lifecycle in both protocols:**

1. **pr-review / bug-review phase stays open through the human merge decision.** Do not stop the cycle
   at the end of the review phase. The human merge gate is unchanged.
2. **Close after merge:** once the human merges, run `context_cycle(type:"phase-end")` for the review
   phase, then `context_cycle(type:"stop")`.
3. **Retro post-merge, after close:** invoke `/uni-retro`. Strict ordering: **merge → close cycle →
   retro**.

**Reconciliation — trivially safe (both review AND close are non-destructive):**

- **(a) The review never purges (ADR-001).** With the review-purge removed, no `context_cycle_review`
  path reclaims the buffer. Candidates survive every review.
- **(b) Cycle-close never purges (code-traced, read-only, crt-057 worktree
  `crates/unimatrix-server/src/`).** `context_cycle type:"stop"` drains only the
  `pending_entries_analysis` retrospective queue and writes an audit row; it touches no registered
  buffer, held buffer, session registration, or sweep. `context_cycle stop` is a Unimatrix lifecycle
  marker, disjoint from the server-side transcript hold store.
- **(c) Retro retrieves against a stopped cycle, repeatedly.** The review handler has no cycle
  open/closed guard; `take_transcripts_for_feature` / `snapshot()` scan registered ∪ held buffers by
  the `feature_cycle` string, independent of lifecycle state. Held buffers are keyed by
  `feature_cycle`, unaffected by stop. A post-close retro retrieval runs normally — and may retrieve
  **repeatedly, in any scope, non-destructively** (no one-shot to sequence around).

**Therefore the ordering composes with nothing to prove about reaper placement:** there is no reaper
on the path at all. **merge → close → retro** preserves full candidates and is runnable post-close.

**Residual exposure (softened, graceful).** The only loss vector is the ordinary TTL/cap aging every
buffer already has: an older dev-phase session buffer may hit the 24h stale-sweep TTL or the 64-session
cap before a late post-merge retro fires, degrading those candidates to `Reconstructed`/empty. This is
independent of cycle open/closed status (buffers bounded solely by cap+TTL — #4857, ADR-001 §b) and,
unlike the prior contract, is **not** compounded by any earlier purge — nothing is lost to a review, only
to aging. Accepted because: the report is buffer-content-independent (ARCHITECTURE §5), so only verbatim
candidates degrade, never the summary; degradation is visible via loss propagation (ADR-003), never
silent, never a crash (AC-06); and the retro may re-retrieve non-destructively at any time before aging.

### Consequences

Easier: merge/rework activity is captured and attributed to the cycle; the post-merge `/uni-retro` is
the consistent verbatim-candidate delivery point (closes #5219); the close-then-retro order is
**trivially** safe (no reaper anywhere), a marked simplification over the prior contract's careful
buffer-timing proof; the retro may retrieve as many scoped slices as it needs.

Harder: the cycle is held open longer (through a potentially multi-day merge); dev-phase candidates
may age out before the post-merge retro (graceful — ADR-003, and re-retrievable until aged); the
protocol ordering is strict. The Prerequisite edge to ADR-001 remains meaningful: this ordering's
safety rests on the review being non-destructive — a future change that reintroduced any review-side or
close-side purge would break retro-after-close, so ADR-001 must be read before touching this ordering.

Cross-refs: ADR-001 (Prerequisite — non-destructive review; no reaper on the path; reversing that
invalidates this ordering), ADR-003 (loss propagation / graceful degradation), #4857 (cap+TTL
backstops), #5219 (self-learning harvest), `uni-delivery-protocol.md:516-521`,
`uni-bugfix-protocol.md:418-435`.
