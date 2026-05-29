# Test Plan: dockerfile

**File**: `Dockerfile`

## Model Bake-In Removal (R-05)

### Test 1: No model-download commands in Dockerfile

```
Name: verify_no_model_download_in_dockerfile
Method: Grep Dockerfile content.
Assert:
  1. No line containing "model-download" exists (builder stage lines 96-101 removed).
  2. No line containing "rm -rf /data/.cache/huggingface" exists.
  3. No line containing "COPY --from=builder /data/.cache/unimatrix" exists (runtime stage line 122 removed).
  4. No line containing "HOME=/data" exists in the builder stage ENV block (only in runtime stage).
Risk: R-05 scenario 3
```

### Test 2: No model files in built image (AC-01)

```
Name: verify_no_model_files_in_image
Method: docker build + image inspection.
Assert:
  1. docker build succeeds without model download steps.
  2. No model files at /data/.cache/unimatrix/models/ in the image.
  3. Image size is at least 150 MB smaller than pre-nan-015 baseline.
Risk: R-05 scenarios 1-2
Note: Requires Docker build -- Stage 3c only.
```

## Shared Volume Configuration (R-06, Critical)

### Test 3: /shared directory exists with correct ownership

```
Name: verify_shared_directory_ownership
Method: Inspect Dockerfile content + built image.
Assert (Dockerfile content):
  1. A RUN directive creates /shared/models directory.
  2. Ownership is set to 65532:65532 (chown).
  3. Permissions are set to 0700 (chmod).
Assert (built image -- Stage 3c):
  4. docker run --entrypoint="" <image> stat /shared shows uid=65532, gid=65532, mode=0700.
Risk: R-06 scenarios 1, 3
```

### Test 4: VOLUME directive declares both mount points

```
Name: verify_volume_directive
Method: Grep Dockerfile.
Assert: VOLUME directive is `VOLUME ["/data", "/shared"]` (both paths declared).
Risk: R-06 -- ensures Docker auto-creates the mount point.
```

### Test 5: UNIMATRIX_MODEL_CACHE set in runtime ENV

```
Name: verify_env_var_in_runtime_stage
Method: Grep Dockerfile runtime stage.
Assert:
  1. ENV block includes UNIMATRIX_MODEL_CACHE=/shared/models.
  2. The env var is in the runtime stage (Stage 3), not only the builder stage.
  3. The value is an absolute path (/shared/models), not relative.
Risk: R-01 (integration) -- env var must be present for resolve_cache_dir() to redirect.
```

## CI Smoke Test Audit (R-10, High)

### Test 6: release.yml does not assume baked-in models

```
Name: audit_release_yml_model_assumptions
Method: Review .github/workflows/release.yml content.
Assert:
  1. Container build steps (build-container-x64, build-container-arm64) do not assume models exist in the image.
  2. If any step runs the container and checks health, it allows time for model download OR pre-populates the shared volume.
  3. Binary build steps (build-linux-x64, build-linux-arm64) are unaffected -- they download models natively.
Risk: R-10
```

## HEALTHCHECK Semantics (R-14, Med)

### Test 7: HEALTHCHECK tests liveness not model readiness

```
Name: verify_healthcheck_semantics
Method: Inspect HEALTHCHECK directive in Dockerfile.
Assert:
  1. HEALTHCHECK command is `unimatrix --project-dir /data health`.
  2. The health command tests daemon liveness (HTTP reachable, schema version), not model loaded status.
  3. start-period is sufficient for daemon startup (10s current value).
  4. Document: health=healthy means "daemon running" not "models loaded."
Risk: R-14
```

## Read-Only Mount Error Clarity (R-11, Low)

### Test 8: Error path for :ro empty volume

```
Name: verify_ro_empty_volume_error_path
Method: Code inspection of ensure_model() error propagation.
Assert:
  1. When fs::create_dir_all fails with PermissionDenied, the error includes the path that failed.
  2. The error propagates as EmbedError::Io (or Download) with context.
Risk: R-11
Note: Runtime verification requires Docker with :ro mount -- deferred to Stage 3c if Docker is available.
```

## Dockerfile Structure Validation Summary

| Check | Method | Blocks Gate? |
|-------|--------|-------------|
| No model-download lines | grep | Yes |
| No COPY model lines | grep | Yes |
| /shared created with 65532:65532 | grep + image inspect | Yes |
| VOLUME includes /shared | grep | Yes |
| ENV includes UNIMATRIX_MODEL_CACHE | grep | Yes |
| Image size reduction >= 150 MB | docker images | Yes (AC-01) |
| release.yml audit | file review | Yes (R-10) |
| HEALTHCHECK unchanged | grep | No (informational) |
