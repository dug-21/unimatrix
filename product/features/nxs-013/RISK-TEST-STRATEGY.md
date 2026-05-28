# Risk-Based Test Strategy: nxs-013

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Dockerfile ENV removal causes container to fail loading config when no per-project config exists yet (first-run cold start) | High | Low | High |
| R-02 | Log label string changes in `log_config_provenance` accidentally alter control flow or break pattern-matched log consumers | Med | Low | Med |
| R-03 | Provenance log label changes are untestable via automated assertion — AC-03/AC-10 rely on manual log inspection only | Med | Med | Med |
| R-04 | Documentation edits to PRODUCT-VISION.md or WAVE2-ROADMAP.md extend beyond the constrained W2-1 volume sections, causing unintended content changes | Med | Low | Med |
| R-05 | README edits conflict with concurrent PRs touching the Configuration section or container quickstart line | Low | Low | Low |
| R-06 | `UNIMATRIX_CONFIG` env var set explicitly by operators stops working because code path was accidentally modified alongside label changes | High | Low | High |
| R-07 | `DEFAULT_CONFIG_TOML` header comment edit accidentally corrupts the TOML template content, causing config parse failures on first run | High | Low | High |
| R-08 | docker-compose.yml comment edits accidentally break YAML syntax (indentation, quoting) making `docker compose up` fail | Med | Low | Med |

## Risk-to-Scenario Mapping

### R-01: Container Cold Start Without UNIMATRIX_CONFIG ENV
**Severity**: High
**Likelihood**: Low
**Impact**: Container fails to start or loads no config on first run. `write_default_config_if_absent` writes to data dir, but if path resolution fails without the ENV, the daemon could error out before reaching that code.

**Test Scenarios**:
1. Build Docker image from modified Dockerfile. Run container with empty `unimatrix-data` volume. Verify daemon starts and `write_default_config_if_absent` writes config.toml to the data directory.
2. `docker inspect --format '{{.Config.Env}}'` on the built image confirms `UNIMATRIX_CONFIG` is absent and `HOME=/data` is present.
3. Verify startup logs contain "primary config" messages (not "env override" messages), confirming the per-project path is used.

**Coverage Requirement**: At least one actual Docker build + run cycle. Static Dockerfile review is insufficient per Unimatrix lesson #4582 (nan-014 required fix commits for issues invisible to static review).

### R-02: Log Label Change Alters Control Flow
**Severity**: Med
**Likelihood**: Low
**Impact**: If the label change accidentally modifies the match arms in `log_config_provenance` (e.g., changes which `SourceStatus` variant is matched, or alters the log level), the provenance reporting could suppress or misreport config sources.

**Test Scenarios**:
1. Existing 7 provenance tests pass without modification — confirms `SourceStatus` enum matching is unchanged.
2. Existing 4 category authority tests pass — confirms merge semantics are untouched.
3. Code review verifies that only string literals inside `info!()` / `debug!()` / `warn!()` macro calls changed, not the match arm patterns or log levels.

**Coverage Requirement**: `cargo test --workspace` passes with zero test file changes. Code review confirms string-only changes in `log_config_provenance`.

### R-03: Log Label Changes Are Untestable via Automation
**Severity**: Med
**Likelihood**: Med
**Impact**: AC-03 ("labels per-project as primary and global as defaults") and AC-10 ("startup logs show primary config messages") can only be verified by manual log inspection or a tracing-test harness. Without automated assertion, label correctness depends entirely on code review. Per Unimatrix lesson #4147, log-level ACs that lack a testability resolution recurrently cause gate failures.

**Test Scenarios**:
1. Code review of the exact string literals in `log_config_provenance` — verify "primary" appears in the per-project branch and "defaults" appears in the global branch.
2. Manual daemon startup with both config files present — inspect log output for correct labels.
3. Manual daemon startup with neither config file — inspect fallback messages for correct labels.

**Coverage Requirement**: AC-03 and AC-10 are verified by code review + manual log inspection. The specification (FR-03) already defines this as the verification method. No tracing-test harness is required for this feature — the labels are cosmetic, not behavioral.

### R-04: Documentation Edit Scope Creep
**Severity**: Med
**Likelihood**: Low
**Impact**: Edits to PRODUCT-VISION.md or WAVE2-ROADMAP.md that extend beyond the W2-1 volume description could introduce factual errors or contradict other planning content. Per SR-03/SR-04, these are constrained edits.

**Test Scenarios**:
1. `git diff` on PRODUCT-VISION.md shows changes only within the W2-1 volume description lines (~448-459).
2. `git diff` on WAVE2-ROADMAP.md shows changes only within the W2-1 volume list lines (~39-43).
3. Correction annotation ("Updated to reflect nan-014 shipped design") is present per ADR-004.

**Coverage Requirement**: PR diff review confirms edit boundaries. No lines outside the specified sections are modified.

### R-05: README Merge Conflict
**Severity**: Low
**Likelihood**: Low
**Impact**: If another PR modifies the README Configuration section concurrently, merge conflicts require manual resolution.

**Test Scenarios**:
1. Before delivery, check for open PRs touching README.md.

**Coverage Requirement**: Pre-delivery check only. No automated test needed.

### R-06: Explicit UNIMATRIX_CONFIG Override Breaks
**Severity**: High
**Likelihood**: Low
**Impact**: Operators who set `UNIMATRIX_CONFIG` explicitly (Kubernetes ConfigMap, docker-compose environment block) lose config override capability. This would be a regression for advanced deployments.

**Test Scenarios**:
1. Run daemon with `UNIMATRIX_CONFIG` set to a valid path — verify config is loaded from that path (env override takes precedence).
2. Run daemon with `UNIMATRIX_CONFIG` set to a non-existent path — verify appropriate error/fallthrough behavior unchanged.
3. Code review confirms zero changes to `load_config` function body.

**Coverage Requirement**: Existing provenance tests cover the env override path. Code review confirms `load_config` is untouched. Manual verification with explicit `UNIMATRIX_CONFIG` if Docker build is performed.

### R-07: DEFAULT_CONFIG_TOML Template Corruption
**Severity**: High
**Likelihood**: Low
**Impact**: If the header comment edit accidentally breaks TOML syntax (e.g., missing `#` prefix on a comment line, or stray characters entering the template body), `write_default_config_if_absent` writes an unparseable config.toml. On next startup, `load_config` fails to parse per-project config.

**Test Scenarios**:
1. Existing config parsing tests pass — they exercise `DEFAULT_CONFIG_TOML` parsing.
2. Code review confirms changes are limited to `#`-prefixed comment lines in the header, not template body content.
3. Run `unimatrix config` and verify the generated file parses correctly.

**Coverage Requirement**: `cargo test --workspace` covers TOML parsing. Code review confirms comment-only changes.

### R-08: docker-compose.yml YAML Syntax Error
**Severity**: Med
**Likelihood**: Low
**Impact**: Malformed YAML in docker-compose.yml causes `docker compose config` to fail, blocking container deployment.

**Test Scenarios**:
1. Run `docker compose -f docker-compose.yml config` to validate YAML syntax after edits.
2. Code review confirms comment-only changes (lines starting with `#`) — no structural YAML modifications.

**Coverage Requirement**: YAML validation command or code review confirming comment-only edits.

## Integration Risks

The architecture explicitly states all seven components are independent with no inter-component dependencies. Integration risk is therefore minimal. The primary integration surface is between the Dockerfile ENV removal (C1) and the `load_config` code path:

- **C1 <-> load_config**: Removing `UNIMATRIX_CONFIG` from Dockerfile ENV changes which `load_config` step activates first in the container. Without the ENV, Step 0 (env override) is skipped, and Step 2 (per-project) becomes the effective primary. This is the intended behavior, but the path resolution depends on `HOME=/data` (ADR-005, #4573) remaining in the ENV block. If `HOME=/data` were accidentally removed alongside `UNIMATRIX_CONFIG`, all path resolution would break.
- **C3 <-> provenance types**: Log label changes in `log_config_provenance` consume `ConfigProvenance` and `SourceStatus` types from PR #636. The types are unchanged; only string literals change. Risk is low but any accidental match-arm edit would break provenance reporting.
- **C7 <-> config parsing**: `DEFAULT_CONFIG_TOML` is parsed by `load_config` pipeline. Comment-only changes cannot affect parsing, but an edit that extends past the comment block into template body would.

## Edge Cases

- **Empty data volume, no global config, no UNIMATRIX_CONFIG**: Container cold start. `write_default_config_if_absent` must succeed, then `load_config` Step 2 loads the newly written default. This is the personal cloud first-run scenario.
- **Both global and per-project config exist**: Merge semantics must produce per-project overriding global. Unchanged behavior, but the new log labels must correctly report both as loaded.
- **UNIMATRIX_CONFIG points to a path inside the data volume**: Unusual but valid. The env override and per-project paths could point to the same file. `load_config` handles this (env override wins at Step 0, Step 2 is still evaluated but the merge is idempotent).
- **UNIMATRIX_CONFIG set to empty string**: Edge case in env var handling. `load_config` Step 0 should treat empty string as unset or fail gracefully.
- **docker-compose.yml with UNIMATRIX_CONFIG uncommented by operator**: The commented example in FR-02 must be valid YAML when uncommented. Incorrect indentation or quoting would silently fail or cause parse errors.

## Security Risks

This feature has a minimal security surface:

- **No new untrusted input**: The feature removes an ENV default and changes log strings. No new data ingestion, no new file paths accepted from users, no new network endpoints.
- **Config file path handling**: Unchanged. `load_config` already validates paths. The Dockerfile change only affects which default path is checked, not how paths are validated.
- **Log injection**: The new log label strings are hardcoded literals, not user-supplied. No injection risk from the label changes themselves. The `path` field in `SourceStatus::Loaded` is logged but was already logged before nxs-013.
- **Blast radius**: If a malicious config.toml is placed in the data volume, it could alter daemon behavior (categories, weights, etc.). This is unchanged from before nxs-013 — the data volume was always writable. The removal of the bind-mount pattern actually reduces the attack surface slightly (one fewer external mount point to manage).

## Failure Modes

| Failure | Expected Behavior | Severity |
|---------|-------------------|----------|
| Container starts with no config files anywhere | `write_default_config_if_absent` writes default config; daemon starts with compiled defaults | Normal operation |
| `HOME=/data` accidentally removed from Dockerfile | All path resolution fails; daemon cannot find data directory | Critical — but out of nxs-013 scope (HOME removal would be a separate bug) |
| `DEFAULT_CONFIG_TOML` header edit corrupts TOML | `write_default_config_if_absent` writes unparseable file; `load_config` returns parse error; daemon fails to start | High — caught by existing config parsing tests |
| docker-compose.yml YAML broken | `docker compose up` fails with parse error | Med — caught by `docker compose config` validation |
| Log labels swapped (global labeled "primary", per-project labeled "defaults") | Operators misinterpret config hierarchy from logs; no functional impact | Low — cosmetic but confusing |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (default config discovery path change) | R-01 | No external users — initial correct design. R-01 validates cold-start correctness. |
| SR-02 (distroless verification) | R-01, R-03 | Verification via `docker inspect` and log output. R-03 acknowledges log-based verification limitation. |
| SR-03 (PRODUCT-VISION.md scope creep) | R-04 | Edit boundary enforced via PR diff review. ADR-004 constrains to W2-1 volume description only. |
| SR-04 (WAVE2-ROADMAP.md purpose) | R-04 | OQ-02 resolved: correct with annotation. ADR-004 documents decision. |
| SR-05 (unresolved OQs) | -- | All three OQs resolved in architecture (ADR-001, ADR-002, ADR-003). No residual risk. |
| SR-06 (provenance test assertions) | R-02 | Verified: tests assert on `SourceStatus` enum variants, not log strings. Zero test changes required. |
| SR-07 (README merge conflicts) | R-05 | Pre-delivery check for concurrent README PRs. Low likelihood. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-01, R-06, R-07) | 8 scenarios |
| Medium | 4 (R-02, R-03, R-04, R-08) | 9 scenarios |
| Low | 1 (R-05) | 1 scenario |
| **Total** | **8** | **18 scenarios** |

### Non-Negotiable Coverage

1. **`cargo test --workspace` passes with zero test file changes** (R-02, R-07, AC-09)
2. **Docker build succeeds** (R-01, per lesson #4582)
3. **`docker inspect` confirms ENV block correctness** — `UNIMATRIX_CONFIG` absent, `HOME=/data` present (R-01)
4. **Code review confirms `load_config` is unmodified** (R-06, NFR-01)
5. **Code review confirms `log_config_provenance` changes are string-literal-only** (R-02)
6. **PR diff review confirms documentation edit boundaries** (R-04)
