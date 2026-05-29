# dockerfile -- Pseudocode

## Purpose

Remove ONNX model bake-in from the Docker image (~166 MB reduction), create the `/shared/models` directory structure with correct ownership for the nonroot user, set `UNIMATRIX_MODEL_CACHE` env var to redirect model resolution, and declare `/shared` as a volume mount point alongside `/data`.

## File

`Dockerfile`

## Changes

### 1. Remove Model Bake-In from Builder Stage (lines 95-101)

**Remove** the entire model bake-in block:

```dockerfile
# REMOVE these lines:
# --- Model bake-in ---
# HOME=/data so model-download writes to /data/.cache/unimatrix/models/.
ENV HOME=/data
RUN mkdir -p /data && \
    LD_LIBRARY_PATH=/usr/local/lib target/release/unimatrix model-download && \
    LD_LIBRARY_PATH=/usr/local/lib target/release/unimatrix model-download --nli && \
    rm -rf /data/.cache/huggingface
```

**Replace** with `/data` and `/shared` directory setup:

```dockerfile
# --- Directory setup ---
# /data: integrity-critical persistent storage (databases, config, logs).
# /shared: re-downloadable assets (ONNX models). Separate volume for backup separation.
# UID 65532 = nonroot user in distroless/cc-debian12:nonroot.
RUN mkdir -p /data /shared/models && \
    chown -R 65532:65532 /data /shared && \
    chmod 0700 /data /shared
```

This consolidates the existing `/data` setup (lines 103-106) into the same RUN layer. Remove the now-redundant separate `/data` chown/chmod block.

### 2. Remove Model COPY from Runtime Stage (line 122)

**Remove**:

```dockerfile
# REMOVE these lines:
# Baked-in models (embedding + NLI). Only copy the unimatrix model cache,
# not the huggingface hub cache (which has duplicate blobs and symlinks).
COPY --from=builder --chown=65532:65532 /data/.cache/unimatrix/ /data/.cache/unimatrix/
```

No replacement needed. Models will be downloaded at runtime to the shared volume.

### 3. Update Runtime Stage Comment (line 2)

**Change** the stage 2 header comment from:

```
#   2. builder  — compile, install ORT (SHA-256 verified), bake models
```

To:

```
#   2. builder  — compile, install ORT (SHA-256 verified)
```

### 4. Copy `/shared` Directory to Runtime Stage

**Add** after the `/data` COPY (line 124):

```dockerfile
# /shared directory with correct ownership and permissions.
COPY --from=builder --chown=65532:65532 /shared /shared
```

This ensures the `/shared/models` directory exists in the runtime image with correct 65532:65532 ownership, even before any volume is mounted.

### 5. Add UNIMATRIX_MODEL_CACHE to Runtime ENV Block (line 128)

**Change** the ENV block from:

```dockerfile
ENV HOME=/data \
    LD_LIBRARY_PATH=/usr/local/lib \
    UNIMATRIX_LOG=info
```

To:

```dockerfile
ENV HOME=/data \
    LD_LIBRARY_PATH=/usr/local/lib \
    UNIMATRIX_LOG=info \
    UNIMATRIX_MODEL_CACHE=/shared/models
```

The env var name must match exactly: `UNIMATRIX_MODEL_CACHE`. This is the string that `resolve_cache_dir()` reads via `std::env::var()`.

### 6. Update VOLUME Directive (line 133)

**Change**:

```dockerfile
VOLUME ["/data"]
```

To:

```dockerfile
VOLUME ["/data", "/shared"]
```

### 7. Update Run Comment (line 9)

**Change**:

```
# Run:    docker run -v unimatrix-data:/data unimatrix
```

To:

```
# Run:    docker run -v unimatrix-data:/data -v unimatrix-shared:/shared unimatrix
```

## Error Handling

No runtime error handling in Dockerfile changes. Build-time errors:
- If `mkdir -p /shared/models` fails, `docker build` fails with a clear error.
- If `chown` fails (e.g., UID 65532 does not exist in builder), build fails. This cannot happen because the builder is `rust:slim-bookworm` which has standard UID support.

Runtime permission errors when the container starts are handled by `ensure_model()` / `ensure_nli_model()` in the Rust code -- they return `EmbedError::Io` which triggers the retry state machine.

## Key Test Scenarios

### T-01: Image builds successfully without model download (R-05)

```
docker build -t unimatrix-test .
# Build succeeds. No model-download steps in build output.
# Build time reduced by model download + bake-in time.
```

### T-02: Image size reduced by at least 150 MB (AC-01)

```
# Compare docker images output for old vs new image.
# New image uncompressed size should be >= 150 MB smaller.
```

### T-03: No model files in built image (R-05)

```
# Inspect image filesystem:
docker run --rm --entrypoint="" unimatrix-test ls /data/.cache/
# Should error or show empty -- no .cache/unimatrix/ directory with models.
# Note: distroless has no ls. Use crane or docker export + tar inspection.
```

### T-04: /shared directory exists with correct ownership (R-06, AC-09)

```
# Inspect image:
# /shared exists, owned by 65532:65532, permissions 0700
# /shared/models exists, owned by 65532:65532
```

### T-05: UNIMATRIX_MODEL_CACHE env var set correctly

```
docker inspect unimatrix-test --format='{{.Config.Env}}'
# Must contain UNIMATRIX_MODEL_CACHE=/shared/models
```

### T-06: VOLUME directive includes both mount points

```
docker inspect unimatrix-test --format='{{.Config.Volumes}}'
# Must show both /data and /shared
```

### T-07: HEALTHCHECK unchanged (R-14)

```
docker inspect unimatrix-test --format='{{.Config.Healthcheck}}'
# Must match existing: unimatrix --project-dir /data health
```

### T-08: No model-download or COPY model lines remain in Dockerfile (R-05)

```
# Static analysis: grep Dockerfile for:
#   - "model-download" (should not appear in RUN)
#   - "COPY.*\.cache/unimatrix" (should not appear)
#   - "/data/.cache/huggingface" (should not appear)
```
