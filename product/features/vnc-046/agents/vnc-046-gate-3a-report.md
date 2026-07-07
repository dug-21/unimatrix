# Agent Report: vnc-046-gate-3a (Validator, Gate 3a)

**Gate result:** REWORKABLE FAIL
**Report:** `product/features/vnc-046/reports/gate-3a-report.md`

## Outcome
Design is strong: 7 components map 1:1 to the architecture; interfaces match the Integration
Surface; all R-01…R-16 mapped to component test plans; bidirectional-N≥2 + assembled-wiring-only
constraints enforced structurally; #800 planned as a first-class integration-harness build; all
stewardship blocks present.

One gate-blocking gap (OQ-2): the per-slug `SessionRegistry` is built with an empty
`SignatureScanner`, so `signal_class_counts` values stay zero (hollow FR-9) and HTTPS diverges
from UDS on signal-bearing transcripts (AC-07 parity). Confirmed against source
(`activity_fold_handler.rs`, `session.rs:305/350`, `main.rs:820/852`). Fix is mechanical —
wire `.with_signature_scanner(...)` from `r.transcript_signals`. Routed to uni-architect
(ADR-002 + `context_correct`) and uni-pseudocode (project-provisioner.md P1 block + a non-zero-count
test).

OQ-1, OQ-3, OQ-4, OQ-5 adjudicated RESOLVED acceptably (details + non-blocking follow-ups in the
gate report).

## Knowledge Stewardship
- Queried: read-only review of vnc-046 design artifacts (ARCHITECTURE, ADR-001…005, SPECIFICATION,
  RISK-TEST-STRATEGY, ACCEPTANCE-MAP, IMPLEMENTATION-BRIEF, all 7 pseudocode + 7 test-plan files,
  5 agent reports); grounded OQ-2/3/4 against source (`crates/unimatrix-server/src`:
  `activity_fold_handler.rs`, `infra/session.rs`, `main.rs`, `http_provision.rs`).
- Stored: nothing novel to store -- the OQ-2 finding is feature-specific (bugs/gaps are not lessons);
  the generalizable shape ("a source-level construction-parity claim can silently omit a builder
  step") is already covered by pattern #5629 (construction parity + funnel completeness) and #5427
  (source-assertion tests blind to argument threading). No 2+-feature validation pattern emerged
  that is not already captured.
