# SPECIFICATION: nan-014 Container Packaging (MIT Image)

## Objective

Deliver a production-grade containerized deployment path for Unimatrix's MIT (open-source) image, enabling any developer to run a personal cloud instance with a single `docker run` command. The container must work air-gapped (no runtime internet), run non-root, support both x86_64 and ARM64, and integrate into the existing release pipeline. This is the W2-1 deliverable: "any developer deploys and operates Unimatrix without ops friction."

---

## Functional Requirements

### FR-1: Dockerfile (Three-Stage cargo-chef Build)

- FR-1.1: A `Dockerfile` at the repository root produces a runnable image containing the `unimatrix` binary, `libonnxruntime.so`, and both ONNX models (embedding + NLI quantized).
- FR-1.2: Stage 1 (planner) uses `rust:1.89-slim-bookworm` to extract the `cargo-chef` recipe from `Cargo.toml`/`Cargo.lock`.
- FR-1.3: Stage 2 (builder) cooks dependencies from the recipe, then compiles the workspace. ORT is installed with SHA-256 hash verification for both architectures. The binary is stripped.
- FR-1.4: A separate model-download stage runs `unimatrix model-download` and `unimatrix model-download --nli` to retrieve both ONNX models. This stage caches independently of source changes.
- FR-1.5: Stage 3 (runtime) uses `gcr.io/distroless/cc-debian12:nonroot` as the base image. Only the binary, `libonnxruntime.so`, model files, and a `/data` directory (owned by UID 65534) are copied in.
- FR-1.6: The Dockerfile uses `TARGETARCH` build arg to select the correct ORT tarball (`x64` vs `aarch64`).
- FR-1.7: `cargo-chef` is installed with a pinned version (`cargo install cargo-chef --version X.Y.Z`).
- FR-1.8: The `patches/anndists` directory is copied into the build context for the workspace patch dependency.
- FR-1.9: `ENTRYPOINT ["unimatrix"]` and `CMD ["serve", "--foreground"]`.
- FR-1.10: `VOLUME ["/data"]` declared for the data mount point.
- FR-1.11: `HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD ["unimatrix", "health"]` directive included.
- FR-1.12: `ENV` directives set `ORT_LIB_LOCATION`, `ORT_PREFER_DYNAMIC_LINK=1`, and `LD_LIBRARY_PATH=/usr/local/lib` as needed for builder and runtime stages.
- FR-1.13: No `EXPOSE` directive -- no HTTP listener exists until W2-2.

### FR-2: docker-compose.yml

- FR-2.1: A `docker-compose.yml` at the repository root defines a single `unimatrix` service using the locally built image or `ghcr.io/dug-21/unimatrix`.
- FR-2.2: Named volume `unimatrix-data` mounted at `/data`.
- FR-2.3: Optional `config.toml` bind mount at `/etc/unimatrix/config.toml` documented in comments (commented-out `volumes` entry with explanation).
- FR-2.4: A `docker-compose.override.yml` debug pattern documented in comments, showing how to swap to `debian:12-slim` for shell access.
- FR-2.5: No port mappings -- UDS mode only until W2-2.

### FR-3: .dockerignore

- FR-3.1: A `.dockerignore` at the repository root excludes: `target/`, `.git/`, `product/`, `packages/`, `.claude/`, `.github/`, `patches/anndists/target/`, test fixtures, and documentation files.
- FR-3.2: The build context, after exclusions, contains only source code, `Cargo.toml`/`Cargo.lock`, `patches/` (source only), `.cargo/config.toml`, and the `Dockerfile` itself.

### FR-4: `serve --foreground` Flag

- FR-4.1: Add a `--foreground` flag to the `Serve` subcommand in `crates/unimatrix-server/src/main.rs`.
- FR-4.2: When `--foreground` is set, the daemon runs directly as PID 1 without fork, setsid, or stdout/stderr redirection. The `tokio_main_daemon` function is called directly (no `prepare_daemon_child`, no `run_daemon_launcher`).
- FR-4.3: The foreground mode provides identical functionality to the daemon child process: UDS listener, MCP UDS acceptor, background tick loop, ML inference (embedding + NLI), PidGuard acquisition, signal handling.
- FR-4.4: SIGTERM and SIGINT trigger graceful shutdown via the existing `shutdown_signal()` + `daemon_token` cancellation path.
- FR-4.5: The shared daemon logic is extracted into a common function callable by both `--foreground` (directly) and `--daemon` (via the spawned child process), avoiding code duplication. The existing `--daemon` path must remain unchanged (SR-06 mitigation).
- FR-4.6: `--foreground` is mutually exclusive with `--daemon` and `--stdio`. Clap validation enforces this.
- FR-4.7: PidGuard behavior is unchanged -- it guards the current process regardless of whether it was forked.
- FR-4.8: Tracing output goes to stderr (same as daemon mode). No log file redirection is performed in foreground mode.

### FR-5: `health` CLI Subcommand

- FR-5.1: Add a `Health` variant to the `Command` enum in `crates/unimatrix-server/src/main.rs`.
- FR-5.2: The `health` subcommand is synchronous (no tokio runtime), matching the existing pattern for `Hook`, `Export`, `Stop`, etc. (Unimatrix procedure #1192).
- FR-5.3: The health check connects to the daemon's MCP UDS socket (resolved via `ProjectPaths.mcp_socket_path`) and verifies the daemon is responsive.
- FR-5.4: Exit code 0 when the daemon is running and responsive. Exit code 1 otherwise (socket missing, connection refused, timeout).
- FR-5.5: The socket path resolution uses the same `ProjectPaths` logic as `serve --foreground`, ensuring both subcommands resolve the same socket (SR-11 mitigation). In the container, `--project-dir /data` is passed (or `UNIMATRIX_PROJECT_DIR=/data` is set via Dockerfile `ENV`) to ensure deterministic path resolution.
- FR-5.6: The health check has a timeout (5 seconds maximum) to prevent the HEALTHCHECK from hanging.
- FR-5.7: No output on success (exit 0 only). On failure, a brief diagnostic message is written to stderr.

### FR-6: CI Container Build Jobs

- FR-6.1: Add three jobs to `.github/workflows/release.yml`: `build-container-x64`, `build-container-arm64`, and `create-container-manifest`.
- FR-6.2: `build-container-x64` runs on `ubuntu-22.04`. `build-container-arm64` runs on `ubuntu-22.04-arm`. Each builds a single-platform image and pushes to GHCR.
- FR-6.3: Both jobs use `docker/build-push-action` with `cache-from: type=gha` and `cache-to: type=gha,mode=max` for Docker layer caching.
- FR-6.4: `create-container-manifest` depends on both build jobs. It merges the per-arch images into a multi-arch OCI manifest at `ghcr.io/dug-21/unimatrix:v{version}` and pushes it.
- FR-6.5: Container jobs are triggered on the same `v*` tag push as existing binary and npm jobs.
- FR-6.6: Add `packages: write` to the workflow-level `permissions` block.
- FR-6.7: Container build jobs run in parallel with the existing binary build jobs (`build-linux-x64`, `build-linux-arm64`). They do not depend on or block the binary/npm release path (SR-10 mitigation).
- FR-6.8: The `create-container-manifest` job does not block `package-npm` or `create-release`. These are independent dependency chains.
- FR-6.9: GHCR authentication uses `docker/login-action` with `GITHUB_TOKEN` (no PAT required).

---

## Non-Functional Requirements

### NFR-1: Image Size

The final runtime image must be under 350 MB (compressed). Budget breakdown:
- Base image (distroless/cc): ~32 MB
- `unimatrix` binary (stripped): ~31 MB
- `libonnxruntime.so`: ~14 MB
- Embedding model (all-MiniLM-L6-v2): ~87 MB
- NLI model (quantized, qint8): ~79 MB
- Overhead (directories, metadata): <10 MB
- Total estimate: ~253 MB

### NFR-2: Build Context Size

The build context (after `.dockerignore`) must be under 5 MB. Source code, Cargo files, and patches only.

### NFR-3: Security — Non-Root Execution

All processes inside the container run as UID 65534 (distroless `nonroot` user). No process runs as UID 0 at runtime. The runtime base image `gcr.io/distroless/cc-debian12:nonroot` enforces this.

### NFR-4: Security — Supply Chain Verification

- ORT tarballs verified by SHA-256 hash in the Dockerfile. A hash mismatch causes the build to fail immediately.
- `cargo-chef` installed with a pinned version.
- No runtime internet access required. The image is fully self-contained after build.

### NFR-5: Air-Gap Capability

After `docker pull`, the container operates without any network access. All models are baked into image layers. No model download, no external service dependency at startup.

### NFR-6: Build Performance

- Dependency-only source changes (no `Cargo.toml`/`Cargo.lock` modifications) rebuild only the final `cargo build` step, not the dependency cook step. Expected savings: 3-5 minutes per build.
- Model download is in a separate stage, cached independently of source changes.
- GHA Docker layer cache (`type=gha`) persists between CI runs.

### NFR-7: Container Lifecycle

- Container start: `unimatrix serve --foreground` creates `/data` directory structure, loads models from baked-in paths, starts UDS listener and background tick.
- Container stop: SIGTERM triggers graceful shutdown -- vector index persisted, adaptation state saved, database compacted, PidGuard released.
- Container restart: PidGuard correctly detects PID namespace reset (PID 1 in new container is not PID 1 from old container). Stale PID file handling via `/proc/{pid}/cmdline` check works inside the container.

### NFR-8: Graceful Error Messages

When `/data` volume has incorrect permissions (ownership drift on existing named volumes), the daemon produces a clear error message identifying the permission mismatch, not a panic or opaque I/O error (SR-08 mitigation).

### NFR-9: CI Pipeline Independence

Container build failures do not block binary artifact creation, npm packaging, or GitHub release creation. The container and binary/npm pipelines are independent dependency chains within the same workflow (SR-10 mitigation).

---

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `docker build -t unimatrix .` succeeds from the repo root on both x86_64 and ARM64, producing a runnable image under 350 MB. | Build on both architectures. `docker images unimatrix --format '{{.Size}}'` confirms size. |
| AC-02 | `docker run -v unimatrix-data:/data ghcr.io/dug-21/unimatrix` starts the daemon in foreground mode, creates the data directory structure under `/data`, and the ONNX embedding model is available without internet access. | Run container with `--network=none`. Verify process starts, `/data` structure created, logs show embedding model loaded. |
| AC-03 | `docker compose up` starts the service with the named volume `unimatrix-data` mounted at `/data`. | Run `docker compose up -d`, verify container is running, volume is mounted. |
| AC-04 | The container runs as non-root (UID 65534). No process inside the container runs as UID 0. | `docker exec <container> /proc/1/status` (or `docker top`) shows UID 65534. Note: distroless has no shell, so verification uses `docker top` or inspect. |
| AC-05 | ONNX Runtime is installed in the builder stage with SHA-256 hash verification. A tampered or wrong-hash download causes the build to fail. | Modify the SHA-256 constant in the Dockerfile, run `docker build`. Build must fail with a checksum error. |
| AC-06 | `unimatrix serve --foreground` runs the full daemon (UDS listener, tick loop, ML inference) as PID 1 without forking. SIGTERM triggers graceful shutdown. | Run `unimatrix serve --foreground`, verify PID 1, send SIGTERM, verify clean exit with vector dump and DB compaction logs. |
| AC-07 | `unimatrix health` exits 0 when the daemon is running and responsive, exits 1 otherwise. The Dockerfile `HEALTHCHECK` uses this command. | Start daemon, run `unimatrix health` (exit 0). Stop daemon, run `unimatrix health` (exit 1). `docker inspect --format='{{.State.Health.Status}}'` shows `healthy`. |
| AC-08 | The `release.yml` workflow includes container build jobs for both x86_64 and ARM64, producing a multi-arch manifest at `ghcr.io/dug-21/unimatrix:v{version}` on `v*` tag push. | Push a `v*` tag. Verify GHCR manifest contains both `linux/amd64` and `linux/arm64` platforms. `docker manifest inspect` confirms. |
| AC-09 | `.dockerignore` excludes `target/`, `.git/`, `product/`, `packages/`, `.claude/`, and test fixtures. Build context is under 5 MB. | `docker build` output shows context size. Verify excluded paths are not in the build context (no `COPY` of excluded paths succeeds). |
| AC-10 | The image contains zero enterprise/commercial code. Only the 9 MIT-licensed workspace crates are compiled. | Inspect Dockerfile -- only this repository's workspace is compiled. No `unimatrix-collective` dependency, no enterprise feature flags. |
| AC-11 | The NLI model (quantized) is baked into the image alongside the embedding model. Both are available at container startup without downloading. | Run container with `--network=none`. Logs show both embedding and NLI models loaded successfully. |
| AC-12 | A `docker-compose.override.yml` example is documented (in comments) showing how to swap to `debian:12-slim` for debug shell access. | Read `docker-compose.yml` comments. The override pattern is documented and syntactically valid YAML. |

---

## Domain Models

### Container Architecture

| Term | Definition |
|------|-----------|
| **MIT image** | The open-source container image built from this repository. Contains only MIT/Apache-2.0 licensed crates. Published to `ghcr.io/dug-21/unimatrix`. |
| **Enterprise image** | A separate container image built from the private `unimatrix-collective` repository. Not part of nan-014. License boundary enforced by repository separation. |
| **Foreground mode** | `serve --foreground` -- the daemon runs as PID 1 without fork/setsid. Identical functionality to the daemon child process. Container-native execution mode. |
| **Health check** | `unimatrix health` -- a sync CLI subcommand that verifies daemon liveness by connecting to the MCP UDS socket. Exit 0 = healthy, exit 1 = unhealthy. |
| **cargo-chef** | A third-party Cargo plugin that extracts a dependency "recipe" from the workspace, enabling Docker layer caching of dependency compilation separate from source compilation. |
| **distroless** | Google's minimal container base images. The `cc-debian12:nonroot` variant provides glibc 2.36 + libstdc++ (required by ORT) with no shell, no package manager. |
| **Named volume** | A Docker-managed volume (`unimatrix-data`) that persists across container restarts. Inherits directory ownership from the image on first mount. |

### Volume Layout (MIT)

```
/data/                             # Named volume mount point
  projects/
    {project-hash}/
      unimatrix.db                 # Knowledge database
      analytics.db                 # Analytics database
      vectors/                     # HNSW index files
  models/                          # User-supplied models (GGUF future)

/usr/local/share/unimatrix/models/ # Baked-in ONNX models (image layer)
  sentence-transformers_all-MiniLM-L6-v2/
    model.onnx
    tokenizer.json
  nli-model/
    model.onnx

/etc/unimatrix/config.toml         # Optional read-only bind mount
```

### Dockerfile Stages

```
Stage 1: planner     rust:1.89-slim-bookworm    Extract cargo-chef recipe
Stage 2: builder     rust:1.89-slim-bookworm    Cook deps + compile + strip
Stage 3: models      builder (reuse)            Run model-download for both models
Stage 4: runtime     distroless/cc:nonroot      Copy binary, ORT, models, /data dir
```

---

## User Workflows

### Workflow 1: Developer Deploys Personal Cloud

```
1. docker pull ghcr.io/dug-21/unimatrix:v{version}
   -- or --
   docker build -t unimatrix .     (from repo root)

2. docker run -v unimatrix-data:/data ghcr.io/dug-21/unimatrix
   - Container starts in foreground mode (PID 1)
   - /data directory created with UID 65534 ownership
   - Embedding and NLI models loaded from baked-in paths
   - UDS listener started
   - HEALTHCHECK begins reporting healthy

3. docker compose up -d            (alternative: compose-based)
   - Named volume auto-created
   - Optional config.toml bind mount
```

### Workflow 2: Developer Debugs Container Issues

```
1. Create docker-compose.override.yml from documented template
2. Override image to debian:12-slim for shell access
3. docker compose up
4. docker exec -it unimatrix-1 /bin/bash
```

### Workflow 3: Release Pipeline Builds Container

```
1. Developer pushes v* tag
2. release.yml triggers
3. Parallel: binary builds (x64, arm64) + container builds (x64, arm64)
4. Container builds push per-arch images to GHCR
5. create-container-manifest merges into multi-arch manifest
6. Independent: package-npm + create-release (not blocked by container jobs)
```

### Workflow 4: HEALTHCHECK Verifies Liveness

```
1. Docker engine runs HEALTHCHECK every 30s
2. unimatrix health connects to MCP UDS socket
3. Exit 0 -> container marked healthy
4. Exit 1 -> container marked unhealthy after 3 retries
5. Orchestrators can restart unhealthy containers
```

---

## Constraints

### C-1: ORT Version Pin

ORT version must be `1.20.1`, matching the `ort = "=2.0.0-rc.9"` crate dependency. Upgrading ORT requires upgrading the `ort` crate and is out of scope.

### C-2: glibc Compatibility

Builder (`rust:1.89-slim-bookworm`, glibc 2.36) and runtime (`distroless/cc-debian12`, glibc 2.36) must both be Debian 12. If Rust 1.89 images shift to Debian 13, this constraint is violated and requires remediation.

### C-3: No QEMU Cross-Compilation

QEMU adds 15-25x build time and risks compiler segfaults. x86_64 and ARM64 must build on native GHA runners (`ubuntu-22.04` and `ubuntu-22.04-arm`).

### C-4: patches/anndists Workspace Patch

The workspace patches `anndists 0.1.4` via a local path. The Dockerfile must copy `patches/` into the build context. The `.dockerignore` must exclude `patches/anndists/target/` but include `patches/anndists/src/` and `patches/anndists/Cargo.toml`.

### C-5: Single Binary, Single Process

The container runs one `unimatrix` process (PID 1). No sidecar processes, no init systems (tini, dumb-init), no supervisors.

### C-6: Distroless Has No Shell

Runtime debugging cannot use `docker exec ... /bin/sh`. Mitigations: `docker top`, `docker logs`, `docker exec --debug` (Docker Desktop 4.27+), and the debug override pattern with `debian:12-slim`.

### C-7: PidGuard Container Restart Behavior

On container restart, the PID namespace resets. PID 1 in the new container is not the same process as PID 1 in the old container. `handle_stale_pid_file` must correctly identify the stale PID via `/proc/{pid}/cmdline` (which will fail to find a unimatrix process) and proceed (SR-09).

### C-8: ProjectPaths Base Directory Override

The container must configure project data paths to land under `/data`. Either `--project-dir /data` or `UNIMATRIX_PROJECT_DIR=/data` must be set so that `ensure_data_directory` resolves correctly.

### C-9: Foreground Mode Must Not Break Daemon Mode

The `--foreground` code change touches the daemon startup path. The shared daemon logic must be extracted into a common function called by both `--foreground` and `--daemon --daemon-child`, not by conditionally skipping steps in the existing daemon path. Both modes must be tested (SR-06).

### C-10: No EXPOSE Directive

No `EXPOSE` port directive until W2-2 delivers the HTTPS listener. Shipping `EXPOSE 8443` before the listener exists is misleading (SR-07).

---

## Dependencies

### Crates (Existing)

- `clap` -- CLI argument parsing (add `--foreground` flag, `Health` subcommand)
- `tokio` -- async runtime (foreground mode reuses `tokio_main_daemon`)
- `nix` -- Unix process management (signal handling in foreground mode)
- All 9 workspace crates compiled into the single binary

### External (Build-Time)

- `cargo-chef` -- Dockerfile dependency layer caching (pinned version)
- ONNX Runtime 1.20.1 -- system-installed in builder stage, SHA-256 verified
- `rust:1.89-slim-bookworm` -- builder base image
- `gcr.io/distroless/cc-debian12:nonroot` -- runtime base image

### CI Actions

- `docker/setup-buildx-action` -- BuildKit builder setup
- `docker/login-action` -- GHCR authentication
- `docker/build-push-action` -- Image build and push with caching
- `docker/metadata-action` -- Image tag and label management

### Existing Components

- `tokio_main_daemon` (`main.rs`) -- daemon async entry point, refactored to share logic with foreground mode
- `ProjectPaths` (`project.rs`) -- path resolution with `base_dir` override
- `PidGuard` (`infra/pidfile.rs`) -- process lock management
- `shutdown::graceful_shutdown` -- lifecycle cleanup
- `shutdown::shutdown_signal` -- SIGTERM/SIGINT listener
- `EmbedConfig::resolve_cache_dir` -- model path resolution (needs override for baked-in container paths)
- `release.yml` -- existing CI pipeline, extended with container jobs

---

## NOT in Scope

- **Enterprise image**: Ships from private `unimatrix-collective` repository. nan-014 delivers zero commercial code.
- **HTTP/HTTPS transport (W2-2)**: Container initially runs UDS mode only. Adding `EXPOSE 8443` and transport flags is additive when W2-2 lands.
- **Self-signed TLS certificate generation**: Depends on W2-2 delivering the HTTPS listener.
- **Auto-generated bearer token**: Depends on W2-2 delivering static token auth.
- **GGUF model baking**: GGUF models are too large for image layers (1+ GB). `/data/models/` path accommodates future GGUF via volume mount.
- **Kubernetes manifests, Helm charts**: docker-compose is the deployment surface for MIT personal cloud.
- **ORT SHA-256 fix in existing release.yml binary jobs**: Backporting checksums to the binary build pipeline is a separate concern (#4274 follow-up). The Dockerfile includes checksums from day one.
- **`config.toml` read-only enforcement at runtime**: The compose file documents the pattern; the daemon does not refuse to start if config is writable.
- **Multi-project routing**: Single-project mode initially. W2-3 TenantRouter is additive at the daemon service layer.
- **Distroless image digest pinning**: Use tag-based reference for initial delivery. Digest pinning (SR-04) is a hardening follow-up.
- **ORT tarball mirroring or caching as GHA artifacts**: SHA-256 verification is the supply chain gate for nan-014. Mirror/cache fallback is a hardening follow-up (SR-01).

---

## Open Questions

1. **Model bake-in path**: The embedding model cache is resolved via `EmbedConfig::resolve_cache_dir()` which defaults to `~/.cache/unimatrix/models/`. In the container, should this be overridden via `XDG_CACHE_HOME=/data` env var, or should a separate baked-in model path (e.g., `/usr/local/share/unimatrix/models/`) be used with a config.toml `[embed] cache_dir` override? The baked-in path approach keeps models in the immutable image layer while `/data/models/` remains available for user-supplied models. Architect decision needed.

2. **Health check protocol**: Should `unimatrix health` attempt a full MCP `ping` over the UDS socket, or simply verify the socket exists and accepts a TCP connection? Full MCP ping provides stronger liveness verification but requires more implementation. Socket connection check is simpler and matches Docker HEALTHCHECK conventions.

3. **Docker stop timeout**: Docker sends SIGTERM then waits `stop_grace_period` (default 10s) before SIGKILL. The daemon's graceful shutdown includes vector index persistence and DB compaction. Should the compose file set `stop_grace_period: 30s` to match the daemon's 15s stop timeout plus safety margin?

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 20 entries returned; relevant: #4554 (W2-1 feature context), #4274 (ORT SHA-256 supply chain lesson), #1192 (sync CLI subcommand procedure), #1199 (binary rename ADR). No directly reusable specification patterns found; this is the first container packaging specification.
