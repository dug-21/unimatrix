# nan-015: Shared Model Volume for ONNX Models

## Problem Statement

ONNX models (~166 MB: ~87 MB embedding + ~79 MB NLI quantized) are currently baked into the Docker image at build time. This has three consequences:

1. **Image bloat**: 166 MB of model files in image layers. Every image pull transfers these files even though they rarely change.
2. **Rebuild cost**: Every `docker build` re-downloads both models from HuggingFace during the builder stage (lines 98-101 of the Dockerfile), even when only application code changed. The `model-download` step cannot be cached because it depends on the compiled binary.
3. **No multi-container sharing**: If multiple containers need the same models (e.g., dev + production, or multiple project-specific containers), each image carries its own copy.

Additionally, the current design conflates two categories of data with different backup profiles: integrity-critical data (`unimatrix-data`: databases, vector indexes, config) and re-downloadable assets (ONNX models). Backing up `unimatrix-data` currently captures model files needlessly, since they can be re-obtained from HuggingFace Hub at any time.

The original product vision (PRODUCT-VISION.md, ASS-043 FINDINGS.md) specified a `unimatrix-shared` volume for ONNX models. nan-014 simplified to a single volume with models baked into the image for zero-config startup. Now that the container is shipping and the tradeoffs are understood, this feature completes the originally intended separation.

## Goals

1. Move ONNX model files (embedding + NLI) from Docker image layers to a shared named volume (`unimatrix-shared`) mounted at a dedicated path in the container.
2. Reduce Docker image size by ~166 MB.
3. Enable multiple containers to share a single set of model files via the same named volume.
4. Preserve zero-config startup: models are automatically downloaded on first run if not present on the volume.
5. Maintain air-gap capability: operators can pre-populate the shared volume with models; no runtime internet required if models are present.
6. Clean backup separation: `unimatrix-data` contains only integrity-critical data; `unimatrix-shared` contains re-downloadable assets.

## Non-Goals

- **Embedding model SHA-256 verification**: Filed separately as #651. nan-015 does not add hash pinning for the embedding model. NLI model hash verification already exists and continues to work.
- **GGUF model management**: GGUF models (W2-4) will use the same `unimatrix-shared` volume in the future, but GGUF support is not part of nan-015.
- **Enterprise multi-volume layout**: The enterprise image (private repo) has its own volume architecture. nan-015 addresses only the MIT/OSS image.
- **Init-container pattern**: The WAVE2-ROADMAP mentions init-containers for GGUF. nan-015 uses the daemon's existing `ensure_model` / `ensure_nli_model` auto-download path at startup instead. Init-containers are a future GGUF concern.
- **Model version pinning or update mechanism**: Models are downloaded by the `hf-hub` crate at fixed repo IDs. No version management UI or update notification is in scope.
- **Changes to `EmbedConfig::resolve_cache_dir()` platform logic**: The cross-platform `dirs::cache_dir()` resolution for non-container use (Linux, macOS, Windows) is unchanged. Only the container path needs adjustment.

## Background Research

### Current Model Loading Architecture

**Cache path resolution chain** (container context):
1. `EmbedConfig::resolve_cache_dir()` (`crates/unimatrix-embed/src/config.rs:40-51`): If `cache_dir` field is `Some`, uses it directly. Otherwise calls `dirs::cache_dir()`, which on Linux returns `$HOME/.cache`. Falls back to `.unimatrix/models` if `dirs::cache_dir()` returns `None`.
2. In the container, `HOME=/data` (Dockerfile line 128), so `dirs::cache_dir()` returns `/data/.cache`. The resolved path becomes `/data/.cache/unimatrix/models/`.
3. Both `OnnxProvider::new()` and `NliServiceHandle::spawn_load_task()` call `resolve_cache_dir()` to locate models.

**Embedding model loading** (`main.rs:612`): `embed_handle.start_loading(EmbedConfig::default())` -- uses default `cache_dir: None`, so `resolve_cache_dir()` resolves via `dirs::cache_dir()`.

**NLI model loading** (`main.rs:677`): `EmbedConfig::default().resolve_cache_dir()` is called explicitly to populate `NliConfig.cache_dir`. Same resolution path.

**Model download CLI** (`main.rs:1425-1496`): `handle_model_download()` also uses `EmbedConfig::default().resolve_cache_dir()`. Same path.

**Key insight**: All three consumers (embed startup, NLI startup, model-download CLI) use `EmbedConfig::default().resolve_cache_dir()`. The path is determined entirely by `$HOME` in the container. Changing `$HOME` or overriding `cache_dir` in `EmbedConfig` would redirect all model I/O to a different location.

### Current Dockerfile Model Flow

```
Builder stage:
  ENV HOME=/data                          # line 97
  unimatrix model-download                # downloads embedding to /data/.cache/unimatrix/models/
  unimatrix model-download --nli          # downloads NLI to /data/.cache/unimatrix/models/
  rm -rf /data/.cache/huggingface         # clean HF hub cache duplicates

Runtime stage:
  COPY --from=builder /data/.cache/unimatrix/ /data/.cache/unimatrix/    # line 122
  COPY --from=builder /data /data                                         # line 125
  ENV HOME=/data                          # line 128
  VOLUME ["/data"]                        # line 133
```

The `VOLUME ["/data"]` declaration means `/data` is the mount point for `unimatrix-data`. Models at `/data/.cache/unimatrix/models/` are inside this volume, which means:
- They are included in `unimatrix-data` backups (wasteful).
- They cannot be shared independently with other containers.

### Volume Design Precedents

**ASS-043 FINDINGS.md**: Recommended separate `unimatrix-shared` volume for MIT image models + config. Enterprise uses three volumes (control, knowledge, shared:ro).

**nan-014 decision**: Simplified to single volume + models baked into image. Rationale: zero-config startup, reduced operational complexity. Documented in nan-014 SCOPE "Volume Layout Reconciliation" and ADR-004 (nxs-013, entry #4636).

**nxs-013**: Updated PRODUCT-VISION.md and WAVE2-ROADMAP.md to reflect the shipped single-volume design. Both now say "ONNX models baked into the image."

### Auto-Download Mechanism

The `ensure_model()` and `ensure_nli_model()` functions (`crates/unimatrix-embed/src/download.rs`) already handle the "download if not present" case. They check for file existence and non-zero size before downloading. This means:
- If the shared volume is empty on first run, models download automatically.
- If the shared volume is pre-populated (air-gap), no download occurs.
- If models are corrupted (zero-byte), they are re-downloaded.

NLI SHA-256 verification (`NliServiceHandle::spawn_load_task()` line 284) runs after `resolve_model_dir()` returns. This continues to work regardless of where the models are stored.

### Graceful Degradation

Both service handles (`EmbedServiceHandle`, `NliServiceHandle`) implement `Loading -> Ready | Failed -> Retrying` state machines with exponential backoff (3 retries). The server starts immediately; model loading is async. Missing models produce `EmbedNotReady` / `NliNotReady` errors until loaded, with cosine fallback for NLI.

## Proposed Approach

### Volume Architecture

Add a second named volume `unimatrix-shared` mounted at `/shared` in the container:

```
unimatrix-data   -> /data    (databases, vector indexes, config, logs -- integrity-critical)
unimatrix-shared -> /shared  (ONNX models -- re-downloadable)
```

Model cache resolves to `/shared/models/` instead of `/data/.cache/unimatrix/models/`.

### Implementation Strategy

**1. Dockerfile changes**:
- Remove model bake-in steps (lines 96-101: `HOME=/data`, `model-download`, `model-download --nli`, `rm -rf /data/.cache/huggingface`).
- Remove model COPY step (line 122: `COPY --from=builder /data/.cache/unimatrix/ /data/.cache/unimatrix/`).
- Add `/shared` directory creation with correct ownership (65532:65532) and permissions (0700) in the builder stage.
- Add `VOLUME ["/data", "/shared"]` (or two separate VOLUME directives).
- Set environment variable to redirect model cache: either `UNIMATRIX_MODEL_CACHE=/shared/models` or adjust `EmbedConfig` defaults for the container context.

**2. docker-compose.yml changes**:
- Add `unimatrix-shared` named volume definition.
- Mount it at `/shared` alongside the existing `unimatrix-data` at `/data`.
- Update comments to explain the volume separation.

**3. Cache path redirection** (choose one):
- **Option A (ENV-based)**: Set a new environment variable (e.g., `UNIMATRIX_MODEL_CACHE=/shared/models`) in the Dockerfile. Modify `EmbedConfig::resolve_cache_dir()` to check this env var before falling back to `dirs::cache_dir()`. Minimal code change.
- **Option B (Config-based)**: Use `EmbedConfig.cache_dir = Some("/shared/models")` set via config.toml default in the container. No code change to `resolve_cache_dir()`, but requires the config to be loaded before model download.
- **Option C (HOME-based)**: Keep `HOME=/data` for project data but set a separate env var for cache. `dirs::cache_dir()` on Linux uses `$XDG_CACHE_HOME` if set, falling back to `$HOME/.cache`. Setting `XDG_CACHE_HOME=/shared` would redirect to `/shared/unimatrix/models/`.

**Recommended: Option A** -- explicit, self-documenting, no side effects on other `dirs::` calls, and works for both the daemon startup path and the `model-download` CLI path.

**4. model-download CLI**: Must also respect the redirected cache path so that `docker exec unimatrix model-download` writes to the shared volume, not the data volume.

**5. Documentation updates**:
- Update PRODUCT-VISION.md W2-1 to describe the two-volume model.
- Update WAVE2-ROADMAP.md W2-1 to match.
- Update docker-compose.yml backup example in comments.

### Startup Behavior

On first container start with an empty `unimatrix-shared` volume:
1. Daemon starts, initializes embed handle (`start_loading`).
2. `EmbedConfig::resolve_cache_dir()` returns `/shared/models/`.
3. `ensure_model()` finds no cached files, downloads from HuggingFace.
4. `ensure_nli_model()` same.
5. Both models are now on the shared volume, persisting across container restarts and available to other containers.

On subsequent starts: models found on volume, no download. Air-gap: operator pre-populates the volume.

### Multi-Container Sharing

Multiple containers mounting the same `unimatrix-shared` volume will share model files. The `ensure_model()` / `ensure_nli_model()` functions use file existence + non-zero size checks, which are safe for concurrent read-after-write on a Docker named volume (local storage driver). ONNX sessions open model files read-only after initial load. No file locking concerns for the read path.

Write contention during initial download: if two containers start simultaneously with an empty volume, both may attempt to download. The `hf-hub` crate writes to its own cache first, then `ensure_model()` copies to the target directory. In the worst case, one container overwrites the other's copy with an identical file. This is safe but wasteful. A file lock or sentinel file could prevent this, but given the rarity of the scenario (only on first-ever start), it is not worth the complexity.

## Acceptance Criteria

- AC-01: Docker image built without model bake-in is at least 150 MB smaller (uncompressed image size) than the current image (measured via `docker images`).
- AC-02: `docker-compose.yml` defines both `unimatrix-data` and `unimatrix-shared` named volumes, with `unimatrix-shared` mounted at `/shared`.
- AC-03: On first start with an empty `unimatrix-shared` volume, the daemon automatically downloads both ONNX models (embedding + NLI) to the shared volume and enters `Ready` state for both services.
- AC-04: On subsequent starts, no model download occurs -- models are loaded from the shared volume.
- AC-05: `unimatrix model-download` (run inside the container) writes models to the shared volume, not the data volume.
- AC-06: Two separate containers mounting the same `unimatrix-shared` volume can both load models successfully (verified by health check passing on both).
- AC-07: NLI SHA-256 hash verification (`nli_model_sha256` config) continues to work with models on the shared volume.
- AC-08: Air-gap scenario: container starts successfully with a pre-populated shared volume and no internet access (verified by running with `--network none` after volume population).
- AC-09: `HEALTHCHECK` continues to pass (no regression from model path change).
- AC-10: PRODUCT-VISION.md and WAVE2-ROADMAP.md W2-1 sections updated to describe the two-volume model (unimatrix-data + unimatrix-shared). WAVE2-ROADMAP "ONNX baked into image (correct)" annotation corrected.
- AC-11: Documentation includes security guidance: operators using shared volumes should set `embedding_model_sha256` and `nli_model_sha256` to pin model integrity (shared volume widens attack surface vs baked-in models).

## Constraints

1. **Distroless runtime image**: The runtime stage (`gcr.io/distroless/cc-debian12:nonroot`) has no shell, no package manager. Model download must happen via the unimatrix binary itself (already the case via `ensure_model`). No `curl` or `wget` available at runtime.
2. **Non-root container**: UID 65532 (nonroot). The `/shared` directory and its contents must be owned by this UID. Volume permissions must allow read/write by 65532.
3. **`hf-hub` download requires network**: First-run auto-download needs internet access. Air-gap deployments must pre-populate the shared volume.
4. **`EmbedConfig::resolve_cache_dir()` is called in three places**: Daemon embed startup (`main.rs:612` via `EmbedConfig::default()`), NLI startup (`main.rs:677`), and model-download CLI (`main.rs:1431`). All three must resolve to the shared volume path in the container.
5. **Non-container usage must not change**: Developers running `unimatrix` outside Docker must see identical behavior (models at `~/.cache/unimatrix/models/`). The redirection must be container-specific (via environment variable set in the Dockerfile).
6. **ONNX session opens model files read-only**: After initial `Session::builder().commit_from_file()`, the model file is not re-read. Concurrent containers can safely read the same files.
7. **Existing CI/CD**: The `release.yml` container build jobs must be updated to not bake models. Build time should decrease since `model-download` steps are removed from the builder stage. Verify no downstream CI jobs (smoke tests, health checks) assume models are present in the image.

## Resolved Questions

- **OQ-01**: Mount at `/shared`. Set `UNIMATRIX_MODEL_CACHE=/shared/models`. The mount point is the volume; the subdirectory is the code's concern. Leaves room for GGUF models (W2-5) at `/shared/models/gguf/` or similar.
- **OQ-02**: Not in scope. Environment variable covers the container use case. `--cache-dir` flag is a convenience for non-container manual management with no current demand signal. File follow-up if operators request it.
- **OQ-03**: Yes, update as part of AC-10. The "ONNX baked into image (correct)" annotation becomes incorrect after nan-015.
- **OQ-04**: Default to read-write in docker-compose.yml. Auto-download on first run (Goal 4) requires `:rw`. Document `:ro` as optional hardening step after initial population. Air-gap operators who pre-populate can choose `:ro` themselves.

## Tracking

GitHub Issue: #647
