# nan-014: Container Packaging (MIT Image) — Implementation Brief

> v2 — Re-synthesized 2026-05-23. Two design issues resolved since v1:
> 1. PidGuard self-PID guard (ADR-007 added) — concrete fix, no longer ambiguous
> 2. HOME=/data config discovery resolved — UNIMATRIX_CONFIG env var + code change

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nan-014/SCOPE.md |
| Scope Risk Assessment | product/features/nan-014/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-014/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-014/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/nan-014/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-014/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| Dockerfile | pseudocode/dockerfile.md | test-plan/dockerfile.md |
| docker-compose | pseudocode/docker-compose.md | test-plan/docker-compose.md |
| dockerignore | pseudocode/dockerignore.md | test-plan/dockerignore.md |
| serve-foreground | pseudocode/serve-foreground.md | test-plan/serve-foreground.md |
| health-subcommand | pseudocode/health-subcommand.md | test-plan/health-subcommand.md |
| ci-container-jobs | pseudocode/ci-container-jobs.md | test-plan/ci-container-jobs.md |
| pidguard-self-pid | pseudocode/pidguard-self-pid.md | test-plan/pidguard-self-pid.md |
| config-env-override | pseudocode/config-env-override.md | test-plan/config-env-override.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Deliver a production-grade container deployment path for the MIT Unimatrix binary so that any developer can run a personal cloud instance with `docker run -v unimatrix-data:/data ghcr.io/dug-21/unimatrix` -- air-gapped, non-root, dual-arch (x86_64 + ARM64), with ONNX models baked in and a CI pipeline that publishes multi-arch images to GHCR on every release tag. This is the W2-1 deliverable: "any developer deploys and operates Unimatrix without ops friction."

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Foreground mode implementation | Call `tokio_main_daemon` directly -- no function extraction needed. `tokio_main_daemon` IS the shared core; `--daemon` adds setsid before it, `--foreground` calls it directly. | ADR-001, SR-06 | architecture/ADR-001-foreground-mode-implementation.md |
| ORT supply chain verification | SHA-256 hashes as Dockerfile `ARG` values. `sha256sum -c` gate after `curl` download. Build fails on mismatch. | ADR-002, SR-01 | architecture/ADR-002-ort-supply-chain-verification.md |
| Health check mechanism | Sync `std::os::unix::net::UnixStream::connect` to `mcp_socket_path`. No tokio runtime. Exit 0 = healthy, exit 1 = unhealthy. 3s connect timeout. | ADR-003, SR-11 | architecture/ADR-003-health-check-uds-connect.md |
| CI container job independence | Container build jobs form a separate dependency branch -- no `needs` link to binary/npm jobs. Binary/npm releases never blocked by container failures. | ADR-004, SR-10 | architecture/ADR-004-ci-container-job-independence.md |
| Container data path resolution | `--project-dir /data` + `HOME=/data`. All data under `/data` volume. Project hash is `SHA-256("/data")[..16]`. Data at `/data/.unimatrix/{hash}/`. Config discovery via `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` env var (highest-priority source, checked before `dirs::config_dir()`). | ADR-005 | architecture/ADR-005-container-data-path-resolution.md |
| cargo-chef version pinning | `cargo install cargo-chef --version 0.1.71 --locked`. Version pin + lockfile verification. | ADR-006, SR-02 | architecture/ADR-006-cargo-chef-version-pinning.md |
| PidGuard self-PID guard | If `stale_pid == std::process::id()`, skip SIGTERM and reclaim PID file directly. Prevents self-termination on container restart where PID 1 is reused. General correctness fix, not container-specific. | ADR-007, SR-09, R-02 | architecture/ADR-007-pidguard-self-pid-guard.md |

## Files to Create/Modify

### New Files

| File | Summary |
|------|---------|
| `Dockerfile` | Three-stage cargo-chef build: planner, builder (ORT + compile + models + chmod 0700 /data), runtime (distroless nonroot) |
| `docker-compose.yml` | Single-service deployment with `unimatrix-data` named volume at `/data`, debug override documented in comments |
| `.dockerignore` | Exclude `target/`, `.git/`, `product/`, `packages/`, `.claude/`, `.github/`, `patches/anndists/target/`, test fixtures |
| `crates/unimatrix-server/src/health.rs` | Sync health check module: resolve ProjectPaths, connect to MCP UDS socket, exit 0/1 |

### Modified Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/main.rs` | Add `foreground: bool` field to `Serve` variant, add `Health` variant to `Command` enum, add match arms for both |
| `.github/workflows/release.yml` | Add 3 jobs (`build-container-x64`, `build-container-arm64`, `create-container-manifest`), add `packages: write` permission |
| PidGuard source (daemon/pid handling) | Add self-PID check: `if stale_pid == std::process::id() { return Ok(RecoveryAction::Reclaimed); }` before the SIGTERM path in `handle_stale_pid_file` |
| Config loading path | Add `std::env::var("UNIMATRIX_CONFIG")` as highest-priority config file source, checked before `dirs::config_dir()` fallback |

## Data Structures

### CLI Additions (main.rs)

```rust
#[derive(Debug, Subcommand)]
enum Command {
    // ... existing variants ...

    Serve {
        #[arg(long)]
        daemon: bool,
        #[arg(long)]
        stdio: bool,
        /// Run as PID 1 foreground process (container mode).
        #[arg(long, conflicts_with_all = ["daemon", "stdio"])]
        foreground: bool,
    },

    /// Check daemon liveness via UDS socket connect.
    Health,
}
```

### Volume Layout (Container)

```
/data/                                  # Named volume: unimatrix-data (chmod 0700)
  .unimatrix/
    {hash-of-data}/                     # ProjectPaths data_dir
      unimatrix.db                      # Knowledge database
      vector/                           # HNSW index files
      unimatrix.pid                     # PID file (PID 1 in container)
      unimatrix-mcp.sock               # MCP UDS socket
      unimatrix.log                     # Daemon log
  .cache/unimatrix/models/              # HOME=/data -> cache at /data/.cache/unimatrix/models/
    sentence-transformers_all-MiniLM-L6-v2/
      model.onnx
    cross-encoder_nli-minilm2-l6-h768/
      model_quantized.onnx

/etc/unimatrix/config.toml              # Optional read-only bind mount
                                        # Discovered via UNIMATRIX_CONFIG env var
```

### Dockerfile Stages

```
Stage 1: planner     rust:1.89-slim-bookworm    cargo chef prepare -> recipe.json
Stage 2: builder     rust:1.89-slim-bookworm    ORT install + cargo chef cook + cargo build + strip + model-download + mkdir /data + chmod 0700 /data
Stage 3: runtime     distroless/cc-debian12:nonroot    COPY binary, ORT, models, /data dir
```

### Container Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `LD_LIBRARY_PATH` | `/usr/local/lib` | ORT shared library resolution |
| `UNIMATRIX_LOG` | `info` | Tracing filter default |
| `HOME` | `/data` | Puts `~/.unimatrix/` and `~/.cache/` inside the volume |
| `UNIMATRIX_CONFIG` | `/etc/unimatrix/config.toml` | Explicit config path override. Required because `HOME=/data` makes `dirs::config_dir()` resolve to `/data/.config/`, not `/etc/unimatrix/`. Highest-priority config source. |

## Function Signatures

### New: health.rs

```rust
/// Run the health check: resolve ProjectPaths, connect to MCP UDS socket.
/// Exit 0 on success, exit 1 on failure. Sync path, no tokio runtime.
pub fn run(project_dir: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>
```

### Modified: handle_stale_pid_file (PidGuard)

```rust
// In handle_stale_pid_file, BEFORE the is_unimatrix_process check:
if stale_pid == std::process::id() {
    // Stale PID file references our own process -- reclaim directly.
    // Happens on container restart where PID 1 is always reused.
    return Ok(RecoveryAction::Reclaimed);
}
```

### Modified: config loading path

```rust
// Highest-priority config file source (new):
fn resolve_config_path() -> Option<PathBuf> {
    // 1. UNIMATRIX_CONFIG env var (highest priority -- container override)
    if let Ok(path) = std::env::var("UNIMATRIX_CONFIG") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }
    // 2. dirs::config_dir() / "unimatrix" / "config.toml" (existing fallback)
    // ... existing logic unchanged ...
}
```

### Existing (no changes needed)

```rust
// main.rs -- called directly by --foreground (no signature change)
async fn tokio_main_daemon(cli: Cli) -> Result<(), Box<dyn std::error::Error>>

// project.rs -- used by both serve --foreground and health for path resolution
pub fn ensure_data_directory(override_dir: Option<&Path>, base_dir: Option<&Path>)
    -> Result<ProjectPaths, CoreError>

// shutdown.rs -- explicit SIGTERM/SIGINT registration, works as PID 1
pub async fn shutdown_signal()
```

### Dockerfile Key Directives

```dockerfile
ENTRYPOINT ["unimatrix"]
CMD ["serve", "--foreground", "--project-dir", "/data"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["unimatrix", "health", "--project-dir", "/data"]
ENV HOME=/data \
    LD_LIBRARY_PATH=/usr/local/lib \
    UNIMATRIX_LOG=info \
    UNIMATRIX_CONFIG=/etc/unimatrix/config.toml
VOLUME ["/data"]
```

## Constraints

- **ORT version pinned to 1.20.1**: Must match `ort = "=2.0.0-rc.9"` crate dependency
- **glibc 2.36 compatibility**: Builder (`rust:1.89-slim-bookworm`) and runtime (`distroless/cc-debian12`) must both be Debian 12
- **No QEMU cross-compilation**: x86_64 and ARM64 build on native GHA runners only
- **`patches/anndists` must be in build context**: Workspace patch dependency; `.dockerignore` must exclude `patches/anndists/target/` but include source
- **Single binary, single process**: No sidecar, no init system, no supervisor
- **Distroless has no shell**: Debug via `docker top`, `docker logs`, or override to `debian:12-slim`
- **No EXPOSE directive**: No HTTP listener until W2-2
- **PidGuard self-PID guard required (ADR-007)**: `if stale_pid == std::process::id()`, skip SIGTERM and reclaim. This is a resolved design decision with a concrete one-line fix, not an open question.
- **UNIMATRIX_CONFIG env var required (ADR-005)**: Config loading must check `std::env::var("UNIMATRIX_CONFIG")` as highest-priority source. Small code change in config loading path. Without this, `HOME=/data` causes `dirs::config_dir()` to resolve under `/data/.config/`, missing the bind-mounted `/etc/unimatrix/config.toml`.
- **chmod 0700 /data in builder stage (WARN-2)**: Add `RUN chmod 0700 /data` after creating the `/data` directory in the builder stage. Defense-in-depth per vision requirement.
- **Foreground mode must not break daemon mode (SR-06)**: Zero modification to `tokio_main_daemon` or the `--daemon` dispatch path. Existing daemon tests are the regression gate.

## Dependencies

### Crates (existing, no new dependencies)

- `clap` -- add `--foreground` flag with `conflicts_with_all`, add `Health` subcommand
- `tokio` -- foreground mode reuses `tokio_main_daemon` (existing async runtime)
- `std::os::unix::net::UnixStream` -- health check sync socket connect

### External (build-time, Dockerfile)

- `cargo-chef` -- pinned version + `--locked` (dependency layer caching)
- ONNX Runtime 1.20.1 -- SHA-256 verified per-arch tarballs from GitHub Releases
- `rust:1.89-slim-bookworm` -- builder base image
- `gcr.io/distroless/cc-debian12:nonroot` -- runtime base image

### CI Actions (release.yml)

- `docker/setup-buildx-action` -- BuildKit builder setup
- `docker/login-action` -- GHCR authentication via `GITHUB_TOKEN`
- `docker/build-push-action` -- image build + push with GHA layer cache
- `docker/metadata-action` -- image tag and label management

## NOT in Scope

- Enterprise image (private `unimatrix-collective` repo)
- HTTP/HTTPS transport, TLS certificates, bearer tokens (W2-2)
- GGUF model baking (too large for image layers)
- Kubernetes manifests, Helm charts
- ORT SHA-256 backport to existing release.yml binary jobs (#4274 follow-up)
- `config.toml` read-only enforcement at runtime
- Multi-project routing (W2-3 TenantRouter)
- Distroless image digest pinning (hardening follow-up)
- ORT tarball mirroring/caching as GHA artifacts (hardening follow-up)
- Schema version check in HEALTHCHECK (follow-up enhancement)

## Alignment Status

**Overall: PASS with 2 WARNs** (no approval needed)

| Check | Status |
|-------|--------|
| Vision Alignment | PASS -- delivers W2-1 goal |
| Milestone Fit | PASS -- no future-milestone over-build |
| Scope Gaps | WARN -- two items below |
| Scope Additions | PASS -- no unauthorized additions |
| Architecture Consistency | PASS -- internally consistent |
| Risk Completeness | PASS -- 14 risks, 30 scenarios |

### WARN-1: HEALTHCHECK covers liveness only, not schema version

Vision says HEALTHCHECK verifies "daemon liveness and schema version currency." Implementation delivers liveness only (UDS socket connect). Schema mismatch would prevent daemon startup, which the health check detects indirectly (daemon not running = unhealthy). Adding schema version to health response is a follow-up.

### WARN-2: Volume permissions -- add `chmod 0700 /data` in builder stage

Vision says `chmod 0700` on named volumes. Source docs use distroless `:nonroot` (UID 65534) with `COPY --chown`. Implementation must add `RUN chmod 0700 /data` in the builder stage alongside the `COPY --chown`. One-line addition, no architectural impact. Defense-in-depth for bind-mount scenarios.

## Implementation Notes

### ORT SHA-256 Hashes

The specific SHA-256 hashes for `onnxruntime-linux-x64-1.20.1.tgz` and `onnxruntime-linux-aarch64-1.20.1.tgz` must be captured at implementation time by downloading the files and computing `sha256sum`. These are build-time constants in the Dockerfile `ARG` values.

### cargo-chef Version

Pin to the latest stable release at implementation time. Verify with `cargo install cargo-chef --version` for current latest. Architecture docs suggest 0.1.71; confirm at build time.

### Model Bake-In Path

Builder stage runs `unimatrix model-download` (and `--nli`) which downloads to `$HOME/.cache/unimatrix/models/` (with `HOME=/data` set in builder ENV). The `COPY` directive in the runtime stage must reference the exact output path from the builder. Verify the actual download paths at implementation time.

### v2 Changes from v1 Brief

1. **PidGuard self-PID guard (R-02 resolved)**: Promoted from "Critical Implementation Note" (ambiguous "verify or mitigate") to a resolved decision with ADR-007 reference. The fix is concrete: `if stale_pid == std::process::id()` skip SIGTERM and reclaim directly. Added `pidguard-self-pid` component to the Component Map. Added the PidGuard source file to Modified Files.

2. **HOME=/data config discovery (R-13 resolved)**: Promoted from constraint with "must trace" language to a resolved decision in ADR-005. The fix is concrete: `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` env var set in Dockerfile, checked as highest-priority config source before `dirs::config_dir()`. Added `config-env-override` component to the Component Map. Added config loading path to Modified Files with function signature. Removed `XDG_CACHE_HOME` from Dockerfile ENV (HOME=/data achieves the same result for cache paths).

3. **chmod 0700 /data**: Added to Dockerfile stages description and Constraints section per WARN-2 recommendation.
