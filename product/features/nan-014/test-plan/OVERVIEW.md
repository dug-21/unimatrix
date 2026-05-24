# nan-014 Test Strategy Overview

## Test Domains

nan-014 spans three verification domains with different tooling:

| Domain | Components | Tooling |
|--------|-----------|---------|
| Rust unit tests | pidguard-self-pid, config-env-override, serve-foreground, health-subcommand | `cargo test` |
| Container/shell validation | dockerfile, docker-compose, dockerignore | `docker build`, `docker run`, shell assertions |
| Static analysis | ci-container-jobs, dockerignore | YAML structure review, grep |

## Risk-to-Test Mapping

| Risk | Priority | Component(s) | Test Type | Scenario Count |
|------|----------|-------------|-----------|----------------|
| R-01 | High | serve-foreground | Unit + integration (smoke) | 3 |
| R-02 | High | pidguard-self-pid | Unit | 3 |
| R-03 | High | health-subcommand | Unit | 3 |
| R-04 | High | dockerfile | Shell (docker build) | 3 |
| R-05 | High | serve-foreground | Shell (docker stop) | 2 |
| R-06 | Med | serve-foreground | Unit (error path) | 2 |
| R-07 | Med | dockerfile, dockerignore | Shell (docker build) | 2 |
| R-08 | Med | dockerfile | Shell (docker run --network=none) | 2 |
| R-09 | Med | dockerignore | Shell + grep | 4 |
| R-10 | Med | ci-container-jobs | Static YAML analysis | 2 |
| R-11 | Low | serve-foreground | Unit (clap) | 2 |
| R-12 | Low | dockerfile | Shell (docker images) | 1 |
| R-13 | Med | config-env-override | Unit | 2 |
| R-14 | Low | dockerfile | Static review (FROM directive) | 1 |

## Cross-Component Test Dependencies

1. **serve-foreground -> pidguard-self-pid**: Foreground mode exercises PidGuard. The self-PID guard (R-02) must pass before foreground mode can be validated in a container restart scenario.
2. **health-subcommand -> serve-foreground**: Health check validates the socket created by foreground mode. Both must resolve identical `ProjectPaths`.
3. **dockerfile -> serve-foreground + health-subcommand**: Container builds the binary containing both code changes. Dockerfile tests depend on the Rust code compiling correctly.
4. **docker-compose -> dockerfile**: Compose uses the built image. Compose tests depend on a successful image build.
5. **ci-container-jobs -> dockerfile**: CI jobs build the Dockerfile. YAML correctness is static but semantic correctness depends on Dockerfile validity.

## Test Execution Order

1. `cargo test` -- unit tests for pidguard-self-pid, config-env-override, health-subcommand, serve-foreground (clap parsing)
2. `cargo build --release` -- binary builds (prerequisite for integration and container tests)
3. Integration smoke tests (`pytest -m smoke`) -- regression baseline
4. `docker build` -- Dockerfile validation (R-04, R-07, R-08, R-09, R-12)
5. `docker run` / `docker compose up` -- runtime validation (R-05, R-06)
6. Static YAML review -- ci-container-jobs (R-10)

## Integration Harness Plan

### Applicable Existing Suites

nan-014 changes the server CLI (new `--foreground` flag, new `Health` subcommand) and modifies PidGuard. These changes touch server tool logic and lifecycle behavior.

| Suite | Relevance | Reason |
|-------|-----------|--------|
| `smoke` | MANDATORY | Minimum gate -- verify no regression from CLI changes |
| `protocol` | Run | New CLI variant must not break MCP handshake |
| `tools` | Run | Verify tool discovery still lists all 12 tools after CLI changes |
| `lifecycle` | Run | PidGuard change could affect restart persistence behavior |

### Suites NOT Required

| Suite | Why Skip |
|-------|----------|
| `confidence` | No confidence logic changes |
| `contradiction` | No contradiction logic changes |
| `security` | No security boundary changes |
| `volume` | No storage/scale changes |
| `edge_cases` | No edge case logic changes |
| `adaptation` | No adaptation logic changes |

### New Integration Tests Needed

No new infra-001 integration tests are needed for nan-014. Rationale:

1. **`--foreground` mode** is functionally identical to `--daemon-child` from the MCP protocol perspective. The existing `protocol`, `tools`, and `lifecycle` suites already exercise the full MCP interface. The infra-001 harness runs against the binary in stdio mode, which exercises `tokio_main_daemon` -- the same function `--foreground` calls.

2. **`health` subcommand** is a sync CLI tool that connects to the UDS socket. It is not an MCP tool -- it is invoked by Docker HEALTHCHECK, not by agents. Unit tests cover its logic; container-level tests cover its integration with the daemon.

3. **PidGuard self-PID guard** is exercised before MCP server initialization. The existing `lifecycle` suite's restart persistence tests validate that PidGuard reclamation works (the binary restarts and data persists). The self-PID guard is a pre-condition fix that unit tests cover directly.

4. **Config env override** is read at startup before MCP tools are registered. No MCP-visible behavior change.

### Gate Requirements

- `pytest -m smoke` MUST pass (minimum gate)
- `pytest suites/test_protocol.py suites/test_tools.py suites/test_lifecycle.py -v` SHOULD pass
- Any failures triaged per USAGE-PROTOCOL.md decision tree
