# ASS-043: Container + Packaging Strategy

**Date**: 2026-04-09
**Tier**: 2 (independent, parallel with Tier 1)
**Feeds**: W2-1 (container packaging)
**Related open issue**: #517 (ONNX Runtime packaging decision)

**Breadth**: code+ecosystem (internal build config, CI pipeline, Cargo workspace + external container ecosystem, ONNX packaging, base image landscape)
**Approach**: evaluation
**Confidence required**: directional

---

## Question

How should the two Wave 2 containers be structured — the MIT developer cloud image and the commercial enterprise image — and what is the correct ONNX Runtime packaging approach for each?

Wave 2 introduces the evolution two distinct container images:

| Image | Auth | Control plane | License | Use case |
|-------|------|---------------|---------|----------|
| `unimatrix` (MIT) | startup-generated static token | none | MIT | single developer, multiple machines, Codespaces, multiple projects |
| `unimatrix-enterprise` (commercial) | OAuth 2.1 + RBAC | yes | commercial | multi-user, multi-agent, compliance |

---

## Why It Matters

W2-1 (container packaging) cannot be scoped without these decisions. The two-image model changes volume layout, secrets injection, build pipeline, and ONNX packaging approach. The MIT image is the adoption driver — it must be simple to run (`docker run`, single token, one volume). The enterprise image is the commercial product — it must satisfy SOC 2 operational requirements.

---

## What to Explore

### 0. Two-Image Architecture

Before evaluating packaging specifics, confirm the two-image model and its implications:

**MIT image (`unimatrix`):**
- HTTPS transport + static token auth (no OAuth, no control plane)
- Single project volume (no `unimatrix-control`)
- Goal: `docker run -v unimatrix-data:/data -p 8443:8443 ghcr.io/unimatrix/unimatrix` — one command, one volume, works
- Must be genuinely runnable by a developer in under 5 minutes with no prior infrastructure
- License: MIT. Published to public registry.

**Enterprise image (`unimatrix-enterprise`):**
- HTTPS transport + OAuth 2.1 + RBAC + control plane DB
- Full volume layout (control, knowledge, analytics, shared)
- Two listeners (content + admin port)
- License: commercial. Distribution channel TBD (private registry, customer download, or ghcr.io with license gate).

Evaluate: can the two images share the same Dockerfile with build args, or do they require separate Dockerfiles? The MIT image must not contain any commercial code — this is a license boundary, not just a build configuration.

### 1. ONNX Runtime Packaging (#517)
- **Option A — `ort` crate `download-binaries` feature**: the `ort` crate downloads pre-built ONNX Runtime binaries at build time. Simpler build setup; no system dependency. Requires internet access during build (not during runtime). Evaluate: reproducibility, binary version pinning, CI caching, SHA-256 verification.
- **Option B — System-installed ONNX Runtime**: ONNX Runtime installed in the base image. Container build links against it. Evaluate: which base image provides a compatible version, whether the version satisfies the model requirements, layer caching.
- Evaluate specifically for the **container use case** — the STDIO dev use case may have a different answer.
- What is the resulting container layer size difference between options? Smaller images matter for pull times and air-gap bundle size.
- Dev Simplicity: docker run.... as a means for full operations w/out other actions is a direct value for adoption.

### 2. Base Image Selection
- Candidates: distroless/cc, Alpine (musl), Debian slim (glibc), Ubuntu minimal.
- The project already ships to two Linux targets — identify what those are and whether both are glibc or musl matters for ONNX compatibility.
- SQLite and llama.cpp (if W2-5 proceeds) have their own linking requirements. Evaluate base image against all three native dependencies.
- Distroless is ideal from a security surface standpoint but limits shell access for debugging. Is the operational tradeoff acceptable?
- Multi-stage build: builder image (full Rust toolchain + deps) produces the binary; final image is minimal. Confirm the build artifact is self-contained.

### 3. Multi-Architecture Support
- x86_64 and ARM64 are the target arches. Does the enterprise container need both from day one?
- Cross-compilation in CI: evaluate `cross` crate vs. Docker buildx multi-arch vs. native ARM64 runner.
- ONNX Runtime for ARM64: are pre-built binaries available for both architectures? (Option A dependency)
- What does the CI/CD pipeline look like for producing a multi-arch manifest?

### 4. Volume Layout

**MIT image** (simple — minimize setup friction):
- Single data volume (`unimatrix-data`) containing: knowledge DB(s), analytics DB(s), static token file, ONNX models
- One volume = one `docker run` flag = lowest friction for the developer use case
- Evaluate: is a single volume acceptable for the MIT image, or does separating the token file from the data DBs matter for this tier?
- Project already creates hashed directories by project.. assumed this would continue to support multiple projects for both enterprise and developer cloud version.

**Enterprise image** (per backup policy and compliance):
- `unimatrix-control` — control plane DB (integrity-critical, audit log, backs up on its own policy)
- `unimatrix-knowledge` — per-repo `unimatrix.db` files (integrity-critical)
- `unimatrix-shared` — ONNX models + `config.toml` as read-only bind mount

Evaluate: should `unimatrix-control` and `unimatrix-knowledge` be combined (same backup cadence) or kept separate (control plane may be on a different backup policy for SOC 2)?

### 5. Secrets Injection

**MIT image**: secrets are minimal — TLS cert/key only (or self-signed generated at startup). Static token is generated by the daemon itself and stored in the data volume. No secrets manager needed. Evaluate: self-signed cert generated at startup (zero-config) vs. user-provided PEM (more flexible). Should the MIT image generate a self-signed cert by default if no cert is provided?

**Enterprise image**: at container startup, the following must be available:
- TLS certificate + private key
- OAuth client secret (or JWKS endpoint config)
- Bootstrap admin credential (first-run only, then consumed)
- `config.toml` (contains sensitive configuration)

Evaluate: Docker secrets, environment variables, mounted files from a secrets manager (Vault, AWS Secrets Manager). Minimum viable for Wave 2 that doesn't foreclose more sophisticated injection later.

### 6. Build Pipeline Integration
- How do two container builds fit into the existing Cargo workspace and CI?
- Dockerfile location: separate Dockerfiles per image, or a single parameterized Dockerfile with build args that selects which crate to compile? The MIT image must not compile or include any commercial crate — a build arg that changes a `--features` flag is insufficient if the commercial crate is in the same workspace.
- What `cargo` commands does each Dockerfile execute? Ensure the existing `cargo install unimatrix` path remains unchanged.
- Docker layer caching: `cargo chef` or equivalent to avoid full rebuild on non-dependency changes. Evaluate whether the two images can share cached layers.
- CI/CD: should MIT and enterprise images publish on the same release tag, or on separate triggers?

#### 7. Potential for future additional Inference
- GGUF model inclusion is on the roadmap.  This or another small model type could be considered for addtinoal functionality.  Portability of shared model volume or other implications for GGUF


---

## Output

1. **Two-image architecture confirmation** — MIT image spec, enterprise image spec, license boundary enforcement in build
2. **ONNX Runtime packaging decision** — chosen option with rationale, layer size comparison, air-gap assessment
3. **Base image recommendation** — compatibility matrix against ONNX, SQLite, and llama.cpp; applies to both images
4. **Multi-arch strategy** — yes/no for Wave 2 day-one, CI/CD approach if yes
5. **Volume layout** — per image: MIT (minimal) and enterprise (compliance-appropriate)
6. **Secrets injection approach** — per image: MIT (self-signed + volume token) and enterprise (full secrets manager path)
7. **Build pipeline integration sketch** — Dockerfile structure per image, CI trigger model, layer caching

---

## Constraints (all Hard)

- Non-root container user (non-negotiable, both images)
- `config.toml` as read-only bind mount — never in a writable data volume (both images)
- Air-gap deployable: no runtime internet dependencies once deployed (both images)
- Single binary per container — one Unimatrix process, not a microservice mesh
- MIT image must contain zero commercial code — build pipeline must enforce this at the crate boundary
- Must support two hardware architectures: x86_64 and ARM64. Linux only — which Linux base is part of the research.

---

## Prior Art

**Codebase state:**
- `ort = "=2.0.0-rc.9"` with `ort-sys = "=2.0.0-rc.9"`, default-features disabled (`crates/unimatrix-embed/Cargo.toml`)
- Dynamic linking against system-installed ORT at `/usr/local/lib` via `.cargo/config.toml` (`ORT_LIB_LOCATION`, `ORT_PREFER_DYNAMIC_LINK=1`)
- CI release builds use native runners: `ubuntu-22.04` (x64) and `ubuntu-22.04-arm` (arm64) — both glibc 2.35 baseline
- ORT downloaded via `curl|tar` from GitHub releases in CI, copied to `/usr/local/lib/`
- Binary stripped, RUNPATH verified, model-download smoke test in CI

**Unimatrix entries (researcher should retrieve for full context):**
- **#4274** (lesson): Release pipeline downloads ORT without SHA-256 checksum — supply chain risk. Fix: pin expected hash alongside ORT_VERSION.
- **#4284** (lesson): Pin x64 to ubuntu-22.04 to match arm64 glibc 2.35 floor. ubuntu-latest (24.04, glibc 2.39) creates silent compatibility gap.
- **#4554** (feature): W2-1 container packaging — named volumes, non-root user, HEALTHCHECK on liveness + schema version.
- **#2808** (ADR-006): Lazy embedding model initialization — relevant to container startup behavior.
- **#2804** (lesson): ONNX-backed components use feature-flagged stubs and pure function extraction.
