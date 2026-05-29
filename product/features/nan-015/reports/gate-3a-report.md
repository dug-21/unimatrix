# Gate 3a Report: nan-015

> Gate: 3a (Design Review)
> Date: 2026-05-29
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 4 component pseudocode files align with architecture decomposition, ADRs, and integration surface |
| Specification coverage | PASS | All 13 FRs and 6 NFRs addressed in pseudocode; no scope additions |
| Risk coverage | PASS | All 15 risks (R-01 through R-15) mapped to test scenarios in component test plans |
| Interface consistency | PASS | Shared env var name, function signatures, volume paths consistent across all pseudocode files |
| Knowledge stewardship compliance | FAIL | Architect report missing `## Knowledge Stewardship` section entirely |

## Detailed Findings

### 1. Architecture Alignment
**Status**: PASS

**Evidence**:
- **cache-path-resolution**: Pseudocode implements the exact four-level precedence chain from ADR-002 (config field > env var > dirs > fallback). Empty-string guard present per ADR-002 invariant. Single function modification matches Architecture Section 1.
- **dockerfile**: Pseudocode removes all three model bake-in steps identified in Architecture Section 2 (builder model-download, HuggingFace cleanup, runtime COPY). Creates `/shared/models` with 65532:65532 ownership and 0700 permissions. Sets `UNIMATRIX_MODEL_CACHE=/shared/models` and declares `VOLUME ["/data", "/shared"]`.
- **compose-config**: Adds `unimatrix-shared` named volume and mounts at `/shared` per Architecture Section 3. ADR-003 `:ro` hardening guidance included in comments.
- **documentation**: Updates PRODUCT-VISION.md and WAVE2-ROADMAP.md W2-1 sections per Architecture Section 5.
- ADR-001 (env var), ADR-002 (precedence), ADR-003 (default :rw) all reflected accurately in corresponding pseudocode.

### 2. Specification Coverage
**Status**: PASS

**Evidence**:
- **FR-01** (env var): cache-path-resolution pseudocode implements exact precedence chain. Verified: config field > env var > dirs > fallback.
- **FR-02** (single resolution function): Pseudocode modifies only `resolve_cache_dir()`. No new resolution functions introduced. Call site table in OVERVIEW.md confirms all 8 sites flow through this function.
- **FR-03** (model bake-in removal): dockerfile pseudocode removes builder model-download (lines 95-101), HuggingFace cleanup, and runtime COPY (line 122).
- **FR-04** (shared volume config): dockerfile pseudocode creates `/shared` with 65532:65532 and 0700, declares `VOLUME ["/data", "/shared"]`, sets ENV.
- **FR-05** (compose volumes): compose-config pseudocode defines both named volumes with correct mount points.
- **FR-06** through **FR-10**: Addressed by the combination of cache-path-resolution + dockerfile + compose-config changes. Auto-download (FR-06), cached loading (FR-07), CLI writes (FR-08), multi-container (FR-09), and air-gap (FR-10) all rely on the env var redirect working correctly, which the pseudocode implements.
- **FR-11** (NLI hash continuity): cache-path-resolution pseudocode preserves `resolve_cache_dir()` return type and contract. Test plan R-04 includes code inspection of verify-then-load ordering.
- **FR-12** (documentation): documentation pseudocode updates both PRODUCT-VISION.md and WAVE2-ROADMAP.md W2-1 sections.
- **FR-13** (security docs): compose-config pseudocode includes `:ro` hardening comment, `nli_model_sha256` guidance, and #651 gap acknowledgment.
- **NFR-01** through **NFR-06**: Addressed -- image size reduction (NFR-01) via bake-in removal, non-container unchanged (NFR-02) via env var only in Dockerfile, startup latency (NFR-03) unchanged, download resilience (NFR-04) via existing retry, HEALTHCHECK (NFR-05) unchanged, permission safety (NFR-06) via 0700.
- No scope additions detected. Pseudocode does not implement features beyond what the specification requires.

### 3. Risk Coverage
**Status**: PASS

**Evidence**: All 15 risks mapped to test scenarios across component test plans.

| Risk | Test Plan File | Scenarios |
|------|---------------|-----------|
| R-01 (precedence) | cache-path-resolution.md | 4 unit test scenarios (ADR-002 matrix) |
| R-02 (call site divergence) | cache-path-resolution.md | Static grep verification |
| R-03 (embedding tampering) | documentation.md | Tests 3-5: hash pinning guidance, #651 gap, no false assurance |
| R-04 (NLI hash broken) | cache-path-resolution.md | Code inspection of verify-then-load ordering |
| R-05 (incomplete removal) | dockerfile.md | Tests 1-2: grep for removed lines + image size |
| R-06 (/shared permissions) | dockerfile.md | Tests 3-4: ownership/permissions inspection |
| R-07 (empty env var) | cache-path-resolution.md | 1 dedicated unit test |
| R-08 (partial corruption) | cache-path-resolution.md | Code inspection of retry path |
| R-09 (compose misconfiguration) | compose-config.md | Tests 1-3: volume definitions, mount points, compose config validation |
| R-10 (CI smoke tests) | dockerfile.md | Test 6: release.yml audit |
| R-11 (read-only error clarity) | dockerfile.md | Test 8: error path inspection |
| R-12 (env var bleed) | cache-path-resolution.md | Covered by R-01 scenario 2 (env unset) |
| R-13 (concurrent download) | compose-config.md | Test 6: multi-container structure compatibility |
| R-14 (HEALTHCHECK masking) | dockerfile.md | Test 7: HEALTHCHECK semantics review |
| R-15 (doc inconsistency) | documentation.md | Tests 1-2: grep for "baked into" and "unimatrix-shared" |

The ADR-002 test matrix (4 scenarios for cache path precedence) is implemented verbatim in test-plan/cache-path-resolution.md as Tests 1-4 under "R-01: Cache Path Precedence."

### 4. Interface Consistency
**Status**: PASS

**Evidence**:
- **Env var name**: `UNIMATRIX_MODEL_CACHE` is consistent across cache-path-resolution pseudocode (`std::env::var("UNIMATRIX_MODEL_CACHE")`), dockerfile pseudocode (`ENV UNIMATRIX_MODEL_CACHE=/shared/models`), and test plans (cross-component consistency check in cache-path-resolution test plan).
- **Function signature**: `EmbedConfig::resolve_cache_dir(&self) -> PathBuf` unchanged. OVERVIEW.md explicitly states "signature unchanged, body modified."
- **Volume path**: `/shared/models` consistent between Dockerfile `ENV` value and the mount point `/shared` in compose-config.
- **Integration surface table** from architecture matches pseudocode usage:
  - `resolve_cache_dir()` -- modified in cache-path-resolution pseudocode
  - `UNIMATRIX_MODEL_CACHE` -- set in dockerfile pseudocode, read in cache-path-resolution
  - `ensure_model()` / `ensure_nli_model()` -- unchanged, documented in pseudocode as accepting resolved path
  - `NliConfig.cache_dir` -- unchanged, populated from `resolve_cache_dir()`
  - `VOLUME ["/data", "/shared"]` -- in dockerfile pseudocode
  - `unimatrix-shared` -- in compose-config pseudocode
- Data flow between components is coherent: Dockerfile sets env var -> resolve_cache_dir() reads it -> ensure_model()/ensure_nli_model() receive resolved path. No contradictions.

### 5. Knowledge Stewardship Compliance
**Status**: FAIL

**Evidence**:

| Agent | Report File | Has Section? | Content |
|-------|-------------|-------------|---------|
| scope-risk | nan-015-agent-0-scope-risk-report.md | Yes | 5 `Queried:` entries. No `Stored:` or "nothing novel" entry. |
| architect | nan-015-agent-1-architect-report.md | **No** | Section entirely absent |
| pseudocode | nan-015-agent-1-pseudocode-report.md | Yes | 3 `Queried:` entries + "Deviations from established patterns: none" |
| spec | nan-015-agent-2-spec-report.md | Yes | 1 `Queried:` entry |
| testplan | nan-015-agent-2-testplan-report.md | Yes | 1 `Queried:` entry + "nothing novel to store" with reason |

**Issues**:
1. **architect report** (`nan-015-agent-1-architect-report.md`): Missing `## Knowledge Stewardship` section entirely. The architect is an active-storage agent and stored 4 ADRs via Unimatrix (#4650-4653), but did not document this in a Knowledge Stewardship block. This is a **REWORKABLE FAIL** per gate rules.
2. **scope-risk report** (`nan-015-agent-0-scope-risk-report.md`): Has `Queried:` entries but no `Stored:` or "nothing novel to store -- {reason}" entry. This is a **WARN** -- the section exists but is incomplete.

## Validation Checklist Results

| # | Checklist Item | Status |
|---|---------------|--------|
| 1 | Cache path resolution pseudocode matches ADR-002 precedence chain exactly | PASS |
| 2 | Empty string guard for UNIMATRIX_MODEL_CACHE is present in pseudocode | PASS |
| 3 | Dockerfile pseudocode removes all model bake-in steps identified in architecture | PASS |
| 4 | Dockerfile pseudocode creates /shared with correct ownership (65532:65532) and permissions (0700) | PASS |
| 5 | Compose-config pseudocode adds unimatrix-shared volume and mounts at /shared | PASS |
| 6 | Documentation pseudocode addresses both PRODUCT-VISION.md and WAVE2-ROADMAP.md | PASS |
| 7 | Test plans cover all 15 risks (R-01 through R-15) | PASS |
| 8 | Test plans include the ADR-002 test matrix for cache path precedence (4 scenarios) | PASS |
| 9 | Integration harness plan exists in test-plan/OVERVIEW.md | PASS |
| 10 | Component interfaces in pseudocode match architecture integration surface table | PASS |

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Architect report missing `## Knowledge Stewardship` section | nan-015-agent-1-architect | Add `## Knowledge Stewardship` block with `Stored:` entries documenting ADR storage (#4650, #4651, #4652, #4653) and any `Queried:` entries from briefing/search calls made during design |
| Scope-risk report missing `Stored:` / "nothing novel" entry | nan-015-agent-0-scope-risk | Add `Stored:` or "nothing novel to store -- {reason}" entry to existing Knowledge Stewardship section |
