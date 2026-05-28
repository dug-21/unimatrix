# FINDINGS: Deployment Topology and Feature Gating

**Spike**: ass-061
**Date**: 2026-05-28
**Approach**: investigation
**Confidence**: directional

---

## Findings

### Q1: How does cloud-only functionality get enabled/disabled?

**Answer**: Runtime config gating as the default mechanism. Cargo feature flags reserved exclusively for GGUF/llama.cpp (heavyweight native dependency). Never separate crates for core engine features.

**Evidence**: The codebase already demonstrates both patterns. Runtime gating: `nli_enabled` in `InferenceConfig` gates the entire NLI cross-encoder subsystem cleanly — when `false`, the model never loads and search falls back to cosine. Compile-time gating: `mcp-briefing` and `test-support` feature flags exist but add conditional compilation complexity. The stripped release binary is 30.7 MB; runtime-gated features add kilobytes to low megabytes — negligible. GGUF is the only case where compile-time gating is justified due to llama.cpp FFI platform-specific compilation, signal handler conflicts, and 1-8 GB model files. CI already builds 4 targets; adding Cargo feature permutations beyond one (`infer`) would create combinatorial CI problems.

**Recommendation**: Gate dashboard, health monitoring, and metrics endpoint via `config.toml` sections (e.g., `[dashboard] enabled = true`) following the `nli_enabled` pattern. Use Cargo feature flag `features = ["infer"]` only for GGUF. The W2-5 hypothesis is confirmed correct.

---

### Q2: Where does the UI/dashboard live architecturally?

**Answer**: A runtime-config-gated static SPA embedded in the binary using `rust-embed`, served on the W2-2 content port (8443) under a `/ui` path prefix. Minimum viable: read-only knowledge browser and health status page.

**Evidence**: W2-2 already exposes the MCP tool surface on the content port — `context_search`, `context_get`, `context_status`, `context_graph` provide all data a knowledge browser needs. `rust-embed` (or `include_dir`) is the standard Rust pattern for single-binary web apps; compressed SPA bundles add 1-5 MB. The distroless runtime (`gcr.io/distroless/cc-debian12:nonroot`) has no shell or package manager, so a separate UI container would need its own base image. Embedding preserves the single-binary principle without docker-compose overhead.

Minimum viable cloud-only UI scope:
- Health dashboard: daemon status, tick metadata, knowledge stats, vector index health, model loading status (data source: `context_status`)
- Knowledge browser: search, view, browse graph, inspect confidence (data sources: `context_search`, `context_get`, `context_graph`)
- NOT in MVP: entry editing, admin operations, user management (enterprise), real-time streaming

**Recommendation**: Embed a static SPA via `rust-embed`. Default `[dashboard] enabled = false` for npm/local; `true` in container config. Serve on the W2-2 content port under `/ui`. Build SPA separately (Vite/Svelte), include dist output as build artifact. Scope as W2-6 or Wave 3 — not a W2-1/W2-2 blocker. Enterprise admin console belongs in the private repository.

---

### Q3: How does config.toml discovery unify across both local and container topologies?

**Answer**: The existing three-layer config hierarchy (global -> per-project -> `UNIMATRIX_CONFIG` env override) already handles both topologies. nxs-013 (#648) co-locating per-project config with the data directory was the key enabler. No new config discovery mechanism is needed.

**Evidence**: Current `load_config()` in `infra/config.rs` loads three layers (last-wins): global at `~/.unimatrix/config.toml`, per-project at `{data_dir}/config.toml`, and env override via `UNIMATRIX_CONFIG`. Container mode works because the Dockerfile sets `HOME=/data`, placing global config at `/data/.unimatrix/config.toml` inside the data volume. `dirs::home_dir()` fallback correctly handles rootless containers with a warn-level log. The `write_default_config_if_absent()` path already supports first-run container initialization.

Config value classification:
- Topology-agnostic: `[server] instructions`, `[knowledge] categories`, `[confidence] preset/weights`
- Topology-specific: listen address (`0.0.0.0` vs `127.0.0.1`), TLS paths, `nli_enabled` (default `true` container / `false` local), `[dashboard] enabled`, rayon pool size

**Recommendation**: Do NOT create a `config.container.toml` overlay pattern — `UNIMATRIX_CONFIG` already serves this purpose. Bake a container-optimized default config into the image during Docker build (nli_enabled=true, listen 0.0.0.0, dashboard enabled). Users override via volume mount or `UNIMATRIX_CONFIG`. Individual env var overrides (`UNIMATRIX_LISTEN_ADDR`, etc.) are a lower-priority future enhancement following 12-factor patterns.

---

### Q4: How should ONNX (~85MB) and GGUF (1-8GB) models be distributed per topology?

**Answer**: ONNX models baked into container image (current design is correct). GGUF models via separate named volume with init-container download pattern. npm/local uses download-on-first-use for both (current design is correct).

**Evidence**: Current container image size budget: distroless base (~30 MB) + binary (~30 MB) + ORT library (~20 MB) + ONNX models (~75 MB) = ~155 MB. Lean and acceptable. GGUF at 1-8 GB would 10-50x the model portion, creating 2-9 GB images with problematic pull times and forcing image rebuilds for model swaps (models change independently of the engine). ADR-002 SHA-256 verification applies regardless of distribution method.

Distribution matrix:

| Model type | npm/local | Container (no GGUF) | Container (with GGUF) |
|-----------|-----------|--------------------|-----------------------|
| Embedding ONNX | Download on first use | Bake into image | Bake into image |
| NLI ONNX | Download on first use | Bake into image | Bake into image |
| GGUF | Download on first use | N/A | Separate volume + init |

GGUF container pattern: separate `unimatrix-models` named volume; init container runs `unimatrix model-download --gguf` before main container starts with SHA-256 verification; config references `/models/model.gguf` with `gguf_model_sha256`; air-gap supported by pre-populating the volume offline.

**Recommendation**: Keep ONNX baked into image, GGUF on separate volume with init-container download. File a separate issue for the embedding model SHA-256 verification gap (see Out-of-Scope Discoveries).

---

### Q5: Where exactly is the single-binary principle boundary?

**Answer**: The Unimatrix knowledge engine is always one binary. Docker-compose with multiple containers is NOT a violation when additional containers are operational tooling (model fetcher, backup agent, reverse proxy), not engine components.

**Evidence**: The PRODUCT-VISION.md principle states "all waves add capability to the same binary, not new services" — emphasis on "not new services." The binary already serves multiple transports (stdio, UDS, foreground container mode) from one process with all 14 MCP tools, background tick work, ML inference, and graph operations. ASS-014 Phase 3 architecture validates this at scale — centralized deployment is a single dockerized unimatrix-server with WASM cortical implant as a client, not a sidecar.

Boundary rule:

| Inside the binary | Outside (compose sidecar) |
|------------------|--------------------------|
| Knowledge CRUD, graph ops, ML inference (ONNX+GGUF), background tick, HTTP transport, dashboard/UI (embedded SPA), metrics endpoint, health check | TLS termination (operator choice), GGUF model download (init container), backup agent, log aggregation |

**The test**: If removing the component degrades the engine's ability to store, retrieve, or reason about knowledge — it belongs inside the binary. If removing it only affects operational convenience — it can be a sidecar.

Enterprise framing: the enterprise private repository adds a control plane (admin API, RBAC, audit console) as a separate binary that composes alongside the OSS engine. The single-binary principle applies to the knowledge engine, not the entire deployment topology.

**Recommendation**: Adopt this boundary rule as an ADR. All knowledge engine capabilities in one binary; operational tooling as optional compose sidecars. Docker-compose with 2-3 containers (engine + model init + optional reverse proxy) is standard container operations, not a principle violation.

---

## Unanswered Questions

None. All five Goal questions answered with directional confidence.

---

## Out-of-Scope Discoveries

1. **Embedding model SHA-256 verification gap**: `ensure_model()` for the embedding model does not verify SHA-256 hashes, while `ensure_nli_model()` has optional hash verification via `nli_model_sha256`. Security gap against ADR-002 supply chain integrity requirements. Recommend filing as a separate issue.

2. **`mcp-briefing` feature flag may be unnecessary**: The flag gates `context_briefing` at compile time, but it is in `default` features and no build target disables it. If no consumer ever builds without briefing, this adds complexity with no benefit. Consider converting to runtime config gating or removing entirely.

3. **Container-optimized default config not baked in**: The Dockerfile bakes models but does not generate a container-optimized `config.toml`. First-run uses compiled defaults. A pre-baked config with `nli_enabled = true` and container defaults would improve out-of-box experience.

4. **Model cache duplication in Docker build**: The builder stage briefly holds both HuggingFace cache and Unimatrix cache copies of models. For GGUF-sized models this would require 2x model size in disk during build. Validate build stage disk budget.

---

## Recommendations Summary

| Question | Recommendation |
|----------|---------------|
| Q1 — Feature Gating | Runtime config gating for all features except GGUF; reserve Cargo `features = ["infer"]` exclusively for llama.cpp FFI |
| Q2 — UI/Dashboard | Embed static SPA via `rust-embed`, serve on W2-2 content port under `/ui`, gate with `[dashboard] enabled`, scope as W2-6 or Wave 3 |
| Q3 — Config Discovery | No new mechanism needed — existing three-layer hierarchy works for both topologies; bake container-optimized defaults into the Docker image; do not create overlay patterns |
| Q4 — Model Distribution | ONNX baked into image (current design correct); GGUF on separate named volume with init-container pattern; file issue for embedding model SHA-256 gap |
| Q5 — Single-Binary Boundary | Knowledge engine capabilities always in one binary; operational tooling (init containers, reverse proxy, backup) as optional compose sidecars; document as ADR |
