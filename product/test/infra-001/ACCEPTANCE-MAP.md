# Acceptance Map: infra-001

Maps each SCOPE.md acceptance criterion to source documents, implementation components, and verification approach.

## Acceptance Criteria Traceability

| AC | Criterion | Source Doc | Component | Risk | Verification |
|----|-----------|-----------|-----------|------|-------------|
| AC-01 | docker compose up builds + runs all tests, exit 0 on success | Spec FR-01.2 | C1 (Docker), C8 (Runner) | R-04, R-05 | Run pipeline end-to-end |
| AC-02 | docker compose down -v cleans up | Spec FR-01.3 | C1 (Docker) | — | Run teardown, verify no leftovers |
| AC-03 | TEST_SUITE selects individual or combined suites | Spec FR-06.1 | C8 (Runner) | — | Run with each suite name |
| AC-04 | UnimatrixClient: spawn, handshake, 9 tool wrappers, shutdown | Spec FR-02.1–FR-02.10 | C2 (Client) | R-01, R-02, R-09 | Protocol suite P-01 to P-10 |
| AC-05 | Fresh server per test, no state leakage | Spec FR-05.1, NFR-01 | C5 (Fixtures) | R-02, R-12 | Lifecycle suite: sequential tests verify isolation |
| AC-06 | Protocol suite: handshake, tool discovery, malformed input, shutdown | Spec Suite 1 | C6 (test_protocol.py) | R-01 | Protocol suite (15 tests) |
| AC-07 | Tools suite: every tool, every param, every error | Spec Suite 2 | C6 (test_tools.py) | R-03, R-06 | Tools suite (80 tests) |
| AC-08 | Lifecycle suite: multi-step flows, persistence, audit | Spec Suite 3 | C6 (test_lifecycle.py) | R-12 | Lifecycle suite (25 tests) |
| AC-09 | Volume suite: 1K+ entries, search accuracy, no timeout | Spec Suite 4 | C6 (test_volume.py) | R-11, R-07 | Volume suite (15 tests) |
| AC-10 | Security suite: scanning, capabilities, validation | Spec Suite 5 | C6 (test_security.py) | R-06 | Security suite (30 tests) |
| AC-11 | Confidence suite: 6-factor formula, re-ranking | Spec Suite 6 | C6 (test_confidence.py) | — | Confidence suite (20 tests) |
| AC-12 | Contradiction suite: detection, false positive resistance | Spec Suite 7 | C6 (test_contradiction.py) | R-06 | Contradiction suite (15 tests) |
| AC-13 | Edge cases suite: unicode, boundaries, concurrent, restart | Spec Suite 8 | C6 (test_edge_cases.py) | R-10 | Edge cases suite (25 tests) |
| AC-14 | Smoke tests: ~15 tests in <60s | Spec FR-06.3 | C6 (smoke markers), C8 | R-07 | `pytest -m smoke`, measure time |
| AC-15 | JUnit XML, JSON report, summary, server logs | Spec FR-07.1–FR-07.4 | C8 (Runner) | — | Verify files in /results/ |
| AC-16 | Deterministic seeds, logged on failure | Spec FR-03.9–FR-03.10 | C3 (Generators) | R-10 | Inject failure, verify seed in output |
| AC-17 | Docker image pre-downloads model, offline tests | Spec FR-01.5, NFR-02 | C1 (Docker) | R-05 | Build image, disconnect network, run tests |

## Coverage Matrix: Risks to Acceptance Criteria

| Risk | ACs that Validate |
|------|-------------------|
| R-01 (JSON-RPC framing) | AC-04, AC-06 |
| R-02 (Subprocess orphaning) | AC-04, AC-05 |
| R-03 (Response format changes) | AC-07 |
| R-04 (ONNX version mismatch) | AC-01 |
| R-05 (Model unavailable) | AC-01, AC-17 |
| R-06 (Generator quality) | AC-07, AC-10, AC-12 |
| R-07 (Timeout calibration) | AC-09, AC-14 |
| R-08 (tmpfs unavailable) | AC-01 (degraded) |
| R-09 (stderr deadlock) | AC-04 |
| R-10 (Seed reproducibility) | AC-13, AC-16 |
| R-11 (tmpfs overflow) | AC-09 |
| R-12 (Shared server corruption) | AC-05, AC-08 |

## Component to AC Matrix

| Component | ACs |
|-----------|-----|
| C1 (Docker) | AC-01, AC-02, AC-17 |
| C2 (Client) | AC-04, AC-06 |
| C3 (Generators) | AC-16 |
| C4 (Assertions) | AC-07 (indirectly, via all suite tests) |
| C5 (Fixtures) | AC-05 |
| C6 (Test Suites) | AC-06 through AC-14 |
| C7 (Static Fixtures) | AC-10 (injection/PII data) |
| C8 (Runner Scripts) | AC-01, AC-03, AC-14, AC-15 |
