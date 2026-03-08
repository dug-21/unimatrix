# Test Plan: server-e2e-tests

## Server Pipeline (pipeline_e2e.rs)

| ID | Test | Expected | Risk |
|----|------|----------|------|
| T-E2E-skip | Skip when ONNX model absent | Descriptive skip message | R-02 |
| T-E2E-01 | Active above deprecated | Active ranks higher | R-06 |
| T-E2E-02 | Supersession injection | Successor appears in results | R-06 |
| T-E2E-03 | Provenance boost | lesson-learned > convention | R-06 |
| T-E2E-04 | Co-access boost | Co-accessed entry ranks higher | R-06, R-08 |
| T-E2E-05 | Golden regression (top-3) | Exact top-3 IDs match | R-03 |
| T-TSL-01 | TestServiceLayer construction | Succeeds with valid model | R-06 |

## ONNX Handling

All tests call `skip_if_no_model()` at start. Tests are `#[tokio::test]`. Co-access timestamps use extreme values (0 or very large) to avoid boundary issues (R-08).

## TestServiceLayer

Must mirror production ServiceLayer::new() parameter wiring. Constructor takes store_path, opens real Store, creates real VectorIndex, uses real EmbedServiceHandle.
