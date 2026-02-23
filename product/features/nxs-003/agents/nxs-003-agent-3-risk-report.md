# Agent Report: nxs-003-agent-3-risk

**Agent ID**: nxs-003-agent-3-risk
**Role**: Risk-Based Test Strategy Specialist
**Feature**: nxs-003 (Embedding Pipeline)
**Date**: 2026-02-23

## Artifact Produced

- `product/features/nxs-003/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count | Risks |
|----------|-------|-------|
| Critical | 2 | R-01 (L2 normalization), R-02 (mean pooling attention mask) |
| High | 5 | R-03 (batch/single consistency), R-04 (model loading), R-05 (download failure), R-06 (truncation), R-07 (thread safety) |
| Medium | 6 | R-08 (concatenation), R-09 (degenerate input), R-10 (cache paths), R-11 (NaN/infinity), R-12 (trait object safety), R-13 (catalog dimension), R-14 (batch boundaries) |
| Low | 1 | R-15 (ort RC stability) |
| Integration | 5 | IR-01..05 |
| Security | 4 | SR-01..04 |

**Total: 23 risk items, ~114 test scenarios**

## Key Risks for Human Attention

1. **R-01 (L2 Normalization)** — CRITICAL. nxs-002's DistDot metric assumes unit-length vectors. Non-unit embeddings silently corrupt every similarity score. The normalization function's handling of near-zero vectors (degenerate input) is an open question.

2. **R-02 (Mean Pooling)** — CRITICAL. If padding tokens leak into the pooled embedding, batch-generated embeddings differ from single-generated ones. This is silent — no error, just worse quality. AC-11 (batch == single) is the primary detection signal.

3. **R-05 (Download Failure)** — HIGH. First-use experience depends on network access and HuggingFace availability. No checksum verification means corrupted partial downloads could produce confusing errors.

4. **IR-05 (Synchronous Download Blocking)** — The first `OnnxProvider::new()` call blocks for 30-60 seconds during model download. Downstream async consumers must account for this.

## Open Questions

1. Near-zero vector normalization behavior (return zero vector vs error?)
2. Attention mask sum = 0 handling (division by zero in pooling)
3. Model file integrity — defer checksum verification or implement now?
4. Concurrent construction safety with `hf-hub` downloads
5. Test tier gating strategy (`#[ignore]` vs feature flag)

## Self-Check

- [x] Every risk has a Risk ID (R-01 through R-15)
- [x] Every risk has at least one test scenario
- [x] Severity and likelihood assessed for each risk
- [x] Integration risks section present and non-empty (5 risks)
- [x] Edge cases section present and non-empty (5 groups)
- [x] Failure modes describe expected behavior under failure (6 modes)
- [x] RISK-TEST-STRATEGY.md written to feature root (not in test-plan/)
- [x] No placeholder risks — each risk is specific to nxs-003
- [x] Security risks section present — untrusted inputs and blast radius assessed (4 risks)

## Dependencies Read

- SCOPE.md (approved scope)
- RESEARCH-ruvector.md (prior art)
- PRODUCT-VISION.md (product vision)
- crates/unimatrix-store/src/lib.rs (store API)
- crates/unimatrix-vector/src/lib.rs (vector API)
- product/features/nxs-001/RISK-TEST-STRATEGY.md (pattern reference)
- product/features/nxs-002/RISK-TEST-STRATEGY.md (pattern reference)
