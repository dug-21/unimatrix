## ADR-004: Late-Bind Cycle Attribution via the Hold's Existing Filter; Coverage = Declaration Coverage, Never a Fabricated Zero

### Context
Surface B's per-session accumulator must be attributed to the right cycle at review. At ingest a session has only its `session_id` (#4828, the UDS/HTTP split varies the namespace by transport); the cycle may resolve late (`set_feature_force`) or not at all. crt-052 Wave B already solved this: the hold stamps each held buffer with its `feature_cycle` at drain (`transcript_hold.rs:106-116`, from `state.feature` at `session.rs:857-861`), holds ONLY buffers carrying an attributed cycle (empty cycle → not held, freed at drain), and re-adoption FAILS LOUD on cycle mismatch (`readopt_inner`, #981). Re-implementing cycle resolution in the accumulator would duplicate #981-hardened logic and risk diverging.

Counter coverage = cycle-declaration coverage (SCOPE Constraint 4; SR-09): a cycle's fold reflects only sessions that declared it; an undeclared session purges at drain and its fold dies — correct fail-loud, the same failure class as the #750 believable zero. crt-054's obligation is to **never fabricate a zero** for an undeclared/purged session; absence is signalled (crt-055 surfaces it via a `raw_signals_available`-style flag), never counted.

This ADR is the producer-only successor of the prior crt-054 ADR-005 (#5003). The decision is unchanged in substance; the references to persisting an `activity_session_count` column are removed (crt-054 persists nothing — crt-055 owns whatever coverage/honesty column it lands). Note the asymmetry with Surface A, ADR-007: the `compaction_events` row is written **regardless of declaration** (server-authoritative at the handler, keyed by `session_id`), and is only *attributable* to a cycle at review — so Surface A's coverage is not declaration-gated, only Surface B's is.

### Decision
Late-bind at review via the hold's existing `feature_cycle` filter; do not resolve cycles in the accumulator. The accumulator keys on `session_id` at ingest and resolves nothing. At review, crt-055's collection (built on the producer's `activity_snapshot()` collector, ADR-003) selects exactly the held-union-registered sessions bound to the reviewed cycle — the same filter `take_transcripts_for_feature` uses. Undeclared/unmatched sessions are dropped with their audited terminal purge (`TRIGGER_READOPT_MISMATCH` / `TRIGGER_STALE_SWEEP` / `TRIGGER_CAP_EVICT`); the accumulator dies with the buffer, its bytes attributed to no cycle, and the drop is audited counts-only/content-free so the loss is visible, not silent.

crt-054 surfaces coverage honestly to crt-055: an empty/absent fold for a cycle is signalled as "no held activity reached this cycle," never emitted as a measured `0`. crt-054 must never produce a fabricated zero; crt-055 renders "unavailable" vs "0" per its fail-loud presentation guard.

### Consequences
Easier: reuses settled fail-loud Wave B machinery (no re-keying mid-stream, no #981 re-implementation); multi-session sums correctly by construction; partial coverage explicit, not silent.

Harder: a cycle whose sessions mostly didn't declare it yields a low/absent fold a naive reader could misread — mitigated by the honesty signal + the ADR-009 regression guard; the honesty signal must thread to crt-055's display surface.

Cross-refs: crt-052 Wave B (#981 stamp-at-drain + fail-loud re-adopt), #4828 (attribution chain), lesson #4998 (believable-zero class), ADR-003 (the filter-reusing read), ADR-007 (Surface A is NOT declaration-gated — the contrast), ADR-009 (regression guard). Removed vs prior ADR-005: the persisted `activity_session_count` column (crt-055-owned now).
