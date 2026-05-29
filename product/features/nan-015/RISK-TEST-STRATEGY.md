# Risk-Based Test Strategy: nan-015

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Cache path precedence violation: env var, config field, dirs fallback, or last-resort return incorrect path, causing models to land on wrong volume | High | Med | Critical |
| R-02 | Call site divergence: one or more of the 7 non-test call sites constructs a cache path independently of `resolve_cache_dir()`, splitting models across volumes | High | Low | High |
| R-03 | Embedding model tampered on writable shared volume; no SHA-256 verification until #651 ships — ONNX session loads compromised computation graph | High | Low | High |
| R-04 | NLI hash verification breaks after path redirect — verify-then-load ordering (lesson #4642) violated through new cache path | High | Low | High |
| R-05 | Dockerfile model bake-in removal incomplete — residual COPY or model-download step leaves stale models in image layers | Med | Med | High |
| R-06 | `/shared` directory ownership/permissions wrong in built image — UID 65532 cannot write, first-run download fails with PermissionDenied | High | Med | Critical |
| R-07 | Empty env var (`UNIMATRIX_MODEL_CACHE=""`) treated as set, producing a relative path at filesystem root instead of falling through to dirs | Med | Low | Med |
| R-08 | Partial file from interrupted download passes non-zero-size existence check but fails ONNX session load; retry state machine does not re-download | Med | Low | Med |
| R-09 | docker-compose.yml missing `unimatrix-shared` volume definition or mount, container starts without shared volume | Med | Low | Med |
| R-10 | CI smoke tests assume baked-in models — container health check times out because model download was not expected | Med | Med | High |
| R-11 | Read-only mount on empty volume: `fs::create_dir_all` fails but error message does not indicate the root cause is `:ro` mount | Low | Low | Low |
| R-12 | Non-container regression: `UNIMATRIX_MODEL_CACHE` accidentally bleeds into native dev environment, redirecting models to nonexistent path | Low | Low | Low |
| R-13 | Concurrent first-run: two containers with same empty shared volume both download simultaneously, one overwrites the other's partial file mid-write | Med | Low | Med |
| R-14 | HEALTHCHECK regression: health check passes before models are loaded (checking liveness not readiness), masking a broken model path | Med | Low | Med |
| R-15 | Documentation inconsistency: PRODUCT-VISION.md or WAVE2-ROADMAP.md still says "ONNX baked into image" after nan-015 ships | Low | Med | Med |

## Risk-to-Scenario Mapping

### R-01: Cache Path Precedence Violation
**Severity**: High
**Likelihood**: Med
**Impact**: Models written to `/data/.cache/` instead of `/shared/models/`, defeating volume separation. Backup bloat returns silently. Multi-container sharing breaks.

**Test Scenarios**:
1. Unit test: `UNIMATRIX_MODEL_CACHE=/tmp/test-cache` set, `cache_dir: None` — `resolve_cache_dir()` returns `/tmp/test-cache`
2. Unit test: `UNIMATRIX_MODEL_CACHE` unset, `cache_dir: None` — returns `dirs::cache_dir()` + `unimatrix/models`
3. Unit test: `UNIMATRIX_MODEL_CACHE=/tmp/test-cache` set, `cache_dir: Some("/explicit")` — returns `/explicit` (field wins over env var)
4. Unit test: `UNIMATRIX_MODEL_CACHE` unset, `dirs::cache_dir()` returns `None` — returns `.unimatrix/models` fallback

**Coverage Requirement**: All four precedence levels tested in isolation with every combination of set/unset inputs. ADR-002 test matrix must be implemented verbatim.

### R-02: Call Site Divergence
**Severity**: High
**Likelihood**: Low
**Impact**: Some model operations write to shared volume, others to data volume. Inconsistent model state across startup paths.

**Test Scenarios**:
1. Code inspection: grep all call sites from Architecture table (8 sites including test_support). Verify each uses `EmbedConfig::default().resolve_cache_dir()` or passes through `resolve_cache_dir()`.
2. Integration test: set `UNIMATRIX_MODEL_CACHE`, run `model-download` CLI, verify models appear at env var path (not `/data/.cache/`).

**Coverage Requirement**: Static verification that no call site constructs a path outside `resolve_cache_dir()`. The integration test for `model-download` CLI validates the most independent call site.

### R-03: Embedding Model Tampering on Writable Volume
**Severity**: High
**Likelihood**: Low
**Impact**: Compromised embedding model loaded into ONNX Runtime. Potential for arbitrary computation, data exfiltration via crafted embeddings. No hash check catches it until #651.

**Test Scenarios**:
1. Verify documentation (AC-11) explicitly states embedding model hash enforcement is not yet available and references #651.
2. Verify `:ro` hardening guidance is present in docker-compose.yml comments.
3. Verify that `nli_model_sha256` config is documented as recommended for shared volume deployments.

**Coverage Requirement**: Documentation review confirms the gap is acknowledged, not hidden. No runtime test can validate this until #651 ships.

### R-04: NLI Hash Verification Broken by Path Redirect
**Severity**: High
**Likelihood**: Low
**Impact**: NLI model tampering goes undetected. Lesson #4642 (verify-then-load ordering) violated — tampered model loaded before hash checked.

**Test Scenarios**:
1. Integration test: set `nli_model_sha256` to correct hash, start daemon with `UNIMATRIX_MODEL_CACHE` set — NLI reaches Ready state.
2. Integration test: set `nli_model_sha256` to incorrect hash — NLI degrades gracefully to cosine fallback, does NOT load the model.
3. Code inspection: verify `spawn_load_task()` still calls hash verification before `Session::builder().commit_from_file()` through the new path.

**Coverage Requirement**: Both correct-hash-passes and wrong-hash-rejects scenarios tested with the redirected cache path. Verify-then-load ordering confirmed by code inspection.

### R-05: Incomplete Model Bake-In Removal
**Severity**: Med
**Likelihood**: Med
**Impact**: Image still contains ~166 MB of model files. Image size reduction AC-01 fails. Models may load from baked-in path instead of shared volume.

**Test Scenarios**:
1. Build the Docker image, inspect layers — no model files exist at `/data/.cache/unimatrix/models/` or any other path in the image.
2. Compare image size before/after: at least 150 MB reduction (AC-01).
3. Verify no `model-download` or `COPY --from=builder /data/.cache/unimatrix` lines remain in the Dockerfile.

**Coverage Requirement**: Image size measurement plus filesystem inspection of built image.

### R-06: `/shared` Directory Permission Failure
**Severity**: High
**Likelihood**: Med
**Impact**: First-run download fails immediately with `io::Error::PermissionDenied`. Daemon enters degraded state. Zero-config startup (Goal 4) broken.

**Test Scenarios**:
1. Build image, inspect `/shared` — ownership is `65532:65532`, permissions are `0700`.
2. Start container with empty shared volume — `ensure_model()` successfully creates subdirectories and writes model files (AC-03).
3. Verify the `mkdir` / `chown` / `chmod` directives exist in the Dockerfile builder stage.

**Coverage Requirement**: Both static (image inspection) and dynamic (container start) verification.

### R-07: Empty String Env Var
**Severity**: Med
**Likelihood**: Low
**Impact**: `PathBuf::from("")` creates a relative path, models written to container's CWD or root. Silent misbehavior.

**Test Scenarios**:
1. Unit test: `UNIMATRIX_MODEL_CACHE=""` — `resolve_cache_dir()` falls through to `dirs::cache_dir()`, does NOT return empty PathBuf.

**Coverage Requirement**: Explicit empty-string test case per ADR-002.

### R-08: Partial File Corruption
**Severity**: Med
**Likelihood**: Low
**Impact**: Non-zero-size corrupt file passes existence check. ONNX session load fails. If retry does not re-trigger download, service stays in Failed state permanently.

**Test Scenarios**:
1. Place a corrupt (non-zero, non-ONNX) file at the model path. Start daemon. Verify ONNX load fails, retry state machine activates, and service eventually enters Failed state (not stuck in Loading).
2. Verify NLI path: corrupt file fails SHA-256 check before ONNX load attempt.

**Coverage Requirement**: At least one test with a corrupt model file verifying the failure-then-retry path.

### R-09: docker-compose.yml Volume Misconfiguration
**Severity**: Med
**Likelihood**: Low
**Impact**: Container starts without `/shared` mount. `UNIMATRIX_MODEL_CACHE=/shared/models` points to an empty directory in the container's writable layer. Models download to ephemeral storage, lost on restart.

**Test Scenarios**:
1. `docker compose config` shows both `unimatrix-data` and `unimatrix-shared` volume definitions (AC-02).
2. Verify `unimatrix-shared` is mounted at `/shared` in the service definition.

**Coverage Requirement**: Compose config validation.

### R-10: CI Smoke Tests Assume Baked-In Models
**Severity**: Med
**Likelihood**: Med
**Impact**: CI container health check fails because model download takes longer than the health check timeout. Release pipeline breaks.

**Test Scenarios**:
1. Audit `release.yml` for any step that runs the container image and expects immediate model availability.
2. If smoke tests exist, verify they either pre-populate the shared volume or allow sufficient time for download.

**Coverage Requirement**: CI pipeline review. Any container-based test must account for download latency.

### R-11: Read-Only Mount Error Clarity
**Severity**: Low
**Likelihood**: Low
**Impact**: Operator mounts `:ro` on empty volume, gets opaque `PermissionDenied` error. Debugging requires understanding the volume/mount interaction.

**Test Scenarios**:
1. Start container with empty volume mounted `:ro`. Verify error logs include actionable information (e.g., the path that failed and the operation attempted).

**Coverage Requirement**: Log message review in the `:ro` empty-volume failure path.

### R-12: Non-Container Env Var Bleed
**Severity**: Low
**Likelihood**: Low
**Impact**: Developer accidentally exports `UNIMATRIX_MODEL_CACHE` in their shell. Models go to unexpected path. Confusing "models not found" behavior.

**Test Scenarios**:
1. Unit test: `UNIMATRIX_MODEL_CACHE` unset — `resolve_cache_dir()` returns platform default (existing behavior preserved, NFR-02).

**Coverage Requirement**: Covered by R-01 scenario 2.

### R-13: Concurrent First-Run Download
**Severity**: Med
**Likelihood**: Low
**Impact**: Two containers write the same model file simultaneously. One may read a partially-written file from the other. ONNX session load fails.

**Test Scenarios**:
1. Start two containers with the same empty `unimatrix-shared` volume. Both eventually reach healthy state (AC-06).

**Coverage Requirement**: Multi-container test. Acceptable if both eventually succeed even if one retries.

### R-14: HEALTHCHECK Masking
**Severity**: Med
**Likelihood**: Low
**Impact**: Health check reports "healthy" before models finish loading. Operator believes models are ready when they are not.

**Test Scenarios**:
1. Verify HEALTHCHECK tests daemon liveness (HTTP endpoint reachable) and schema version, not model readiness. Document that health=healthy means "daemon running," not "models loaded."
2. On first start with empty volume, health check eventually passes after model download completes (AC-09).

**Coverage Requirement**: Health check semantics documented. Integration test confirms health check passes after full startup.

### R-15: Documentation Inconsistency
**Severity**: Low
**Likelihood**: Med
**Impact**: Operators reading PRODUCT-VISION.md or WAVE2-ROADMAP.md get incorrect information about volume architecture.

**Test Scenarios**:
1. Grep PRODUCT-VISION.md and WAVE2-ROADMAP.md for "baked into" — no remaining references to baked-in models (AC-10).
2. Both files describe two-volume model (`unimatrix-data` + `unimatrix-shared`).

**Coverage Requirement**: Documentation review.

## Integration Risks

- **resolve_cache_dir() <-> UNIMATRIX_MODEL_CACHE**: The env var is the sole integration point between the Dockerfile and the Rust binary. If the env var name is misspelled in either location, models silently fall back to `/data/.cache/` (R-01). The env var string must be identical in both the Dockerfile `ENV` directive and the `std::env::var()` call.
- **Dockerfile <-> docker-compose.yml**: The VOLUME directive in the Dockerfile declares `/shared` as a mount point. The docker-compose.yml must mount `unimatrix-shared` at `/shared`. Mismatch means Docker creates an anonymous volume at `/shared` from the Dockerfile directive, ignoring the named volume.
- **NliConfig.cache_dir <-> resolve_cache_dir()**: NLI startup calls `resolve_cache_dir()` and stores the result in `NliConfig.cache_dir`. This value propagates to `resolve_model_dir()` inside `spawn_load_task()`. If the path is correct at resolution time but the volume is unmounted later (unlikely in Docker), NLI hash verification runs against a stale or missing file.
- **model-download CLI <-> daemon startup**: Both must resolve to the same path. If the CLI is run via `docker exec` while the daemon is already running, both access the same volume concurrently. The CLI writes models; the daemon reads them. Since ONNX sessions open files read-only after initial load, no conflict occurs. But if the daemon is retrying a failed download while the CLI is also downloading, both may write simultaneously (R-13).

## Edge Cases

- **`UNIMATRIX_MODEL_CACHE` set to a path with spaces or special characters**: `PathBuf::from()` handles this correctly on Linux, but paths with newlines or null bytes would cause undefined behavior. Low risk in container context where the Dockerfile controls the value.
- **`UNIMATRIX_MODEL_CACHE` set to a relative path** (e.g., `models/`): Resolves relative to the daemon's CWD (`/data` in the container). Models would land on the data volume, defeating separation. The architecture does not guard against this — the Dockerfile sets an absolute path.
- **Shared volume on NFS or cloud storage driver**: File ownership inheritance from the image layer (65532:65532) is guaranteed only for the local Docker storage driver. NFS mounts may require `uid`/`gid` mount options. The spec acknowledges this as a known limitation (C-10).
- **Disk full on shared volume**: `ensure_model()` download fails partway. Non-zero-size partial file exists. Next startup attempts ONNX load, fails, retries (R-08). Eventually enters Failed state with no automatic cleanup.
- **Model file deleted from shared volume while daemon is running**: ONNX session holds an open file descriptor. On Linux, the file remains readable (unlinked but open). On next restart, `ensure_model()` re-downloads. No runtime impact.
- **Extremely long first-run download** (slow network): No timeout in `hf-hub` download. The daemon stays in `Loading` state indefinitely. Health check continues to report the daemon as reachable but ML endpoints return `NotReady`.

## Security Risks

- **Untrusted input: ONNX model files on shared volume**: The shared volume is writable by default (ADR-003). Any process with access to the Docker socket or the volume can replace model files. ONNX model files can contain custom operators that execute arbitrary code during session construction (lesson #4642). NLI has SHA-256 verification; embedding model does not until #651. **Blast radius**: Compromised embedding model affects all embedding computations silently — vector search results become unreliable or exfiltrate data patterns. Compromised NLI model is caught by hash verification if `nli_model_sha256` is configured.
- **Untrusted input: `UNIMATRIX_MODEL_CACHE` env var**: If an attacker can inject environment variables into the container (e.g., via Kubernetes pod spec), they can redirect model storage to a path they control. Mitigation: the env var is set in the Dockerfile, not externally configurable by default.
- **Path traversal**: `UNIMATRIX_MODEL_CACHE` is used directly as a `PathBuf`. A value like `../../etc/` would be valid. In the container context, the Dockerfile controls this value, so exploitation requires modifying the Dockerfile or runtime env injection.
- **Volume mount substitution**: An attacker who can modify docker-compose.yml can mount a malicious volume at `/shared` containing tampered models. This is equivalent to Docker socket access — already a full-compromise scenario.

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|-------------------|----------|
| Shared volume not mounted | Models download to container's ephemeral `/shared` (created by Dockerfile). Lost on restart. Functional but not persistent. | Mount the named volume and restart. |
| Download fails (network) | `EmbedServiceHandle` / `NliServiceHandle` enter Failed state, retry 3x with exponential backoff. Server starts; ML endpoints return `NotReady`. | Restore network, restart container, or pre-populate volume. |
| `/shared` not writable (`:ro` on empty volume) | `fs::create_dir_all` fails with `PermissionDenied`. Propagated as `EmbedError::Io`. Retry fails identically. Degraded state. | Remount as `:rw`, run first startup, then optionally switch back to `:ro`. |
| Corrupt model file on volume | ONNX session construction fails. Service enters Failed, retries. On NLI path, SHA-256 check rejects file before load. Embedding path loads then fails at session build. | Delete corrupt file, restart container (triggers re-download). |
| Hash mismatch (NLI) | `spawn_load_task()` detects mismatch before loading. NLI degrades to cosine fallback. Warning logged. | Replace model file with correct version, or update `nli_model_sha256` config. |
| Env var misspelled in Dockerfile | Falls through to `dirs::cache_dir()` -> `/data/.cache/unimatrix/models/`. Models on data volume. Silent regression to pre-nan-015 behavior. | Fix env var name in Dockerfile. Unit tests for `resolve_cache_dir()` catch this if they test with the env var set. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (cache path precedence — High/Med) | R-01 | Resolved by ADR-002: four-level precedence with explicit ordering. Empty-string guard added. Unit test matrix specified. |
| SR-02 (supply-chain attack surface — High/Low) | R-03, R-04 | Partially mitigated: ADR-003 defaults `:rw` for usability, documents `:ro` hardening. NLI hash verification preserved (R-04). Embedding hash gap remains until #651 (R-03). |
| SR-03 (first-run network dependency — Med/Med) | R-10 | Architecture preserves existing retry/backoff (3 retries, exponential). No new timeout added. CI must account for download latency (R-10). |
| SR-04 (embedding hash gap in docs — Med/High) | R-03 | Resolved by FR-13 / AC-11: documentation explicitly acknowledges #651 gap. Operators not misled. |
| SR-05 (CI model-presence assumptions — Med/Med) | R-10 | Architecture confirms release.yml container builds are unaffected (no model-download in new Dockerfile). Smoke tests that run the container must be audited (R-10). |
| SR-06 (call site divergence — High/Med) | R-02 | Resolved structurally: ADR-001 env var is checked inside `resolve_cache_dir()`. All 7 non-test call sites use `EmbedConfig::default()` which has `cache_dir: None`, triggering the env var check. No call-site changes needed. |
| SR-07 (partial file corruption — Med/Low) | R-08 | Accepted: existing retry state machine handles ONNX load failure on corrupt files. Atomic write deferred (out of scope). NLI path has hash check as early detection. |
| SR-08 (non-container env var bleed — Low/Low) | R-12 | Mitigated by `UNIMATRIX_` prefix convention and documentation. Unit test confirms unset env var preserves platform default. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-06) | 6 scenarios |
| High | 4 (R-02, R-03, R-04, R-05, R-10) | 11 scenarios |
| Medium | 5 (R-07, R-08, R-09, R-13, R-14, R-15) | 7 scenarios |
| Low | 2 (R-11, R-12) | 2 scenarios |
