# nxs-013: Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nxs-013/SCOPE.md |
| Scope Risk Assessment | product/features/nxs-013/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nxs-013/architecture/ARCHITECTURE.md |
| Specification | product/features/nxs-013/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/nxs-013/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nxs-013/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1: Dockerfile ENV Block | pseudocode/C1-dockerfile-env.md | test-plan/C1-dockerfile-env.md |
| C2: docker-compose.yml Comments | pseudocode/C2-docker-compose-comments.md | test-plan/C2-docker-compose-comments.md |
| C3: log_config_provenance Labels | pseudocode/C3-provenance-labels.md | test-plan/C3-provenance-labels.md |
| C4: README Configuration Section | pseudocode/C4-readme-config.md | test-plan/C4-readme-config.md |
| C5: PRODUCT-VISION.md W2-1 | pseudocode/C5-product-vision-w2-1.md | test-plan/C5-product-vision-w2-1.md |
| C6: WAVE2-ROADMAP.md W2-1 | pseudocode/C6-wave2-roadmap-w2-1.md | test-plan/C6-wave2-roadmap-w2-1.md |
| C7: DEFAULT_CONFIG_TOML Header | pseudocode/C7-default-config-header.md | test-plan/C7-default-config-header.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Align documentation, container defaults, and log labeling with the reality that per-project `config.toml` (at `~/.unimatrix/{hash}/config.toml`) is the canonical configuration surface. Remove the misleading `UNIMATRIX_CONFIG` ENV default from the Dockerfile so the daemon loads per-project config from the data volume naturally, update provenance log labels to reflect the per-project/global hierarchy, and update all documentation to present a single-directory operational model where backup = snapshot one volume.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Remove UNIMATRIX_CONFIG from Dockerfile ENV | Remove the default; operators who need it add it explicitly | ADR-001, SR-01 | architecture/ADR-001-remove-unimatrix-config-from-dockerfile-env.md |
| No provenance summary line | Individual source labels ("primary", "defaults") are sufficient; no aggregate summary | ADR-002, OQ-03 | architecture/ADR-002-no-provenance-summary-line.md |
| docker-compose.yml shows env var example, not bind mount | Commented UNIMATRIX_CONFIG env var example for advanced use; remove bind mount pattern | ADR-003, OQ-01 | architecture/ADR-003-docker-compose-env-var-example.md |
| Correct roadmap/vision volume descriptions | Update W2-1 sections to match nan-014 shipped single-volume design with annotation | ADR-004, OQ-02 | architecture/ADR-004-correct-roadmap-volume-descriptions.md |

## Files to Create/Modify

| File | Change | Summary |
|------|--------|---------|
| `Dockerfile` | Remove line | Remove `UNIMATRIX_CONFIG=/etc/unimatrix/config.toml` from runtime ENV block (line ~131) |
| `docker-compose.yml` | Comment rewrite | Replace config bind-mount comments (lines 14-17) with in-volume config explanation + commented UNIMATRIX_CONFIG env var example + backup guidance |
| `crates/unimatrix-server/src/main.rs` | String literal update | Update log message strings in `log_config_provenance` (lines ~1347-1375): "project" -> "primary (per-project)", "global" -> "defaults (global)" |
| `README.md` | Prose update | Configuration section (~line 238): lead with per-project as canonical; container description (~line 62): remove `/etc/unimatrix/` reference |
| `product/PRODUCT-VISION.md` | Prose update | W2-1 volume description (~lines 448-459): single `unimatrix-data` volume, remove `unimatrix-shared` for config, update `[Medium]` security requirement (line 456) to replace bind-mount guidance with env var injection |
| `product/WAVE2-ROADMAP.md` | Prose update | W2-1 volume list (~lines 39-43): single volume, add "Updated to reflect nan-014 shipped design" annotation |
| `crates/unimatrix-server/src/infra/config.rs` | Comment update | `DEFAULT_CONFIG_TOML` header (~lines 3130-3138): emphasize per-project as canonical, global as defaults |

## Data Structures

No new types. Existing types consumed but not modified:

| Type | Location | Role |
|------|----------|------|
| `ConfigLoadResult { config: UnimatrixConfig, provenance: ConfigProvenance }` | config.rs:2092 | Return value of `load_config` -- unchanged |
| `ConfigProvenance { global: SourceStatus, project: SourceStatus, env_override: SourceStatus }` | config.rs:2078 | Provenance metadata -- unchanged |
| `SourceStatus { Loaded { path }, NotFound { path }, NotApplicable }` | config.rs:2067 | Per-source status enum -- unchanged |

## Function Signatures

No new functions. Existing functions touched:

| Function | Location | Change |
|----------|----------|--------|
| `log_config_provenance(provenance: &ConfigProvenance)` | main.rs:1347 | Log string literals only -- match arms, log levels, and control flow unchanged |
| `load_config(home_dir: &Path, data_dir: &Path) -> Result<ConfigLoadResult, ConfigError>` | config.rs:2120 | NOT modified -- three-layer merge untouched |
| `write_default_config_if_absent(path: &Path, force: bool)` | config.rs | NOT modified -- already writes to per-project path |

## Constraints

| ID | Constraint | Source |
|----|-----------|--------|
| C-01 | No behavioral change to `load_config` -- three-layer merge (global -> per-project -> env override) must remain byte-identical | SCOPE.md, NFR-01 |
| C-02 | All existing tests must pass with zero test file changes | SCOPE.md, NFR-02, SR-06 (tests are type-based, not string-based) |
| C-03 | `UNIMATRIX_CONFIG` env var stays in code -- only Dockerfile default removed | SCOPE.md, ADR-001 |
| C-04 | Distroless runtime -- verification via `docker inspect` and log output only, no shell | SCOPE.md, SR-02 |
| C-05 | PR #636 provenance types (`ConfigLoadResult`, `ConfigProvenance`, `SourceStatus`) are NOT modified | SCOPE.md |
| C-06 | Documentation edits constrained to specific sections only -- no broad revision | NFR-05, SR-03, SR-04 |
| C-07 | docker-compose.yml comments target new users setting up their first deployment — explain the correct pattern, not a migration path | ADR-003 |

## Dependencies

| Dependency | Type | Status |
|-----------|------|--------|
| PR #636 (ConfigLoadResult, ConfigProvenance, SourceStatus types) | Code foundation | Merged 2025-05-25 |
| ADR-005 (Container Data Path Resolution, Unimatrix #4573) | Design decision | Established -- `HOME=/data` ensures per-project config resolves inside data volume |
| Two-Level TOML Config Merge pattern (Unimatrix #2395) | Pattern | Established -- merge pipeline unchanged by nxs-013 |

No external crates or services required. No new Rust dependencies.

## NOT in Scope

- Moving unimatrix.db -- already in the correct location
- Changing merge order -- global -> per-project -> env override remains unchanged
- Removing global config support -- continues to work as defaults layer
- Removing UNIMATRIX_CONFIG env var from code -- remains for explicit operator use
- Changing `write_default_config_if_absent` target path -- already writes to per-project path
- Schema changes or new CLI flags -- no new Rust types, APIs, or migrations
- Changing `ConfigLoadResult`, `ConfigProvenance`, or `SourceStatus` types
- Modifying any test files -- all tests must pass as-is
- Broad PRODUCT-VISION.md revision -- only W2-1 volume description
- Broad WAVE2-ROADMAP.md revision -- only W2-1 volume list

## Alignment Status

Full alignment. 6 checks PASS, 0 variances.

| Check | Status |
|-------|--------|
| Vision Alignment | PASS -- serves Vision Goals #8 (developer-friendly deployment) and #9 (domain-agnostic platform) |
| Milestone Fit | PASS -- targets W2-1 (Container Packaging) exclusively, no premature capabilities |
| Scope Gaps | PASS -- all 10 ACs addressed in specification and architecture |
| Scope Additions | PASS -- no additions detected |
| Architecture Consistency | PASS -- 7 independent components, integration surface accurately mapped |
| Risk Completeness | PASS -- all 7 scope risks traced to mitigations, 8 risks, 18 test scenarios |

Vision document edits (PRODUCT-VISION.md, WAVE2-ROADMAP.md) are factual corrections to match nan-014 shipped reality, not revisions of intent. ADR-004 documents the rationale.

## Risk Summary

8 risks identified, 18 test scenarios. High-priority risks:

| Risk | Description | Mitigation |
|------|-------------|-----------|
| R-01 | Container cold start without UNIMATRIX_CONFIG ENV | Docker build + run cycle, `docker inspect` verification |
| R-06 | Explicit UNIMATRIX_CONFIG override breaks | Existing provenance tests + code review confirms `load_config` untouched |
| R-07 | DEFAULT_CONFIG_TOML header edit corrupts TOML | Existing config parsing tests + code review confirms comment-only changes |

Non-negotiable coverage gates:
1. `cargo test --workspace` passes with zero test file changes
2. Docker build succeeds
3. `docker inspect` confirms `UNIMATRIX_CONFIG` absent, `HOME=/data` present
4. Code review confirms `load_config` is unmodified
5. Code review confirms `log_config_provenance` changes are string-literal-only
6. PR diff review confirms documentation edit boundaries

## Provenance Log Label Mapping

Reference for C3 implementor:

| Current Label | New Label |
|---------------|-----------|
| `"global config loaded"` | `"defaults config loaded (global)"` |
| `"global config not found; using compiled defaults"` | `"defaults config not found (global); using compiled defaults"` |
| `"project config loaded"` | `"primary config loaded (per-project)"` |
| `"project config not found; using compiled defaults"` | `"primary config not found (per-project); write default with 'unimatrix config'"` |

The `env_override` branch labels remain unchanged.
