# Test Plan Overview: vnc-001 MCP Server Core

## Test Strategy

### Unit Tests (per component)
Each component has focused unit tests in its own test module. Tests exercise the component's public API in isolation using temp directories and fresh Store instances.

### Integration Tests (cross-component)
Integration tests in `tests/integration/` exercise the full server pipeline: MCP init -> tool call -> audit event -> shutdown.

### Risk-Driven Focus
Tests prioritize risks from RISK-TEST-STRATEGY.md. Critical risks (R-01, R-04, R-06, R-11, R-12, R-14) get exhaustive coverage. High risks get thorough coverage. Medium risks get basic coverage.

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test Location |
|---------|----------|-------------|---------------|
| R-01 | Critical | server, main | server tests + integration |
| R-02 | High | project | project unit tests |
| R-03 | High | project | project unit tests |
| R-04 | Critical | store schema | store tests (existing + new) |
| R-05 | High | registry | registry unit tests |
| R-06 | Critical | registry, identity | registry + identity tests |
| R-07 | Medium | audit | audit unit tests |
| R-08 | High | shutdown | shutdown tests |
| R-09 | High | shutdown | shutdown tests |
| R-10 | High | embed-handle | embed-handle unit tests |
| R-11 | Critical | tools | tools unit tests + integration |
| R-12 | Critical | identity, audit, tools | identity + tools + integration |
| R-13 | High | error | error unit tests |
| R-14 | Critical | tools | tools unit tests |
| R-15 | High | project | project unit tests |
| R-16 | High | registry, audit | registry + audit concurrency tests |

## Cross-Component Test Dependencies

- **identity tests** require a working AgentRegistry (with Store)
- **tools tests** require UnimatrixServer (full subsystem chain)
- **shutdown tests** require Store + VectorIndex
- **integration tests** require the complete server binary

## Test Infrastructure

- `tempfile::TempDir` for isolated database instances per test
- `Store::open(temp_path)` for fresh databases
- Tests do NOT depend on model downloads (embed handle tested in Loading/Failed states)
- redb transactions provide isolation between concurrent tests

## Acceptance Criteria Coverage

| AC-ID | Primary Test Location | Type |
|-------|----------------------|------|
| AC-01 | cargo build | shell |
| AC-02 | integration | test |
| AC-03 | server unit | test |
| AC-04 | project unit | test |
| AC-05 | project unit | test |
| AC-06 | project unit | test |
| AC-07 | registry unit | test |
| AC-08 | registry unit | test |
| AC-09 | registry unit | test |
| AC-10 | audit unit | test |
| AC-11 | integration | test |
| AC-12 | shutdown unit | test |
| AC-13 | integration | test |
| AC-14 | error unit | test |
| AC-15 | grep | shell |
| AC-16 | project unit | test |
| AC-17 | store unit | test |
| AC-18 | integration | test |
| AC-19 | cargo build + grep | shell |
