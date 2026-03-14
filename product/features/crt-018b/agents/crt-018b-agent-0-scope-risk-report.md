# Agent Report: crt-018b-agent-0-scope-risk

## Output

- Produced: `product/features/crt-018b/SCOPE-RISK-ASSESSMENT.md`
- Line count: 37 (limit: 100)

## Risk Summary

| Severity | Count |
|----------|-------|
| High     | 2 (SR-03, SR-07) |
| Medium   | 4 (SR-01, SR-02, SR-04, SR-06) |
| Low      | 2 (SR-05, SR-08) |

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-07 (High)** — Tick error semantics for `consecutive_bad_cycles` are unspecified. If `compute_report()` fails mid-tick, the counter behavior (increment / hold / reset) determines whether auto-quarantine fires on stale data. Must be resolved before implementation, not left to implementer discretion. Evidence: pattern #1366 (tick loop error recovery).

2. **SR-03 (High)** — Auto-quarantine is silent and irreversible without manual restore. A false-positive Ineffective/Noisy classification (e.g., temporarily under-voted entry) silently removes an entry from retrieval. Audit event richness and the recommended default (`UNIMATRIX_AUTO_QUARANTINE_CYCLES`) for new deployments must be specified in the scope or spec.

3. **SR-04 (Medium)** — The ±0.05 utility delta magnitude was validated in isolation but not against the full crt-019 adaptive confidence weight range (0.15–0.25). At the low end of spread, the utility delta dominates relative to confidence. The spec needs a combined formula showing all active signals simultaneously.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found outcome and gate ADR entries; no direct crt-018b domain match
- Queried: `/uni-knowledge-search` for "outcome rework confidence scoring search ranking" — found #724 (behavior-based ranking test pattern), #485 (deprecated/superseded penalty ADR); both inform SR-04
- Queried: `/uni-knowledge-search` for risk patterns (category: pattern) — no directly applicable cross-feature risk patterns found
- Queried: `/uni-knowledge-search` for "auto-quarantine background tick in-memory state restart" — found #1366 (tick loop error recovery pattern); directly informed SR-07
- Stored: entry #1542 "Background Tick Writers: Define Error Semantics for Consecutive Counters Before Implementation" via `/uni-store-pattern` — novel pattern visible across crt-018b scope + bugfix-236 tick loop failure
