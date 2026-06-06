# Agent Report: vnc-026-agent-0-scope-risk

Mode: scope-risk. Deliverable: `product/features/vnc-026/SCOPE-RISK-ASSESSMENT.md` (42 lines, under the 100-line cap).

## Risk Summary

| Severity | Count | IDs |
|----------|-------|-----|
| High | 3 | SR-01, SR-02, SR-06 |
| Medium | 8 | SR-03, SR-04, SR-05, SR-07, SR-08, SR-09, SR-10, SR-11 |
| Low | 0 | — |

Plus 4 assumptions (A-1…A-4), 5 design recommendations.

## Top 3 for Architect/Spec Attention

1. **SR-01** (High/High) — `build-request.js` parity port from the 4,183-line `hook.rs`: edge-case divergence (malformed JSONL, UTF-8, missing fields). Parity corpus should be designed first, generated from the Rust hook as oracle.
2. **SR-06** (High/Med) — the disk event queue persists raw transcript bytes (potentially secrets) unencrypted in `~/.unimatrix/{hash}/` with no retention/cleanup in scope. Evidence: Unimatrix #4711.
3. **SR-02** (High/Med) — byte-identical JSON envelopes across serde_json vs JSON.stringify (AC-04): pin envelopes to literal templates, fixtures as sole authority.

## Knowledge Stewardship
- Queried: /uni-knowledge-search x4 — found #4473 (warn+continue masks failure paths → SR-05/SR-10), #4711 (raw-conversation bytes disk posture → SR-06), #1195/#1201 (prefix-match settings merge → SR-08), npm packaging coverage-gap lessons (#4292/#4328). All cited as evidence in the assessment.
- Stored: nothing novel to store — risks identified are feature-specific; no new cross-feature pattern beyond those already captured in #4473/#4711/#1195.
