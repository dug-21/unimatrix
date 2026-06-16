## ADR-006: crt-054's Obligation Is Survival-to-Review — Never Zero or Drop the Counter Before the crt-052 Hold Purge

### Context
This ADR is the producer-only successor of the prior crt-054 ADR-007 (#5005), which placed the read at all four `context_cycle_review` success returns and persisted on the full-pipeline path. **That entire mechanism moved to crt-055** — the four-success-returns coexistence, `store_cycle_review`, and the read-before-purge *read* are now crt-055's (crt-055 SCOPE Constraint 2 + 6). crt-054 persists nothing and does not run the review pipeline.

What remains is crt-054's narrow, real obligation: the in-memory Surface B counter must remain **accurate and readable until crt-055 reads `activity_snapshot()` at review**. The counter lives inside the `TranscriptBuffer` (ADR-001); it rides the crt-052 Wave B hold across drains; `purge_cycle_transcripts` (`server.rs:561` → `clear()` + `purge_held_for_feature`) zeroes/drops the buffers, and the accumulator dies with them. If crt-054 zeroes or drops the counter before that purge, or if it short-circuits any lifecycle path that would otherwise carry the buffer to review, every counter reads zero (SR-08).

### Decision
Bind crt-054's survival obligation as a producer-side invariant, NOT a read-sequencing mechanism (that mechanism is crt-055's):

1. **Never zero or drop the counter independently.** The accumulator's only lifecycle is the buffer's lifecycle. crt-054 introduces no reset, no per-turn flush, no separate eviction. `clear()` and the crt-052 purge are the *only* things that zero it, and they are crt-055/crt-052-owned and fire *after* the review read by construction (crt-055 read-before-purge, Constraint 6).
2. **Ride the hold, do not fight it.** Because the accumulator is embedded in the buffer (ADR-001), it automatically survives drain→hold→re-adopt with the buffer. crt-054 adds no code that could drop the accumulator while the buffer survives.
3. **Hard dependency on crt-052 Wave B staying ON / non-disableable** (ADR-010, SR-08). A config that re-enabled purge-before-read, or disabled the hold, would break the fold silently. crt-054 depends on Wave B being unconditional and asserts its presence at startup (ADR-010). A regression that moved any purge before crt-055's read is caught by crt-055's read-before-purge test and crt-054's believable-zero guard (ADR-009).

crt-054 writes no column and inserts itself at no review return. Its contract to crt-055 is solely: *the counter you read at review is the true accumulated value for the cycle's held sessions.*

### Consequences
Easier: crt-054's review-time surface area collapses to "expose `activity_snapshot()`, change nothing about purge ordering"; the four-returns/single-writer complexity is entirely crt-055's; no risk of crt-054 minting a detached activity row.

Harder: crt-054's correctness is now *coupled* to crt-055 honoring read-before-purge and to crt-052 Wave B staying on — both are asserted (ADR-009 guard, ADR-010 startup check) rather than owned, so the coupling is explicit, not silent.

Cross-refs: ADR-001 (accumulator inside the buffer — why it rides the hold), ADR-003 (`activity_snapshot()` is what crt-055 reads), ADR-010 (Wave B verified precondition), ADR-009 (the regression guard that catches a survival regression), crt-052 Wave B (the hold), crt-055 Constraint 6 (read-before-purge, the consumer side). Superseded scope vs prior ADR-007: the four success returns, `store_cycle_review` persist, and the read placement (all crt-055 now).
