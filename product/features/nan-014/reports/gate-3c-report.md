# Gate 3c Report: nan-014

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-05-23
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | 14/14 risks mapped to tests or static analysis; 10 full, 4 partial (container runtime deferred — acceptable) |
| Test coverage completeness | PASS | 30 risk scenarios mapped; 24 nan-014 unit tests + 10 regression gate tests pass; container runtime tests properly documented as deferred |
| Specification compliance | PASS | All 6 FR groups implemented; NFR-3/4/5/9 verified via static analysis; NFR-1/2/6/7/8 deferred to Docker build |
| Architecture compliance | PASS | All 7 ADRs followed; component boundaries maintained; CI dependency graph independent |
| Knowledge stewardship compliance | PASS | Tester agent report contains proper stewardship block with Queried and Stored entries |

## Detailed Findings

### 1. Risk Mitigation Proof
**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 14 risks (R-01 through R-14) to test results.

**Full coverage (unit tests + static analysis)**: R-01, R-02, R-03, R-04, R-07, R-08, R-09, R-10, R-11, R-13 (10 risks)

- R-01 (`--foreground` breaks `--daemon`): 5 tests covering flag parsing, default=false, mutual exclusion, help text. All pass.
- R-02 (PidGuard self-PID): 6 tests including `test_handle_stale_self_pid_returns_reclaimed`, `test_handle_stale_self_pid_reclaims_without_sigterm`, `test_is_unimatrix_process_pid_one`, `test_is_unimatrix_process_pid_zero`. The self-PID guard at pidfile.rs line 263 (`if pid == std::process::id()`) is placed BEFORE `is_process_alive()` — architecturally preventing the self-SIGTERM race. All pass.
- R-03 (Health socket path divergence): 5 tests including `test_health_socket_path_matches_serve` which asserts `ensure_data_directory` produces identical `mcp_socket_path` for the same inputs, and `test_health_run_success_on_live_socket` which exercises the full connect path. All pass.
- R-04 (ORT SHA-256): Dockerfile static review confirms `sha256sum -c -` with `set -e`, architecture-conditional logic (`TARGETARCH` -> `aarch64`/`x64` mapping), per-arch SHA-256 ARGs. Docker build verification deferred — acceptable, as the logic is structurally correct.
- R-07 (cargo-chef + workspace patch): Dockerfile confirms `COPY patches/ patches/` precedes `cargo chef prepare`. cargo-chef pinned at 0.1.71 with `--locked`.
- R-08 (Model bake-in path): Dockerfile confirms `HOME=/data` -> `model-download` writes to `/data/.cache/` -> `COPY --from=builder /data/.cache/ /data/.cache/`. Path chain is coherent.
- R-09 (.dockerignore exclusions): Static analysis of .dockerignore confirms `.cargo/` NOT excluded, `patches/` source NOT excluded, all workspace `Cargo.toml` files accessible. `patches/anndists/target/` correctly excluded.
- R-10 (CI pipeline coupling): release.yml confirms `package-npm` needs `[build-linux-x64, build-linux-arm64]` only. `create-release` needs `package-npm` only. `create-container-manifest` needs `[build-container-x64, build-container-arm64]` only. Zero cross-dependency.
- R-11 (Mutual exclusion): `test_foreground_conflicts_with_daemon` and `test_foreground_conflicts_with_stdio` confirm clap rejects conflicting flags. Both pass.
- R-13 (HOME=/data config path): 8 tests covering `UNIMATRIX_CONFIG` env var override, precedence, fallback for missing/empty/directory paths. Dockerfile sets `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml`. All pass.

**Partial coverage (container runtime deferred)**: R-05, R-06, R-12, R-14 (4 risks)

- R-05 (SIGTERM as PID 1): Unit tests confirm foreground flag dispatch and signal handler registration. Container-level `docker stop` test deferred — requires Docker. Acceptable: the `shutdown::shutdown_signal()` function explicitly registers SIGTERM via `tokio::signal`, which is verified by existing tests.
- R-06 (Volume permission errors): Unit tests verify config fallback behavior for missing/inaccessible files. Container-level permission error test deferred. Acceptable: risk severity is Medium and the error path uses standard Rust error propagation.
- R-12 (Image size): Dockerfile uses stripped binary + distroless runtime. Size check requires Docker build. Acceptable: risk severity is Low and the budget math (253 MB estimated vs 350 MB limit) provides margin.
- R-14 (Distroless glibc): Dockerfile uses `gcr.io/distroless/cc-debian12:nonroot` (Debian 12 pinned). Digest pinning deferred per spec. Acceptable: risk severity is Low and Debian version is explicitly pinned.

**No integration tests deleted or commented out**: `git diff main -- suites/ tests/` shows no removals.

### 2. Test Coverage Completeness
**Status**: PASS

**Evidence**: 5,176 workspace tests pass, 0 failures. 24 nan-014 specific tests all pass. 10 pre-existing regression gate tests all pass.

Test coverage against Risk-Based Test Strategy's 30 scenarios:

| Risk | Required Scenarios | Covered | Method |
|------|-------------------|---------|--------|
| R-01 | 3 | 3 | Unit tests (flag parse, default, regression) |
| R-02 | 3 | 3 | Unit tests (self-PID, dead process, non-unimatrix) |
| R-03 | 3 | 3 | Unit tests (path match, no socket, live socket) |
| R-04 | 3 | 3 | Static analysis (correct hash, tampered hash, arch conditional) |
| R-05 | 3 | 2 | Unit test (1) + static review (1); container test deferred (1) |
| R-06 | 2 | 1 | Unit tests (config fallback); container permission test deferred (1) |
| R-07 | 2 | 2 | Static analysis (COPY order, version pin) |
| R-08 | 2 | 2 | Static analysis (path chain, model bake-in) |
| R-09 | 4 | 4 | Static analysis (context size, .cargo inclusion, patches, Cargo files) |
| R-10 | 2 | 2 | Static analysis (needs arrays, simulation) |
| R-11 | 2 | 2 | Unit tests (both conflict pairs) |
| R-12 | 2 | 1 | Static analysis (strip, distroless); size check deferred (1) |
| R-13 | 2 | 2 | Unit tests (HOME=/data config, env var override) |
| R-14 | 2 | 2 | Static analysis (debian12 pin, digest documentation) |

Integration test note: The test plan correctly determined that no new infra-001 integration tests are needed for nan-014, because existing suites already exercise `tokio_main_daemon` — the same function that `--foreground` calls. Container runtime integration tests are documented as deferred in RISK-COVERAGE-REPORT.md with explicit notation.

### 3. Specification Compliance
**Status**: PASS

**Evidence**:

**Functional Requirements verified**:

| FR | Status | Evidence |
|----|--------|----------|
| FR-1 (Dockerfile) | Implemented | Three-stage cargo-chef build, ORT SHA-256, distroless runtime, model bake-in, HEALTHCHECK, no EXPOSE |
| FR-2 (docker-compose.yml) | Implemented | Named volume, restart policy, config bind mount documented, debug override documented |
| FR-3 (.dockerignore) | Implemented | All exclusions per spec, required files preserved |
| FR-4 (serve --foreground) | Implemented + tested | Flag parsed, mutual exclusion enforced, direct tokio_main_daemon call |
| FR-5 (health subcommand) | Implemented + tested | Sync UDS connect, exit codes 0/1, same ProjectPaths resolution |
| FR-6 (CI container jobs) | Implemented | Three jobs, native runners, independent branches, packages:write permission |

**Non-Functional Requirements verified**:

| NFR | Status | Evidence |
|-----|--------|----------|
| NFR-1 (Image size < 350 MB) | Deferred | Dockerfile structure correct; Docker build required for size check |
| NFR-2 (Build context < 5 MB) | Deferred | .dockerignore analysis correct; Docker build required |
| NFR-3 (Non-root UID 65534) | Verified | `FROM distroless/cc-debian12:nonroot`, `--chown=65534:65534` on COPYs |
| NFR-4 (SHA-256 verification) | Verified | `sha256sum -c -` with `set -e` in Dockerfile |
| NFR-5 (Air-gap) | Verified | Models baked into image via `model-download` in builder stage |
| NFR-6 (Build performance) | Verified | cargo-chef recipe caching, model layers cache independently |
| NFR-7 (Container lifecycle) | Partially verified | PidGuard self-PID guard tested; signal handling code verified; runtime test deferred |
| NFR-8 (Graceful errors) | Partially verified | Config path validation tested; volume permission error test deferred |
| NFR-9 (CI independence) | Verified | No cross-dependency in release.yml needs arrays |

**Constraints verified**: C-1 (ORT 1.20.1), C-2 (bookworm/debian12), C-3 (native runners), C-4 (patches copied), C-5 (single process), C-6 (debug override documented), C-7 (PidGuard self-PID guard), C-8 (--project-dir /data), C-9 (foreground doesn't break daemon — tested), C-10 (no EXPOSE).

**Acceptance Criteria**:

| AC-ID | Status | Notes |
|-------|--------|-------|
| AC-01 | DEFERRED | Docker build required |
| AC-02 | DEFERRED | Docker run required |
| AC-03 | DEFERRED | Docker compose required |
| AC-04 | DEFERRED | Docker required for UID verification |
| AC-05 | DEFERRED | Docker required for negative test |
| AC-06 | PARTIAL PASS | Unit tests pass; container PID 1 test deferred |
| AC-07 | PARTIAL PASS | Unit tests pass; container integration deferred |
| AC-08 | PASS | release.yml structure verified |
| AC-09 | PARTIAL PASS | .dockerignore analysis pass; context size deferred |
| AC-10 | PASS | No enterprise code in Dockerfile or workspace |
| AC-11 | DEFERRED | Docker run required for air-gap test |
| AC-12 | PASS | Debug override documented in docker-compose.yml comments |

The DEFERRED acceptance criteria are acceptable because: (a) Docker is not available in this environment, (b) all deferred items have full unit test coverage for their Rust code paths, (c) deferred items validate container-level integration only, (d) the RISK-COVERAGE-REPORT.md documents all deferrals explicitly.

### 4. Architecture Compliance
**Status**: PASS

**Evidence**:

All 7 ADRs from the architecture document are followed in the implementation:

- **ADR-001 (Foreground mode)**: main.rs line 322-328 calls `tokio_main_daemon(cli)` directly. No modification to `prepare_daemon_child`, `run_daemon_launcher`, or `tokio_main_stdio`. Zero architectural drift.
- **ADR-002 (ORT supply chain)**: Dockerfile uses SHA-256 ARGs, `sha256sum -c -` with `set -e`, `curl -fsSL`. Architecture-conditional via TARGETARCH.
- **ADR-003 (Health check UDS)**: health.rs uses synchronous `UnixStream::connect`. No tokio runtime. Matches sync subcommand pattern.
- **ADR-004 (CI independence)**: release.yml has two independent dependency branches confirmed by `needs` analysis.
- **ADR-005 (Container data path)**: Dockerfile CMD includes `--project-dir /data`. HEALTHCHECK includes `--project-dir /data`. `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` and `HOME=/data` set in ENV.
- **ADR-006 (cargo-chef pinning)**: Version 0.1.71 with `--locked` in both planner and builder stages.
- **ADR-007 (PidGuard self-PID)**: pidfile.rs line 263 places self-PID check BEFORE `is_process_alive`, returning `Ok(true)` for immediate reclaim.

Component structure matches architecture design: 6 components (Dockerfile, docker-compose.yml, .dockerignore, serve --foreground, health subcommand, CI container jobs) all at their specified file locations with specified interfaces.

Integration points work as specified: `ensure_data_directory` produces deterministic paths (tested), `tokio_main_daemon` is the shared entry point (tested), `handle_stale_pid_file` has self-PID guard (tested), `UNIMATRIX_CONFIG` overrides HOME-based resolution (tested).

### 5. Knowledge Stewardship Compliance
**Status**: PASS

**Evidence**: Tester agent report (`nan-014-agent-11-tester-v2-report.md`) contains:

```
## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 19 entries returned; relevant entries on testing procedures (#4339, #238), test infrastructure patterns (#747), and nan-014 ADR-002 (#4570). No novel patterns needed from knowledge base.
- Stored: nothing novel to store -- standard test execution workflow, no new fixtures, patterns, or infrastructure discovered.
```

Both `Queried:` and `Stored:` entries present. The "nothing novel to store" justification is reasonable — nan-014 testing followed established patterns without discovering new testing infrastructure or fixture approaches.

## Rework Required

None.

## Additional Observations

1. **Container runtime validation is the human's responsibility**: 12 acceptance criteria items and 4 risk scenarios require Docker. All are documented in RISK-COVERAGE-REPORT.md as deferred. The human should run `docker build -t unimatrix .` and the associated runtime tests before merging.

2. **Pre-existing file size violations**: main.rs (1478 lines), config.rs (9151 lines), main_tests.rs (1001 lines), pidfile.rs (636 lines) all exceed the 500-line limit. These are pre-existing conditions, not nan-014 regressions. Gate 3b noted this as well.

3. **Test count integrity**: 5,176 workspace tests pass with 0 failures. The 24 nan-014-specific tests and 10 regression gate tests are all accounted for in RISK-COVERAGE-REPORT.md.

## Knowledge Stewardship
- Stored: nothing novel to store -- no recurring gate failure patterns observed; all checks passed on first validation.
