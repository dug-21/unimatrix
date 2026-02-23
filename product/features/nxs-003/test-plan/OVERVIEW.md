# nxs-003: Embedding Pipeline -- Test Plan Overview

## Test Strategy

Testing follows the risk-based strategy from `RISK-TEST-STRATEGY.md`. Tests are organized into two tiers:

**Tier 1: Unit tests (no model required)**
- Normalization arithmetic (R-01)
- Mean pooling with attention mask (R-02)
- Text concatenation (R-08)
- Config construction (R-10)
- Model catalog metadata (R-13)
- Trait object safety (R-12)
- MockProvider (AC-19)
- Error enum construction (AC-14)

**Tier 2: Integration tests (model required)**
- Full embedding pipeline (R-04)
- Batch vs single consistency (R-03)
- Semantic similarity (AC-08)
- Thread safety (R-07)
- Empty/degenerate input (R-09)
- Batch boundaries (R-14)
- Download and cache (R-05, R-10)

## Risk-to-Test Mapping

| Risk | Severity | Test Plan File | Key Tests |
|------|----------|---------------|-----------|
| R-01 | Critical | normalize.md | L2 norm correctness, near-zero vector, diverse inputs |
| R-02 | Critical | pooling.md | Attention mask effect, hand-crafted example (AC-18) |
| R-03 | High | onnx.md | Batch vs single consistency (AC-11) |
| R-04 | High | onnx.md, download.md | Model loading success, error variants |
| R-05 | High | download.md | Download, cache validation, corrupt files |
| R-06 | High | onnx.md | Truncation behavior for long text |
| R-07 | High | onnx.md | Concurrent Arc<OnnxProvider> usage (AC-10) |
| R-08 | Medium | text.md | prepare_text edge cases (AC-06) |
| R-09 | Medium | onnx.md | Empty string, whitespace, special chars (AC-12) |
| R-10 | Medium | config.md, download.md | Cache path resolution, custom dir |
| R-11 | Medium | normalize.md, onnx.md | NaN/infinity in output |
| R-12 | Medium | provider.md | Object safety compile-time tests (AC-09) |
| R-13 | Medium | model.md | All 7 models 384-d metadata (AC-17) |
| R-14 | Medium | onnx.md | Batch size boundaries (AC-04) |
| R-15 | Low | lib.md | Build verification |

## Test Organization

Tests are organized as `#[cfg(test)] mod tests` blocks within each source file. Integration tests requiring ONNX models are marked with `#[ignore]` for offline runs.

### Per-Component Test Locations

| Component | Test Location | Type |
|-----------|--------------|------|
| error (C1) | error.rs::tests | Unit |
| config (C2) | config.rs::tests | Unit |
| model (C3) | model.rs::tests | Unit |
| normalize (C4) | normalize.rs::tests | Unit |
| pooling (C5) | pooling.rs::tests | Unit |
| text (C6) | text.rs::tests | Unit |
| provider (C7) | provider.rs::tests | Unit (compile-time) |
| download (C8) | download.rs::tests | Unit + Integration |
| onnx (C9) | onnx.rs::tests | Integration |
| test-helpers (C10) | test_helpers.rs::tests | Unit |
| lib (C11) | lib.rs::tests | Build verification |

## Test Priority Order (from Risk Strategy)

1. R-01: L2 normalization -- test FIRST
2. R-02: Mean pooling attention mask
3. R-03: Batch vs single consistency
4. R-04: Model loading
5. R-09: Empty/degenerate input
6. R-08: Title+content concatenation
7. R-13: Model catalog dimension
8. R-14: Batch size boundaries
9. R-07: Thread safety
10. R-05, R-10, R-11, R-12, R-15
