# Agent Report: vnc-046-agent-3-risk (Architecture-Risk Mode)

## Deliverable
`product/features/vnc-046/RISK-TEST-STRATEGY.md` — 16 risks, ~31 named scenarios, full INV-T/K/C
bidirectional invariant→scenario map, SR-01…SR-09 traceability.

## Risk summary
- Critical: 3 — R-01 (one-directional false-GREEN), R-02 (assembled-wiring bypass), R-03 (field-census blind to argument threading).
- High: 8 — R-04 (store_config/inference_config white-box-only gap), R-05 (hold-pairing OOM), R-06 (test-double bypass), R-07 (latent per-slug field), R-08 (config not derived over wire), R-09 (P2 knowledge-read leak), R-10 (INV-T2 fold gap), R-15 (distillation persistence blast radius).
- Medium: 5 — R-11 (hot path), R-12 (#800 fixture), R-13 (#925 not subsumed), R-14 (500-not-404), R-16 (UDS==HTTPS parity).

## Key risks for human attention
1. The two white-box-only config fields (`store_config`, `inference_config`) are the coverage hole — must ship bidirectional wiring-pin units + an explicit AC-06 coverage-enumeration entry, never silently omitted.
2. The field-census guard (ADR-003) is a source-assertion — #5427 shows it is blind to whether a per-slug field is actually threaded to the write path. Behavioral routing (R-02) is the real enforcement; the census + wiring-pin are complements.
3. INV-C / AC-07 must DERIVE config and folds over the wire (#5285), not seed server fields or the attribution join — a seeded test is a believable-but-fake green.

## Knowledge Stewardship
- Queried: /uni-knowledge-search (context_search) for bidirectional-isolation lessons and construction-parity/boot-assertion risk patterns; context_get on #5427, #5285 -- surfaced #5348, #5347, #5427, #5285, #5172/#4974, #5629, #5170; all directly mapped into the risk register.
- Stored: nothing novel -- the generalizable cross-feature patterns are already captured (#5427 source-assertion blindness, #5285 derive-over-wire, #5348 bidirectional isolation); no 2+-feature risk pattern emerged that is not already stored. Feature-specific risks live in RISK-TEST-STRATEGY.md; #930 defect specifics stay on the GH issue (bugs are GH issues, not lessons).
