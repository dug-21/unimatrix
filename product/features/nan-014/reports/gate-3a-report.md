# Gate 3a Report: nan-014

> Gate: 3a (Component Design Review)
> Date: 2026-05-23
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | WARN | FR-1.4 spec/arch mismatch on model stage (see details) |
| Specification coverage | WARN | FR-5.7 pseudocode inconsistency (print "healthy" vs. no output) |
| Risk coverage | PASS | All 14 risks, 30 scenarios mapped with tests |
| Interface consistency | PASS | All shared types consistent across pseudocode files |
| Knowledge stewardship | FAIL | Architect report missing Knowledge Stewardship block |

## Detailed Findings

### 1. Architecture Alignment
**Status**: WARN

**Evidence**: The architecture (ARCHITECTURE.md Component 1) specifies model download happens IN the builder stage, NOT as a separate stage:

> "Model download happens in the builder stage (not a separate stage), after the binary is compiled, so it uses the just-built binary directly."

The Dockerfile pseudocode (`dockerfile.md`) implements this correctly -- model download is in Stage 2 (builder) at lines 118-122, not a separate stage.

However, the SPECIFICATION (FR-1.4) says:

> "A separate model-download stage runs `unimatrix model-download`..."

And the Specification Domain Models section lists four stages:

> Stage 3: models -- builder (reuse) -- Run model-download for both models

The architecture explicitly overrode this spec detail and the pseudocode follows the architecture. This is architecturally coherent but the spec and architecture disagree. Since the architecture is the authoritative design document and the pseudocode follows it correctly, this is a WARN not a FAIL. The spec should be reconciled but does not block implementation.

**Issue**: Spec FR-1.4 describes a "separate model-download stage" but the architecture and pseudocode both place model download in the builder stage. Minor inconsistency; architecture's rationale is sound.

### 2. Specification Coverage
**Status**: WARN

**Evidence**: All functional requirements (FR-1 through FR-6), non-functional requirements (NFR-1 through NFR-9), and constraints (C-1 through C-10) are addressed in pseudocode.

One minor inconsistency: FR-5.7 says "No output on success (exit 0 only)." The health-subcommand pseudocode has two versions:

1. Initial pseudocode (line 71): `// Print nothing to stdout on success (FR-5.7).` -- correct.
2. `test_health_run_success_on_live_socket` in test-plan (line 99): `"healthy" printed to stdout` -- contradicts FR-5.7.

The architecture (ADR-003, line 24) says: `prints "healthy" to stdout, returns exit code 0`.

The simplified implementation pseudocode (lines 112-129) correctly prints nothing on success (`Ok(())`), but the architecture and test plan reference "healthy" output. The pseudocode follows the spec, which is correct.

**Issue**: Test plan scenario `test_health_run_success_on_live_socket` asserts "healthy" printed to stdout, contradicting FR-5.7 ("No output on success"). The test plan should assert no stdout output on success.

### 3. Risk Coverage
**Status**: PASS

**Evidence**: All 14 risks from the Risk-Based Test Strategy are mapped to test scenarios in the component test plans.

| Risk | Priority | Covered By Test Plan | Scenario Count Match |
|------|----------|---------------------|---------------------|
| R-01 | High | serve-foreground (3 scenarios) | Yes |
| R-02 | High | pidguard-self-pid (3 scenarios) | Yes |
| R-03 | High | health-subcommand (3 scenarios) | Yes |
| R-04 | High | dockerfile (3 scenarios) | Yes |
| R-05 | High | serve-foreground + dockerfile (2 scenarios) | Yes |
| R-06 | Med | serve-foreground (2 scenarios) | Yes |
| R-07 | Med | dockerfile + dockerignore (2 scenarios) | Yes |
| R-08 | Med | dockerfile (2 scenarios) | Yes |
| R-09 | Med | dockerignore (4 scenarios) | Yes |
| R-10 | Med | ci-container-jobs (2 scenarios) | Yes |
| R-11 | Low | serve-foreground (2 scenarios) | Yes |
| R-12 | Low | dockerfile (1 scenario) | Yes |
| R-13 | Med | config-env-override (2 scenarios) | Yes |
| R-14 | Low | dockerfile (1 scenario) | Yes |
| **Total** | | | **30 scenarios** |

Integration risks (ProjectPaths resolution, ORT library loading, model download in builder, PidGuard+flock) are all addressed through cross-component test dependencies documented in test-plan/OVERVIEW.md.

Edge cases from the risk strategy (empty /data volume, pre-existing volume from older version, socket path length, concurrent docker run, HEALTHCHECK during startup, SIGKILL after timeout) are referenced in relevant component test plans.

### 4. Interface Consistency
**Status**: PASS

**Evidence**: All interfaces defined in the pseudocode OVERVIEW.md match their usage across component pseudocode files.

- `Command::Serve { foreground: bool }`: Defined in OVERVIEW.md, implemented identically in serve-foreground.md (lines 17-32).
- `Command::Health`: Defined in OVERVIEW.md, implemented in health-subcommand.md (lines 135-143).
- `health::run(Option<&Path>)`: Signature in OVERVIEW.md matches health-subcommand.md (line 26). The testability note suggests returning `i32` instead, which is an acceptable implementation refinement.
- `tokio_main_daemon(Cli)`: Referenced as unchanged in both serve-foreground.md (line 52) and OVERVIEW.md. No signature change proposed. Consistent.
- `ensure_data_directory(Option<&Path>, Option<&Path>)`: Used identically in health-subcommand.md (line 44) and referenced in serve-foreground.md.
- `handle_stale_pid_file(&Path, Duration)`: Existing signature preserved in pidguard-self-pid.md (line 17). Return type unchanged.
- `UNIMATRIX_CONFIG` env var: Set in Dockerfile pseudocode (line 153), read in config-env-override.md (line 32), referenced in docker-compose.md (line 28).
- `--project-dir /data`: Used consistently in Dockerfile CMD (line 166), HEALTHCHECK (line 160), health-subcommand pseudocode (line 44), and serve-foreground data flow.

The data flow diagram in OVERVIEW.md (lines 23-61) accurately reflects the component interactions. No contradictions found between pseudocode files.

### 5. Knowledge Stewardship Compliance
**Status**: FAIL

**Evidence**:

| Agent | Report File | Has Block | Entries |
|-------|------------|-----------|---------|
| architect | nan-014-agent-1-architect-report.md | NO | -- |
| pseudocode | nan-014-agent-1-pseudocode-report.md | YES | Queried: 4 entries |
| spec | nan-014-agent-2-spec-report.md | YES | Queried: briefing |
| testplan | nan-014-agent-2-testplan-report.md | YES | Queried + Stored (nothing novel) |
| researcher | nan-014-researcher-report.md | YES | Queried + Stored (nothing novel) |
| vision-guardian | nan-014-vision-guardian-report.md | YES | Queried + Stored (nothing novel) |
| synthesizer | nan-014-synthesizer-report.md | NO | -- |
| synthesizer-v2 | nan-014-synthesizer-v2-report.md | NO | -- |

The **architect** agent report (`nan-014-agent-1-architect-report.md`) has NO `## Knowledge Stewardship` block. The architect is an active-storage agent that should have `Stored:` entries for the 7 ADRs it created (which were stored as Unimatrix entries #4569-#4575 per the report). The ADRs were stored but the stewardship block documenting this is absent.

The **synthesizer** and **synthesizer-v2** reports also lack stewardship blocks, but synthesizers are typically process-coordination agents and may not be required to have stewardship blocks. The architect, however, is explicitly listed as an active-storage agent and its missing block is a REWORKABLE FAIL per gate rules.

**Issue**: Architect report missing `## Knowledge Stewardship` block. Must be added with `Stored:` entries referencing the ADR Unimatrix entries (#4569-#4575).

### Specific Validation Targets

#### PidGuard Self-PID Guard (ADR-007)
**Status**: PASS

The pseudocode in `pidguard-self-pid.md` correctly implements ADR-007:
- Self-PID check (`if pid == std::process::id()`) placed BEFORE `is_process_alive` (line 33).
- Returns `Ok(true)` to signal reclaim-ready (line 36).
- Tracing log includes PID for diagnostics (line 34).
- Existing code paths below the guard remain unchanged (lines 38-53).
- Test plan covers: self-PID detection, PID 1 simulation, non-self PID regression, existing daemon test pass-through.

#### UNIMATRIX_CONFIG Env Var (ADR-005)
**Status**: PASS

The pseudocode in `config-env-override.md` correctly implements the ADR-005 config discovery:
- Env var checked first, before all other config sources (line 32).
- File existence check before loading (line 35-36).
- Graceful fallback when env var set but file missing (lines 39-41).
- Merge precedence documented: env > project > global > defaults (lines 87-90).
- Test plan covers R-13 with env var override, fallback, nonexistent file, and merge precedence tests.

#### Foreground Mode (ADR-001)
**Status**: PASS

The pseudocode in `serve-foreground.md` correctly implements ADR-001:
- `conflicts_with_all = ["daemon", "stdio"]` on the foreground field (line 30).
- Match arm calls `tokio_main_daemon(cli)` directly, no `prepare_daemon_child` (lines 51-52).
- Placed BEFORE the daemon match arm for correct pattern matching priority (line 41).
- Lists explicitly what is NOT modified: `tokio_main_daemon`, `tokio_main_stdio`, `prepare_daemon_child`, etc. (lines 84-91).
- Test plan covers clap mutual exclusion, default values, and regression gates.

#### Health Subcommand (ADR-003)
**Status**: PASS

The pseudocode in `health-subcommand.md` correctly implements ADR-003:
- Sync path, no tokio runtime (line 6).
- Uses `std::os::unix::net::UnixStream::connect` (line 122).
- Resolves `ProjectPaths` via `ensure_data_directory(project_dir, None)` -- same as serve (line 44).
- Exit 0 on success, exit 1 on failure (lines 122-128).
- Test plan covers socket path consistency (R-03), live socket, missing socket, and container path resolution.

## Rework Required (if REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Missing Knowledge Stewardship block in architect report | architect (or coordinator) | Add `## Knowledge Stewardship` section to `nan-014-agent-1-architect-report.md` with `Stored:` entries listing Unimatrix entries #4569-#4575 (ADR-001 through ADR-007) |
| Test plan stdout assertion contradicts FR-5.7 | testplan agent | Fix `test_health_run_success_on_live_socket` in `test-plan/health-subcommand.md` to assert NO stdout output on success, not "healthy" printed to stdout |
