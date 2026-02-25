# Test Plan Overview: infra-001

## Test Strategy

All tests exercise the `unimatrix-server` binary through MCP JSON-RPC over stdio. Tests are black-box: they validate observable behavior through the protocol, not internal state.

## Risk-to-Test Mapping

| Risk | Test Coverage | Suite |
|------|-------------|-------|
| R-01 (JSON-RPC framing) | P-06, P-08, P-11, P-12 + rapid tool calls in volume suite | Protocol |
| R-02 (Subprocess orphaning) | P-10, E-24 + implicit via 200+ tests running without lock errors | Protocol, Edge Cases |
| R-03 (Response format changes) | P-15 + format parameter tests in every tool | Protocol, Tools |
| R-04 (ONNX version mismatch) | P-01 (server starts) + first search test (triggers embedding) | Protocol, Tools |
| R-05 (Model unavailable) | AC-17 Docker offline test (manual), search tests exercise embeddings | Tools |
| R-06 (Generator quality) | Contradiction tests validate generated pairs trigger detection; security tests validate payloads detected | Contradiction, Security |
| R-07 (Timeout calibration) | All tests use 10s default; volume uses extended; smoke subset in <60s | All suites |
| R-08 (tmpfs unavailable) | Docker config; tests pass on regular /tmp too | Docker (manual) |
| R-09 (stderr deadlock) | Implicit via all tests (stderr drained); explicit via volume (100 rapid stores) | Volume, Edge Cases |
| R-10 (Seed reproducibility) | Generator tests use fixed seeds; failure logs seed | Generators (implicit) |
| R-11 (tmpfs overflow) | Volume suite monitors: 5K entries must fit in 512MB | Volume |
| R-12 (Shared server corruption) | Volume uses ordered assertions (>= N, not == N); lifecycle uses defined order | Volume, Lifecycle |

## Test Count Targets

| Suite | Target | Markers |
|-------|--------|---------|
| Protocol | ~15 | smoke (3) |
| Tools | ~80 | smoke (5) |
| Lifecycle | ~25 | smoke (3) |
| Volume | ~15 | volume, slow |
| Security | ~30 | security |
| Confidence | ~20 | |
| Contradiction | ~15 | |
| Edge Cases | ~25 | smoke (4) |
| **Total** | **~225** | smoke: ~15 |

## Smoke Test Selection (~15 tests, <60s)

These tests validate the critical path:
1. test_initialize_returns_capabilities (P-01)
2. test_server_info (P-02)
3. test_json_format_responses_parseable (P-15)
4. test_store_minimal (T-01)
5. test_search_returns_results (T-13)
6. test_store_roundtrip (T-03)
7. test_status_empty_db (T-46)
8. test_list_tools_returns_nine (P-03)
9. test_store_search_find_flow (L-01)
10. test_correction_chain_integrity (L-02)
11. test_isolation_no_state_leakage (L-06)
12. test_injection_detection (S-01)
13. test_unicode_cjk_roundtrip (E-01)
14. test_empty_database_operations (E-08)
15. test_graceful_shutdown (P-10)

## Fixture Strategy

| Fixture | Scope | Used By | Purpose |
|---------|-------|---------|---------|
| server | function | Protocol, Tools, Security, Confidence, Contradiction, Edge Cases | Fresh server per test |
| shared_server | module | Volume, Lifecycle | Accumulate state across module |
| populated_server | function | Some Tools tests | Pre-loaded 50-entry dataset |
| admin_server | function | Security (capability tests) | Server with admin agent_id reference |

## Test Data Strategy

- Deterministic seeds for all generated data (FR-03.9)
- Static JSON fixtures for injection patterns, PII, unicode (C7)
- `make_entry()` with overrides for individual tests
- `make_entries(n)` for batch loading
- `make_contradicting_pair()` for contradiction tests
- `make_correction_chain()` for lifecycle tests
- All seeds logged on failure (FR-03.10)
