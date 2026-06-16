# Test Plan — Fail-loud presentation guard (per-metric availability)

**Component**: `RetrospectiveReport` presentation + `unimatrix-observe` per-metric `raw_signals_available` flags
**Risks**: R-06 (believable-zero past guard — CRITICAL), R-17 (pre-divided ratio), R-04 (held-route, presentation side)
**ACs**: AC-01 ("unavailable" not "0"), AC-21 (coarse/directional behavioral-signal honesty)

> ADR-003 (sequenced FIRST — Wave 1, lowest risk, de-risks the believable-zero class before columns land). Per-metric flags, NOT a single cycle-wide flag. Plus the binding coarse-signal honesty rule: behavioral counts render with a directional qualifier, visually distinct from exactly-counted aggregates.

## Unit tests

### Per-metric "unavailable" not "0" (R-06, AC-01)
- `test_empty_source_renders_unavailable_per_metric` — synthesize a cycle per empty source class:
  - zero `cycle_events` → phase metrics "unavailable"
  - zero `compaction_events` → `compaction_count` / reread "unavailable"
  - empty fold → transcript metrics "unavailable"
  - zero served-knowledge → reuse "unavailable"
  Assert each renders literally "unavailable", NEVER the literal "0".
- `test_per_metric_flags_independent` (R-06) — assert the `available` flags are INDEPENDENT: one empty source does NOT flip another metric's flag; one present source does NOT mask another's emptiness.
- `test_measured_zero_distinct_from_unavailable` — a genuine measured zero (e.g. `compaction_count > 0`, `compaction_reread_count == 0`) renders as `0`/measured, NOT "unavailable" — the two are distinguishable.

### Ratio honesty (R-17, AC-01)
- `test_ratio_zero_of_zero_unavailable` (R-17) — "0 of 0" (empty denominator) → "unavailable".
- `test_ratio_zero_of_n_measured` (R-17) — "0 of N" → a genuine measured rate (drives off the stored num/den pair, not a pre-divided number).

### Coarse-signal presentation honesty (R-06, AC-21) — binding decision
- `test_behavioral_signals_carry_directional_qualifier` (AC-21) — `transcript_error_count`, `transcript_refusal_count`, `signal_class_counts_json` render WITH a coarse/directional qualifier (e.g. "~", "directional", "approx") in the rendered report. A non-zero value reads as "directional signal of N", never an authoritative exact tally.
- `test_exact_aggregates_do_not_carry_qualifier` (AC-21) — exactly-counted aggregates (`compaction_count`, phase counts, rework ratio) do NOT carry the qualifier. Assert the two presentations are DISTINGUISHABLE (the honesty boundary between auditable and directional signals).

## Integration tests (MCP harness)
- `test_cycle_review_empty_source_renders_unavailable` (harness, AC-01) — full review on a cycle with each empty source class → rendered report shows "unavailable" per metric, never literal "0".
- `test_cycle_review_behavioral_signals_directional_qualifier` (harness, AC-21) — rendered report carries the directional qualifier on behavioral signals; exact aggregates do not — distinguishable in the real rendered output.

## Edge cases
- Cycle with ZERO declared sessions → every source-derived metric "unavailable", no fabricated zeros (R-06).
- Mixed present/empty sources in one cycle → some metrics measured, some "unavailable" (per-metric granularity, not cycle-wide).
- A behavioral signal of exactly 0 → still rendered with the directional framing (a directional "no signal observed", not an exact "0").

## Expected behaviors / assertions summary
- Empty source → "unavailable" per metric, never "0"; flags independent.
- "0 of 0" → unavailable; "0 of N" → measured rate.
- Behavioral signals carry a coarse/directional qualifier; exact aggregates do not — distinguishable.
