# nan-014: Container Packaging (MIT Image)

## Problem Statement

Unimatrix has no containerized deployment path. Developers wanting to run a personal cloud instance must build from source or use the npm-distributed binary with manual ORT setup. This blocks the W2-1 vision goal: "any developer deploys and operates Unimatrix without ops friction." The container must work air-gapped (no runtime internet), run non-root, support both x86_64 and ARM64, and produce a one-command deployment experience: `docker run -v unimatrix-data:/data ghcr.io/dug-21/unimatrix`.

This feature delivers the MIT (open-source) image only. The enterprise image ships from the private `unimatrix-collective` repository with its own Dockerfile — the license boundary is enforced by repository separation.

## Goals

1. Production-grade Dockerfile at repo root that builds the `unimatrix` binary with all 9 workspace crates, ONNX Runtime, and both ONNX models (embedding + NLI) into a minimal runtime image.
2. `docker-compose.yml` at repo root for single-command personal cloud deployment with named volume and optional config override.
3. Multi-architecture support (x86_64 + ARM64) via native GitHub Actions runners, with a merged OCI manifest published to `ghcr.io/dug-21/unimatrix`.
4. Container build jobs integrated into the existing `release.yml` pipeline, triggered on the same `v*` tag push as binary and npm releases.
5. SHA-256 verification for ONNX Runtime downloads in the Dockerfile, closing the supply chain gap documented in Unimatrix entry #4274.
6. `unimatrix health` CLI subcommand for HEALTHCHECK liveness verification inside the container.
7. `.dockerignore` to exclude build artifacts, test fixtures, and non-essential files from the build context.

## Non-Goals

- **Enterprise image**: Ships from the private repo. nan-014 delivers zero commercial code, zero enterprise volume layout, zero OAuth infrastructure.
- **HTTP/HTTPS transport (W2-2)**: The container initially runs the daemon in its existing UDS mode. W2-2 (HTTPS + static bearer token) is a separate feature. The Dockerfile and compose file are designed so that adding `EXPOSE 8443` and transport flags is additive, not a rebuild.
- **Self-signed TLS certificate generation**: Depends on W2-2 delivering the HTTPS listener. The container skeleton ships without TLS. When W2-2 lands, `rcgen`-based cert generation and TLS config are added.
- **Auto-generated bearer token**: Depends on W2-2 delivering the static token auth middleware. Not part of nan-014.
- **GGUF model baking**: GGUF models are too large for image layers (1-4 GB). The `/data/models/` volume layout accommodates future GGUF files via volume mount, but nan-014 does not include `llama.cpp` or GGUF support.
- **Kubernetes manifests, Helm charts, or orchestrator-specific packaging**: Out of scope. docker-compose is the deployment surface for the MIT personal cloud.
- **ORT SHA-256 fix in release.yml**: While related (#4274), fixing the existing CI pipeline's missing checksum is a separate concern. The Dockerfile will include checksums from day one; backporting to release.yml is a follow-up.
- **`config.toml` as read-only bind mount enforcement**: The compose file documents the pattern, but runtime enforcement (refusing to start if config is writable) is not part of nan-014.

## Background Research

### ASS-043 Findings (Complete)

ASS-043 answered all 8 research questions for container packaging. Key decisions already made:

**Two-image architecture**: Confirmed. Separate Dockerfiles in separate repositories. MIT image at repo root, enterprise in private repo. License boundary enforced by repository separation.

**Base image**: `gcr.io/distroless/cc-debian12:nonroot` for runtime. Provides glibc 2.36 + libstdc++ (required by ORT). No shell, no package manager — minimal attack surface. Builder: `rust:1.89-slim-bookworm` (glibc-compatible with runtime). The `:nonroot` tag sets UID 65534, satisfying non-root container requirement directly.

**ONNX Runtime**: System-installed in builder stage, not `download-binaries`. ORT 1.20.1 pinned with SHA-256 hash. `libonnxruntime.so` (~14 MB) copied to runtime stage alongside binary. `TARGETARCH` build arg selects correct ORT tarball (x64 vs aarch64).

**Multi-arch**: Ship x86_64 + ARM64 from day one. Per-arch native GHA runners (no QEMU — 15-25x too slow for Rust). Three-job pattern: `build-container-x64`, `build-container-arm64`, `create-container-manifest`. Mirrors existing `release.yml` binary build pattern.

**Volume layout (MIT)**: Single `/data` volume. Contains per-project databases (`projects/{hash}/unimatrix.db`), models (`models/`), and token file (future, W2-2). `config.toml` is a separate optional read-only bind mount, not in the data volume.

**Build pipeline**: Three-stage Dockerfile with `cargo-chef` for dependency layer caching. Planner (recipe extraction) -> builder (cook + compile) -> runtime (distroless). GHA Docker layer cache via `cache-from: type=gha`.

### Codebase Analysis

**Existing infra-001 Dockerfile** (`product/test/infra-001/Dockerfile`): Two-stage test harness. Uses `rust:1.89-bookworm` builder (not slim). ORT installed via `wget` without SHA-256. Binary still named `unimatrix-server` (stale — renamed to `unimatrix` per ADR-002 / nan-004). Python test runtime as second stage. Does not use `cargo-chef`. Reference for ORT installation pattern but not for production image structure.

**Project paths** (`crates/unimatrix-engine/src/project.rs`): `ProjectPaths` struct resolves `~/.unimatrix/{project-hash}/` for all data. In the container, `--project-dir` or environment variable must override detection to point into `/data/projects/`. The `ensure_data_directory` function accepts a `base_dir` override parameter, which maps to the container's `/data` volume.

**Model cache** (`crates/unimatrix-embed/src/config.rs`): `EmbedConfig::resolve_cache_dir()` defaults to `~/.cache/unimatrix/models/`. In the container, this must resolve to `/data/models/` — achievable via config.toml `[embed] cache_dir` or by setting `XDG_CACHE_HOME=/data` environment variable in the Dockerfile.

**Binary CLI** (`crates/unimatrix-server/src/main.rs`): Subcommands include `serve --daemon`, `serve --stdio`, `stop`, `hook`, `model-download`, `export`, `import`, `snapshot`, `eval`, `version`. No `health` subcommand exists. The container ENTRYPOINT needs `serve --daemon` or a new foreground mode, plus a `health` subcommand for HEALTHCHECK.

**Release pipeline** (`.github/workflows/release.yml`): Triggered on `v*` tag push. Two parallel native-runner jobs (x64 on ubuntu-22.04, arm64 on ubuntu-22.04-arm). ORT version `1.20.1` pinned as env var. Binary bundled with `libonnxruntime.so` in artifact. npm packaging follows. Container jobs would parallel the binary jobs.

**`.cargo/config.toml`**: `ORT_LIB_LOCATION=/usr/local/lib`, `ORT_PREFER_DYNAMIC_LINK=1`. These must be set in the builder stage via `ENV` directives.

### Volume Layout Reconciliation

The product vision (updated 7b7e6c4a) specifies two named volumes for MIT:
- `unimatrix-data` — databases (integrity-critical)
- `unimatrix-shared` — ONNX models + config.toml (read-only bind)

ASS-043 recommends a single `/data` volume with models inside it, plus optional `config.toml` bind mount.

Resolution: follow the updated product vision. Two volumes:
- `unimatrix-data` mounted at `/data` — databases, token file, vector indexes
- `config.toml` as optional read-only bind mount at `/data/config.toml`

Models baked into the image layer (87 MB embedding + 79 MB NLI quantized = ~166 MB). This achieves zero-config startup — no model download step needed. The `/data/models/` path remains available for user-supplied models (GGUF future).

### W2-2/W2-3 Dependency Analysis

**W2-2 (HTTPS + static token)**: NOT a hard blocker for nan-014. The container can ship with the daemon running in UDS mode. This is useful for local Docker development with mounted project dirs. When W2-2 lands, the compose file adds `ports: ["8443:8443"]` and the Dockerfile adds `EXPOSE 8443`. The container skeleton is designed for this additive change.

**W2-3 (security model)**: NOT a hard blocker. Bearer token validation is additive middleware. No container structural changes needed.

**WAVE2-ROADMAP note**: "W2-1 wraps W2-2 + W2-3" implies nan-014 should deliver after server features. However, the Dockerfile and CI pipeline are independent of transport/auth implementation. Delivering the container skeleton first enables iterative testing of W2-2 in a container environment. The compose file will need a minor update when W2-2 ships (add port mapping, TLS cert mount), but the Dockerfile itself needs no structural changes.

### Container Entrypoint Decision

The container should NOT use `serve --daemon` (daemonizes via fork — wrong for containers). Containers expect PID 1 to be the main process. The correct entrypoint is `serve --stdio` (foreground mode) or a new `serve --foreground` flag. Current `serve --stdio` runs MCP over stdin/stdout, which is also wrong for a container. The container needs the daemon's full functionality (UDS listener, tick loop, ML inference) running in the foreground without daemonizing.

This is a small code change: the daemon startup path without the fork/setsid/redirect steps. The `tokio_main_daemon` function already does the real work after forking — extracting it into a foreground mode is straightforward.

## Proposed Approach

### Deliverables

1. **`Dockerfile`** (repo root): Three-stage cargo-chef build. ORT with SHA-256 verification. Models baked in at build time via `model-download`. Distroless runtime. `HEALTHCHECK` directive. `VOLUME ["/data"]`. `ENTRYPOINT ["unimatrix"]` + `CMD ["serve", "--foreground"]`.

2. **`docker-compose.yml`** (repo root): Named volume `unimatrix-data`. Optional `config.toml` bind mount documented in comments. Debug override documented (`docker-compose.override.yml` swapping to `debian:12-slim` for shell access).

3. **`.dockerignore`** (repo root): Exclude `target/`, `.git/`, `product/`, `packages/`, `.claude/`, `.github/`, `patches/anndists/target/`, test fixtures.

4. **`unimatrix serve --foreground`** (code change): Run the daemon in PID-1 foreground mode — full daemon functionality (UDS, ticks, ML) without fork/setsid. Signal handler for SIGTERM graceful shutdown.

5. **`unimatrix health`** (code change): CLI subcommand that checks daemon liveness by connecting to the UDS socket and sending a ping. Exit 0 = healthy, exit 1 = unhealthy. Used by Dockerfile `HEALTHCHECK`.

6. **Container CI jobs** (`.github/workflows/release.yml`): Two parallel jobs (`build-container-x64`, `build-container-arm64`) using `docker/build-push-action` + GHA cache. Third job `create-container-manifest` merges into `ghcr.io/dug-21/unimatrix:{version}`. Triggered on same `v*` tag. Requires `packages: write` permission for GHCR push.

### Architecture Rationale

- **cargo-chef three-stage**: Caches all ~50 dependency compilations. Source-only changes rebuild only the final `cargo build` step. Saves 3-5 minutes per build. Worth the Dockerfile complexity for a 9-crate workspace.
- **Models baked in**: The embedding model (87 MB) and NLI model (79 MB) are small enough that baking them into image layers provides true zero-config startup. No first-run model download, no internet dependency post-pull. GGUF models (1+ GB) are explicitly excluded from baking.
- **Foreground mode**: Containers expect PID 1 management. Fork-based daemon mode would make the container exit immediately after fork. The foreground flag runs the same daemon logic without process detachment.
- **Health subcommand over HTTP health endpoint**: No HTTPS transport exists yet (W2-2). UDS-based health check works regardless of transport mode and is always available.

## Acceptance Criteria

- AC-01: `docker build -t unimatrix .` succeeds from the repo root on both x86_64 and ARM64, producing a runnable image under 350 MB.
- AC-02: `docker run -v unimatrix-data:/data ghcr.io/dug-21/unimatrix` starts the daemon in foreground mode, creates the data directory structure under `/data`, and the ONNX embedding model is available without internet access.
- AC-03: `docker compose up` starts the service with the named volume `unimatrix-data` mounted at `/data`.
- AC-04: The container runs as non-root (UID 65534, distroless `nonroot` user). No process inside the container runs as UID 0.
- AC-05: ONNX Runtime is installed in the builder stage with SHA-256 hash verification. A tampered or wrong-hash download causes the build to fail.
- AC-06: `unimatrix serve --foreground` runs the full daemon (UDS listener, tick loop, ML inference) as PID 1 without forking. SIGTERM triggers graceful shutdown.
- AC-07: `unimatrix health` exits 0 when the daemon is running and responsive, exits 1 otherwise. The Dockerfile `HEALTHCHECK` uses this command.
- AC-08: The `release.yml` workflow includes container build jobs for both x86_64 and ARM64, producing a multi-arch manifest at `ghcr.io/dug-21/unimatrix:v{version}` on `v*` tag push.
- AC-09: `.dockerignore` excludes `target/`, `.git/`, `product/`, `packages/`, `.claude/`, and test fixtures. Build context is under 5 MB (source + patches + Cargo files only).
- AC-10: The image contains zero enterprise/commercial code. Only the 9 MIT-licensed workspace crates are compiled.
- AC-11: The NLI model (quantized) is baked into the image alongside the embedding model. Both are available at container startup without downloading.
- AC-12: A `docker-compose.override.yml` example is documented (in comments or README section) showing how to swap to `debian:12-slim` for debug shell access.

## Constraints

- **ORT version pinned to 1.20.1**: Must match the `ort = "=2.0.0-rc.9"` crate dependency. Upgrading ORT requires upgrading the `ort` crate and is a separate task.
- **glibc floor**: Builder (`rust:1.89-slim-bookworm`, glibc 2.36) must be compatible with runtime (`distroless/cc-debian12`, glibc 2.36). Both are Debian 12 — compatible.
- **`patches/anndists`**: The workspace patches `anndists 0.1.4` via a local path patch. The Dockerfile must copy the `patches/` directory into the build context.
- **No QEMU cross-compilation**: QEMU adds 15-25x build time and risks compiler segfaults. x86_64 and ARM64 must build on native runners.
- **GHCR permissions**: The `release.yml` workflow needs `packages: write` permission to push to `ghcr.io`. This requires a repository settings change or PAT configuration.
- **Single binary**: The container runs one `unimatrix` process. No sidecar processes, no init systems, no supervisors.
- **distroless has no shell**: All debugging must use `docker exec --debug` (Docker Desktop 4.27+), ephemeral debug containers, or log output. The debug override with `debian:12-slim` mitigates this for development.
- **`ProjectPaths` base_dir override**: The container must configure the project data path to land under `/data`. The existing `base_dir` parameter in `ensure_data_directory` or environment variable configuration makes this possible without code changes to path resolution logic.

## Multi-Project Statement

The container initially runs in single-project mode, matching the current per-project daemon model. Multi-project routing — where one container serves multiple projects and workstations — is delivered by W2-3 (TenantRouter) on top of W2-2 (HTTP transport). nan-014 deliverables (Dockerfile, volume layout, foreground mode, health check, CI pipeline) require no restructuring when multi-project lands — the changes are additive at the daemon's service layer, not the container's infrastructure.

## Resolved Questions

- OQ-01: **Foreground mode implementation** — Use `--foreground` flag on `serve`. Runs `tokio_main_daemon` directly without the fork/setsid/redirect preamble. Same daemon functionality (UDS, ticks, ML), no process detachment. PidGuard behavior unchanged — it guards the same process, just not forked. Standard pattern (nginx, postgres, redis all have equivalent flags).
- OQ-02: **Model download layer caching** — Download models in a separate Dockerfile stage after compilation, so model layers cache independently of source changes. Model files change rarely; source changes frequently. Two independent cache chains.
- OQ-03: **GHCR auth** — Add `packages: write` to `release.yml` permissions block alongside existing `contents: write` and `id-token: write`. No PAT needed — `GITHUB_TOKEN` with the added scope is sufficient.
- OQ-04: **Volume ownership with distroless nonroot** — Create `/data` directory with correct ownership (UID 65534) in the builder stage, then `COPY --from=builder --chown=65534:65534 /data /data` into the runtime stage. Docker named volumes inherit the directory's ownership from the image on first mount. No shell or chown at runtime needed.
- OQ-05: **Config mount path** — `/etc/unimatrix/config.toml`. Conventional, keeps config outside the data volume, and the daemon already supports config path override. Document the bind mount in `docker-compose.yml` comments.

## Tracking

https://github.com/dug-21/unimatrix/issues/629

Feature: nan-014
Phase: Nanoprobes (build, deploy, CI)
Wave: W2-1 (Personal Cloud Delivery)
Research prerequisite: ASS-043 (COMPLETE)
