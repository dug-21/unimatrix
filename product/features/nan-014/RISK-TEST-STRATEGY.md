# Risk-Based Test Strategy: nan-014

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `--foreground` flag breaks existing `--daemon` startup path | High | Low | High |
| R-02 | PidGuard stale file handling incorrect when container restarts with retained volume (PID 1 reuse across PID namespaces) | High | Med | High |
| R-03 | `health` subcommand resolves a different socket path than `serve --foreground`, causing HEALTHCHECK to always fail | High | Med | High |
| R-04 | ORT SHA-256 verification script has architecture-conditional logic error (wrong hash for TARGETARCH, or hash constant transcription error) | High | Med | High |
| R-05 | Foreground mode does not receive SIGTERM because PID 1 in containers has no default signal disposition | High | Low | High |
| R-06 | `/data` volume ownership drift on existing named volumes causes daemon startup panic instead of actionable error | Med | Med | Med |
| R-07 | `cargo-chef` recipe extraction fails for the 9-crate workspace with `patches/anndists` path dependency | Med | Med | Med |
| R-08 | Model bake-in path mismatch: builder stage downloads models to one path, runtime COPY references a different path | Med | Med | Med |
| R-09 | `.dockerignore` excludes a required file (e.g., `.cargo/config.toml`, `patches/` source, `Cargo.lock`) causing build failure | Med | Med | Med |
| R-10 | Container build jobs block binary/npm release if `needs` dependency graph is wired incorrectly | Med | Low | Med |
| R-11 | `--foreground` and `--daemon` not mutually exclusive — user passes both, undefined behavior | Low | Low | Low |
| R-12 | Image size exceeds 350 MB budget due to debug symbols, duplicate ORT copies, or unbaked model residue | Low | Med | Low |
| R-13 | `HOME=/data` in container causes `dirs::config_dir()` to resolve under `/data`, breaking config.toml bind mount at `/etc/unimatrix/config.toml` | Med | Med | Med |
| R-14 | distroless base image tag `:nonroot` changes glibc version in a future update, causing binary segfault | Med | Low | Low |

## Risk-to-Scenario Mapping

### R-01: --foreground breaks existing --daemon startup path
**Severity**: High
**Likelihood**: Low
**Impact**: Regression in the primary non-container deployment mode. All existing users who run `unimatrix serve --daemon` would be broken by a release.

**Test Scenarios**:
1. Run `unimatrix serve --daemon` after the foreground flag is added. Verify the launcher spawns a child, the child calls `prepare_daemon_child` (setsid), and `tokio_main_daemon` starts the UDS listener. Confirm the existing daemon integration tests pass unchanged.
2. Run `unimatrix serve --foreground`. Verify `tokio_main_daemon` is called directly without `prepare_daemon_child`. Confirm UDS listener, tick loop, and ML inference all function.
3. Verify that the `Serve` enum variant's `foreground` field defaults to `false` so that `serve --daemon` dispatch is unchanged.

**Coverage Requirement**: The existing daemon integration test suite must pass with zero modifications. Any test change is a regression signal. New foreground-specific tests must verify the skip of setsid and direct `tokio_main_daemon` call.

### R-02: PidGuard stale file handling in container restart
**Severity**: High
**Likelihood**: Med
**Impact**: Container fails to start on restart because PidGuard cannot reclaim the lock. The stale PID file on the volume says PID 1, and PID 1 in the new container IS the new unimatrix process (not yet fully initialized), so `is_unimatrix_process(1)` may return true, leading to SIGTERM of self.

**Test Scenarios**:
1. Simulate container restart: create a PID file containing `1` on a mock volume, start the daemon. Verify `handle_stale_pid_file` correctly resolves — either the process is dead (new container, PID 1 not yet running at PID file check time) or `is_unimatrix_process` identifies PID 1 as the current process and PidGuard acquires the lock.
2. Test with a PID file containing a non-existent PID (e.g., 99999). Verify `is_process_alive` returns false and PidGuard reclaims.
3. Test the race window: if `handle_stale_pid_file` runs BEFORE `PidGuard::acquire`, and PID 1 is the current process, verify the code does not SIGTERM itself. The sequence is: `handle_stale_pid_file` → reads PID 1 → `is_process_alive(1)` returns true → `is_unimatrix_process(1)` returns true (it IS the current process) → sends SIGTERM to self.

**Coverage Requirement**: The PID 1 self-SIGTERM scenario must be explicitly tested or architecturally prevented. This is the highest-likelihood container-specific bug. The `handle_stale_pid_file` call sequence relative to PidGuard acquisition must be verified.

### R-03: Health subcommand socket path divergence
**Severity**: High
**Likelihood**: Med
**Impact**: HEALTHCHECK always returns unhealthy. Docker marks the container as unhealthy after 3 retries. Orchestrators may restart the container in a loop.

**Test Scenarios**:
1. Start `serve --foreground` with `--project-dir /tmp/test-data`. Run `health --project-dir /tmp/test-data`. Verify `health` connects to the same socket that `serve` created by checking the resolved `mcp_socket_path` is identical.
2. Start `serve --foreground` without `--project-dir`. Run `health` without `--project-dir`. Verify both resolve the same `ProjectPaths` from the same `HOME` and `cwd`.
3. Start `serve --foreground` with `--project-dir /data` and `HOME=/data`. Run `health` with `--project-dir /data` and `HOME=/data`. Verify the socket paths match — this is the exact container environment.

**Coverage Requirement**: A test must assert that `ensure_data_directory(Some("/data"), None)` produces the same `mcp_socket_path` when called twice with the same inputs. Additionally, an integration test should start foreground mode and run the health check binary against the live socket.

### R-04: ORT SHA-256 verification logic error
**Severity**: High
**Likelihood**: Med
**Impact**: Wrong ORT binary for the architecture. Binary crash at runtime (aarch64 binary loaded on x64 or vice versa), or build succeeds with a tampered binary.

**Test Scenarios**:
1. Build the Dockerfile with the correct SHA-256 hashes on x64. Verify build succeeds and `libonnxruntime.so` is the correct architecture (`file libonnxruntime.so` output matches).
2. Modify one SHA-256 hash to an incorrect value. Verify `docker build` fails at the `sha256sum -c` step with a clear checksum error.
3. Verify the `TARGETARCH` conditional logic: build with `--platform linux/amd64` and confirm the x64 hash is used; build with `--platform linux/arm64` and confirm the arm64 hash is used. Check that the arch string mapping (`arm64` -> `aarch64`, `amd64` -> `x64`) is correct.

**Coverage Requirement**: The tampered-hash test (scenario 2) is the critical gate. It must be run during implementation validation. Architecture selection logic should be reviewed line-by-line.

### R-05: SIGTERM not received by PID 1 in container
**Severity**: High
**Likelihood**: Low
**Impact**: `docker stop` sends SIGTERM, daemon ignores it, Docker waits `stop_grace_period` then SIGKILLs. Ungraceful shutdown: vector index not persisted, database not compacted, data loss.

**Test Scenarios**:
1. Run `unimatrix serve --foreground` as PID 1 (e.g., inside a container). Send SIGTERM. Verify graceful shutdown logs appear (vector dump, DB compaction, PidGuard release).
2. Verify that `shutdown::shutdown_signal()` explicitly registers SIGTERM via `tokio::signal::unix::signal(SignalKind::terminate())` — this is what makes PID 1 signal handling work. Confirm this code path is exercised in foreground mode.
3. Run `docker stop` on a running container. Verify the exit code is 0 (clean shutdown) and logs show graceful shutdown, not a SIGKILL.

**Coverage Requirement**: The explicit SIGTERM registration in `shutdown_signal()` is already implemented and tested. The container-specific test is: `docker stop` produces graceful shutdown logs. If `stop_grace_period` is not set in compose, verify the default 10s is sufficient for the daemon's shutdown sequence.

### R-06: Volume permission errors produce opaque panic
**Severity**: Med
**Likelihood**: Med
**Impact**: User sees a panic backtrace or generic "Permission denied" instead of an actionable error message explaining that `/data` ownership must be UID 65534.

**Test Scenarios**:
1. Create `/data` owned by root (UID 0). Run the daemon as UID 65534. Verify the error message names the path and expected ownership, not a bare `io::Error`.
2. Run with a read-only `/data` directory. Verify the daemon fails with a clear message, not a panic.

**Coverage Requirement**: The `ensure_data_directory` function must map `PermissionDenied` to a context-rich error. Test by mocking a non-writable directory.

### R-07: cargo-chef recipe extraction fails with workspace patch
**Severity**: Med
**Likelihood**: Med
**Impact**: Docker build fails at the `cargo chef prepare` step. No image produced.

**Test Scenarios**:
1. Run `cargo chef prepare --recipe-path recipe.json` locally from the workspace root. Verify it succeeds and the recipe JSON includes the `patches/anndists` dependency.
2. In the Dockerfile, verify that `COPY patches/ patches/` happens BEFORE `cargo chef prepare` and `cargo chef cook`.

**Coverage Requirement**: The `cargo chef prepare` step must be tested locally before the Dockerfile is finalized. The `patches/` directory copy order in the Dockerfile must be reviewed.

### R-08: Model bake-in path mismatch
**Severity**: Med
**Likelihood**: Med
**Impact**: Container starts but embedding/NLI models are not found. ML inference fails. All knowledge operations return errors.

**Test Scenarios**:
1. In the builder stage, after `unimatrix model-download`, print the actual file paths of the downloaded models. Verify the `COPY --from=models` directive in the runtime stage references these exact paths.
2. Run the container with `--network=none`. Verify both models load successfully from logs (no "model not found" or "downloading model" messages).

**Coverage Requirement**: The model paths must be traced through: download location (determined by `XDG_CACHE_HOME` or `EmbedConfig::resolve_cache_dir`) -> COPY source in Dockerfile -> runtime environment variable or config that tells the daemon where to find models.

### R-09: .dockerignore excludes required build files
**Severity**: Med
**Likelihood**: Med
**Impact**: Docker build fails with missing file errors. Could be subtle — e.g., `.cargo/config.toml` excluded by a `*.toml` glob, causing ORT link errors instead of a clear missing-file error.

**Test Scenarios**:
1. Run `docker build` from the repo root. Verify the build context size is under 5 MB.
2. Verify `.cargo/config.toml` is NOT excluded (it contains `ORT_LIB_LOCATION` and `ORT_PREFER_DYNAMIC_LINK`).
3. Verify `patches/anndists/Cargo.toml` and `patches/anndists/src/` are included but `patches/anndists/target/` is excluded.
4. Verify all workspace `Cargo.toml` files and `Cargo.lock` are included.

**Coverage Requirement**: The `.dockerignore` must be reviewed against the full list of files needed by `cargo build --workspace`. A successful `docker build` is the integration test.

### R-10: Container CI jobs block binary/npm release
**Severity**: Med
**Likelihood**: Low
**Impact**: ARM64 runner unavailability delays or blocks the entire release, even though binaries and npm packages are ready.

**Test Scenarios**:
1. Review the `release.yml` dependency graph. Verify `create-release` and `package-npm` have NO `needs` dependency on `build-container-*` or `create-container-manifest`.
2. Simulate: if `build-container-arm64` fails, verify `create-release` still runs.

**Coverage Requirement**: Static analysis of the workflow YAML. The `needs` arrays of `package-npm` and `create-release` must not reference any container job.

### R-11: --foreground and --daemon not mutually exclusive
**Severity**: Low
**Likelihood**: Low
**Impact**: User passes both flags, behavior is undefined (whichever match arm fires first).

**Test Scenarios**:
1. Run `unimatrix serve --foreground --daemon`. Verify clap rejects with a clear error message.
2. Run `unimatrix serve --foreground --stdio`. Verify clap rejects.

**Coverage Requirement**: Unit test for clap validation. Use `Cli::try_parse_from` with conflicting flags and assert error.

### R-12: Image exceeds 350 MB size budget
**Severity**: Low
**Likelihood**: Med
**Impact**: Larger pull times, more registry storage. Not a functional failure but violates NFR-1.

**Test Scenarios**:
1. Build the image. Run `docker images unimatrix --format '{{.Size}}'`. Verify under 350 MB.
2. If over budget, check for: unstripped binary, duplicate ORT copies, build artifacts leaked into runtime stage.

**Coverage Requirement**: Image size check in CI (can be a comment in the PR, or a container test job step).

### R-13: HOME=/data breaks config.toml bind mount discovery
**Severity**: Med
**Likelihood**: Med
**Impact**: The daemon's `load_config` resolves config from `$HOME/.config/...` which lands inside `/data`, ignoring the bind-mounted `/etc/unimatrix/config.toml`. User-provided config is silently ignored.

**Test Scenarios**:
1. Set `HOME=/data`. Run `load_config` with a bind-mounted config at `/etc/unimatrix/config.toml`. Verify the daemon loads the bind-mounted config, not a default from `/data/.config/`.
2. Verify the daemon has a `--config` flag or `UNIMATRIX_CONFIG` env var that overrides `HOME`-based resolution.

**Coverage Requirement**: The config loading path must be traced with `HOME=/data`. If no explicit config path override exists, this is a functional gap the implementation agent must address.

### R-14: Distroless base image tag changes glibc version
**Severity**: Med
**Likelihood**: Low
**Impact**: Binary compiled against glibc 2.36 (bookworm) fails at runtime if distroless updates to glibc 2.38+ (trixie). Segfault or "GLIBC_X.XX not found" error.

**Test Scenarios**:
1. Verify the Dockerfile uses `gcr.io/distroless/cc-debian12:nonroot` (Debian 12 pinned, not `cc:nonroot` which tracks latest Debian).
2. If using tag-only reference, document the digest at build time for future comparison.

**Coverage Requirement**: Code review of the `FROM` directive. Digest pinning is a hardening follow-up (deferred per spec), but the Debian version must be pinned.

## Integration Risks

- **ProjectPaths resolution chain**: `serve --foreground`, `health`, and `stop` all resolve paths via `ensure_data_directory`. In the container, `HOME=/data` + `--project-dir /data` creates a non-standard resolution context. The hash of `/data` is stable but the base directory becomes `/data/.unimatrix/`. Any code that assumes `~/.unimatrix/` (with HOME as a user's home directory) may break.
- **ORT library loading at runtime**: The builder copies `libonnxruntime.so` to `/usr/local/lib/`. The runtime stage sets `LD_LIBRARY_PATH=/usr/local/lib`. If the COPY destination or env var diverge, the binary starts but crashes on first embedding operation with a dynamic linker error.
- **Model download in builder stage**: The builder runs the just-compiled `unimatrix model-download` binary to fetch models. This binary needs ORT to initialize. If ORT is not correctly installed in the builder stage before this step, the model download fails silently or with an opaque error.
- **PidGuard + flock across container restarts**: The flock is released when the process exits, but the PID file persists on the named volume. On restart, `handle_stale_pid_file` reads PID 1. In the new container, PID 1 may or may not be running yet at the time of the check — this is a race condition between `handle_stale_pid_file` and the startup sequence.

## Edge Cases

- **Empty `/data` volume (first run)**: `ensure_data_directory` must create all subdirectories. Verify the full directory tree is created with correct ownership.
- **Pre-existing `/data` volume from older version**: Schema migration must run on container upgrade. If the old database is from a newer version (downgrade), the daemon must fail gracefully with a version mismatch error, not corrupt the database.
- **Socket path length limit**: Unix socket paths have a 108-byte limit on Linux. The path `/data/.unimatrix/{16-char-hash}/unimatrix-mcp.sock` is ~60 bytes — safe, but verify the hash is exactly 16 chars.
- **Concurrent `docker run` on same volume**: Two containers mounting the same named volume. PidGuard's flock should prevent dual-start, but the error message must be clear ("another instance is running"), not a deadlock.
- **`HEALTHCHECK` during startup**: The daemon takes time to initialize (schema migration, model loading, index building). `--start-period=10s` in the HEALTHCHECK directive gives a grace window, but if startup takes longer on ARM64 or large volumes, the container may be marked unhealthy prematurely.
- **SIGKILL after SIGTERM timeout**: If graceful shutdown exceeds Docker's `stop_grace_period` (default 10s), Docker SIGKILLs. The database may have uncommitted WAL entries. SQLite WAL recovery on next start must be verified.

## Security Risks

- **ORT tarball download (untrusted input)**: The Dockerfile downloads a tarball from GitHub Releases. If the SHA-256 gate is implemented incorrectly (e.g., hash checked but result ignored, or wrong file hashed), a compromised ORT binary gains code execution inside the build and all future container runs. Blast radius: full compromise of the container and any mounted volumes. Mitigation: SHA-256 verification with `set -e` and `sha256sum -c` producing a non-zero exit on mismatch.
- **cargo-chef from crates.io (untrusted input)**: Installed via `cargo install` from crates.io. A compromised cargo-chef version could inject code into the compiled binary. Blast radius: same as ORT — full container compromise. Mitigation: version pinning + `--locked`. No binary hash verification is practical.
- **`/data` volume contents (semi-trusted input)**: The daemon reads databases, PID files, and config from the mounted volume. A malicious or corrupt database could trigger SQLite parsing bugs. Blast radius: limited to the daemon process (no root, no network listener until W2-2). Mitigation: SQLite's built-in integrity checks; daemon runs as UID 65534.
- **UDS socket (trusted input in container context)**: The MCP UDS socket is accessible only within the container's filesystem namespace. No external network exposure until W2-2. Blast radius: contained to the container. No additional mitigation needed for nan-014.
- **No EXPOSE directive**: Correct — no network listener exists. Shipping `EXPOSE 8443` before W2-2 would mislead users into thinking a port is open and potentially configuring firewall rules prematurely.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| ORT SHA-256 mismatch during build | Build fails immediately at `sha256sum -c`. Clear error: "FAILED" with hash comparison. |
| Model download fails during build | Build fails at `unimatrix model-download`. Error output shows download failure reason. |
| `/data` not writable at startup | Daemon exits with error message naming the path and required ownership (UID 65534). No panic. |
| PidGuard cannot acquire lock (another instance) | Daemon exits with `DatabaseLocked` error. Message: "another Unimatrix instance is running." |
| SIGTERM received during operation | Graceful shutdown: cancel tick loop, persist vector index, compact database, release PidGuard, exit 0. |
| SIGKILL after timeout | Ungraceful exit. On next start: SQLite WAL recovery runs automatically. PidGuard stale file handling reclaims lock. |
| Health check timeout (daemon overloaded) | `unimatrix health` exits 1 after 3-second timeout. Docker retries 3 times before marking unhealthy. |
| ARM64 CI runner unavailable | Container build jobs fail. Binary/npm release proceeds independently (ADR-004). |
| GHCR push auth failure | Container build job fails. Workflow logs show auth error. Binary release unaffected. |
| Container image pulled on unsupported arch | Docker fails with "no matching manifest for platform" if only x64/arm64 are published. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (ORT download SPOF + supply chain) | R-04 | SHA-256 verification gate (ADR-002). Mirror/cache fallback deferred as hardening follow-up. |
| SR-02 (cargo-chef unpinned version) | R-07 | Version pinned with `--locked` (ADR-006). Binary hash verification deferred — impractical for cargo-install. |
| SR-03 (Model baking increases build time) | R-08 | Separate model download stage in Dockerfile. Model layers cache independently of source changes. |
| SR-04 (Distroless mutable tag) | R-14 | Debian version pinned via `cc-debian12`. Full digest pinning deferred per spec (hardening follow-up). |
| SR-05 (Volume layout divergence from roadmap) | R-13 | `HOME=/data` puts all data under single volume. Migration path to multi-volume documented but not tested — accepted as future W2-3 concern. |
| SR-06 (--foreground breaks --daemon) | R-01 | ADR-001: direct `tokio_main_daemon` call, zero modification to daemon path. Existing daemon tests are the regression gate. |
| SR-07 (Premature EXPOSE directive) | -- | Spec constraint C-10 explicitly forbids EXPOSE. No architecture risk — resolved by omission. |
| SR-08 (Volume ownership drift) | R-06 | Daemon must produce actionable error on permission failure, not panic. NFR-8 in spec. |
| SR-09 (PidGuard + container restart) | R-02 | PidGuard's `/proc/{pid}/cmdline` check works inside containers. Self-SIGTERM race on PID 1 is the residual risk. |
| SR-10 (CI pipeline coupling) | R-10 | Independent job branches (ADR-004). No `needs` cross-dependency. |
| SR-11 (Health socket path divergence) | R-03 | Both commands use `--project-dir /data` with same `ensure_data_directory` (ADR-005). Deterministic hash. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 5 (R-01 through R-05) | 14 scenarios |
| Medium | 5 (R-06 through R-10, R-13) | 11 scenarios |
| Low | 3 (R-11, R-12, R-14) | 5 scenarios |
| **Total** | **14** | **30 scenarios** |
