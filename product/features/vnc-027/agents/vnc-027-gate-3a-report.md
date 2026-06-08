# Agent Report: vnc-027-gate-3a

Role: Validator (Gate 3a — Component Design Review). Date: 2026-06-08.

## Result
PASS (3 WARNs). Report: product/features/vnc-027/reports/gate-3a-report.md

## Checks
5/5 evaluated. 4 PASS, 1 WARN (stewardship — synthesizer Session-1 report missing block; all Stage 3a producers comply). No FAIL, no scope concern.

## Open-question adjudications
- OQ1 (opt-out pruning): CONSISTENT — additive-only is a WIRE-contract constraint; settings.json is a generated install artifact; AC-08 bidirectional matrix requires Unimatrix-owned-entry pruning (correctly scoped via isUnimatrixHook).
- OQ2 (ts-rs additive vs AC-11 byte-unchanged): SURVIVES — wire goldens truly byte-unchanged; ts-rs bindings change additively (existing shapes unchanged) with drift check green after regen. WARN: AC-11/ACCEPTANCE-MAP wording imprecise; tester must assert additive diff, not literal zero-diff on bindings file.
- OQ3 (FR-16 keying): CORRECT — keyed on `canonical` from normalizeEventName, explicitly not effectiveEvent/request.type; Stop-must-NOT-delete assertable negative pinned; matches ADR-006 §3.

## WARNs
W1 synthesizer report lacks stewardship block (Session-1 agent, non-blocking).
W2 AC-11 "ts-rs bindings byte-unchanged" wording imprecise — reword + tester guidance.
W3 ADR-004 silent on opt-out stripping (pseudocode adds it correctly per AC-08) — optional ADR confirmation.

## Knowledge Stewardship
- Queried: reviewed in-feature ADR/pattern entries surfaced in agent reports (#4798, #4743, #4780, #4809, #3448) to ground gate checks against approved decisions; no external query needed beyond the source documents.
- Stored: nothing novel to store -- Gate 3a findings are feature-specific (vnc-027 design conformance + 3 OQ adjudications), not a cross-feature systemic validation pattern. The ts-rs/wire "byte-unchanged" distinction (W2) is a candidate cross-feature lesson but is observed once here; defer to Stage 3c/retro if it recurs.
