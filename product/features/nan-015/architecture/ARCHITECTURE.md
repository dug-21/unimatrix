# nan-015: Shared Model Volume — Architecture

## System Overview

nan-015 separates ONNX model storage from the integrity-critical data volume. Today, ~166 MB of ONNX models (embedding + NLI) are baked into the Docker image and stored at `/data/.cache/unimatrix/models/` inside the `unimatrix-data` volume. This feature introduces a second named volume (`unimatrix-shared`) mounted at `/shared`, redirects model cache resolution to `/shared/models/` via the `UNIMATRIX_MODEL_CACHE` environment variable, and removes the model bake-in from the Docker build.

The change touches three layers: the Rust cache path resolution function (`EmbedConfig::resolve_cache_dir()`), the Docker build/runtime configuration (Dockerfile, docker-compose.yml), and CI workflows (release.yml). Non-container usage is unaffected — the env var is only set inside the Dockerfile.

## Component Breakdown

### 1. Cache Path Resolution (`unimatrix-embed`)

**File**: `crates/unimatrix-embed/src/config.rs`

**Responsibility**: Single source of truth for where model files are stored. All model I/O flows through `EmbedConfig::resolve_cache_dir()`.

**Change**: Insert a new env var check (`UNIMATRIX_MODEL_CACHE`) as the highest-priority fallback when `cache_dir` field is `None`. The precedence chain becomes:

```
1. EmbedConfig.cache_dir field (explicit, e.g. from config or test override)
2. UNIMATRIX_MODEL_CACHE env var (container redirect)
3. dirs::cache_dir() + "unimatrix/models" (platform default)
4. ".unimatrix/models" (last resort fallback)
```

### 2. Dockerfile (`Dockerfile`)

**Responsibility**: Build the container image without baked-in models; set up `/shared` directory with correct ownership; set `UNIMATRIX_MODEL_CACHE` env var.

**Changes**:
- Remove model bake-in steps (builder stage lines 96-101: `HOME=/data`, `model-download`, `model-download --nli`, `rm -rf /data/.cache/huggingface`)
- Remove model COPY step (runtime stage line 122: `COPY --from=builder /data/.cache/unimatrix/ ...`)
- Add `/shared/models` directory creation with ownership 65532:65532 and permissions 0700 in the builder stage
- Add `UNIMATRIX_MODEL_CACHE=/shared/models` to the runtime ENV block
- Change `VOLUME ["/data"]` to `VOLUME ["/data", "/shared"]`

### 3. Compose Configuration (`docker-compose.yml`)

**Responsibility**: Define named volumes and mount points for operator use.

**Changes**:
- Add `unimatrix-shared` named volume definition
- Mount at `/shared` alongside existing `unimatrix-data` at `/data`
- Update comments: backup guidance, `:ro` hardening note, volume separation explanation

### 4. CI Release Workflow (`.github/workflows/release.yml`)

**Responsibility**: Build and publish container images. Currently the Dockerfile handles model bake-in during `docker build`. After nan-015, the build no longer downloads models — the resulting image is ~166 MB smaller.

**Changes**: None to release.yml itself. The container build jobs (`build-container-x64`, `build-container-arm64`) run `docker build` against the Dockerfile, which will no longer contain model-download steps. The binary build jobs (`build-linux-x64`, `build-linux-arm64`) download models via `model-download` for test execution — this is unaffected (those are native builds, not container builds).

### 5. Documentation (`PRODUCT-VISION.md`, `WAVE2-ROADMAP.md`)

**Responsibility**: Keep product documentation consistent with shipped design.

**Changes**: Update W2-1 sections to describe the two-volume model. Correct the "ONNX baked into image (correct)" annotation.

## Component Interactions

```
                    ┌─────────────────────────┐
                    │      Dockerfile ENV      │
                    │ UNIMATRIX_MODEL_CACHE=   │
                    │   /shared/models         │
                    └────────────┬────────────┘
                                 │ (env var read at runtime)
                                 ▼
┌──────────────────────────────────────────────────────────┐
│              EmbedConfig::resolve_cache_dir()            │
│  1. self.cache_dir (Some → return)                       │
│  2. $UNIMATRIX_MODEL_CACHE (set → return PathBuf)        │
│  3. dirs::cache_dir() + "unimatrix/models"               │
│  4. ".unimatrix/models" fallback                         │
└────────────┬─────────────┬──────────────┬────────────────┘
             │             │              │
             ▼             ▼              ▼
     ┌──────────┐  ┌─────────────┐  ┌──────────────┐
     │ Embed    │  │ NLI startup │  │ model-download│
     │ startup  │  │ (NliConfig  │  │ CLI           │
     │ (OnnxPro-│  │  .cache_dir)│  │               │
     │ vider)   │  │             │  │               │
     └────┬─────┘  └──────┬──────┘  └───────┬───────┘
          │               │                 │
          ▼               ▼                 ▼
     ┌─────────────────────────────────────────┐
     │          /shared/models/                 │
     │  ├── sentence-transformers_.../          │
     │  │   ├── model.onnx                     │
     │  │   └── tokenizer.json                 │
     │  └── cross-encoder_.../                 │
     │      ├── model.onnx                     │
     │      └── tokenizer.json                 │
     └─────────────────────────────────────────┘
            unimatrix-shared volume
```

### Data Flow

1. **Container start**: Daemon starts, `EmbedServiceHandle::start_loading(EmbedConfig::default())` is called. `EmbedConfig::default()` has `cache_dir: None`, so `resolve_cache_dir()` checks `UNIMATRIX_MODEL_CACHE` env var, finds `/shared/models`, returns that path.
2. **OnnxProvider::new()** calls `resolve_cache_dir()` internally — same resolution, same path.
3. **NLI startup**: `EmbedConfig::default().resolve_cache_dir()` called at main.rs:677/1032, result stored in `NliConfig.cache_dir`. Same path.
4. **model-download CLI**: `EmbedConfig::default().resolve_cache_dir()` at main.rs:1431. Same path.
5. **ensure_model() / ensure_nli_model()**: Check file existence at the resolved path. If empty volume, download from HuggingFace. If populated, return immediately.
6. **Non-container**: `UNIMATRIX_MODEL_CACHE` is not set, falls through to `dirs::cache_dir()`. Behavior unchanged.

### Error Boundaries

- **Download failure**: `ensure_model()` / `ensure_nli_model()` return `EmbedError::Download`. The service handles (`EmbedServiceHandle`, `NliServiceHandle`) enter `Failed` state with exponential backoff retry (up to `MAX_RETRIES = 3`). Server starts; embedding/NLI return `NotReady` errors until loaded.
- **Permission error**: If `/shared/models` is not writable (wrong UID or `:ro` mount on empty volume), `fs::create_dir_all` fails with `io::Error`. Propagates as `EmbedError::Io`. Same retry behavior.
- **Corrupt file**: Non-zero-size but invalid ONNX file causes `Session::builder().commit_from_file()` to fail. NLI path has SHA-256 verification that catches this before load (lesson #4642). Embedding path lacks hash verification (#651, out of scope).

## Technology Decisions

| Decision | ADR | Rationale |
|----------|-----|-----------|
| Env var for cache redirect | ADR-001 | Explicit, self-documenting, no side effects on `dirs::` calls |
| Precedence chain ordering | ADR-002 | Explicit field > env var > platform default; prevents silent misdirection |
| Shared volume default `:rw` | ADR-003 | Required for zero-config first-run auto-download (Goal 4) |

## Integration Points

### Existing Components Affected

1. **`EmbedConfig::resolve_cache_dir()`** — Modified to add env var check
2. **Dockerfile** — Model bake-in removed, `/shared` directory added, env var set
3. **docker-compose.yml** — Second volume added
4. **PRODUCT-VISION.md** / **WAVE2-ROADMAP.md** — W2-1 text updated

### Components NOT Affected (verified)

1. **`ensure_model()` / `ensure_nli_model()`** — Accept `cache_dir: &Path`, agnostic to resolution source
2. **`OnnxProvider::new()`** — Calls `resolve_cache_dir()` internally, gets new path transparently
3. **`NliServiceHandle::spawn_load_task()`** — Receives `NliConfig.cache_dir`, uses it directly
4. **NLI SHA-256 verification** — Operates on file at resolved path, agnostic to volume
5. **HEALTHCHECK** — Tests daemon liveness and schema version, not model paths
6. **release.yml** — Container builds use Dockerfile (which changes); binary builds are unaffected
7. **eval harness** (`eval/profile/layer.rs:263`) — Uses `EmbedConfig::default().resolve_cache_dir()`, inherits env var in container, unaffected outside container

### Call Sites That Resolve Cache Dir (Complete List)

| Call Site | File:Line | How It Uses Resolution |
|-----------|-----------|----------------------|
| Embed startup (bridge) | `main.rs:612` | `EmbedConfig::default()` passed to `start_loading` → `OnnxProvider::new()` calls `resolve_cache_dir()` |
| NLI startup (bridge) | `main.rs:677` | `EmbedConfig::default().resolve_cache_dir()` → `NliConfig.cache_dir` |
| Embed startup (daemon) | `main.rs:968` | Same as bridge path |
| NLI startup (daemon) | `main.rs:1032` | Same as bridge path |
| model-download CLI | `main.rs:1431` | `EmbedConfig::default().resolve_cache_dir()` directly |
| Eval harness NLI | `eval/profile/layer.rs:263` | `EmbedConfig::default().resolve_cache_dir()` → `NliConfig.cache_dir` |
| Embed reconstruct | `embed_reconstruct.rs:43` | `EmbedConfig::default()` → `OnnxProvider::new()` internally |
| Test support | `test_support.rs:36` | `config.resolve_cache_dir()` (test only) |

All non-test sites use `EmbedConfig::default()` which has `cache_dir: None`, triggering the full resolution chain. The env var insertion at step 2 captures all of them without modifying any call site. (SR-06 resolved.)

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `EmbedConfig::resolve_cache_dir(&self) -> PathBuf` | Method on `EmbedConfig` | `crates/unimatrix-embed/src/config.rs:40` |
| `UNIMATRIX_MODEL_CACHE` | Environment variable, `Option<String>` | Read via `std::env::var()` inside `resolve_cache_dir()` |
| `ensure_model(model: EmbeddingModel, cache_dir: &Path) -> Result<PathBuf>` | Function | `crates/unimatrix-embed/src/download.rs:11` |
| `ensure_nli_model(model: NliModel, cache_dir: &Path) -> Result<PathBuf>` | Function | `crates/unimatrix-embed/src/download.rs:76` |
| `NliConfig.cache_dir: PathBuf` | Struct field | `crates/unimatrix-server/src/infra/nli_handle.rs:45` |
| `VOLUME ["/data", "/shared"]` | Docker directive | `Dockerfile` runtime stage |
| `unimatrix-shared` | Docker named volume | `docker-compose.yml` |

## Open Questions

1. **Eval harness in container**: The eval harness (`eval/profile/layer.rs:263`) calls `EmbedConfig::default().resolve_cache_dir()` which will resolve to `/shared/models/` inside a container. If eval is ever run inside a container (not currently the case), the eval container would need the shared volume mounted. Low risk — eval runs natively today.

2. **GGUF future path**: WAVE2-ROADMAP mentions GGUF models will share `unimatrix-shared`. The `/shared/models/` path leaves room for GGUF at `/shared/models/gguf/` or similar, but the GGUF subdirectory layout is not defined by nan-015.
