# Agent Report — crt-055-agent-2-spec

**Role**: Specification writer
**Task**: Author SPECIFICATION.md for crt-055 (context_cycle_review redesign).

## Produced
- `product/features/crt-055/specification/SPECIFICATION.md`

## Counts
- Functional requirements: 23 (FR-01..FR-23)
- Non-functional requirements: 8 (NFR-01..NFR-08)
- Acceptance criteria: 19 (AC-01..AC-19), each with verification method
- Folded point-issues captured as ACs: #556 (AC-04), #320 (AC-06), #593 (AC-15), #206-item-4 (AC-16)

## Key domain model decisions
- Drew the consumer/producer boundary explicitly: crt-055 READS `compaction_events` (Surface A) and `activity_snapshot()`/`ActivitySnapshot` (Surface B); it never produces them, never writes that table, never bumps the producer's `CURRENT_SCHEMA_VERSION`.
- Anchored single-writer + no-zero-clobber on pattern #4178 and the four-success-return discipline (#4750): new columns written only by the full-pipeline `store_cycle_review()`; memo-hit / purged-retain / force+purged returns no-write.
- Dual reload modeled as two distinct columns (`context_reload_pct` cross-session, `compaction_reread_count` post-compaction within-cycle), one shared overlap engine, never collapsed (AC-13).
- Fail-loud guard modeled per-metric off `raw_signals_available`; "unavailable" not "0" (AC-01) plus the #750 silent-zero regression guard (AC-09).
- Attribution: sessions→cycle via the `feature_cycle` declaration chain at review; undeclared sessions die fail-loud, not as fabricated zeros.

## Risk coverage in ACs
- SR-01 → AC-17 (single writer / three #5022 assertions)
- SR-02 → AC-18 (guarded recompute coexistence)
- SR-03 → AC-02, AC-03 (three-path migration + pinned version)
- SR-04/SR-05 → AC-10 (no token field), dependency note to context_correct #5006
- SR-06 → AC-13 (dual reload not collapsed)
- SR-08 → AC-09 (held-route non-empty fold regression)
- SR-09 → AC-08 (read-before-purge ordering)
- SR-10 → AC-11, AC-12, AC-14 (attribution + i64 width)
- SR-11 → AC-12, FR-15 (multi-compaction boundary rule)

## Open questions left for architect/design session
1. Rank 1–3 aggregate column shapes (ass-077 RQ-2) — architect.
2. `compaction_reread` boundary-selection rule for multi-compaction sessions — architect.
3. #206-4 knowledge-that-helped: response-time-only vs durable column — design call.
4. Confirm crt-055 consumption assumes class indices `0=error`, `1=refusal` (catalog itself is crt-054's).
5. Internal wave order (recommend: fail-loud guard → aggregates → reload pair + fold).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced crt-054 ADR-007/008/006 (#5035/#5032/#5031) and pattern #4178; applied to anchor producer/consumer boundary and single-writer constraint. Read-only tier — no storage.
