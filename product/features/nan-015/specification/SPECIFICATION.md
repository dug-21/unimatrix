# nan-015: Shared Model Volume for ONNX Models -- Specification

## Objective

Move ONNX model files (~166 MB: ~87 MB embedding + ~79 MB NLI quantized) from Docker image layers to a shared named volume (`unimatrix-shared`) mounted at `/shared`. This reduces image size by ~150 MB, enables multi-container model sharing, cleanly separates integrity-critical data (`unimatrix-data`) from re-downloadable assets (`unimatrix-shared`), and preserves zero-config startup through the existing auto-download mechanism.

---

## Functional Requirements

### FR-01: UNIMATRIX_MODEL_CACHE Environment Variable

`EmbedConfig::resolve_cache_dir()` must check `UNIMATRIX_MODEL_CACHE` environment variable when the `cache_dir` config field is `None`. Resolution precedence: `EmbedConfig.cache_dir` field (highest) > `UNIMATRIX_MODEL_CACHE` env var > `dirs::cache_dir()` platform default > `.unimatrix/models` fallback. The config field wins unconditionally so tests and operator overrides are never bypassed by the env var.

**Verification**: Unit test sets `UNIMATRIX_MODEL_CACHE=/tmp/test-models` and asserts `resolve_cache_dir()` returns that path. Second test leaves the var unset and asserts the `dirs::cache_dir()` fallback.

### FR-02: Single Resolution Function for All Call Sites

All three model-loading call sites (daemon embedding startup at `main.rs:612`, NLI startup at `main.rs:677`, and `model-download` CLI at `main.rs:1431`) must resolve model cache path through the same `resolve_cache_dir()` function with the env var precedence from FR-01.

**Verification**: Code inspection confirms no call site constructs a cache path independently. Integration test runs `model-download` with `UNIMATRIX_MODEL_CACHE` set and confirms models land at the specified path.

### FR-03: Dockerfile Model Bake-In Removal

Remove from the Dockerfile:
- Builder-stage model download steps (`unimatrix model-download` and `unimatrix model-download --nli`).
- Builder-stage HuggingFace cache cleanup (`rm -rf /data/.cache/huggingface`).
- Runtime-stage model COPY step (`COPY --from=builder /data/.cache/unimatrix/ /data/.cache/unimatrix/`).

**Verification**: `docker build` succeeds without model download. No model files exist in the built image at any path.

### FR-04: Dockerfile Shared Volume Configuration

The Dockerfile must:
- Create `/shared` directory with ownership `65532:65532` and permissions `0700`.
- Declare `VOLUME ["/data", "/shared"]`.
- Set `ENV UNIMATRIX_MODEL_CACHE=/shared/models`.

**Verification**: Built image inspection shows `/shared` exists with correct ownership/permissions. Environment variable is set in the image metadata.

### FR-05: docker-compose.yml Volume Definitions

`docker-compose.yml` must define both named volumes and mount them:
- `unimatrix-data` mounted at `/data` (existing).
- `unimatrix-shared` mounted at `/shared` (new).

**Verification**: `docker compose config` shows both volume definitions and mount points.

### FR-06: Auto-Download on Empty Volume

When the container starts with an empty `unimatrix-shared` volume, the daemon automatically downloads both ONNX models (embedding + NLI) to `/shared/models/` via the existing `ensure_model()` and `ensure_nli_model()` functions, then enters `Ready` state for both services.

**Verification**: Start container with fresh volumes; health check passes after download completes. Model files exist on the shared volume.

### FR-07: Cached Model Loading on Subsequent Starts

When models already exist on the `unimatrix-shared` volume, no download occurs on container start. Models are loaded directly from the shared volume.

**Verification**: Restart container; daemon logs show no download activity. Startup time is faster than first-run.

### FR-08: model-download CLI Writes to Shared Volume

`unimatrix model-download` and `unimatrix model-download --nli` executed inside the container write models to `/shared/models/`, not to `/data/.cache/`.

**Verification**: Run `docker exec unimatrix model-download`; verify model files appear under `/shared/models/` and not under `/data/`.

### FR-09: Multi-Container Model Sharing

Two separate containers mounting the same `unimatrix-shared` volume can both load models and reach healthy state.

**Verification**: Start two containers with the same `unimatrix-shared` volume (separate `unimatrix-data` volumes); both pass health checks.

### FR-10: Air-Gap Operation

A container starts successfully with a pre-populated shared volume and no internet access. No download is attempted.

**Verification**: Pre-populate volume, start container with `--network none`; health check passes. Daemon logs show no download attempts.

### FR-11: NLI Hash Verification Continuity

NLI SHA-256 hash verification (`nli_model_sha256` config) continues to function with models stored on the shared volume. The verify-then-load ordering (lesson #4642) is preserved.

**Verification**: Set `nli_model_sha256` in config; verify correct hash passes, incorrect hash causes graceful fallback to cosine.

### FR-12: Documentation Updates

- PRODUCT-VISION.md W2-1 section updated to describe the two-volume model (`unimatrix-data` + `unimatrix-shared`). Remove "ONNX models baked into the image" annotation.
- WAVE2-ROADMAP.md W2-1 updated to match.
- docker-compose.yml comments explain volume separation and backup guidance.

**Verification**: Documentation review confirms two-volume description is consistent across all three files.

### FR-13: Security Documentation for Shared Volume

Documentation includes guidance that operators using shared volumes should set `nli_model_sha256` to pin model integrity, acknowledges that embedding model SHA-256 enforcement is tracked separately (#651), and documents `:ro` mount as optional hardening after initial population.

**Verification**: Documentation review confirms all three points are present and that operators are not misled about the embedding hash enforcement gap (SR-04).

---

## Non-Functional Requirements

### NFR-01: Image Size Reduction

Docker image (uncompressed) must be at least 150 MB smaller than the current image with baked-in models.

**Measurement**: Compare `docker images` output before and after the change.

### NFR-02: Non-Container Behavior Unchanged

Developers running `unimatrix` outside Docker (without `UNIMATRIX_MODEL_CACHE` set) see identical model resolution behavior: models at `~/.cache/unimatrix/models/` (Linux/macOS) or platform equivalent.

**Measurement**: Run `resolve_cache_dir()` without the env var set; assert it returns the `dirs::cache_dir()` path.

### NFR-03: Startup Latency with Cached Models

Container startup with models present on the shared volume must not be slower than current startup with baked-in models. The path resolution change adds no measurable overhead.

**Measurement**: Startup time comparison (health check pass time) between current baked-in and new shared-volume with cached models.

### NFR-04: First-Run Download Resilience

First-run model download respects the existing retry policy with exponential backoff (3 retries) in `ensure_model()` / `ensure_nli_model()`. If download fails after retries, the daemon enters degraded state (`EmbedNotReady` / `NliNotReady`) and remains operational for non-ML endpoints.

**Measurement**: Existing graceful degradation behavior is preserved; no new failure modes introduced.

### NFR-05: HEALTHCHECK Continuity

The existing `HEALTHCHECK` continues to pass with the new volume layout.

**Measurement**: `docker inspect --format='{{.State.Health.Status}}'` returns `healthy` after model download completes.

### NFR-06: Volume Permission Safety

`/shared` directory and contents are accessible only to UID 65532 (nonroot). Other users in the container cannot read or modify model files.

**Measurement**: File permissions on `/shared` are `0700`, owned by `65532:65532`.

---

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | Docker image built without model bake-in is at least 150 MB smaller (uncompressed) than current image | `docker images` size comparison |
| AC-02 | `docker-compose.yml` defines both `unimatrix-data` and `unimatrix-shared` named volumes, with `unimatrix-shared` mounted at `/shared` | `docker compose config` inspection |
| AC-03 | On first start with empty `unimatrix-shared` volume, daemon auto-downloads both ONNX models to shared volume and enters `Ready` state | Container start test with fresh volumes; health check assertion |
| AC-04 | On subsequent starts, no model download occurs -- models loaded from shared volume | Container restart test; log inspection for absence of download activity |
| AC-05 | `unimatrix model-download` inside container writes to shared volume, not data volume | `docker exec` test; file location assertion |
| AC-06 | Two containers mounting same `unimatrix-shared` volume both load models successfully | Dual-container test; health check on both |
| AC-07 | NLI SHA-256 hash verification works with models on shared volume | Config-driven hash test; correct hash passes, wrong hash degrades gracefully |
| AC-08 | Air-gap: container starts with pre-populated shared volume and `--network none` | Network-isolated container test; health check passes |
| AC-09 | `HEALTHCHECK` continues to pass (no regression) | `docker inspect` health status |
| AC-10 | PRODUCT-VISION.md and WAVE2-ROADMAP.md W2-1 updated to describe two-volume model | Documentation review |
| AC-11 | Documentation includes security guidance: hash pinning for shared volumes, `:ro` hardening, and #651 gap acknowledgment | Documentation review |

---

## Domain Models

### Entities

| Term | Definition |
|------|-----------|
| `unimatrix-data` volume | Docker named volume mounted at `/data`. Contains integrity-critical data: databases (`knowledge.db`, `analytics.db`), vector indexes, configuration, logs. Must be backed up. |
| `unimatrix-shared` volume | Docker named volume mounted at `/shared`. Contains re-downloadable assets: ONNX models. Can be reconstructed from HuggingFace Hub. Backup optional. |
| Model cache directory | The directory where ONNX model files are stored and loaded from. Resolved by `EmbedConfig::resolve_cache_dir()`. In container: `/shared/models/`. Outside container: `~/.cache/unimatrix/models/`. |
| `UNIMATRIX_MODEL_CACHE` | Environment variable that overrides model cache directory resolution. Set in the Dockerfile to `/shared/models`. Not set outside containers. |
| `ensure_model()` | Function in `crates/unimatrix-embed/src/download.rs` that checks for model file existence (non-zero size), downloads from HuggingFace if absent, and returns the path. |
| `ensure_nli_model()` | Same pattern as `ensure_model()` for the NLI cross-encoder model. |
| `resolve_cache_dir()` | Method on `EmbedConfig` that determines the model cache directory. Resolution precedence: `cache_dir` config field > `UNIMATRIX_MODEL_CACHE` env var > `dirs::cache_dir()` platform default > `.unimatrix/models` fallback. |

### Relationships

```
unimatrix-shared (volume)
  └── /shared/models/
        ├── embedding model (~87 MB)
        └── NLI model (~79 MB, quantized)

unimatrix-data (volume)
  └── /data/
        ├── knowledge.db
        ├── analytics.db
        ├── vector indexes
        ├── config.toml
        └── logs/

resolve_cache_dir() ──uses──> EmbedConfig.cache_dir (if Some)
                    ──uses──> UNIMATRIX_MODEL_CACHE (if set)
                    ──uses──> dirs::cache_dir() (platform default)
                    ──uses──> .unimatrix/models (final fallback)
```

---

## User Workflows

### Workflow 1: First-Run (Internet Available)

1. Operator runs `docker compose up`.
2. Docker creates empty `unimatrix-data` and `unimatrix-shared` volumes.
3. Daemon starts, `resolve_cache_dir()` returns `/shared/models/`.
4. `ensure_model()` finds no cached embedding model, downloads from HuggingFace.
5. `ensure_nli_model()` finds no cached NLI model, downloads from HuggingFace.
6. NLI hash verification runs if `nli_model_sha256` is configured.
7. Both services enter `Ready` state. Health check passes.

### Workflow 2: Subsequent Starts

1. Operator restarts container or runs `docker compose up` again.
2. Volumes persist. Models exist on `unimatrix-shared`.
3. `ensure_model()` / `ensure_nli_model()` find existing files, skip download.
4. Services enter `Ready` state immediately.

### Workflow 3: Air-Gap Deployment

1. Operator pre-populates `unimatrix-shared` volume with model files (via `docker cp`, bind mount, or manual copy).
2. Container starts with `--network none` or in a network-isolated environment.
3. `ensure_model()` / `ensure_nli_model()` find existing files, skip download.
4. Services enter `Ready` state. No internet required.

### Workflow 4: Multi-Container Sharing

1. Operator defines two services in `docker-compose.yml`, both mounting the same `unimatrix-shared` volume (separate `unimatrix-data` volumes).
2. First container downloads models on first start.
3. Second container finds models already present, skips download.
4. Both containers load models read-only after initial ONNX session setup.

### Workflow 5: Manual Model Download

1. Operator runs `docker exec unimatrix model-download` inside a running container.
2. CLI resolves cache path via `resolve_cache_dir()` to `/shared/models/`.
3. Models download to the shared volume.

### Workflow 6: Hardened Deployment (Read-Only Volume)

1. Operator populates the shared volume (Workflow 3 or 5).
2. Operator changes docker-compose mount to `/shared:ro`.
3. Container starts; models load from read-only mount. Auto-download is disabled by filesystem permissions (writes fail, but existing files load successfully).

---

## Constraints

### C-01: Distroless Runtime Image

The runtime stage (`gcr.io/distroless/cc-debian12:nonroot`) has no shell, no package manager. Model download must happen via the unimatrix binary itself through `ensure_model()` / `ensure_nli_model()`. No `curl` or `wget` available. (Maps to SCOPE Constraint 1.)

### C-02: Non-Root Container

Container runs as UID 65532 (nonroot). `/shared` and its contents must be owned by `65532:65532`. Volume permissions must allow read/write by this UID. (Maps to SCOPE Constraint 2.)

### C-03: Network Dependency for First-Run

`hf-hub` crate requires internet access for first-run model download. Air-gap deployments must pre-populate the shared volume before starting the container. (Maps to SCOPE Constraint 3.)

### C-04: Cache Path Resolution Precedence (SR-01, SR-06)

The resolution chain for model cache path must be unambiguous and documented:
1. `EmbedConfig.cache_dir` config field (highest priority).
2. `UNIMATRIX_MODEL_CACHE` env var.
3. `dirs::cache_dir()` platform default.
4. `.unimatrix/models` final fallback.

All three call sites must go through the single `resolve_cache_dir()` function. No parallel resolution logic permitted. This addresses SR-01 (incorrect precedence) and SR-06 (call site divergence).

### C-05: Non-Container Usage Unchanged (SR-08)

Developers running `unimatrix` outside Docker must see identical behavior. The `UNIMATRIX_MODEL_CACHE` env var is set only in the Dockerfile. If a developer accidentally sets it in their shell, it will redirect model storage -- this is documented as the env var's intended behavior, not a bug. (Maps to SCOPE Constraint 5, addresses SR-08.)

### C-06: Read-Only Model Access After Load

ONNX session opens model files read-only via `Session::builder().commit_from_file()`. After initial load, the model file is not re-read. Concurrent containers can safely read the same files. (Maps to SCOPE Constraint 6.)

### C-07: CI/CD Compatibility (SR-05)

`release.yml` and all CI jobs that build or test the container image must be updated to not assume models are baked in. Smoke tests and health checks that run the container must allow time for model download or pre-populate the shared volume. Any downstream job that depends on baked-in models must be identified and updated.

### C-08: Verify-Then-Load Ordering (SR-02)

Hash verification must occur before model loading, as established in lesson #4642. The shared volume widens the attack surface (a compromised container or volume mount can replace model files), making this ordering more critical. Embedding model SHA-256 enforcement is out of scope (#651) but NLI hash verification must be confirmed to work through the new path.

### C-09: Partial File Corruption Handling (SR-07)

The existing `ensure_model()` / `ensure_nli_model()` functions use file existence + non-zero size checks. A partial write from a crashed download could leave a non-zero-size corrupt file that passes the existence check but fails ONNX session load. When ONNX session load fails on a present file, the existing retry state machine (`Loading -> Failed -> Retrying`, 3 retries with exponential backoff) handles recovery. The ONNX load failure is logged and the service enters `Failed` state, then retries. The ensure functions re-check on retry.

### C-10: Docker Volume Driver Assumptions

`/shared` directory ownership (65532:65532) inheritance from the image layer is guaranteed for the local storage driver. NFS or cloud volume drivers may handle ownership differently. This is documented as a known limitation.

---

## Dependencies

### Rust Crates

| Crate | Role | Change Required |
|-------|------|----------------|
| `unimatrix-embed` | Contains `EmbedConfig::resolve_cache_dir()`, `ensure_model()`, `ensure_nli_model()` | Add `UNIMATRIX_MODEL_CACHE` env var check to `resolve_cache_dir()` |
| `unimatrix-server` | Contains `main.rs` with three call sites that invoke `resolve_cache_dir()` | No change (call sites already use `resolve_cache_dir()`) |
| `hf-hub` | HuggingFace model download | No change |
| `ort` | ONNX Runtime session loading | No change |

### External Services

| Service | Role | Dependency Type |
|---------|------|----------------|
| HuggingFace Hub | Model download source | Runtime (first-run only, or manual `model-download`) |

### Existing Components

| Component | Role | Change Required |
|-----------|------|----------------|
| `Dockerfile` | Container image build | Remove model bake-in, add `/shared` volume, set `UNIMATRIX_MODEL_CACHE` |
| `docker-compose.yml` | Container orchestration | Add `unimatrix-shared` volume definition and mount |
| `release.yml` | CI/CD pipeline | Update for no-bake-in image build |
| `PRODUCT-VISION.md` | Product documentation | Update W2-1 volume description |
| `WAVE2-ROADMAP.md` | Roadmap documentation | Update W2-1 volume description |

---

## NOT in Scope

- **Embedding model SHA-256 verification**: Tracked as #651. nan-015 does not add hash pinning for the embedding model. NLI hash verification already exists and is preserved.
- **GGUF model management**: The `unimatrix-shared` volume will host GGUF models in the future (W2-4), but GGUF support is not part of nan-015.
- **Enterprise multi-volume layout**: The enterprise image (private repo) has its own volume architecture. nan-015 addresses only the MIT/OSS image.
- **Init-container pattern**: WAVE2-ROADMAP mentions init-containers for GGUF. nan-015 uses the daemon's existing auto-download path. Init-containers are a future GGUF concern.
- **Model version pinning or update mechanism**: Models are downloaded by `hf-hub` at fixed repo IDs. No version management UI or update notification.
- **Changes to `EmbedConfig::resolve_cache_dir()` platform logic for non-container use**: The cross-platform `dirs::cache_dir()` resolution for non-container use (Linux, macOS, Windows) is unchanged. Only the container path is affected via the env var.
- **`--cache-dir` CLI flag**: No current demand signal. The env var covers the container use case. Follow-up if operators request it.
- **File locking for concurrent first-run downloads**: If two containers start simultaneously with an empty volume, both may download. This is safe but wasteful, and the rarity of the scenario does not justify the complexity.
- **Atomic write (temp file + rename) for download**: While SR-07 recommends this, the existing `ensure_model()` behavior (existence + non-zero size check, with retry on ONNX load failure) provides sufficient resilience. Atomic write is an improvement that can be addressed separately.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- returned 13 entries. Key relevant entries: #4647 (model hash pinning procedure), #4642 (hash verification ordering lesson), #4636 (ADR-004 volume descriptions), #69/#70 (hf-hub and cache directory ADRs), #4570 (ORT supply chain verification ADR), #4579 (container build pattern). These informed constraints C-04, C-08, and the verify-then-load ordering emphasis.
