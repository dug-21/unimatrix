# nan-015: Shared Model Volume for ONNX Models -- Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nan-015/SCOPE.md |
| Scope Risk Assessment | product/features/nan-015/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-015/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-015/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/nan-015/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-015/ALIGNMENT-REPORT.md |
| ADR-001 Env Var Redirect | product/features/nan-015/architecture/ADR-001-env-var-cache-redirect.md |
| ADR-002 Cache Path Precedence | product/features/nan-015/architecture/ADR-002-cache-path-precedence.md |
| ADR-003 Shared Volume Default RW | product/features/nan-015/architecture/ADR-003-shared-volume-default-rw.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| cache-path-resolution | pseudocode/cache-path-resolution.md | test-plan/cache-path-resolution.md |
| dockerfile | pseudocode/dockerfile.md | test-plan/dockerfile.md |
| compose-config | pseudocode/compose-config.md | test-plan/compose-config.md |
| documentation | pseudocode/documentation.md | test-plan/documentation.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Move ONNX model files (~166 MB: ~87 MB embedding + ~79 MB NLI quantized) from Docker image layers to a shared named volume (`unimatrix-shared`) mounted at `/shared`, reducing image size by at least 150 MB, enabling multi-container model sharing, and cleanly separating integrity-critical data from re-downloadable assets. Zero-config startup is preserved through the existing auto-download mechanism, and air-gap deployments remain supported via volume pre-population.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Cache redirect mechanism | `UNIMATRIX_MODEL_CACHE` env var checked in `resolve_cache_dir()` when `cache_dir` field is `None`. Explicit, self-documenting, no side effects on `dirs::` calls. | SCOPE OQ-01, ADR-001 | architecture/ADR-001-env-var-cache-redirect.md (Unimatrix #4650) |
| Cache path precedence | (1) `EmbedConfig.cache_dir` field > (2) `UNIMATRIX_MODEL_CACHE` env var > (3) `dirs::cache_dir()` + suffix > (4) `.unimatrix/models` fallback. Config field wins unconditionally; empty env var treated as unset. | ADR-002, RISK R-01 | architecture/ADR-002-cache-path-precedence.md (Unimatrix #4651) |
| Shared volume default mode | Default `:rw` in docker-compose.yml for zero-config auto-download. Document `:ro` as optional hardening after initial population. | SCOPE OQ-04, ADR-003 | architecture/ADR-003-shared-volume-default-rw.md (Unimatrix #4652) |
| Mount point | `/shared` with models at `/shared/models/`. Leaves room for GGUF at `/shared/models/gguf/` in W2-5. | SCOPE OQ-01 | architecture/ADR-001-env-var-cache-redirect.md |
| CI release.yml | No changes to release.yml itself -- Dockerfile change propagates automatically. Smoke tests need audit for model-presence assumptions. | Architecture Section 4 | N/A |
| `--cache-dir` CLI flag | Not in scope. No current demand signal. Env var covers the container use case. | SCOPE OQ-02, SPEC Not in Scope | N/A |
| Two-volume documentation update | PRODUCT-VISION.md and WAVE2-ROADMAP.md corrected to describe `unimatrix-data` + `unimatrix-shared`. | SCOPE OQ-03 | Unimatrix #4653 |

## Files to Create/Modify

### Rust Code

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-embed/src/config.rs` | Modify | Add `UNIMATRIX_MODEL_CACHE` env var check to `resolve_cache_dir()` between the `cache_dir` field check and `dirs::cache_dir()` fallback |

### Container/Infrastructure

| File | Action | Summary |
|------|--------|---------|
| `Dockerfile` | Modify | Remove model bake-in steps (builder model-download, runtime COPY), create `/shared/models` with 65532:65532 ownership and 0700 perms, set `UNIMATRIX_MODEL_CACHE=/shared/models`, declare `VOLUME ["/data", "/shared"]` |
| `docker-compose.yml` | Modify | Add `unimatrix-shared` named volume, mount at `/shared`, add backup guidance and `:ro` hardening comments |

### Documentation

| File | Action | Summary |
|------|--------|---------|
| `product/PRODUCT-VISION.md` | Modify | Update W2-1 section: replace "ONNX baked into image" with two-volume model description |
| `product/WAVE2-ROADMAP.md` | Modify | Update W2-1: correct "ONNX baked into image (correct)" annotation to two-volume model |

## Data Structures

### Modified: `EmbedConfig::resolve_cache_dir()` Resolution Chain

```rust
pub fn resolve_cache_dir(&self) -> PathBuf {
    // 1. Explicit config field (highest priority -- test overrides, operator config)
    if let Some(ref dir) = self.cache_dir {
        return dir.clone();
    }

    // 2. Container redirect via environment variable
    if let Ok(env_dir) = std::env::var("UNIMATRIX_MODEL_CACHE") {
        if !env_dir.is_empty() {
            return PathBuf::from(env_dir);
        }
    }

    // 3. Platform-specific default (unchanged)
    if let Some(cache) = dirs::cache_dir() {
        return cache.join("unimatrix").join("models");
    }

    // 4. Last resort fallback
    PathBuf::from(".unimatrix").join("models")
}
```

### Unchanged (reference only)

- `ensure_model(model: EmbeddingModel, cache_dir: &Path) -> Result<PathBuf>` -- accepts resolved path, unchanged
- `ensure_nli_model(model: NliModel, cache_dir: &Path) -> Result<PathBuf>` -- accepts resolved path, unchanged
- `NliConfig.cache_dir: PathBuf` -- populated from `resolve_cache_dir()`, unchanged

## Function Signatures

| Function | Crate | Change |
|----------|-------|--------|
| `EmbedConfig::resolve_cache_dir(&self) -> PathBuf` | `unimatrix-embed` | Add env var check at step 2 (between field check and dirs fallback) |
| `ensure_model(model: EmbeddingModel, cache_dir: &Path) -> Result<PathBuf>` | `unimatrix-embed` | No change -- accepts resolved path |
| `ensure_nli_model(model: NliModel, cache_dir: &Path) -> Result<PathBuf>` | `unimatrix-embed` | No change -- accepts resolved path |

### Call Sites (all use `EmbedConfig::default()` with `cache_dir: None`)

| Call Site | File:Line | Notes |
|-----------|-----------|-------|
| Embed startup (bridge) | `main.rs:612` | Via `start_loading` -> `OnnxProvider::new()` |
| NLI startup (bridge) | `main.rs:677` | `resolve_cache_dir()` -> `NliConfig.cache_dir` |
| Embed startup (daemon) | `main.rs:968` | Same as bridge path |
| NLI startup (daemon) | `main.rs:1032` | Same as bridge path |
| model-download CLI | `main.rs:1431` | Direct `resolve_cache_dir()` call |
| Eval harness NLI | `eval/profile/layer.rs:263` | Non-container only, inherits env var |
| Embed reconstruct | `embed_reconstruct.rs:43` | Via `OnnxProvider::new()` |
| Test support | `test_support.rs:36` | Test only |

## Constraints

1. **Distroless runtime**: No shell or package manager. Model download via `ensure_model()` / `ensure_nli_model()` only.
2. **Non-root container**: UID 65532 (nonroot). `/shared` must be owned 65532:65532 with 0700 perms.
3. **Network for first-run**: `hf-hub` requires internet for first download. Air-gap must pre-populate.
4. **Cache path precedence**: config field > env var > `dirs::cache_dir()` > `.unimatrix/models`. All call sites through single `resolve_cache_dir()`. (ADR-002)
5. **Non-container unchanged**: `UNIMATRIX_MODEL_CACHE` set only in Dockerfile. Native dev behavior identical.
6. **Read-only after load**: ONNX sessions open files read-only. Concurrent containers safe for reads.
7. **CI compatibility**: Audit `release.yml` for model-presence assumptions in smoke tests.
8. **Verify-then-load ordering**: NLI SHA-256 verification before ONNX session construction (lesson #4642). Preserved through path change.
9. **Empty string guard**: `UNIMATRIX_MODEL_CACHE=""` must fall through to `dirs::cache_dir()`, not produce relative path.
10. **Volume driver assumption**: Ownership inheritance from image layer guaranteed for local driver only. NFS/cloud may differ (documented limitation).

## Dependencies

### Rust Crates

| Crate | Role | Change |
|-------|------|--------|
| `unimatrix-embed` | `EmbedConfig::resolve_cache_dir()`, `ensure_model()`, `ensure_nli_model()` | Add env var check to `resolve_cache_dir()` |
| `unimatrix-server` | `main.rs` with call sites | No change |
| `hf-hub` | HuggingFace download | No change |
| `ort` | ONNX Runtime | No change |
| `dirs` | Platform cache dir | No change |

### External Services

| Service | Role | When |
|---------|------|------|
| HuggingFace Hub | Model download | First-run only, or manual `model-download` |

## NOT in Scope

- **Embedding model SHA-256 verification** -- tracked as #651
- **GGUF model management** -- future W2-4/W2-5, will use same `unimatrix-shared` volume
- **Enterprise multi-volume layout** -- private repo, separate volume architecture
- **Init-container pattern** -- future GGUF concern
- **Model version pinning / update mechanism** -- fixed repo IDs in `hf-hub`
- **`EmbedConfig::resolve_cache_dir()` platform logic changes for non-container** -- only container path affected
- **`--cache-dir` CLI flag** -- no demand signal, env var covers container use case
- **File locking for concurrent first-run downloads** -- rare scenario, safe but wasteful
- **Atomic write (temp file + rename) for download** -- deferred; existing retry handles corrupt files

## Alignment Status

**Overall: PASS with 1 WARN**

| Check | Status |
|-------|--------|
| Vision Alignment | PASS -- completes ASS-043 intended volume separation |
| Milestone Fit | PASS -- Nanoprobes W2-1, correct milestone |
| Scope Gaps | PASS -- all 11 ACs addressed |
| Scope Additions | PASS -- no material additions |
| Architecture Consistency | WARN (V-01) |
| Risk Completeness | PASS -- 15 risks, full traceability |

### V-01: Specification Precedence Ordering (WARN)

SPECIFICATION C-04 and Domain Models list env var as highest priority in cache path resolution. Architecture and risk strategy correctly place config field first. **Fix during delivery**: correct SPECIFICATION C-04 ordering to match architecture (config field > env var > dirs > fallback). No human approval needed -- documentation fix, not design change.

### Vision Principles Satisfied

- **Zero infrastructure**: Two named volumes still zero-infrastructure for operators
- **Graceful degradation**: Existing retry/backoff state machines unchanged
- **Single binary**: No new services
- **Container is optional**: Non-container behavior explicitly unchanged
