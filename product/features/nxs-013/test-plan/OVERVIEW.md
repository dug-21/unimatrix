# Test Plan Overview: nxs-013

## Test Strategy

nxs-013 is a documentation/config alignment feature with zero behavioral code changes. The test strategy has three layers:

1. **Regression gate** -- `cargo test --workspace` must pass with zero test file changes. This is the primary automated safety net, covering R-02 (log label control flow), R-06 (UNIMATRIX_CONFIG override), and R-07 (DEFAULT_CONFIG_TOML parsing).
2. **Container verification** -- Docker build + `docker inspect` + log output. Covers R-01 (cold start) and R-08 (YAML syntax). Per Unimatrix lesson #4582, static review alone is insufficient for Dockerfile changes.
3. **PR diff review** -- Boundary enforcement on documentation edits. Covers R-04 (scope creep) and R-05 (merge conflicts). Code review confirms string-only changes for R-02 and R-03.

No new unit tests or integration tests are needed -- existing tests assert on structural types (`SourceStatus` enum variants), not on string literals.

## Risk-to-Test Mapping

| Risk ID | Priority | Test Layer | Verification Method |
|---------|----------|------------|---------------------|
| R-01 | High | Container | Docker build, `docker inspect` ENV check, startup log inspection |
| R-02 | Med | Regression + Review | `cargo test --workspace` (7 provenance tests + 4 category authority tests), code review of string-only changes |
| R-03 | Med | Review | Code review of exact string literals in `log_config_provenance`, manual log inspection |
| R-04 | Med | Review | `git diff` boundary check on PRODUCT-VISION.md and WAVE2-ROADMAP.md |
| R-05 | Low | Pre-delivery | Check for open PRs touching README.md |
| R-06 | High | Regression + Review | Existing provenance tests cover env override path, code review confirms `load_config` untouched |
| R-07 | High | Regression + Review | Existing config parsing tests exercise `DEFAULT_CONFIG_TOML`, code review confirms comment-only changes |
| R-08 | Med | Container | `docker compose -f docker-compose.yml config` YAML validation, code review confirms comment-only changes |

## Cross-Component Test Dependencies

All 7 components are independent. No component depends on another component's output. No cross-component integration tests are needed.

The only integration surface requiring attention:
- **C1 (Dockerfile) <-> load_config**: Removing `UNIMATRIX_CONFIG` changes which `load_config` step activates in the container. Verified by Docker build + log inspection (R-01).
- **C3 (main.rs) <-> provenance types**: Log label changes consume unchanged `ConfigProvenance`/`SourceStatus` types. Verified by existing provenance tests (R-02).
- **C7 (config.rs) <-> config parsing**: Header comment changes must not corrupt TOML. Verified by existing config parsing tests (R-07).

## Integration Harness Plan

### Suite Applicability

nxs-013 does NOT modify any server tool logic, store/retrieval behavior, confidence system, contradiction detection, security boundaries, or schema/storage. The changes are:
- Dockerfile ENV removal (container config, not server code)
- docker-compose.yml comment changes (no code)
- Log string literal changes in `log_config_provenance` (cosmetic, not behavioral)
- Documentation prose updates (no code)
- Config header comment update (no code)

Per the suite selection table:

| Feature touches... | Applies? | Suite |
|--------------------|----------|-------|
| Any server tool logic | No | -- |
| Store/retrieval behavior | No | -- |
| Confidence system | No | -- |
| Contradiction detection | No | -- |
| Security (scanning, caps) | No | -- |
| Schema or storage changes | No | -- |
| Any change at all | **Yes** | `smoke` (minimum gate) |

**Suites to run in Stage 3c:**
- `smoke` -- MANDATORY minimum gate (~15 tests, <60s). Confirms no regression from the log string changes in main.rs.

**Suites NOT needed:**
- `tools`, `protocol`, `lifecycle`, `volume`, `security`, `confidence`, `contradiction`, `edge_cases`, `adaptation` -- none of these behaviors are affected by nxs-013.

### Gap Analysis

No gaps. The existing smoke suite covers the critical path (store/search/correct/quarantine/confidence/briefing/restart). Since nxs-013 makes zero behavioral changes to any of these paths, smoke is sufficient to confirm no accidental regression.

### New Integration Tests Needed

**None.** Rationale:
- No new tool or tool parameter added.
- No new lifecycle flow introduced.
- No new security boundary.
- No new confidence/scoring behavior.
- The log label changes (C3) are cosmetic -- they produce `tracing` output consumed by the subscriber, not by MCP responses. Integration tests exercise MCP JSON-RPC responses, not server log output.
- A tracing-capture integration test for log labels would be a new harness infrastructure capability (significant change), which should be a separate GH Issue per USAGE-PROTOCOL.md guidance.

### Docker-Based Verification (Outside Harness)

R-01 and R-08 require Docker commands that are outside the integration harness scope:
1. `docker build -t unimatrix-test .` -- build succeeds
2. `docker inspect --format '{{.Config.Env}}' unimatrix-test` -- `UNIMATRIX_CONFIG` absent, `HOME=/data` present
3. `docker compose -f docker-compose.yml config` -- YAML validates
4. Container startup log inspection -- "primary config" and "defaults config" labels present

## Non-Negotiable Coverage Gates

1. `cargo test --workspace` passes with zero test file changes (R-02, R-06, R-07, AC-09)
2. Docker build succeeds (R-01, per lesson #4582)
3. `docker inspect` confirms `UNIMATRIX_CONFIG` absent, `HOME=/data` present (R-01, AC-01)
4. Code review confirms `load_config` is unmodified (R-06, NFR-01)
5. Code review confirms `log_config_provenance` changes are string-literal-only (R-02, R-03)
6. PR diff review confirms documentation edit boundaries (R-04)
7. Integration smoke tests pass (minimum gate)
