# Risk Coverage Report: nan-015

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Cache path precedence violation | `test_resolve_cache_dir_env_var_used_when_field_none`, `test_resolve_cache_dir_unset_env_falls_to_dirs`, `test_resolve_cache_dir_config_field_wins_over_env_var`, `test_resolve_cache_dir_fallback_path_construction` | PASS | Full |
| R-02 | Call site divergence | Static grep: all 5 non-test call sites use `resolve_cache_dir()`. No hardcoded paths. No independent env var reads. | PASS | Full |
| R-03 | Embedding model tampering on writable volume | docker-compose.yml comments: `:ro` hardening (line 16), `nli_model_sha256` guidance (line 45), #651 gap acknowledged (line 46) | PASS | Full (documentation) |
| R-04 | NLI hash verification broken by path redirect | Code inspection: `nli_handle.rs:282-283` confirms SHA-256 verification BEFORE `Session::builder()`. `NliConfig.cache_dir` populated from `resolve_cache_dir()`. Verify-then-load ordering preserved. | PASS | Full |
| R-05 | Incomplete model bake-in removal | Dockerfile grep: no `model-download`, no `COPY --from=builder /data/.cache/unimatrix`, no `COPY.*models` lines remain. | PASS | Full |
| R-06 | `/shared` directory ownership/permissions wrong | Dockerfile: `mkdir -p /data /shared/models` (line 99), `chown -R 65532:65532 /data /shared` (line 100), `chmod 0700 /data /shared` (line 101). Runtime COPY preserves ownership (lines 116, 119). | PASS | Full |
| R-07 | Empty env var produces relative path | `test_resolve_cache_dir_empty_env_var_falls_through` | PASS | Full |
| R-08 | Partial file corruption | Code inspection: `ensure_model()` uses `file_size() == 0` check (download.rs:57). NLI path has SHA-256 before ONNX load (nli_handle.rs:282). Retry state machine (Loading -> Failed -> Retrying) unchanged. | PASS | Full (inspection) |
| R-09 | docker-compose.yml volume misconfiguration | Static inspection: `unimatrix-data` defined (line 31), `unimatrix-shared` defined (line 37), mounted at `/data` (line 13) and `/shared` (line 14). Both are top-level named volumes. | PASS | Full |
| R-10 | CI smoke tests assume baked-in models | release.yml audit: container build jobs (`build-container-x64`, `build-container-arm64`) only build+push images, never run containers. Binary smoke test runs `unimatrix version` (no models needed). `model-download` runs natively (unaffected). | PASS | Full |
| R-11 | Read-only mount error clarity | Code inspection: `ensure_model()` `fs::create_dir_all` propagates `io::Error` with path context via `EmbedError::Io`. | PASS | Partial (no runtime `:ro` test -- Docker not available) |
| R-12 | Non-container env var bleed | Covered by R-01 scenario 2: `test_resolve_cache_dir_unset_env_falls_to_dirs` confirms unset env var preserves platform default. | PASS | Full |
| R-13 | Concurrent first-run download | Structural review: `unimatrix-shared` is a top-level named volume (docker-compose.yml line 37), shareable by multiple services. No runtime multi-container test (Docker not available). | PASS | Partial (structural only) |
| R-14 | HEALTHCHECK masking | Dockerfile line 131-132: `HEALTHCHECK` runs `unimatrix --project-dir /data health` -- tests daemon liveness, not model readiness. start-period=10s. | PASS | Full |
| R-15 | Documentation inconsistency | `grep -ic "baked into"` returns 0 for both PRODUCT-VISION.md and WAVE2-ROADMAP.md. `grep -c "unimatrix-shared"` returns 2 for PRODUCT-VISION.md, 1 for WAVE2-ROADMAP.md. Two-volume architecture described in both. | PASS | Full |

## Cross-Component Verification

| Check | Result | Detail |
|-------|--------|--------|
| Env var name consistency | PASS | `UNIMATRIX_MODEL_CACHE` identical in `config.rs:42` and `Dockerfile:125` |
| Dockerfile VOLUME matches compose mount | PASS | `VOLUME ["/data", "/shared"]` (Dockerfile:128) matches compose mounts at `/data` and `/shared` |
| Env var value is absolute path | PASS | `UNIMATRIX_MODEL_CACHE=/shared/models` (absolute) |

## Test Results

### Unit Tests
- Total: 4,618
- Passed: 4,618
- Failed: 0
- Ignored: 28

#### nan-015-specific unit tests (unimatrix-embed config.rs)
- `test_resolve_cache_dir_env_var_used_when_field_none` -- PASS (R-01 scenario 1)
- `test_resolve_cache_dir_unset_env_falls_to_dirs` -- PASS (R-01 scenario 2, R-12)
- `test_resolve_cache_dir_config_field_wins_over_env_var` -- PASS (R-01 scenario 3)
- `test_resolve_cache_dir_fallback_path_construction` -- PASS (R-01 scenario 4)
- `test_resolve_cache_dir_empty_env_var_falls_through` -- PASS (R-07)

### Integration Tests (infra-001 smoke suite)
- Total: 23
- Passed: 23
- Failed: 0
- Deselected: 343

Smoke suite results (all PASS):
- `test_cold_start_search_equivalence` (adaptation)
- `test_base_score_active` (confidence)
- `test_contradiction_detected` (contradiction)
- `test_unicode_cjk_roundtrip` (edge_cases)
- `test_empty_database_operations` (edge_cases)
- `test_restart_persistence` (edge_cases)
- `test_server_process_cleanup` (edge_cases)
- `test_store_search_find_flow` (lifecycle)
- `test_correction_chain_integrity` (lifecycle)
- `test_isolation_no_state_leakage` (lifecycle)
- `test_concurrent_search_stability` (lifecycle)
- `test_cycle_start_goal_does_not_block_response` (lifecycle)
- `test_initialize_returns_capabilities` (protocol)
- `test_server_info` (protocol)
- `test_graceful_shutdown` (protocol)
- `test_injection_patterns_detected` (security)
- `test_store_minimal` (tools)
- `test_store_roundtrip` (tools)
- `test_search_returns_results` (tools)
- `test_status_empty_db` (tools)
- `test_get_with_string_id` (tools)
- `test_deprecate_with_string_id` (tools)
- `test_store_1000_entries` (volume)

No xfail markers added. No GH Issues filed. No pre-existing failures encountered.

## Gaps

| Risk | Gap | Reason |
|------|-----|--------|
| R-11 | No runtime `:ro` mount test | Docker build/run not available in this environment. Error propagation path verified via code inspection. |
| R-13 | No multi-container runtime test | Docker compose multi-service test not available. Structural compatibility (top-level named volume) confirmed. |
| AC-01 | Image size reduction not measured | Docker build not available. Dockerfile verified to contain no model files. |
| AC-03-AC-09 | Container runtime ACs not tested | Docker build/run not available. All preconditions verified via static analysis (Dockerfile structure, compose config, env var wiring). |

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PARTIAL | Dockerfile contains no model-download or COPY-model lines. Image size measurement requires Docker build (not available). |
| AC-02 | PASS | docker-compose.yml defines both `unimatrix-data` (line 31) and `unimatrix-shared` (line 37) named volumes; `unimatrix-shared` mounted at `/shared` (line 14). |
| AC-03 | PARTIAL | Dockerfile creates `/shared/models` (line 99), sets ownership 65532:65532 (line 100), sets 0700 (line 101), sets `UNIMATRIX_MODEL_CACHE=/shared/models` (line 125). Runtime test requires Docker. |
| AC-04 | PARTIAL | Code path verified: `resolve_cache_dir()` returns `/shared/models` when env var set. Runtime re-start test requires Docker. |
| AC-05 | PARTIAL | `model-download` CLI call site (main.rs:1431) uses `resolve_cache_dir()` -- will write to `/shared/models` when env var set. Runtime test requires Docker. |
| AC-06 | PARTIAL | `unimatrix-shared` is a top-level named volume, shareable. Runtime multi-container test requires Docker. |
| AC-07 | PARTIAL | NLI SHA-256 verification (nli_handle.rs:282-310) preserved through new cache path. Unit test `test_hash_mismatch_transitions_to_failed` exists. Runtime test with shared volume requires Docker. |
| AC-08 | PARTIAL | Air-gap path verified: when `unimatrix-shared` is pre-populated, no download needed (models found at `/shared/models/`). Runtime `--network none` test requires Docker. |
| AC-09 | PARTIAL | HEALTHCHECK unchanged (Dockerfile:131-132). Runtime health check test requires Docker. |
| AC-10 | PASS | `grep -ic "baked into"` returns 0 for both docs. `grep -c "unimatrix-shared"` returns non-zero for both PRODUCT-VISION.md (2) and WAVE2-ROADMAP.md (1). Two-volume architecture described. |
| AC-11 | PASS | docker-compose.yml contains: `:ro` hardening comment (line 16, 44), `nli_model_sha256` guidance (line 45), #651 gap acknowledgment (line 46). |
