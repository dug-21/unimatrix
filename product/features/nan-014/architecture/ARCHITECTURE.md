# nan-014: Container Packaging (MIT Image) — Architecture

## System Overview

nan-014 delivers a production-grade container deployment path for the MIT Unimatrix binary. It spans three domains: container infrastructure (Dockerfile, compose, .dockerignore), daemon lifecycle code changes (foreground mode, health subcommand), and CI pipeline integration (dual-arch container builds + GHCR manifest).

The container packages the same single `unimatrix` binary produced by the existing workspace, plus `libonnxruntime.so` and both ONNX models (embedding + NLI), into a distroless runtime image. No new crates are created. Code changes are confined to `crates/unimatrix-server/src/main.rs` (CLI surface) and a new `health.rs` module.

## Component Breakdown

### Component 1: Dockerfile (Three-Stage cargo-chef Build)

**Responsibility**: Build the `unimatrix` binary with all workspace crates, install ORT with SHA-256 verification, download and bake ONNX models, produce a minimal distroless runtime image.

**Files**: `Dockerfile` (repo root)

**Stages**:

1. **planner** (`rust:1.89-slim-bookworm`): Copy workspace, run `cargo chef prepare` to extract `recipe.json` (dependency fingerprint). This stage changes only when `Cargo.toml` or `Cargo.lock` change.

2. **builder** (`rust:1.89-slim-bookworm`): Install ORT with SHA-256 verification (architecture-conditional via `TARGETARCH`). Copy `recipe.json` from planner, run `cargo chef cook --release` (cached dependency compilation). Copy full source, run `cargo build --release` + `strip`. Run `unimatrix model-download` and `unimatrix model-download --nli` to bake models. Create `/data` directory owned by UID 65534.

3. **runtime** (`gcr.io/distroless/cc-debian12:nonroot`): Copy binary, ORT shared library, and model files from builder. Set environment variables. Declare `VOLUME ["/data"]`. Set `HEALTHCHECK`. Set `ENTRYPOINT ["unimatrix"]` + `CMD ["serve", "--foreground"]`.

**Key constraints**:
- `patches/anndists` directory must be in build context (workspace patch)
- `.cargo/config.toml` must be copied for `ORT_LIB_LOCATION` and `ORT_PREFER_DYNAMIC_LINK`
- `RUSTFLAGS="-C link-arg=-Wl,-rpath,$ORIGIN"` for rpath
- Model download happens in the builder stage (not a separate stage), after the binary is compiled, so it uses the just-built binary directly. Model layers are part of the builder cache chain — they invalidate when the binary changes, which is acceptable because model downloads are fast (~10s) and models change rarely.

### Component 2: docker-compose.yml

**Responsibility**: Single-command deployment with named volume and optional config override.

**Files**: `docker-compose.yml` (repo root)

**Structure**:
- Service `unimatrix` with image `ghcr.io/dug-21/unimatrix:latest`
- Named volume `unimatrix-data` mounted at `/data`
- Optional config.toml bind mount at `/etc/unimatrix/config.toml` (commented, documented)
- Restart policy `unless-stopped`
- Comments documenting debug override (`docker-compose.override.yml` swapping to `debian:12-slim`)

### Component 3: .dockerignore

**Responsibility**: Minimize build context to source + patches + Cargo files only.

**Files**: `.dockerignore` (repo root)

**Excludes**: `target/`, `.git/`, `product/`, `packages/`, `.claude/`, `.github/`, `patches/anndists/target/`, test fixtures, `.env`, `*.md` (except Cargo-related).

### Component 4: `serve --foreground` (Code Change)

**Responsibility**: Run the full daemon stack (UDS listener, tick loop, ML inference, signal handling) as PID 1 without fork/setsid process detachment.

**Files**: `crates/unimatrix-server/src/main.rs`

**Design** (SR-06 mitigation — extract shared logic):

The current daemon startup has two entry points:
- `run_daemon_launcher` (sync): spawns child process with `--daemon-child`
- `tokio_main_daemon` (async): full server init, called after `prepare_daemon_child()` does setsid

The `--foreground` flag calls `tokio_main_daemon` directly — no launcher, no child spawn, no setsid. The `tokio_main_daemon` function already contains the complete server initialization and shutdown logic; it does not need refactoring.

The critical insight: `prepare_daemon_child()` (setsid) is called in `main()` BEFORE `tokio_main_daemon()`. `tokio_main_daemon` itself is already a clean, self-contained async entry point. Foreground mode skips `prepare_daemon_child()` and calls `tokio_main_daemon` directly. No shared logic extraction is needed because `tokio_main_daemon` IS the shared logic — `--daemon` adds setsid before it, `--foreground` calls it directly.

**Signal handling**: PID 1 in a container does not receive default signal handling. The existing `shutdown::shutdown_signal()` function explicitly registers SIGTERM and SIGINT handlers via `tokio::signal`, which works correctly for PID 1. No changes needed.

**PidGuard**: Requires a self-PID guard (ADR-007). On container restart with a retained named volume, the stale PID file contains `1`. The new container's PID 1 IS the new `unimatrix` process. Without a guard, `handle_stale_pid_file` reads PID 1, calls `is_unimatrix_process(1)` which returns `true` (PID 1's `/proc/1/cmdline` IS unimatrix), and sends SIGTERM to PID 1 — self-termination. The fix: if `stale_pid == std::process::id()`, skip SIGTERM and reclaim the PID file directly. This is a general correctness fix, not container-specific.

### Component 5: `health` CLI Subcommand (Code Change)

**Responsibility**: Check daemon liveness by connecting to the MCP UDS socket. Exit 0 = healthy, exit 1 = unhealthy. Used by Dockerfile `HEALTHCHECK`.

**Files**: `crates/unimatrix-server/src/health.rs` (new), `crates/unimatrix-server/src/main.rs` (register subcommand)

**Design**:
1. Resolve `ProjectPaths` using the same `ensure_data_directory` with the same `--project-dir` override as the daemon.
2. Check if `paths.mcp_socket_path` exists.
3. Attempt a Unix socket connect to the path.
4. On success: print "healthy" to stdout, exit 0.
5. On failure (socket missing, connect refused, timeout): print diagnostic to stderr, exit 1.

**SR-11 mitigation (socket path consistency)**: Both `serve --foreground` and `health` resolve `ProjectPaths` via the same `ensure_data_directory(cli.project_dir.as_deref(), None)` call. In the container, both see the same `--project-dir /data` and same `HOME=/data` environment. Socket path is deterministic: `/data/.unimatrix/{hash}/unimatrix-mcp.sock`.

**Implementation**: Synchronous path (no tokio runtime needed). Use `std::os::unix::net::UnixStream::connect` with a 3-second timeout. This matches the existing sync subcommand pattern (Hook, Export, Import, Version, Stop).

### Component 6: CI Container Jobs (release.yml)

**Responsibility**: Build and publish dual-arch container images to GHCR on `v*` tag push.

**Files**: `.github/workflows/release.yml`

**Jobs**:

1. **`build-container-x64`** (ubuntu-22.04): `docker/build-push-action` with `--platform linux/amd64`. Pushes single-platform image to GHCR with `-amd64` suffix tag. Uses `cache-from: type=gha`.

2. **`build-container-arm64`** (ubuntu-22.04-arm): Same as x64 but `--platform linux/arm64`. Pushes with `-arm64` suffix tag.

3. **`create-container-manifest`** (ubuntu-latest, `needs: [build-container-x64, build-container-arm64]`): Merges platform-specific images into a multi-arch manifest at `ghcr.io/dug-21/unimatrix:v{version}` and `ghcr.io/dug-21/unimatrix:latest`.

**SR-10 mitigation (CI coupling)**: Container jobs are independent of binary/npm jobs. The dependency graph is:

```
build-linux-x64 ──┬── package-npm ── create-release
build-linux-arm64 ─┘
build-container-x64 ──┬── create-container-manifest
build-container-arm64 ─┘
```

No `needs` dependency between the container branch and the binary/npm branch. If ARM64 runner is unavailable, binary/npm releases proceed unblocked. The `create-release` job does not depend on container jobs.

**Permissions**: Add `packages: write` to the workflow-level `permissions` block (alongside existing `contents: write` and `id-token: write`).

## Component Interactions

```
                    Dockerfile
                   ┌──────────────────────────────┐
                   │ planner → builder → runtime   │
                   │                                │
                   │ builder runs:                  │
                   │   cargo chef cook              │
                   │   cargo build --release        │
                   │   unimatrix model-download     │
                   │   unimatrix model-download --nli│
                   └──────────────────────────────┘
                              │
                              ▼
                    docker-compose.yml
                   ┌──────────────────────────────┐
                   │ volume: unimatrix-data → /data │
                   │ ENTRYPOINT: unimatrix          │
                   │ CMD: serve --foreground         │
                   │ HEALTHCHECK: unimatrix health   │
                   └──────────────────────────────┘
                              │
          ┌───────────────────┼───────────────────┐
          ▼                   ▼                   ▼
    serve --foreground    health subcommand    release.yml
    (tokio_main_daemon)   (sync UDS connect)   (3 new jobs)
          │                   │
          └───────┬───────────┘
                  ▼
          ProjectPaths resolution
          (ensure_data_directory)
```

### Data Flow

1. **Build time**: Source → cargo-chef → binary + ORT + models → distroless image
2. **Runtime**: Container starts → `unimatrix serve --foreground` → `tokio_main_daemon` → UDS listener on `/data/{hash}/unimatrix-mcp.sock`
3. **Health check**: `unimatrix health` → resolve same `ProjectPaths` → connect to MCP UDS socket → exit code
4. **CI**: `v*` tag → 2 parallel container build jobs → manifest merge → GHCR push

### Error Boundaries

| Error Source | Propagation | User Surface |
|---|---|---|
| ORT SHA-256 mismatch | Build fails at `sha256sum -c` | Docker build error output |
| Model download failure | Build fails at `model-download` | Docker build error output |
| `serve --foreground` startup failure | Process exits non-zero | Container restart loop; `docker logs` |
| Health check failure | Exit 1 | Docker marks container unhealthy |
| GHCR push auth failure | CI job fails | GitHub Actions UI |
| Container build timeout | CI job fails | GitHub Actions UI; binary/npm unaffected (SR-10) |

## Technology Decisions

| Decision | Choice | ADR |
|---|---|---|
| Foreground mode implementation | Direct `tokio_main_daemon` call, no refactoring | ADR-001 |
| ORT supply chain verification | SHA-256 + multi-source resilience | ADR-002 |
| Health check mechanism | UDS socket connect (sync, no tokio) | ADR-003 |
| Container CI independence | Separate job branch, no cross-dependency | ADR-004 |
| Container data path resolution | `--project-dir /data` CLI flag | ADR-005 |
| cargo-chef version pinning | Pinned version + lockfile hash | ADR-006 |
| PidGuard self-PID guard | Skip SIGTERM when stale PID == current PID | ADR-007 |
| Config discovery in container | `UNIMATRIX_CONFIG` env var, highest-priority source | ADR-005 (updated) |

## Integration Points

### Existing Code Touched

| File | Change | Risk |
|---|---|---|
| `main.rs` | Add `--foreground` flag to `Serve`, add `Health` subcommand variant | Low — additive enum variants |
| `main.rs` dispatch | New match arm for `--foreground` calling `tokio_main_daemon` directly | Medium — SR-06 blast radius |
| `main.rs` dispatch | New match arm for `Health` calling `health::run()` | Low — sync, isolated |
| `release.yml` | 3 new jobs, 1 new permission | Low — additive, independent branch |

### New Files

| File | Purpose |
|---|---|
| `Dockerfile` | Three-stage container build |
| `docker-compose.yml` | Deployment configuration |
| `.dockerignore` | Build context minimization |
| `crates/unimatrix-server/src/health.rs` | Health check module |

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `Command::Serve { foreground: bool }` | New field on existing enum variant | `main.rs` `Command` enum |
| `Command::Health` | New enum variant (no fields) | `main.rs` `Command` enum |
| `health::run(project_dir: Option<&Path>) -> Result<(), Box<dyn Error>>` | `pub fn` — sync, no tokio | New `health.rs` module |
| `tokio_main_daemon(cli: Cli) -> Result<(), Box<dyn Error>>` | Existing `async fn` — no signature change | `main.rs` |
| `ensure_data_directory(override_dir, base_dir)` | Existing `pub fn` — no change | `project.rs` |
| `ProjectPaths.mcp_socket_path` | Existing `PathBuf` field — no change | `project.rs` |
| `shutdown::shutdown_signal()` | Existing `pub async fn` — no change | `shutdown.rs` |
| `EmbedConfig::resolve_cache_dir()` | Existing `pub fn` — container sets `HOME=/data`, cache resolves to `/data/.cache/unimatrix/models/` | `config.rs` |
| `handle_stale_pid_file` | Existing fn — add self-PID guard before SIGTERM path | `pid.rs` or `daemon.rs` |
| `UNIMATRIX_CONFIG` env var | New — highest-priority config path source, checked before `dirs::config_dir()` | config loading path |
| Dockerfile `ENTRYPOINT` | `["unimatrix"]` | New `Dockerfile` |
| Dockerfile `CMD` | `["serve", "--foreground"]` | New `Dockerfile` |
| Dockerfile `HEALTHCHECK` | `["unimatrix", "health", "--project-dir", "/data"]` | New `Dockerfile` |
| GHCR image ref | `ghcr.io/dug-21/unimatrix:{version}` | `release.yml` |

## Container Environment Variables

| Variable | Value | Purpose |
|---|---|---|
| `LD_LIBRARY_PATH` | `/usr/local/lib` | ORT shared library resolution |
| `UNIMATRIX_LOG` | `info` | Tracing filter default |
| `HOME` | `/data` | Overrides distroless default (`/home/nonroot`). Puts `~/.unimatrix/` inside the volume. See ADR-005. |
| `UNIMATRIX_CONFIG` | `/etc/unimatrix/config.toml` | Explicit config path override. Required because `HOME=/data` makes `dirs::config_dir()` resolve to `/data/.config/`, not `/etc/unimatrix/`. See ADR-005. |

## Volume Layout (MIT Container)

```
/data/                                    # Named volume: unimatrix-data
  {project-hash}/                         # ProjectPaths data_dir
    unimatrix.db                          # Knowledge database
    vector/                               # HNSW index files
    unimatrix.pid                         # PID file (PID 1 in container)
    unimatrix.sock                        # Hook IPC socket
    unimatrix-mcp.sock                    # MCP UDS socket
    unimatrix.log                         # Daemon log
    config.toml                           # Per-project config (auto-generated)
  unimatrix/models/                       # XDG_CACHE_HOME=/data → cache at /data/unimatrix/models/
    sentence-transformers_all-MiniLM-L6-v2/
      model.onnx                          # Baked into image, copied to volume on first run
    cross-encoder_nli-minilm2-l6-h768/
      model_quantized.onnx                # Baked into image, copied to volume on first run

/etc/unimatrix/config.toml               # Optional read-only bind mount (not in volume)
```

Note: Models are baked into the image layer at build time. The volume path above shows where the runtime resolves them via `XDG_CACHE_HOME`. The baked models in the image layer are the primary source; the volume path serves as fallback and for user-supplied models.

## Open Questions for Implementation Agents

1. **ORT SHA-256 hashes**: The specific SHA-256 hashes for `onnxruntime-linux-x64-1.20.1.tgz` and `onnxruntime-linux-aarch64-1.20.1.tgz` must be captured at implementation time by downloading the files and computing `sha256sum`. These are build-time constants in the Dockerfile.

2. **cargo-chef version**: Pin to the latest stable release at implementation time. Check `cargo install cargo-chef --version` for current latest.

3. **Distroless digest**: SR-04 recommends pinning to a specific `@sha256:` digest for reproducible builds. The implementation agent should capture the current digest of `gcr.io/distroless/cc-debian12:nonroot` at build time.

4. **Model baking path**: The builder stage runs `unimatrix model-download` which downloads to `XDG_CACHE_HOME` (or default cache dir). The implementation agent must verify the exact output path and ensure the `COPY` directive in the runtime stage references the correct builder path.
