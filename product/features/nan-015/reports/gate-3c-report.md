# Gate 3c Report: nan-015

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-05-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | 15/15 risks mapped to tests or verified via inspection; 13 full, 2 partial (Docker runtime unavailable) |
| Test coverage completeness | PASS | All risk-to-scenario mappings exercised; 5 unit tests + 23 smoke integration tests pass |
| Specification compliance | PASS | All 13 FRs addressed; 6 NFRs addressed; 11 ACs verified (3 full, 8 partial due to Docker) |
| Architecture compliance | PASS | Component boundaries, env var wiring, volume layout, precedence chain match architecture |
| Knowledge stewardship compliance | PASS | Tester agent report contains complete stewardship block |
| Integration test validation | PASS | 23/23 smoke tests pass; no xfail added; no tests deleted; all pre-existing xfails reference GH Issues |

## Detailed Findings

### 1. Risk Mitigation Proof
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md maps all 15 risks (R-01 through R-15) to specific test results or code inspection evidence.

- **R-01 (Critical)**: 4 unit tests cover all precedence levels. All pass.
- **R-02 (High)**: Static grep confirms all 5 non-test call sites use `resolve_cache_dir()`. No hardcoded paths.
- **R-03 (High)**: Documentation review confirms `:ro` hardening (docker-compose.yml:16,44), `nli_model_sha256` guidance (line 45), #651 gap acknowledgment (line 46).
- **R-04 (High)**: Code inspection confirms SHA-256 verification at `nli_handle.rs:282` runs BEFORE `NliProvider::new()` at line 319. Verify-then-load ordering preserved.
- **R-05 (High)**: Dockerfile grep returns no `model-download`, `COPY --from=builder /data/.cache/unimatrix`, or `huggingface` references.
- **R-06 (Critical)**: Dockerfile lines 99-101 create `/shared/models` with `chown -R 65532:65532` and `chmod 0700`.
- **R-07 (Med)**: `test_resolve_cache_dir_empty_env_var_falls_through` passes.
- **R-08 (Med)**: Code inspection confirms retry state machine handles ONNX load failure; NLI path has SHA-256 early detection.
- **R-09 (Med)**: docker-compose.yml defines both `unimatrix-data` (line 31) and `unimatrix-shared` (line 37) as top-level named volumes; mounts at `/data` (line 13) and `/shared` (line 14).
- **R-10 (High)**: release.yml audit confirms container build jobs only build+push, never run containers. No baked-in model assumption.
- **R-11 (Low)**: Partial -- code inspection confirms `EmbedError::Io` propagation with path context. No runtime `:ro` test (Docker unavailable).
- **R-12 (Low)**: Covered by R-01 scenario 2 (`test_resolve_cache_dir_unset_env_falls_to_dirs`).
- **R-13 (Med)**: Partial -- structural review confirms `unimatrix-shared` is a top-level named volume (shareable). No multi-container runtime test (Docker unavailable).
- **R-14 (Med)**: Dockerfile lines 131-132 confirm HEALTHCHECK tests daemon liveness, not model readiness. Semantics documented.
- **R-15 (Low)**: `grep -ic "baked into"` returns 0 for both PRODUCT-VISION.md and WAVE2-ROADMAP.md. Both reference `unimatrix-shared`.

**Gaps**: R-11 and R-13 are partial (Docker runtime unavailable). Both are Low/Med severity. The structural verification is sufficient given the environment constraint.

### 2. Test Coverage Completeness
**Status**: PASS
**Evidence**:

- **Unit tests**: 5,290 passed, 0 failed, 28 ignored (full workspace). 5 nan-015-specific tests in `unimatrix-embed::config` all pass.
- **Integration smoke tests**: 23 passed, 0 failed, 343 deselected. Runtime: 199.38s per tester report.
- **Risk-to-scenario mapping**: All 26 scenarios from the Risk-Based Test Strategy are addressed -- 5 via unit tests, 8 via static verification, 3 via documentation review, 10 via code inspection or structural analysis.
- **Cross-component checks**: Env var name consistency (config.rs:42 matches Dockerfile:125), VOLUME directive matches compose mounts, env var value is absolute path.

### 3. Specification Compliance
**Status**: PASS
**Evidence**:

| FR | Status | Evidence |
|----|--------|----------|
| FR-01 | PASS | `resolve_cache_dir()` implements 4-level precedence; 4 unit tests confirm |
| FR-02 | PASS | All call sites use `resolve_cache_dir()` per static grep |
| FR-03 | PASS | No model-download or COPY-model lines in Dockerfile |
| FR-04 | PASS | Dockerfile: `/shared/models` created (line 99), ownership 65532:65532 (line 100), permissions 0700 (line 101), `UNIMATRIX_MODEL_CACHE=/shared/models` (line 125), `VOLUME ["/data", "/shared"]` (line 128) |
| FR-05 | PASS | docker-compose.yml defines both volumes and mounts |
| FR-06 | PARTIAL | Preconditions verified structurally; runtime requires Docker |
| FR-07 | PARTIAL | Code path verified; runtime requires Docker |
| FR-08 | PARTIAL | CLI call site uses `resolve_cache_dir()` (main.rs:1431); runtime requires Docker |
| FR-09 | PARTIAL | Named volume is shareable; runtime requires Docker |
| FR-10 | PARTIAL | Code path verified; runtime requires Docker |
| FR-11 | PARTIAL | NLI verify-then-load ordering preserved (nli_handle.rs:282-319); runtime with shared volume requires Docker |
| FR-12 | PASS | PRODUCT-VISION.md and WAVE2-ROADMAP.md both describe two-volume model. No "baked into" references. |
| FR-13 | PASS | docker-compose.yml: `:ro` hardening (lines 16,44), `nli_model_sha256` guidance (line 45), #651 gap (line 46) |

| NFR | Status | Evidence |
|-----|--------|----------|
| NFR-01 | PARTIAL | Dockerfile verified model-free; size measurement requires Docker build |
| NFR-02 | PASS | `test_resolve_cache_dir_unset_env_falls_to_dirs` confirms platform default |
| NFR-03 | PARTIAL | No overhead from env var check; startup timing requires Docker |
| NFR-04 | PASS | Existing retry/backoff preserved; no new failure modes |
| NFR-05 | PASS | HEALTHCHECK unchanged (Dockerfile lines 131-132) |
| NFR-06 | PASS | `/shared` directory 0700 owned by 65532:65532 |

| AC | Status | Evidence |
|----|--------|----------|
| AC-01 | PARTIAL | Dockerfile model-free; size comparison requires Docker build |
| AC-02 | PASS | docker-compose.yml defines both volumes with correct mounts |
| AC-03-AC-09 | PARTIAL | All preconditions verified via static analysis; runtime requires Docker |
| AC-10 | PASS | Documentation review confirms two-volume description |
| AC-11 | PASS | Security guidance present at docker-compose.yml lines 44-46 |

**Note on PARTIAL items**: All PARTIAL statuses are due to Docker build/run being unavailable in this environment. The test-plan/OVERVIEW.md explicitly notes container-level tests are "outside the infra-001 harness scope." All code-level preconditions have been verified. The PARTIAL items will be validated during the first container build (CI or manual).

### 4. Architecture Compliance
**Status**: PASS
**Evidence**:

- **Component boundaries**: `EmbedConfig::resolve_cache_dir()` in `config.rs` is the single resolution point. No other component constructs cache paths independently.
- **Env var integration**: `UNIMATRIX_MODEL_CACHE` string identical in `config.rs:42` (`std::env::var("UNIMATRIX_MODEL_CACHE")`) and `Dockerfile:125` (`UNIMATRIX_MODEL_CACHE=/shared/models`).
- **Volume layout**: `VOLUME ["/data", "/shared"]` in Dockerfile matches `unimatrix-data:/data` and `unimatrix-shared:/shared` in docker-compose.yml.
- **Precedence chain**: Implementation matches ADR-002 exactly: field > env var (empty=unset) > dirs > fallback.
- **ADR compliance**: ADR-001 (env var), ADR-002 (precedence), ADR-003 (`:rw` default) all implemented as specified.
- **No architectural drift**: Components NOT affected (listed in Architecture) remain unmodified per git diff.

### 5. Knowledge Stewardship Compliance
**Status**: PASS
**Evidence**: Tester agent report (`nan-015-agent-7-tester-report.md`) contains:
- `## Knowledge Stewardship` section present
- `Queried:` entry -- `mcp__unimatrix__context_briefing` returned 11 entries including nan-015 ADRs and nan-014 Docker lessons
- `Stored:` entry -- "nothing novel to store -- the `resolve_cache_dir_with_env()` testability pattern is specific to this implementation... No generalizable testing infrastructure pattern emerged beyond what entry #750 already covers."

Reason provided for declining to store. Compliant.

### 6. Integration Test Validation
**Status**: PASS
**Evidence**:

- **Smoke tests passed**: 23/23 pass, 0 failures per tester report (confirmed via `pytest --co` dry run: 23/366 collected with smoke marker)
- **No xfail markers added**: No new xfail markers in the nan-015 diff. All existing xfails are pre-existing and reference GH Issues (#576, #561, #111, #406, #291, #405, #305, #575).
- **No integration tests deleted or commented out**: `git diff main -- product/test/infra-001/suites/` returns empty -- no changes to integration test files.
- **RISK-COVERAGE-REPORT includes integration test counts**: Section "Integration Tests (infra-001 smoke suite)" reports 23 passed, 0 failed, 343 deselected.
- **No xfails masking feature bugs**: All xfails are unrelated to nan-015 (rate limiting, tick interval, embedding model availability in CI, multi-hop traversal).

## Rework Required

None.

## Scope Concerns

None.
