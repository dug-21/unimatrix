# FINDINGS: Container + Packaging Strategy

**Spike**: ASS-043
**Date**: 2026-05-22
**Approach**: evaluation
**Confidence**: directional

---

## Findings

### Q0: Two-Image Architecture — Can MIT and enterprise share a Dockerfile?

**Answer**: No. Use separate Dockerfiles. The MIT image must contain zero commercial code — this is a license boundary enforced at the crate level, not a feature flag boundary. A single Dockerfile with build args is insufficient because `cargo build` in a workspace compiles all path-dependency crates transitively, and the enterprise crate's source code would be present in the build context even if not compiled.

**Evidence**:

The current workspace has 9 crates, all under MIT/Apache-2.0 via `workspace.package.license`:
- `unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`, `unimatrix-core` — foundation crates, `license.workspace = true`
- `unimatrix-engine`, `unimatrix-server` — higher-level crates (license field absent in their Cargo.toml but inherits workspace default)
- `unimatrix-adapt`, `unimatrix-observe`, `unimatrix-learn` — intelligence/observation crates, `license.workspace = true`

Per ASS-045 findings, enterprise commercial features ship from a **separate private repository** (`unimatrix-collective`). The enterprise binary is built from that repository, not from this one. This means:

- **MIT image**: Built from this repository. Single `Dockerfile` at repo root. Compiles `unimatrix-server` (which pulls all 8 other crates as dependencies). Produces the `unimatrix` binary. Published to `ghcr.io/dug-21/unimatrix` (public).
- **Enterprise image**: Built from the private `unimatrix-collective` repository. Separate `Dockerfile` in that repository. May vendor or `git` dependency the MIT crates. Produces a `unimatrix-enterprise` binary. Published to a private registry or `ghcr.io` with access controls.

Since the two images are built from different repositories, they inherently have separate Dockerfiles. The license boundary is enforced by repository separation — no commercial code can leak into the MIT image because it does not exist in this repository.

**Shared build layers**: Despite separate Dockerfiles, both images use the same Rust toolchain, same ORT version, and same base image. The enterprise Dockerfile can use the MIT image as a build cache source (`--cache-from`) to avoid recompiling shared crates.

**Recommendation**: Separate Dockerfiles in separate repositories. The MIT image Dockerfile lives at the root of this repo. The enterprise Dockerfile lives in the private repo. Both follow identical multi-stage patterns (builder + runtime) for consistency. Document the license boundary enforcement mechanism in the Dockerfile comments.

---

### Q1: ONNX Runtime Packaging — download-binaries vs system-installed?

**Answer**: Use **system-installed ORT in the container image** (Option B), matching the current CI pattern. For the STDIO dev use case, continue using the current system-install approach.

**Evidence**:

**Option A — `ort` crate `download-binaries` feature**:
- The `ort` crate's `download-binaries` feature downloads pre-built ORT binaries from Microsoft at build time. This is the default behavior when `ORT_STRATEGY` is unset or set to `download`.
- The project has already disabled this (`default-features = false` in `unimatrix-embed/Cargo.toml`) and uses system-installed ORT via `.cargo/config.toml` (`ORT_LIB_LOCATION=/usr/local/lib`, `ORT_PREFER_DYNAMIC_LINK=1`).
- Risk: `download-binaries` fetches from Microsoft CDN at build time without SHA-256 verification (the same supply chain risk already identified in Unimatrix entry #4274 for the CI curl|tar approach). Using `download-binaries` inside a Dockerfile would embed this risk in the container build.
- The downloaded binary may target a different glibc version than the runtime base image, causing silent compatibility issues (per entry #4284's lesson about glibc floor mismatch).
- Advantage: simpler Dockerfile — no explicit ORT install step.

**Option B — System-installed ORT in builder stage**:
- Already proven in CI (`release.yml`): `curl -sL` the ORT tgz, extract, `cp` to `/usr/local/lib/`, `ldconfig`.
- ORT version pinned in workflow env (`ORT_VERSION: "1.20.1"`).
- `libonnxruntime.so.1.20.1` is **14 MB** on disk (measured on the current system). This is the CPU-only, pre-built release from Microsoft. Compact — not the 500 MB debug/full build.
- Pre-built C/C++ tarballs are available for both architectures from Microsoft's GitHub releases: `onnxruntime-linux-x64-{version}.tgz` and `onnxruntime-linux-aarch64-{version}.tgz`.
- In the container, ORT lives in the runtime image alongside the binary. RUNPATH `$ORIGIN` (already set via `RUSTFLAGS="-C link-arg=-Wl,-rpath,$ORIGIN"`) means the binary finds the `.so` adjacent to itself.
- SHA-256 verification: the Dockerfile MUST add a checksum step after `curl|tar` to close the supply chain gap identified in entry #4274. Pin the expected SHA-256 hash per architecture per ORT version.

**Layer size comparison**:
| Component | Size |
|-----------|------|
| `unimatrix` binary (stripped, release) | ~31 MB |
| `libonnxruntime.so.1.20.1` | ~14 MB |
| Embedding model (all-MiniLM-L6-v2, ONNX) | ~87 MB |
| NLI model (quantized, qint8) | ~79-165 MB |
| Base image (distroless/cc) | ~32 MB |
| **Total runtime image estimate** | ~245-330 MB |

Both options produce approximately the same image size. The difference is build reproducibility and supply chain hygiene, where Option B wins because it is explicit and verifiable.

**Air-gap consideration**: Both options require network access at build time only. Once built, the container is self-contained — ORT library is baked in, models are baked in or mounted. No runtime internet dependency. This satisfies the air-gap constraint.

**Recommendation**: Use system-installed ORT in the Dockerfile builder stage. Pin ORT version + SHA-256 hash in the Dockerfile (not just the version). Copy `libonnxruntime.so` alongside the binary in the runtime stage. Close issue #4274 by adding checksum verification in both the CI pipeline and the Dockerfile.

---

### Q2: Base Image Selection

**Answer**: Use `gcr.io/distroless/cc-debian12:nonroot` as the runtime base image for both MIT and enterprise containers.

**Evidence**:

**Candidates evaluated**:

| Image | Size | glibc | Shell | ONNX compat | SQLite compat | Notes |
|-------|------|-------|-------|-------------|---------------|-------|
| `gcr.io/distroless/cc-debian12` | ~32 MB | 2.36 (Debian 12) | No | Yes (glibc + libstdc++) | Yes (statically linked via sqlx) | Minimal attack surface. Includes libstdc++ needed by ORT. |
| `debian:12-slim` | ~75 MB | 2.36 | Yes (bash) | Yes | Yes | Includes apt, shell — larger attack surface but easier debugging. |
| `ubuntu:22.04` (minimal) | ~78 MB | 2.35 | Yes | Yes | Yes | Matches current CI baseline. Largest. |
| Alpine (musl) | ~7 MB | N/A (musl) | Yes (ash) | **No** — ONNX Runtime requires glibc. musl compatibility is broken (GitHub issue #2909, #6800, #9483). | Requires musl-compiled SQLite | **Eliminated.** |

**Why distroless/cc wins**:

1. **ONNX Runtime compatibility**: ORT's pre-built shared libraries are compiled against glibc and depend on libstdc++. The `distroless/cc-debian12` image includes both glibc and libstdc++ (the `cc` variant specifically adds C++ runtime support over the base `distroless` image). This is exactly what ORT needs.

2. **glibc version**: Debian 12 ships glibc 2.36. The project's current glibc floor is 2.35 (Ubuntu 22.04 CI runners, per entry #4284). glibc 2.36 is forward-compatible — binaries built against glibc 2.35 run on 2.36. The container binary is built in the builder stage (which uses a glibc >= 2.35 image), so this works.

3. **Security surface**: Distroless has no shell, no package manager, no coreutils. Attack surface is minimal. This is a non-root container (using the `:nonroot` tag, which sets UID 65534). Satisfies the "non-root container user" hard constraint directly.

4. **SQLite**: The project uses `sqlx` with the `sqlite` feature, which statically compiles SQLite into the binary (bundled `libsqlite3-sys`). No system SQLite dependency. Works on any base image.

5. **llama.cpp / GGUF future (W2-5)**: If W2-5 proceeds, `llama.cpp` also requires glibc and libstdc++. Distroless/cc satisfies this dependency chain without modification.

**No-shell tradeoff**: Distroless lacks a shell for debugging. Mitigation:
- Use `docker exec --debug` (Docker Desktop 4.27+) or ephemeral debug containers (`kubectl debug`) for Kubernetes.
- Log-based debugging via `tracing` is already the primary debugging mechanism.
- For development/debugging: provide a `docker-compose.override.yml` that swaps the base image to `debian:12-slim` for shell access.

**Builder stage image**: Use `rust:1.89-slim-bookworm` (Debian 12 based, matches distroless/cc-debian12 glibc). This ensures the builder and runtime glibc are compatible.

**Recommendation**: Use `gcr.io/distroless/cc-debian12:nonroot` as the runtime base for both images. Use `rust:1.89-slim-bookworm` as the builder base. Document the debug override in docker-compose.

---

### Q3: Multi-Architecture Support

**Answer**: Ship both x86_64 and ARM64 from day one. Use per-architecture native GitHub Actions runners (not QEMU emulation) with a manifest merge step.

**Evidence**:

**Day-one dual-arch is required**: The SCOPE.md lists "Must support two hardware architectures: x86_64 and ARM64" as a Hard constraint. The current CI already builds for both architectures on native runners (`ubuntu-22.04` for x64, `ubuntu-22.04-arm` for arm64). The container build should mirror this.

**QEMU emulation is not viable for Rust**: Rust compilation under QEMU is extremely slow — 15-25x slower than native (Medium: vladkens, 2024). A build that takes 3 minutes native takes 50+ minutes under QEMU. Additionally, QEMU can cause compiler segfaults with Rust. This eliminates the single-runner `docker buildx` with QEMU approach.

**Native runner approach** (recommended):
1. Two CI jobs: `build-container-x64` (runs-on: `ubuntu-22.04`) and `build-container-arm64` (runs-on: `ubuntu-22.04-arm`).
2. Each job runs `docker buildx build --platform linux/amd64` (or `linux/arm64`) and pushes a single-platform image.
3. A third job `create-manifest` runs after both complete: `docker manifest create ghcr.io/dug-21/unimatrix:v{version} --amend ghcr.io/dug-21/unimatrix:v{version}-amd64 --amend ghcr.io/dug-21/unimatrix:v{version}-arm64` then `docker manifest push`.

This mirrors the existing release.yml pattern (parallel native builds, then a merge step for npm packaging). The approach avoids cross-compilation complexity and the `cross` crate entirely.

**ORT ARM64 availability**: Microsoft publishes pre-built `onnxruntime-linux-aarch64-{version}.tgz` for the C/C++ API. Version 1.20.1 is available. The Dockerfile uses `TARGETARCH` build arg to select the correct ORT tarball:
```dockerfile
ARG TARGETARCH
RUN curl -sL "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-$([ "$TARGETARCH" = "arm64" ] && echo "aarch64" || echo "x64")-${ORT_VERSION}.tgz" | tar xz
```

**ONNX model portability**: The embedding model (`all-MiniLM-L6-v2.onnx`) is architecture-independent — ONNX models are compute graphs, not native code. The same model file works on both x86_64 and ARM64 via ORT. No per-arch model variants needed.

**Recommendation**: Two native runner builds (x64 + arm64) + manifest merge. Mirror the existing `release.yml` pattern. Use `TARGETARCH` in the Dockerfile for ORT download selection. Ship both arches on every release tag.

---

### Q4: Volume Layout

**Answer**: MIT image uses a single data volume. Enterprise image uses three volumes per the vision document.

**Evidence**:

**MIT image — single volume `unimatrix-data`**:

The goal is `docker run -v unimatrix-data:/data -p 8443:8443 ghcr.io/dug-21/unimatrix` — one command, one volume, one port. The single volume contains:
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

The `config.toml` is NOT in the data volume — it is a read-only bind mount per the hard constraint. Default: the image ships with a sensible default config embedded, and `config.toml` mount is optional (override only).

Single volume is acceptable for the MIT tier because:
1. Single-user deployment — no backup policy segregation needed.
2. The token file is inside the volume but protected by file permissions (0600, owned by nonroot UID). In a single-user context, anyone with Docker socket access can already read volumes directly, so separating the token provides no meaningful security improvement.
3. The project already uses hashed directories per project inside the data root — this pattern continues naturally inside `/data/projects/`.

**Enterprise image — three volumes**:

Per the vision document (entry #4554) and WAVE2-ROADMAP.md:
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

**Should control and knowledge be combined?** No — keep them separate. Rationale:
1. **Backup cadence**: Control plane DB (user records, audit log) has SOC 2 retention requirements (typically 1 year minimum for audit logs). Knowledge DBs are integrity-critical but can be rebuilt from agent interactions. Different backup policies = different volumes.
2. **Blast radius**: If a knowledge DB corrupts (e.g., failed migration on one project), control plane is unaffected. Volume-level isolation prevents cross-contamination.
3. **Access control**: In Kubernetes, different volumes can have different `PersistentVolumeClaim` storage classes (e.g., control on SSD with snapshots, knowledge on standard).

**ONNX models in shared volume**: Models are read-only at runtime (loaded once at startup per ADR-006/entry #2808). Placing them in a read-only bind mount (`ro` flag) prevents any runtime modification and enables model updates via bind mount swap without touching data volumes. For the MIT image, models are baked into the image (simpler — no separate mount needed for basic use), but can optionally be overridden via a bind mount for air-gap updates.

**Recommendation**: MIT image: single `/data` volume + optional `config.toml` bind mount. Enterprise: three volumes (`control`, `knowledge`, `shared:ro`). Models baked into the MIT image layer; enterprise mounts models from `shared` volume.

---

### Q5: Secrets Injection

**Answer**: MIT image: self-signed TLS cert generated at startup if no cert provided. Enterprise image: file-based secrets mounting from host or secrets manager.

**Evidence**:

**MIT image secrets model** — minimal by design:

| Secret | Source | Mechanism |
|--------|--------|-----------|
| TLS cert + key | Auto-generated or user-provided | If `tls.cert_path` and `tls.key_path` are set in `config.toml`, use those PEM files (bind-mounted). If not set but TLS is enabled, generate a self-signed cert at startup using `rcgen` (already in the Rust ecosystem, maintained by the rustls team). Store the generated cert in the data volume so it persists across restarts. |
| Bearer token | Auto-generated | Generated by the daemon on first run (32-byte OsRng hex, per ASS-041). Stored at `/data/token` with mode 0600. Printed to stdout once on first run. |

Self-signed cert generation at startup is the right default for the MIT image:
- Zero-config TLS. Developer runs `docker run` and gets HTTPS immediately.
- `rcgen` can generate a cert + key pair in <10ms — no startup delay.
- The cert includes `localhost` and `127.0.0.1` as SANs.
- For production use (custom domain, trusted CA cert), the developer bind-mounts their own PEM files and sets the path in `config.toml`.

**Enterprise image secrets model** — file-based mounting:

| Secret | Source | Mechanism |
|--------|--------|-----------|
| TLS cert + key | Secrets manager / cert-manager | Bind-mounted PEM files. Path configured in `config.toml`. Required — no self-signed fallback in enterprise. |
| OAuth client secret | Secrets manager | Mounted as file at a configured path (e.g., `/run/secrets/oauth_client_secret`). Read by the daemon at startup. |
| Bootstrap admin credential | Secrets manager | Mounted as file. Consumed on first run (creates admin user), then the file can be removed. |
| `config.toml` | ConfigMap / bind mount | Read-only bind mount (hard constraint). |

**Why file-based over environment variables**: Environment variables are visible via `docker inspect`, `/proc/*/environ`, and can leak into logs. File-based secrets are the Docker and Kubernetes best practice. Docker Compose supports `secrets:` directive (file-based, no Swarm required). Kubernetes uses `Secret` volumes mounted as files. Both patterns converge on file mounting.

**Why not Docker Swarm secrets**: Docker Swarm secrets require Swarm mode, which is not a reasonable prerequisite for a single-developer deployment. The file-based approach works with plain Docker, Docker Compose, Kubernetes, and Nomad.

**Future-proofing**: File-based injection does not foreclose future integration with Vault, AWS Secrets Manager, or Azure Key Vault. Those systems expose secrets as files via CSI driver (Kubernetes) or init containers. The application reads files — it does not need to know about the secrets manager.

**Recommendation**: MIT image: auto-generate self-signed TLS cert at startup (via `rcgen`) if no cert configured; auto-generate bearer token on first run. Enterprise image: require file-mounted TLS certs and OAuth secrets; no self-signed fallback. Both images read secrets from files, never from environment variables.

---

### Q6: Build Pipeline Integration

**Answer**: Add container build jobs to the existing `release.yml` workflow, triggered on the same `v*` tag push. Use `cargo-chef` for layer caching. MIT and enterprise publish on separate triggers from separate repositories.

**Evidence**:

**Dockerfile structure (MIT image)**:

Three-stage multi-stage build:

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

**cargo-chef benefit**: The `cargo chef cook` step caches all dependency compilation. Only when `recipe.json` changes (i.e., Cargo.toml/Cargo.lock changes) does the dependency layer rebuild. Source code changes only rebuild the final `cargo build` — saving 3-5 minutes on typical builds. The workspace has 9 crates with ~50 dependencies; dependency compilation dominates build time.

**CI integration**:

The existing `release.yml` triggers on `v*` tag push and runs: build-linux-x64, build-linux-arm64, package-npm, create-release. Add two new jobs:

```yaml
build-container-x64:
  needs: []  # runs in parallel with binary builds
  runs-on: ubuntu-22.04
  steps:
    - checkout
    - docker/setup-buildx-action
    - docker/login-action (ghcr.io)
    - docker/build-push-action:
        context: .
        platforms: linux/amd64
        push: true
        tags: ghcr.io/dug-21/unimatrix:${{ github.ref_name }}-amd64
        cache-from: type=gha
        cache-to: type=gha,mode=max

build-container-arm64:
  needs: []
  runs-on: ubuntu-22.04-arm
  # same as above with linux/arm64

create-container-manifest:
  needs: [build-container-x64, build-container-arm64]
  runs-on: ubuntu-latest
  steps:
    - docker manifest create + push
    - tag :latest
```

**Trigger model**: MIT image publishes on the same `v*` tag as the binary/npm releases — same release event, parallel jobs. Enterprise image publishes from the private repo on its own release cadence.

**Shared layer caching**: The two container architectures cannot share Docker layers (different platform = different layer hashes). But each architecture caches its own layers across builds via GitHub Actions cache (`cache-from: type=gha`). cargo-chef layers (dependency compilation) are the most valuable cache — they change infrequently.

**`cargo install unimatrix` path**: The existing `cargo install` path for STDIO local use is unaffected. Container builds use `cargo build --release` and copy the binary — they do not use `cargo install`. These are parallel distribution channels.

**Recommendation**: Three-stage Dockerfile (planner, builder, runtime) with cargo-chef. Add container build jobs to `release.yml` parallel with existing binary builds. Use GHA cache for Docker layer persistence. Same tag trigger for MIT image. Enterprise image on separate trigger from private repo.

---

### Q7: GGUF Model Implications for Container Packaging

**Answer**: GGUF models should be treated as additional volume-mounted artifacts, not baked into the image. The shared model volume design already accommodates this.

**Evidence**:

**Current model landscape**:
| Model | Format | Size | Purpose |
|-------|--------|------|---------|
| all-MiniLM-L6-v2 | ONNX | 87 MB | Embedding (required) |
| nli-minilm2-l6-h768 (quantized) | ONNX | 79 MB | NLI contradiction detection |
| GGUF (W2-5, conditional) | GGUF | 100 MB - 4 GB+ | Local inference (optional) |

GGUF models are self-contained binary files: weights + tokenizer + metadata in a single file. This is architecturally simpler than ONNX (no separate tokenizer.json). However, GGUF models can be significantly larger than the ONNX embedding models — even quantized small models are typically 100 MB+, and useful inference models are 1-4 GB.

**Impact on container design**:

1. **Do NOT bake GGUF models into the image layer**. A 2 GB model file in a Docker layer makes the image enormous and means every model update requires a full image rebuild + push + pull. The ONNX embedding model (87 MB) is small enough to bake in for the MIT image's zero-config experience, but GGUF models cross the threshold where volume mounting is mandatory.

2. **Shared model volume**: The enterprise volume layout already includes `/shared/models/` as a read-only bind mount. GGUF models go here alongside ONNX models. The MIT image's `/data/models/` directory also accommodates this — GGUF files would be placed there via `unimatrix model-download` or manual copy.

3. **Model download at startup**: Per ADR-006 (entry #2808), models are lazily loaded. The GGUF model would follow the same pattern: check if model file exists at configured path on startup, provide a clear error message if missing, degrade gracefully (inference features unavailable but core functionality works).

4. **Architecture portability**: Unlike ONNX Runtime (which needs per-arch native libraries), GGUF models are processed by `llama.cpp` which compiles to native code. The GGUF file itself is architecture-independent — the same file works on x86_64 and ARM64. However, `llama.cpp` itself is a native C++ library that must be compiled per-arch, similar to ORT. It would need the same treatment: install in the builder stage, copy to runtime stage.

5. **SHA-256 pinning**: Per the existing pattern with ONNX models and the lesson from entry #4274, GGUF model files should be hash-pinned in `config.toml`. The `unimatrix model-download` command should verify the hash after download.

**Recommendation**: Design the volume layout to accommodate GGUF from the start — the `/data/models/` (MIT) and `/shared/models/` (enterprise) directories already work. Do not bake GGUF into image layers. If W2-5 proceeds, add `llama.cpp` shared library alongside ORT in the builder stage (same installation pattern). GGUF model files are user-supplied via volume mount or `model-download` command.

---

## Unanswered Questions

**ORT SHA-256 hashes**: The specific SHA-256 hashes for `onnxruntime-linux-x64-1.20.1.tgz` and `onnxruntime-linux-aarch64-1.20.1.tgz` need to be captured and pinned at implementation time. This is a mechanical step, not a research question, but is required before the Dockerfile is finalized.

**Enterprise image distribution channel**: The SCOPE.md mentions "private registry, customer download, or ghcr.io with license gate" for the enterprise image. This is a commercial distribution decision that depends on enterprise go-to-market strategy, not a technical packaging question. The Dockerfile works regardless of the registry choice.

**HEALTHCHECK specifics**: The vision document mentions `HEALTHCHECK on daemon liveness + schema version currency`. The liveness endpoint implementation depends on W2-2 (HTTPS transport) delivering a health endpoint. The Dockerfile HEALTHCHECK directive should be `HEALTHCHECK --interval=30s --timeout=5s CMD ["/usr/local/bin/unimatrix", "health"]` or similar, but the exact health check command depends on whether the binary exposes a CLI health subcommand or an HTTP health endpoint.

---

## Out-of-Scope Discoveries

**cargo-chef + sccache combination**: Web research surfaced that combining cargo-chef (Docker layer caching) with sccache (compiler-level caching) provides even faster rebuilds. sccache wraps `rustc` and caches individual compilation artifacts, so even partial dependency changes only recompile changed crates. Worth evaluating for CI build time optimization in a separate spike if build times become a bottleneck. Not pursued here because the current build time is acceptable.

**OCI Artifacts for model distribution**: Docker's model runner and GGUF Packer use OCI Artifacts (special media types in OCI manifests) to distribute GGUF models via container registries. This could be a future model distribution mechanism — `docker pull ghcr.io/dug-21/unimatrix-models:all-minilm-l6-v2` — enabling model versioning and distribution through the same registry infrastructure as the container images. Not pursued because it is premature for Wave 2.

**Chainguard distroless images**: Chainguard offers commercial distroless images with automated CVE patching and SBOM generation. These could replace `gcr.io/distroless` for enterprise deployments where CVE SLA guarantees matter. Worth evaluating for the enterprise tier's SOC 2 story but out of scope for this spike.

**Static linking ORT**: ORT can be statically linked (building from source without `--build_shared_lib`), which would eliminate the shared library dependency and simplify the runtime image. However, static ORT builds require compiling ORT from source (~30-60 minutes), which makes CI impractical. The dynamic linking approach with a 14 MB `.so` is the pragmatic choice.

---

## Recommendations Summary

- **Q0 (Two-image architecture)**: Separate Dockerfiles in separate repositories. License boundary enforced by repository separation — zero commercial code in the MIT repo.
- **Q1 (ONNX Runtime packaging)**: System-installed ORT in multi-stage Dockerfile builder stage. Pin version + SHA-256 hash. Copy `.so` to runtime stage alongside binary. Close #4274 gap.
- **Q2 (Base image)**: `gcr.io/distroless/cc-debian12:nonroot` for runtime. `rust:1.89-slim-bookworm` for builder. Distroless/cc provides glibc + libstdc++ for ORT. Non-root by default.
- **Q3 (Multi-arch)**: Ship both x86_64 and ARM64 day one. Per-arch native GitHub Actions runners (no QEMU). Docker manifest merge step for multi-arch tag. Mirror existing `release.yml` pattern.
- **Q4 (Volume layout)**: MIT image: single `/data` volume + optional `config.toml` bind mount. Enterprise: three volumes (control, knowledge, shared:ro). Models baked into MIT image; mounted in enterprise.
- **Q5 (Secrets injection)**: File-based secrets, never environment variables. MIT: auto-generated self-signed TLS cert (rcgen) + auto-generated bearer token. Enterprise: file-mounted TLS certs and OAuth secrets, no self-signed fallback.
- **Q6 (Build pipeline)**: Three-stage Dockerfile with cargo-chef. Container builds parallel with existing binary builds in `release.yml`. GHA cache for layer persistence. Same `v*` tag trigger for MIT; separate trigger for enterprise.
- **Q7 (GGUF future)**: Volume-mounted, not baked into image. Shared model directory design already accommodates GGUF. Same lazy-load pattern as ONNX. Hash-pin model files in config.
