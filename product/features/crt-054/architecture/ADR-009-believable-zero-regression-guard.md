## ADR-009: Believable-Zero Regression Guard — Assert the Fold Source Is Non-Empty for a Representative TS-Client Cycle, on the Held Route

### Context
crt-054 exists because #750 was a believable zero: a metric gated on a single event class (`PreToolUse`) computed honestly over zero rows when the TS client stopped emitting that class, producing a `0` that read as "the cycle did nothing" (lesson #4998, ass-077). SCOPE Constraint 2 and 6 require a regression guard so the next edge-event-set or routing change fails a test instead of silently zeroing.

This ADR carries forward the prior crt-054 ADR-009 (#5007) — it remains core. The reconciliation against the producer-only scope: the guard now asserts on crt-054's *own* surface (`activity_snapshot()` returns non-empty for a held cycle), not on a persisted `read_cycle_activity` row (that read is crt-055's). crt-054's two zero-traps under the new scope:
- **Held-route fold miss (SR-02):** if the fold stops hitting the held route (`session.rs:388-401`), drained-session bytes silently don't count. This is the #1 regression risk.
- **Survival regression (SR-08):** if the accumulator is zeroed/dropped before crt-055's review read (ADR-006), every counter reads zero.

A unit test on the accumulator alone is insufficient — the failures are at the routing and survival seams, exactly where #750 lived (pattern #3624: a registered-only or no-op-path test gives false confidence).

### Decision
Bind two structural regression guards as acceptance-level tests:

1. **Non-empty fold source on the held route for a representative TS-client cycle.** Drive a multi-turn, multi-session cycle through the HELD route (drain → hold → re-adopt — the TS-client lifecycle) with non-trivial delta bytes, then assert `activity_snapshot()` for the cycle's held sessions returns `bytes_total > 0`, `delta_count > 0`, and at least one declared session contributing. A registered-only path MUST NOT satisfy this test — it must exercise the held route specifically. If a future edge-event-set or routing change stops feeding the fold, this fails red, not silent.
2. **Survival-to-review ordering.** Assert the `activity_snapshot()` read observes non-zero counters, and that after `purge_cycle_transcripts` the buffers are zeroed — i.e. the counter is alive and accurate up to the purge (ADR-006). A regression that zeroed the accumulator early returns zero here and fails.

Both are integration-level (exercise `SessionRegistry` + the hold + the seam), extending the existing crt-052/vnc-025 transcript fixtures (test infrastructure is cumulative). The attribution honesty contract (ADR-004) is also asserted: an undeclared session's bytes do NOT appear, and absence is signalled rather than emitted as a measured `0`.

A separate guard covers Surface A: assert that a compaction at `handle_compact_payload` writes exactly one `compaction_events` row keyed by `session_id` with `compacted_at` populated, **independent of cycle declaration** (the row is written even for an undeclared session — ADR-007).

### Consequences
Easier: the originating failure class (#750) and crt-054's two zero-traps become red-on-regression, not silent; the test is the executable form of SCOPE Constraint 2/6.

Harder: the test must drive the full drain→hold→re-adopt lifecycle (more setup than a unit test) and use the held route, not the simpler registered-only path — getting this wrong lets the exact bug through; depends on the cumulative crt-052 fixtures being reusable.

Cross-refs: lesson #4998 / #750 (the originating believable zero), pattern #3624 (no-op-path false confidence), SCOPE Constraint 2/6, ADR-001 (held-route fold), ADR-004 (attribution honesty), ADR-006 (survival-to-review under test), ADR-007 (the Surface A declaration-independent row guard).
