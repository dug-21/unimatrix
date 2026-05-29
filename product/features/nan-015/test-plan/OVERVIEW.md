# nan-015 Test Plan Overview

## Test Strategy

nan-015 touches one Rust function (`resolve_cache_dir()`), two infrastructure files (Dockerfile, docker-compose.yml), and two documentation files. Testing splits into:

1. **Unit tests** -- `resolve_cache_dir()` precedence chain (R-01, R-07, R-12)
2. **Static inspection** -- Dockerfile content validation (R-05, R-06), compose config (R-09), call site audit (R-02)
3. **Documentation review** -- Two-volume description consistency (R-15), security guidance (R-03)
4. **Integration/container tests** -- deferred to Stage 3c where Docker build is available (R-04, R-08, R-10, R-11, R-13, R-14)

## Risk-to-Test Mapping

| Risk | Priority | Test Type | Component Plan |
|------|----------|-----------|----------------|
| R-01 | Critical | Unit test (4 scenarios) | cache-path-resolution.md |
| R-02 | High | Code inspection + grep | cache-path-resolution.md |
| R-03 | High | Documentation review | documentation.md |
| R-04 | High | Code inspection + integration | cache-path-resolution.md |
| R-05 | High | Dockerfile content grep | dockerfile.md |
| R-06 | Critical | Dockerfile inspection + container start | dockerfile.md |
| R-07 | Med | Unit test (1 scenario) | cache-path-resolution.md |
| R-08 | Med | Code inspection (retry path) | cache-path-resolution.md |
| R-09 | Med | `docker compose config` | compose-config.md |
| R-10 | High | CI workflow audit | dockerfile.md |
| R-11 | Low | Log message review | dockerfile.md |
| R-12 | Low | Unit test (covered by R-01 scenario 2) | cache-path-resolution.md |
| R-13 | Med | Container test (AC-06) | compose-config.md |
| R-14 | Med | HEALTHCHECK semantics review | dockerfile.md |
| R-15 | Med | Documentation grep | documentation.md |

## Cross-Component Test Dependencies

- **cache-path-resolution must pass before Dockerfile tests** -- if `resolve_cache_dir()` does not read the env var, the Dockerfile ENV directive is meaningless.
- **Env var name consistency** -- the string `UNIMATRIX_MODEL_CACHE` must be identical in `config.rs` and `Dockerfile`. This is a cross-component assertion.
- **Compose depends on Dockerfile** -- the `unimatrix-shared` volume mount at `/shared` must match the Dockerfile's `VOLUME` directive and directory creation.

## Integration Harness Plan

### Feature-to-Suite Mapping

nan-015 modifies `resolve_cache_dir()` in `unimatrix-embed` -- this affects server startup path for model loading. It does **not** change any MCP tool logic, protocol handling, confidence scoring, contradiction detection, or security scanning.

| Suite | Relevance | Run? |
|-------|-----------|------|
| `smoke` | Mandatory minimum gate | Yes -- mandatory |
| `protocol` | No protocol changes | No |
| `tools` | No tool logic changes | No |
| `lifecycle` | No storage/schema changes | No |
| `volume` | No entry volume changes | No |
| `security` | No security boundary changes | No |
| `confidence` | No scoring changes | No |
| `contradiction` | No detection changes | No |
| `edge_cases` | No edge case surface changes | No |

### Gap Analysis

Existing suites exercise the MCP JSON-RPC interface. nan-015's Rust change (`resolve_cache_dir()` env var insertion) is fully testable via unit tests -- the function is pure path resolution with no MCP-visible effect. The MCP tool responses are identical regardless of which directory models are cached in.

**No new integration tests needed.** The behavioral change is:
- Internal: which directory path is returned by `resolve_cache_dir()`
- Not MCP-visible: no tool output, protocol response, or error format changes

The smoke suite confirms the server still starts and serves requests correctly after the code change, which is sufficient integration coverage for this feature.

### Container-Level Tests (Not infra-001)

Several acceptance criteria (AC-01, AC-03 through AC-09) require Docker build and container runtime, which are outside the infra-001 harness scope. These are validated during Stage 3c via shell commands (`docker build`, `docker compose config`, `docker inspect`).
