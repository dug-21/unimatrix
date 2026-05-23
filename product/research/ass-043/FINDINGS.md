# FINDINGS: Container + Packaging Strategy

**Spike**: ASS-043
**Date**: 2026-05-22
**Approach**: evaluation
**Confidence**: directional

---

## Findings

### Q: Can the MIT and enterprise images share a Dockerfile, or do they require separate Dockerfiles?

**Answer**: Separate Dockerfiles in separate repositories. The MIT image must contain zero commercial code -- this is a license boundary enforced at the crate level, not a feature flag boundary. A single Dockerfile with build args is insufficient because the enterprise crate's source code would be present in the build context even if not compiled.

**Evidence**: The current workspace has 9 crates, all under MIT/Apache-2.0 via `workspace.package.license`. Per ASS-045 findings, enterprise commercial features ship from a separate private repository (`unimatrix-collective`). The enterprise binary is built from that repository, not from this one:

- **MIT image**: Built from this repository. Single `Dockerfile` at repo root. Compiles `unimatrix-server` (which pulls all 8 other crates as dependencies). Produces the `unimatrix` binary. Published to `ghcr.io/dug-21/unimatrix` (public).
- **Enterprise image**: Built from the private `unimatrix-collective` repository. Separate `Dockerfile` in that repository. May vendor or `git` dependency the MIT crates. Produces a `unimatrix-enterprise` binary. Published to a private registry or `ghcr.io` with access controls.

Since the two images are built from different repositories, they inherently have separate Dockerfiles. The license boundary is enforced by repository separation. Despite separate Dockerfiles, both images use the same Rust toolchain, ORT version, and base image -- the enterprise Dockerfile can use the MIT image as a build cache source (`--cache-from`) to avoid recompiling shared crates.

**Recommendation**: Separate Dockerfiles in separate repositories. MIT image Dockerfile at repo root. Enterprise Dockerfile in the private repo. Both follow identical multi-stage patterns (builder + runtime). Document the license boundary enforcement in Dockerfile comments.

---

### Q: Should the container use `ort` crate `download-binaries` or system-installed ONNX Runtime?

**Answer**: Use system-installed ORT in the container image (Option B), matching the current CI pattern. For the STDIO dev use case, continue using the current system-install approach.

**Evidence**:

**Option A (`download-binaries`)** is already disabled in the project (`default-features = false` in `unimatrix-embed/Cargo.toml`). The project uses system-installed ORT via `.cargo/config.toml` (`ORT_LIB_LOCATION=/usr/local/lib`, `ORT_PREFER_DYNAMIC_LINK=1`). Risk: `download-binaries` fetches from Microsoft CDN at build time without SHA-256 verification (the same supply chain risk in Unimatrix entry #4274). The downloaded binary may target a different glibc version than the runtime base image (per entry #4284's lesson about glibc floor mismatch).

**Option B (system-installed ORT in builder stage)** is already proven in CI (`release.yml`): `curl -sL` the ORT tgz, extract, copy to `/usr/local/lib/`, `ldconfig`. ORT version pinned in workflow env (`ORT_VERSION: "1.20.1"`). `libonnxruntime.so.1.20.1` is 14 MB on disk (CPU-only pre-built release). Pre-built C/C++ tarballs available for both x86_64 and ARM64 from Microsoft's GitHub releases. RUNPATH `$ORIGIN` already set via `RUSTFLAGS`.

**Layer size comparison**:

| Component | Size |
|-----------|------|
| `unimatrix` binary (stripped, release) | ~31 MB |
| `libonnxruntime.so.1.20.1` | ~14 MB |
| Embedding model (all-MiniLM-L6-v2, ONNX) | ~87 MB |
| NLI model (quantized, qint8) | ~79-165 MB |
| Base image (distroless/cc) | ~32 MB |
| **Total runtime image estimate** | ~245-330 MB |

Both options produce approximately the same image size. The difference is build reproducibility and supply chain hygiene, where Option B wins because it is explicit and verifiable. Both options require network access at build time only -- no runtime internet dependency. Satisfies the air-gap constraint.

**Recommendation**: Use system-installed ORT in the Dockerfile builder stage. Pin ORT version + SHA-256 hash in the Dockerfile (not just the version). Copy `libonnxruntime.so` alongside the binary in the runtime stage. Close issue #4274 by adding checksum verification in both CI and the Dockerfile.

---

### Q: Which base image should both containers use?

**Answer**: Use `gcr.io/distroless/cc-debian12:nonroot` as the runtime base image for both MIT and enterprise containers. Use `rust:1.89-slim-bookworm` as the builder base.

**Evidence**:

| Image | Size | glibc | Shell | ONNX compat | SQLite compat | Notes |
|-------|------|-------|-------|-------------|---------------|-------|
| `gcr.io/distroless/cc-debian12` | ~32 MB | 2.36 (Debian 12) | No | Yes (glibc + libstdc++) | Yes (statically linked via sqlx) | Minimal attack surface. Includes libstdc++ needed by ORT. |
| `debian:12-slim` | ~75 MB | 2.36 | Yes (bash) | Yes | Yes | Larger attack surface but easier debugging. |
| `ubuntu:22.04` (minimal) | ~78 MB | 2.35 | Yes | Yes | Yes | Matches current CI baseline. Largest. |
| Alpine (musl) | ~7 MB | N/A (musl) | Yes (ash) | **No** -- ONNX Runtime requires glibc | Requires musl-compiled SQLite | **Eliminated.** |

Distroless/cc wins because:

1. **ONNX Runtime compatibility**: The `cc` variant includes glibc + libstdc++ (C++ runtime), exactly what ORT needs.
2. **glibc version**: Debian 12 ships glibc 2.36. Project glibc floor is 2.35 (Ubuntu 22.04 CI runners per entry #4284). Forward-compatible -- binaries built against 2.35 run on 2.36.
3. **Security surface**: No shell, no package manager, no coreutils. Minimal attack surface. The `:nonroot` tag sets UID 65534, satisfying the non-root container hard constraint directly.
4. **SQLite**: `sqlx` with `sqlite` feature statically compiles SQLite (bundled `libsqlite3-sys`). No system dependency. Works on any base.
5. **llama.cpp / GGUF future (W2-5)**: `llama.cpp` also requires glibc + libstdc++. Distroless/cc satisfies this without modification.

No-shell tradeoff mitigated by: `docker exec --debug` (Docker Desktop 4.27+), ephemeral debug containers (`kubectl debug`), log-based debugging via `tracing`, and a `docker-compose.override.yml` that swaps to `debian:12-slim` for shell access during development.

**Recommendation**: Use `gcr.io/distroless/cc-debian12:nonroot` for runtime (both images). Use `rust:1.89-slim-bookworm` for builder (glibc-compatible with runtime). Document the debug override in docker-compose.

---

### Q: Should both x86_64 and ARM64 ship from day one, and what is the CI/CD approach?

**Answer**: Ship both x86_64 and ARM64 from day one. Use per-architecture native GitHub Actions runners (not QEMU emulation) with a manifest merge step.

**Evidence**:

Dual-arch is a Hard constraint in SCOPE.md. The current CI already builds for both architectures on native runners (`ubuntu-22.04` for x64, `ubuntu-22.04-arm` for arm64).

QEMU emulation is not viable for Rust: 15-25x slower than native (50+ minutes for a 3-minute build), plus QEMU can cause compiler segfaults with Rust. This eliminates single-runner `docker buildx` with QEMU.

**Recommended approach (native runners)**:

1. Two CI jobs: `build-container-x64` (ubuntu-22.04) and `build-container-arm64` (ubuntu-22.04-arm).
2. Each runs `docker buildx build --platform linux/amd64` (or `linux/arm64`) and pushes a single-platform image.
3. A third job `create-manifest` merges them: `docker manifest create ghcr.io/dug-21/unimatrix:v{version} --amend ...amd64 --amend ...arm64` then pushes.

This mirrors the existing `release.yml` pattern (parallel native builds + merge step). ORT ARM64 pre-built tarballs are available from Microsoft (`onnxruntime-linux-aarch64-{version}.tgz`). The Dockerfile uses `TARGETARCH` build arg to select the correct ORT tarball. ONNX model files are architecture-independent (compute graphs, not native code).

**Recommendation**: Two native runner builds (x64 + arm64) + manifest merge. Mirror existing `release.yml` pattern. Use `TARGETARCH` in Dockerfile for ORT download selection. Ship both arches on every release tag.

---

### Q: What is the correct volume layout for each image?

**Answer**: MIT image uses a single data volume. Enterprise image uses three volumes.

**Evidence**:

**MIT image -- single volume `unimatrix-data`**:

Goal: `docker run -v unimatrix-data:/data -p 8443:8443 ghcr.io/dug-21/unimatrix` -- one command, one volume, one port.

```
/data/
  token                          # Static bearer token (generated on first run, mode 0600)
  projects/
    {project-hash}/
      unimatrix.db               # Knowledge DB
      analytics.db               # Analytics DB
  models/
    sentence-transformers_all-MiniLM-L6-v2/
      model.onnx                 # Embedding model (~87 MB)
      tokenizer.json
```

`config.toml` is NOT in the data volume -- it is a read-only bind mount per the hard constraint. The image ships with a sensible default config embedded; `config.toml` mount is optional (override only). Single volume is acceptable for MIT tier: single-user deployment needs no backup policy segregation, and the token file is protected by file permissions (0600, owned by nonroot UID). Per-project hashed directories continue inside `/data/projects/`.

**Enterprise image -- three volumes**:

```
/control/                        # unimatrix-control volume
  control.db                     # Control plane DB (user registry, RBAC, agent registry)
  audit.db                       # Structured compliance audit log

/knowledge/                      # unimatrix-knowledge volume
  projects/
    {project-hash}/
      unimatrix.db               # Per-repo knowledge DB
      analytics.db               # Per-repo analytics DB

/shared/                         # unimatrix-shared volume (read-only bind)
  models/
    *.onnx                       # ONNX models
  config.toml                    # Configuration
```

Control and knowledge volumes should remain separate: different SOC 2 retention requirements (audit logs typically 1 year minimum), blast radius isolation (knowledge DB corruption does not affect control plane), and Kubernetes storage class flexibility (control on SSD with snapshots, knowledge on standard). Models in shared volume are read-only at runtime (loaded once at startup per ADR-006/entry #2808).

**Recommendation**: MIT image: single `/data` volume + optional `config.toml` bind mount. Enterprise: three volumes (control, knowledge, shared:ro). Models baked into MIT image layer; enterprise mounts models from shared volume.

---

### Q: How should secrets be injected for each image?

**Answer**: MIT image: self-signed TLS cert generated at startup if no cert provided. Enterprise image: file-based secrets mounting from host or secrets manager.

**Evidence**:

**MIT image secrets model**:

| Secret | Source | Mechanism |
|--------|--------|-----------|
| TLS cert + key | Auto-generated or user-provided | If `tls.cert_path` and `tls.key_path` set in `config.toml`, use those PEM files (bind-mounted). If not set, generate self-signed cert at startup via `rcgen` (rustls team maintained). Store generated cert in data volume for persistence across restarts. |
| Bearer token | Auto-generated | Generated by daemon on first run (32-byte OsRng hex per ASS-041). Stored at `/data/token` with mode 0600. Printed to stdout once on first run. |

Self-signed cert generation at startup provides zero-config TLS: `rcgen` generates cert + key in <10ms, cert includes `localhost` and `127.0.0.1` as SANs. For production use (custom domain, trusted CA cert), the developer bind-mounts PEM files.

**Enterprise image secrets model**:

| Secret | Source | Mechanism |
|--------|--------|-----------|
| TLS cert + key | Secrets manager / cert-manager | Bind-mounted PEM files. Required -- no self-signed fallback. |
| OAuth client secret | Secrets manager | Mounted as file at configured path (e.g., `/run/secrets/oauth_client_secret`). |
| Bootstrap admin credential | Secrets manager | Mounted as file. Consumed on first run, then removable. |
| `config.toml` | ConfigMap / bind mount | Read-only bind mount (hard constraint). |

File-based over environment variables: env vars are visible via `docker inspect`, `/proc/*/environ`, and can leak into logs. File-based is Docker and Kubernetes best practice. Docker Compose `secrets:` directive and Kubernetes `Secret` volumes both converge on file mounting. File-based injection does not foreclose future Vault, AWS Secrets Manager, or Azure Key Vault integration.

**Recommendation**: MIT image: auto-generate self-signed TLS cert at startup (via `rcgen`) if no cert configured; auto-generate bearer token on first run. Enterprise: require file-mounted TLS certs and OAuth secrets; no self-signed fallback. Both images read secrets from files, never from environment variables.

---

### Q: How do two container builds fit into the existing Cargo workspace and CI?

**Answer**: Add container build jobs to existing `release.yml`, triggered on the same `v*` tag push. Use `cargo-chef` for layer caching. MIT and enterprise publish on separate triggers from separate repositories.

**Evidence**:

**Dockerfile structure (MIT image)** -- three-stage multi-stage build:

```dockerfile
# Stage 1: cargo-chef planner
FROM rust:1.89-slim-bookworm AS planner
RUN cargo install cargo-chef
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: cargo-chef builder
FROM rust:1.89-slim-bookworm AS builder
RUN cargo install cargo-chef
ARG ORT_VERSION=1.20.1
ARG ORT_SHA256_X64=<hash>
ARG ORT_SHA256_ARM64=<hash>
ARG TARGETARCH

# Install ORT with SHA-256 verification
RUN curl -sL "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-$([ "$TARGETARCH" = "arm64" ] && echo "aarch64" || echo "x64")-${ORT_VERSION}.tgz" -o ort.tgz \
    && echo "${expected_hash}  ort.tgz" | sha256sum -c - \
    && tar xzf ort.tgz \
    && cp onnxruntime-*/lib/libonnxruntime.so* /usr/local/lib/ \
    && ldconfig

WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release && strip target/release/unimatrix

# Stage 3: runtime
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /app/target/release/unimatrix /usr/local/bin/
COPY --from=builder /usr/local/lib/libonnxruntime.so* /usr/local/lib/
ENV LD_LIBRARY_PATH=/usr/local/lib
EXPOSE 8443
VOLUME ["/data"]
ENTRYPOINT ["unimatrix"]
CMD ["serve"]
```

`cargo-chef` caches all dependency compilation. Only `recipe.json` changes (Cargo.toml/Cargo.lock) trigger dependency layer rebuild. Source changes only rebuild final `cargo build` -- saving 3-5 minutes per build. The workspace has 9 crates with ~50 dependencies; dependency compilation dominates build time.

**CI integration**: Add to existing `release.yml` (triggered on `v*` tag push): two parallel container jobs (`build-container-x64` on ubuntu-22.04, `build-container-arm64` on ubuntu-22.04-arm), then `create-container-manifest`. Uses `docker/build-push-action` with `cache-from: type=gha` and `cache-to: type=gha,mode=max`. MIT image publishes on same `v*` tag as binary/npm releases. Enterprise image publishes from private repo on its own cadence. The existing `cargo install unimatrix` path for STDIO local use is unaffected.

**Recommendation**: Three-stage Dockerfile (planner, builder, runtime) with cargo-chef. Add container build jobs to `release.yml` parallel with existing binary builds. GHA cache for Docker layer persistence. Same `v*` tag trigger for MIT; separate trigger for enterprise from private repo.

---

### Q: What are the implications of future GGUF model support for container packaging?

**Answer**: GGUF models should be treated as volume-mounted artifacts, not baked into the image. The shared model volume design already accommodates this.

**Evidence**:

| Model | Format | Size | Purpose |
|-------|--------|------|---------|
| all-MiniLM-L6-v2 | ONNX | 87 MB | Embedding (required) |
| nli-minilm2-l6-h768 (quantized) | ONNX | 79 MB | NLI contradiction detection |
| GGUF (W2-5, conditional) | GGUF | 100 MB - 4 GB+ | Local inference (optional) |

GGUF models are self-contained binary files (weights + tokenizer + metadata). They can be significantly larger than ONNX embedding models -- even quantized small models are 100 MB+, useful inference models are 1-4 GB. Do not bake them into image layers (enormous images, every model update requires full rebuild + push + pull). The ONNX embedding model (87 MB) is small enough to bake into the MIT image for zero-config, but GGUF models cross the threshold where volume mounting is mandatory.

The shared model volume design handles this: enterprise `/shared/models/` (read-only bind mount) and MIT `/data/models/` both accommodate additional model files. Per ADR-006, models are lazily loaded -- GGUF follows the same pattern (check path on startup, degrade gracefully if missing). GGUF files are architecture-independent (same file works on x86_64 and ARM64), but `llama.cpp` itself is native C++ requiring per-arch compilation (same treatment as ORT: install in builder, copy to runtime). Per entry #4274's lesson, GGUF files should be hash-pinned in `config.toml`.

**Recommendation**: Design volume layout to accommodate GGUF from the start -- `/data/models/` (MIT) and `/shared/models/` (enterprise) already work. Do not bake GGUF into image layers. If W2-5 proceeds, add `llama.cpp` shared library alongside ORT in builder stage (same installation pattern). GGUF files are user-supplied via volume mount or `model-download` command.

---

## Unanswered Questions

**ORT SHA-256 hashes**: The specific SHA-256 hashes for `onnxruntime-linux-x64-1.20.1.tgz` and `onnxruntime-linux-aarch64-1.20.1.tgz` need to be captured and pinned at implementation time. This is mechanical, not a research question, but is required before the Dockerfile is finalized.

**Enterprise image distribution channel**: The SCOPE.md mentions "private registry, customer download, or ghcr.io with license gate." This is a commercial distribution decision dependent on enterprise go-to-market strategy, not a technical packaging question. The Dockerfile works regardless of registry choice.

**HEALTHCHECK specifics**: The vision document mentions `HEALTHCHECK on daemon liveness + schema version currency`. The liveness endpoint implementation depends on W2-2 (HTTPS transport) delivering a health endpoint. The Dockerfile HEALTHCHECK directive should be `HEALTHCHECK --interval=30s --timeout=5s CMD ["/usr/local/bin/unimatrix", "health"]` or similar, but the exact command depends on whether the binary exposes a CLI health subcommand or an HTTP health endpoint.

---

## Out-of-Scope Discoveries

- **cargo-chef + sccache combination**: Combining cargo-chef (Docker layer caching) with sccache (compiler-level caching) provides faster rebuilds by caching individual compilation artifacts. Worth evaluating if build times become a bottleneck.

- **OCI Artifacts for model distribution**: Docker's model runner and GGUF Packer use OCI Artifacts (special media types in OCI manifests) to distribute models via container registries. Could enable `docker pull ghcr.io/dug-21/unimatrix-models:all-minilm-l6-v2` for model versioning through registry infrastructure. Premature for Wave 2.

- **Chainguard distroless images**: Commercial distroless images with automated CVE patching and SBOM generation. Could replace `gcr.io/distroless` for enterprise deployments where CVE SLA guarantees matter. Worth evaluating for enterprise SOC 2 story.

- **Static linking ORT**: Eliminates shared library dependency and simplifies runtime image. However, requires compiling ORT from source (~30-60 minutes), making CI impractical. Dynamic linking with 14 MB `.so` is the pragmatic choice.

---

## Recommendations Summary

- **Two-image architecture**: Separate Dockerfiles in separate repositories. License boundary enforced by repository separation -- zero commercial code in the MIT repo.
- **ONNX Runtime packaging**: System-installed ORT in multi-stage Dockerfile builder stage. Pin version + SHA-256 hash. Copy `.so` to runtime stage alongside binary. Close #4274 gap.
- **Base image**: `gcr.io/distroless/cc-debian12:nonroot` for runtime. `rust:1.89-slim-bookworm` for builder. Distroless/cc provides glibc + libstdc++ for ORT. Non-root by default.
- **Multi-arch**: Ship x86_64 and ARM64 day one. Per-arch native GitHub Actions runners (no QEMU). Docker manifest merge step. Mirror existing `release.yml` pattern.
- **Volume layout**: MIT: single `/data` volume + optional `config.toml` bind mount. Enterprise: three volumes (control, knowledge, shared:ro). Models baked into MIT image; mounted in enterprise.
- **Secrets injection**: File-based secrets, never environment variables. MIT: auto-generated self-signed TLS cert (rcgen) + auto-generated bearer token. Enterprise: file-mounted TLS certs and OAuth secrets, no self-signed fallback.
- **Build pipeline**: Three-stage Dockerfile with cargo-chef. Container builds parallel with existing binary builds in `release.yml`. GHA cache for layer persistence. Same `v*` tag trigger for MIT; separate trigger for enterprise.
- **GGUF future**: Volume-mounted, not baked into image. Shared model directory design already accommodates GGUF. Same lazy-load pattern as ONNX. Hash-pin model files in config.
