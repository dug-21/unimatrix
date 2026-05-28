# FINDINGS-RAW: Deployment Topology and Feature Gating

**Spike**: ass-061
**Date**: 2026-05-28
**Researcher**: uni-spike-researcher (spike-061)
**Approach**: investigation
**Confidence**: directional

---

## Q1: Feature Gating Mechanism

**Q: How does cloud-only functionality get enabled/disabled?**

**Answer**: Use **runtime config gating as the default**, with **Cargo feature flags reserved exclusively for heavyweight native dependencies** (GGUF/llama.cpp). Do not use separate crates composed at Docker layer for core engine features.

**Evidence**:

The codebase already demonstrates both patterns and their trade-offs:

1. **Existing Cargo feature flag precedent**: `mcp-briefing` feature gates `context_briefing` tool registration at compile time (5 `#[cfg(feature = "mcp-briefing")]` sites in `mcp/tools.rs` and `mcp/response/mod.rs`). This works but adds conditional compilation complexity. The `test-support` feature flag is used across 6 crates for test helper sharing -- appropriate for test-only code.

2. **Existing runtime config precedent**: `nli_enabled` in `InferenceConfig` gates the entire NLI cross-encoder subsystem at runtime. When `false`, `NliServiceHandle` is constructed but never loads a model; search falls back to cosine. This pattern is clean, testable, and ships a single binary. The ONNX embedding pipeline also demonstrates graceful degradation -- absent model files trigger a fallback with logged warning.

3. **Binary size impact**: The stripped release binary is 30.7 MB. This includes the full MCP server, all 9 crates, ONNX runtime FFI bindings, HNSW, rayon pools, evaluation harness, and all detection rules. Adding a dashboard static asset bundle or additional runtime-gated features adds kilobytes to low megabytes -- negligible relative to the 30.7 MB baseline. The binary size concern is real only for GGUF (llama.cpp FFI adds ~5-15 MB of native code).

4. **Build matrix complexity**: The CI already builds 4 targets (linux-x64, linux-arm64 for both npm and container per ADR-004 #4572). Adding Cargo feature permutations would multiply this. Two build targets (with/without `infer`) is manageable; three or more features creates a combinatorial CI problem.

**Evaluation matrix**:

| Criterion | Cargo features | Runtime config | Separate crates |
|-----------|---------------|----------------|-----------------|
| Build complexity | Two targets per arch (4->8 CI jobs) | Single target (no change) | Independent build + compose |
| Binary size (npm) | Minimal: feature-gated code not linked | Ships all code (~KB overhead) | N/A (binary unchanged) |
| Testability | Must test both feature combinations | All paths testable in one build | Separate test suites |
| Single-binary | Preserved (compile-time variant) | Preserved (one binary) | Violated |
| Runtime flexibility | Requires rebuild to change | Config edit + restart | Container rebuild |
| Existing precedent | `mcp-briefing`, `test-support` | `nli_enabled`, model fallback | None in codebase |

**Recommendation**:

- **Dashboard, health monitoring, metrics endpoint**: Runtime config gating via `config.toml` sections (e.g., `[dashboard] enabled = true`, `[metrics] enabled = true`). Follow the `nli_enabled` pattern. The HTTP server (W2-2) already provides the transport; dashboard is a presentation layer on top.
- **GGUF inference only**: Cargo feature flag `features = ["infer"]`. This is the one case where compile-time gating is justified -- llama.cpp FFI is a heavyweight native dependency with platform-specific compilation, signal handler conflicts, and 1-8 GB model files. The W2-5 hypothesis of `features = ["infer"]` is correct.
- **Never separate crates for core features**: Fragmenting the knowledge engine across docker-compose services violates the single-binary principle and creates operational complexity (service discovery, restart ordering, health checking across services).

---

## Q2: The UI/Dashboard Question

**Q: Where does the UI/dashboard live architecturally?**

**Answer**: A feature-flagged static asset bundle served by the existing HTTP server. Minimum viable: a read-only knowledge browser and health status page. Architecturally, it is a **runtime-config-gated SPA** embedded in the binary, served on the W2-2 content port (8443).

**Evidence**:

1. **W2-2 provides the data API**: The HTTP transport (content port 8443) already exposes the MCP tool surface. A dashboard consumes this same API -- `context_search`, `context_get`, `context_status`, `context_graph` provide all the data a knowledge browser needs.

2. **Rust ecosystem pattern**: `rust-embed` crate (or `include_dir`) embeds static assets into the binary at compile time. This is the standard Rust pattern for single-binary web applications. The binary grows by the compressed size of the SPA bundle (typically 1-5 MB for a production React/Svelte build). Alternatively, the assets can be served from a filesystem path specified in config (more flexible for development).

3. **Single-binary preservation**: Embedding static assets in the binary maintains the single-binary principle. The dashboard is just another runtime capability of the same binary, enabled by config. No separate container, no docker-compose orchestration.

4. **The distroless runtime constraint**: The Dockerfile uses `gcr.io/distroless/cc-debian12:nonroot` -- no shell, no package manager. A separate UI container would need its own base image (nginx, node). Embedding in the binary avoids this entirely.

5. **Enterprise separation**: The PRODUCT-VISION.md and WAVE2-ROADMAP.md clearly delineate OSS vs. enterprise. A read-only knowledge browser and health dashboard belong in OSS. An admin console with user management, RBAC configuration, and audit log exploration belongs in the enterprise private repository.

**Minimum viable cloud-only UI**:

- **Health dashboard**: Daemon status, tick metadata, knowledge stats (entry count by category/status), vector index health, model loading status. Data source: `context_status`.
- **Knowledge browser**: Search entries, view full content, browse graph relationships, inspect confidence breakdown. Data sources: `context_search`, `context_get`, `context_graph`.
- **Not in MVP**: Entry editing (use MCP tools), admin operations, user management (enterprise), real-time streaming (WebSocket, future).

**Recommendation**: Embed a static SPA in the binary using `rust-embed`. Gate with `[dashboard] enabled = false` default (disabled for npm/local users; enabled in container config). Serve on the same W2-2 content port under a `/ui` path prefix. Build the SPA separately (Vite/Svelte or similar), include the dist output as a build artifact. The dashboard is a Wave 2+ feature -- not a W2-1/W2-2 blocker. Scope it as W2-6 or a separate Wave 3 item.

---

## Q3: Configuration Discovery Across Topologies

**Q: How does config.toml discovery unify across both local and container topologies?**

**Answer**: The existing three-layer config hierarchy (global -> per-project -> `UNIMATRIX_CONFIG` env override) already handles both topologies correctly. The nxs-013 merge (#648) co-locating per-project config with the data directory was the key enabler. No new config discovery mechanism is needed -- only documentation of topology-specific patterns.

**Evidence**:

1. **Current config loading (from `infra/config.rs` `load_config()`)**: Three layers, last-wins:
   - Global: `~/.unimatrix/config.toml` (HOME-relative)
   - Per-project: `{data_dir}/config.toml` (co-located with project data, post-nxs-013)
   - Env override: `UNIMATRIX_CONFIG=/path/to/config.toml` (highest priority)

2. **Container mode already works**: The Dockerfile sets `HOME=/data`, so:
   - Global config: `/data/.unimatrix/config.toml` (inside the data volume)
   - Per-project config: `/data/.unimatrix/{hash}/config.toml` (inside the data volume)
   - Env override: `UNIMATRIX_CONFIG=/path` (for Kubernetes ConfigMap/secrets manager)
   - The `docker-compose.yml` already documents this: `UNIMATRIX_CONFIG=/path/to/config.toml`

3. **`dirs::home_dir()` fallback**: When `HOME` is unset (rootless containers), the code falls back to compiled defaults with a warn-level log ("home directory not found; using compiled defaults (R-15)"). This is correct behavior.

4. **`write_default_config_if_absent()` in version command**: The `unimatrix version --project-dir /data` path already creates a default annotated config.toml in the data directory. Container entrypoint could use this for first-run initialization.

**Config value classification**:

| Config section | Topology-specific? | Container default | Local default |
|---------------|-------------------|-------------------|---------------|
| `[server] instructions` | No | Same | Same |
| `[knowledge] categories` | No | Same | Same |
| `[knowledge] freshness_half_life_hours` | No | Same | Same |
| `[confidence] preset / weights` | No | Same | Same |
| `[inference] rayon_pool_size` | Yes (CPU-dependent) | Container CPU limit | Host CPU / 2 |
| `[inference] nli_enabled` | Yes (model availability) | `true` (baked-in) | `false` (default) |
| `[inference] nli_model_sha256` | Yes (pinned per model) | Baked hash | User-managed |
| `[agents] default_trust` | Potentially | `permissive` (single-user) | `permissive` |
| Listen address (W2-2) | Yes | `0.0.0.0:8443` | `127.0.0.1:8443` |
| TLS cert/key paths (W2-2) | Yes | `/data/tls/` | User paths |
| `[dashboard] enabled` (future) | Yes | `true` | `false` |

**Recommendation**:

- **Do NOT create a `config.container.toml` overlay pattern**. The `UNIMATRIX_CONFIG` env var already serves this purpose. A separate overlay mechanism adds config loading complexity with no benefit.
- **Use `UNIMATRIX_CONFIG` for container-specific overrides**. Mount a ConfigMap or secrets manager output to a known path, set `UNIMATRIX_CONFIG` in the container env. This is the Kubernetes-native pattern.
- **Bake a container-optimized default config into the image**. During the Docker build, generate a `config.toml` with container-appropriate defaults (`nli_enabled = true`, listen address `0.0.0.0`, dashboard enabled). Place it at `/data/.unimatrix/config.toml`. Users override via volume mount or `UNIMATRIX_CONFIG`.
- **Add env var overrides for individual settings** (future enhancement): `UNIMATRIX_LISTEN_ADDR`, `UNIMATRIX_TLS_CERT`, etc. These are the 12-factor app pattern for container environments. Lower priority than `UNIMATRIX_CONFIG` file override.

---

## Q4: ML Model Distribution

**Q: How should ONNX (~85MB) and GGUF (1-8GB) models be distributed per topology?**

**Answer**: ONNX models baked into container image (current design is correct). GGUF models via separate named volume with init-container download pattern. npm/local topology uses download-on-first-use for both (current design is correct).

**Evidence**:

1. **Current ONNX distribution**:
   - **Container**: Baked into image during build (Dockerfile: `unimatrix model-download` and `model-download --nli`). Models at `/data/.cache/unimatrix/models/`. Image includes embedding model (~25 MB ONNX) + NLI cross-encoder (~50-85 MB ONNX depending on variant). Total model size in image: 75-110 MB.
   - **npm/local**: Downloaded on first use via `hf-hub` (HuggingFace Hub API) in `ensure_model()` and `ensure_nli_model()`. Cached at `~/.cache/unimatrix/models/`.
   - **SHA-256 verification**: ADR-002 is enforced for ORT runtime library (Dockerfile: `sha256sum -c -`). NLI model hash is optional but recommended (`nli_model_sha256` in config). Embedding model hash is NOT currently verified -- gap identified.

2. **ONNX model sizes (from codebase)**:
   - Embedding (all-MiniLM-L6-v2): ~25 MB ONNX file
   - NLI (minilm2-q8): ~50 MB; (deberta-q8): ~180 MB; (minilm2 FP32): ~313 MB; (deberta FP32): ~541 MB
   - Default NLI is minilm2-q8 at ~50 MB -- reasonable to bake in

3. **GGUF model sizes** (from W2-5 spec): 1-8 GB. This is categorically different from ONNX:
   - Baking 1-8 GB into the container image creates a 2-9 GB image. Container registries and CI/CD pipelines handle this, but pull times become problematic.
   - Update cadence: GGUF models change independently of the engine. Baking them in forces an image rebuild for every model swap.
   - Air-gap: the ADR-002 SHA-256 gate applies regardless of distribution method.

4. **Container image size budget**: Base distroless runtime (~30 MB) + binary (~30 MB) + ORT library (~20 MB) + ONNX models (~75 MB) = ~155 MB. This is a lean container image. Adding GGUF would 10-50x the model portion.

**Recommendation per topology**:

| Model type | npm/local | Container (no GGUF) | Container (with GGUF) |
|-----------|-----------|--------------------|-----------------------|
| Embedding ONNX | Download on first use | Bake into image | Bake into image |
| NLI ONNX | Download on first use | Bake into image | Bake into image |
| GGUF | Download on first use | N/A | Separate volume + init |

**GGUF distribution pattern (container)**:

- **Separate named volume**: `unimatrix-models` volume alongside `unimatrix-data`.
- **Init container**: A lightweight sidecar (`unimatrix model-download --gguf`) runs before the main container starts, downloading the GGUF model to the shared volume. SHA-256 verification runs during download (ADR-002).
- **Config reference**: `[inference] gguf_model_path = "/models/model.gguf"` and `gguf_model_sha256 = "..."` in config.toml.
- **Air-gap pattern**: Pre-populate the models volume offline. The init container checks hash validity and skips download if the model exists and passes verification.

**Embedding model SHA-256 gap**: The embedding model (`ensure_model()`) does NOT verify SHA-256 hashes. The NLI model has optional `nli_model_sha256` verification. The embedding model should gain the same `embedding_model_sha256` config field for supply chain integrity parity. Flag for a separate issue.

---

## Q5: Single-Binary Principle Boundary

**Q: Where exactly is the boundary? Is docker-compose with multiple containers a violation?**

**Answer**: The boundary is: **the Unimatrix knowledge engine is always one binary. Auxiliary operational containers (model fetcher, backup agent, reverse proxy) are NOT violations -- they are infrastructure composition, not engine fragmentation.** Docker-compose with multiple containers is acceptable when the additional containers are operational tooling, not engine components.

**Evidence**:

1. **The principle from PRODUCT-VISION.md**: *"Single binary: all waves add capability to the same binary, not new services"*. The emphasis is on "not new services" -- meaning the knowledge engine's capabilities (search, store, graph, inference, observation) must all be in one binary. It does not mean the deployment must be a single container.

2. **Existing architecture supports this**: The binary already serves multiple transport modes from the same process -- stdio, UDS, and foreground (container) mode. The `serve --foreground` flag (Dockerfile CMD) runs the same binary that `npx unimatrix` runs locally. All 14 MCP tools, all background tick work, all ML inference, all graph operations -- one binary.

3. **The docker-compose precedent**: The current `docker-compose.yml` is already single-service. But the debug override pattern in the same file demonstrates that compose is the natural place for operational containers.

4. **ASS-014 (Phase 3 architecture)**: The centralized deployment decision explicitly uses "dockerized unimatrix-server" as a single container. The WASM cortical implant is a CLIENT, not a sidecar service. This validates the single-binary principle even at scale.

**Proposed boundary rule**:

| Component | Inside the binary | Outside (compose sidecar) | Rationale |
|-----------|------------------|--------------------------|-----------|
| Knowledge CRUD (store, search, get, correct, deprecate) | Yes | -- | Core engine |
| Graph operations (context_graph, edge write) | Yes | -- | Core engine |
| ML inference (ONNX embed, NLI, GGUF) | Yes | -- | Core engine |
| Background tick (confidence, graph enrichment, observations) | Yes | -- | Core engine |
| HTTP transport (W2-2) | Yes | -- | Core engine access layer |
| Dashboard/UI (static SPA) | Yes | -- | Embedded presentation layer |
| Metrics endpoint (Prometheus) | Yes | -- | Observability surface of the engine |
| Health check | Yes | -- | Already in binary (`unimatrix health`) |
| TLS termination | Either | Reverse proxy (nginx/caddy) | Operator choice |
| Model download / init | Either | Init container for GGUF | Operational tooling |
| Backup agent | -- | Sidecar | Operational tooling (volume snapshot) |
| Log aggregation | -- | Sidecar | Operational tooling (standard k8s) |

**The test**: If removing the component degrades the knowledge engine's ability to store, retrieve, or reason about knowledge -- it belongs inside the binary. If removing it only affects operational convenience -- it can be a sidecar.

**Enterprise framing**: The enterprise private repository adds a control plane (admin API, RBAC config, audit console). This is a separate BINARY (not a separate crate in this repo). It composes alongside the OSS knowledge engine binary via docker-compose or Kubernetes. The single-binary principle applies to the knowledge engine, not to the entire deployment topology.

**Recommendation**: Adopt the explicit boundary rule above. Document it as an ADR. The rule is: **all knowledge engine capabilities in one binary; operational tooling as optional compose sidecars.** Docker-compose with 2-3 containers (engine + model init + optional reverse proxy) is not a violation -- it is standard container operations.

---

## Unanswered Questions

None. All five Goal questions answered with directional confidence.

---

## Out-of-Scope Discoveries

1. **Embedding model SHA-256 verification gap**: `ensure_model()` for the embedding model does not verify SHA-256 hashes, while `ensure_nli_model()` has optional hash verification via `nli_model_sha256`. The embedding model should gain the same verification for ADR-002 supply chain integrity parity. This is a security gap, not a topology question. Recommend filing as a separate issue.

2. **`mcp-briefing` feature flag may be unnecessary**: The `mcp-briefing` Cargo feature flag gates `context_briefing` at compile time, but there is no build target that disables it (it is in `default` features). If no consumer ever builds without briefing, this compile-time gate adds complexity with no benefit. Consider converting to runtime config gating or removing the feature flag entirely.

3. **Container-optimized default config not baked in**: The Dockerfile builds and bakes models but does not generate a container-optimized `config.toml`. First-run container startup uses compiled defaults. A pre-baked config with `nli_enabled = true` and appropriate container defaults would improve the out-of-box container experience.

4. **Model cache duplication in Docker build**: The Dockerfile cleans up the HuggingFace hub cache, but the model files are copied from HuggingFace cache to Unimatrix cache during `ensure_model()`. The builder stage briefly holds both copies. For GGUF-sized models this would be significant -- validate the Docker build stage has sufficient disk for 2x model size.
