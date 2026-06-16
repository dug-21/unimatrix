# crt-055 Architect — rev2 report (three binding human decisions)

**Agent**: crt-055-agent-1-architect-rev2 | **Date**: 2026-06-16

Applied three binding product-owner decisions to the crt-055 design. No code, no git. Files only under product/features/crt-055/.

## Decision 1 — context_reload_pct = basis-points INTEGER (0–10000), not REAL (resolves Open Q4)
- Conversion: compute_context_reload_pct returns a percentage; store round(pct × 100) as i64 (37.5% → 3750); presentation ÷100.
- Drops the REAL column and the is_finite()/push_bind(f64) float-bind guard (#4529/#4533 footgun). Every metric column now integer (uniform with crt-047).

## Decision 2 — behavioral signals render COARSE/DIRECTIONAL
- transcript_error_count / transcript_refusal_count / signal_class_counts_json are unvalidated regex matches against content-opaque deltas; not auditable post-hoc.
- Added binding clause to ADR-003: render with a coarse/directional qualifier (leading ~ or "directional"/"approx." label), distinct from exactly-counted aggregates. Presentation-only; stored ints unchanged; orthogonal to the available flag.

## Decision 3 — clock/unit promoted to binding contract clause + integration test
- **Verified in codebase**: PostToolUse read ts is **epoch MILLISECONDS** — ObservationRecord.ts: u64 "epoch millis" (unimatrix-observe/src/types.rs:39), persisted as column observations.ts_millis: i64 (unimatrix-store/src/observations.rs:16). compaction_events.compacted_at is **Unix SECONDS** (SCOPE Surface A).
- **Normalization rule**: at the compaction_reread reckoning, normalize the read ts to seconds via integer floor division read_ts_secs = ts_millis / 1000 (existing session_metrics.rs:115 precedent), then compare read_ts_secs > MIN(compacted_at). Normalize the read side only; compacted_at (seconds) untouched — fix is entirely consumer-side, no producer-contract change.
- Integration test mandated: read at boundary×1000+500ms must count, read 0.5s before must not — fails loud if ÷1000 dropped.
- Corrects the prior "seconds-vs-seconds" assumption (§9), which was wrong.

## Files edited
- product/features/crt-055/SCOPE.md (Consumer persistence table; new Binding constraint 8; Open Q2 + persistence resolution)
- product/features/crt-055/architecture/ARCHITECTURE.md (data-flow diagram, §2 component table, §3 step 4 + diagram, §4 ADR index, §6 column table + note + integration surface, §9 reconciliation correction, §10 Open Q4 resolved)
- product/features/crt-055/architecture/ADR-001-summary-schema-version-v5-migration.md (corrected the REAL-column factual detail → basis-points INTEGER)
- product/features/crt-055/architecture/ADR-003-fail-loud-presentation-guard.md (coarse/directional binding clause)
- product/features/crt-055/architecture/ADR-005-dual-reload-two-columns-two-gates-one-engine.md (basis-points INTEGER + seconds normalization)
- product/features/crt-055/architecture/ADR-006-compaction-reread-boundary-selection.md (binding seconds-normalization clause + integration test)

## Unimatrix ADRs context_corrected (provenance preserved)
- #5038 → #5046 (ADR-003, coarse/directional clause)
- #5040 → #5047 (ADR-005, basis-points INTEGER + gate normalization; 1 incoming edge auto-redirected)
- #5041 → #5048 (ADR-006, binding seconds-normalization)
- No new typed edges asserted — none meet the high traversal-necessity bar; relationships captured in prose; supersession via context_correct (not edges).

## Consequence for other ADRs
- ADR-001 (#5042-era entry, not in my correct-list) stated "REAL DEFAULT 0 for context_reload_pct" — now false. Corrected the **file** for internal consistency; **flagging that ADR-001's Unimatrix entry still carries the stale REAL detail** and should be context_corrected by its owner (or in retro) to "every numeric column INTEGER; context_reload_pct = basis points, no REAL column."

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned the three target ADRs (#5038/#5040/#5041) plus crt-054 producer ADRs (#5030/#5035) and crt-052 transcript ADRs; confirmed compacted_at=seconds in crt-054 ADR-007 and bytes-only envelope in crt-054 ADR-005.
- Stored: context_corrected #5038→#5046, #5040→#5047, #5041→#5048 via the corrected ADRs (provenance-preserving). Nothing else novel to store -- these are feature-specific decision revisions, not generalizable patterns.
