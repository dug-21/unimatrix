# Agent Report: crt-057-agent-3-risk (architecture-risk mode)

**Deliverable:** `product/features/crt-057/RISK-TEST-STRATEGY.md`
**GH #894 comment:** https://github.com/dug-21/unimatrix/issues/894#issuecomment-4880530585

## Summary
14 risks (4 Critical, 4 High, 4 Medium, 2 Low), 35 min scenarios, mapped to the designed system.
Every SR-01..SR-12 traced to concrete scenarios. All 16 spec ACs are covered by at least one risk row.

## Top feature-level risks for human/leader attention
- **R-01 (Critical)** Memo-hit site (site 3) silently ignores the flag — not source-assertable; behavioral path-proven matrix rows are the sole enforcement.
- **R-02 (Critical)** Negative-assertion unreliability — "no purge"/"no candidates" must key on synchronous buffer state, not async audit absence (#4879).
- **R-03 (Critical)** Consumer-reconciliation partial ship (SR-04) — needs an end-to-end harvest-fires test + four-site doc/grep guard.
- **R-04 (Critical)** No-new-persistence leak (AC-09) — content-scan every write sink on all six changed paths incl. cap/TTL reclamation-without-extraction.

## Handoff notes to the tester (uni-tester)
- Behavioral matrix must PROVE which of the four success returns executed per row (no vacuous route through full-pipeline; #4452).
- All negative assertions synchronous-only (#4879).
- AC#10: populated fixture + ratio, guarded against empty-buffer vacuous pass (#3548).
- Content-scan (not field-name check) on sinks; include the OQ-2 warning carrier and reclamation-without-extraction path.
- Extend existing `distill_handler.rs` fixtures (CON-8).

## Flag to delivery leader
- SR-12: confirm no live rebase conflict on `distill_handler.rs` / README against bugfix-891 before delivery (CON-7). Not an architecture risk; no scenario.

## Knowledge Stewardship
- Queried: /uni-knowledge-search (context_search) for lockstep drift, negative-assertion traps, coverage vacuity; context_get #4585 — surfaced #4879, #4585, #4452, #3548, #5089 (each mapped to a risk row).
- Stored: entry #5427 "Source-assertion string-counting tests are blind to argument threading — pair with a behavioral per-site matrix" (pattern; recurs across #4585 and crt-057/#4750; Supports→#4585).
- Declined: crt-057-specific risks — they live in RISK-TEST-STRATEGY.md; #4750/#4585 invariants already exist.
