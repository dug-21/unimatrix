# Security Review: dsn-001-security-reviewer

## Risk Level: medium

## Summary

PR #307 introduces TOML config externalization for Unimatrix (dsn-001). The security posture of the new config loader is solid: file permission enforcement, 64 KB size cap before parse, length-before-scan injection ordering, explicit Admin exclusion, and comprehensive validation tests are all present and correct. One medium-severity functional gap exists: the configured `ConfidenceParams` is threaded to the background tick but is NOT propagated to the inline `compute_confidence` calls in `server.rs`, `services/confidence.rs`, `services/usage.rs`, and `mcp/tools.rs` — all of which hardcode `ConfidenceParams::default()`. This means non-collaborative presets are silently ineffective at most serving-path call sites. This is not a security vulnerability but it is a correctness regression that makes the feature's primary contract (preset selection changes scoring behavior) non-functional outside the background tick. No blocking security findings (injection, privilege escalation, path traversal, or secret leakage) were identified.

## Findings

### Finding 1: `compute_confidence` call sites outside background tick use hardcoded defaults

- **Severity**: medium (functional regression, not a security risk)
- **Location**: `crates/unimatrix-server/src/server.rs` lines 640, 1243, 1319, 1379, 1672, 1687, 1707; `crates/unimatrix-server/src/services/confidence.rs` line 135; `crates/unimatrix-server/src/services/usage.rs` lines 199, 310; `crates/unimatrix-server/src/mcp/tools.rs` line 1775
- **Description**: `Arc<ConfidenceParams>` is resolved from config at startup and passed to the background tick, but all inline `compute_confidence` calls elsewhere use `&unimatrix_engine::confidence::ConfidenceParams::default()` regardless of what preset the operator configured. An operator setting `preset = "empirical"` (w_fresh = 0.34) will see `collaborative` scoring (w_fresh = 0.18) in the search re-ranking path, explicit confidence refresh, correction-chain confidence updates, and the cycle review lesson-learned path. The background tick at 15-minute intervals will eventually apply the correct weights, but the serving path remains on compiled defaults.
- **Recommendation**: Thread `Arc<ConfidenceParams>` to `UnimatrixServer` at construction time (same pattern as `CategoryAllowlist` or `AgentRegistry`) and replace the `ConfidenceParams::default()` literals in the inline call sites. Alternatively, document explicitly that inline confidence recomputations always use the collaborative preset — but that would make non-collaborative presets misleading.
- **Blocking**: No (functional gap, not a security issue; the PR is self-consistent about this being a future wire-up)

### Finding 2: Merged config is not re-validated after two-level merge

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/config.rs`, `load_config()`, line 475
- **Description**: `load_config` validates each config file independently before merge, then returns the merged result without a post-merge `validate_config` call. The `merge_configs` function uses independent merges for `categories` and `boosted_categories`: if the global config sets `categories = ["a", "b"]` and `boosted_categories = ["a"]` (valid at global level), and the per-project config sets `categories = ["c"]` (overrides) but does not set `boosted_categories` (so global `["a"]` passes through), the merged config has `categories = ["c"]` and `boosted_categories = ["a"]` — "a" is not in ["c"]. This violates the `BoostedCategoryNotInAllowlist` constraint that `validate_config` enforces but is never checked on the merged output. In practice this would cause `SearchService.boosted_categories` to contain categories that can never appear in any entry, meaning the provenance boost is silently dead. This is a correctness gap, not a security vulnerability.
- **Recommendation**: Add a `validate_config(&merged, &global_path)` call after `merge_configs` returns in `load_config`. This would catch cross-level constraint violations before they reach subsystem constructors.
- **Blocking**: No

### Finding 3: `[confidence] weights` present for named presets warns but the merged config is not re-validated

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/config.rs`, `validate_config()` line 636; `merge_configs()` line 889
- **Description**: When `preset != "custom"`, a `[confidence] weights` section in the file emits a warning and is correctly ignored by `resolve_confidence_params`. However, the `Option::or` merge at line 889 means global `[confidence] weights` can silently appear in the merged config when the per-project file does not specify weights. If an operator switches from `custom` (global) to `authoritative` (per-project), the merged config will carry the global custom weights. These are never used for named presets (correctly), but the warning is only emitted at per-file validation time, not at merge time, so the operator gets a warning for the global file in isolation but no warning that the merged result still carries the orphaned weights.
- **Recommendation**: Either emit a warning after merge if the merged config has non-None weights but a named preset, or suppress the per-file warning and issue it only on the merged config. The current behavior is not wrong but can be confusing.
- **Blocking**: No

### Finding 4: TOCTOU window between permission check and file read is minimal but exists

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/config.rs`, `load_single_config()` / `check_permissions()` lines 793-823
- **Description**: The code follows the pattern: `check_permissions(path)?` then `std::fs::read(path)`. The ARCHITECTURE.md and code comments correctly note this uses `metadata()` (not `symlink_metadata()`). The TOCTOU window is very narrow — there is no yield point between check and read — but it exists in theory. For a server running as a non-root user reading files in `~/.unimatrix/`, an attacker replacing a valid file with a malicious one between check and read would need local write access to the home directory, at which point they already own the machine. This is an inherent Unix limitation, not a code defect.
- **Recommendation**: No change required. The current mitigation (synchronous check-then-read with no yield) is appropriate for the threat model.
- **Blocking**: No

### Finding 5: `PreCompact` hook added to `.claude/settings.json` without config load

- **Severity**: low (informational)
- **Location**: `.claude/settings.json`, line 80-90
- **Description**: A `PreCompact` hook entry was added, pointing to the release binary at `/workspaces/unimatrix/target/release/unimatrix hook PreCompact`. This is a process-level hook invocation (sync path, no tokio, no config load per `main.rs` dispatch). Consistent with the other hook entries in the file. No security concern, but the hook command dispatches to the `Command::Hook` arm which has a sub-50ms budget. If the `PreCompact` handler does more than the budget allows, Claude's context compaction will be delayed. No config data flows through the hook path.
- **Recommendation**: None. The hook is correctly wired to the sync dispatch path.
- **Blocking**: No

## Blast Radius Assessment

If Finding 1 (hardcoded defaults at inline call sites) has a subtle interaction bug: operators setting non-collaborative presets will observe that search rankings and confidence scores shown inline during tool calls do not reflect their preset. The background tick will eventually converge stored confidence values to the correct weights, but the ephemeral in-memory reranking at query time will always use collaborative weights. Worst case: an operator configuring `empirical` to get high freshness sensitivity sees no behavior change — the feature's primary UX value proposition fails silently. No data corruption, no security compromise, no denial of service.

If Finding 2 (missing post-merge validation) has a subtle operator configuration: a boosted category from the global config silently survives into the merged result despite not being in the merged category allowlist. The SearchService `boosted_categories` HashSet will contain a dead category. No entries can ever be classified under that category post-merge, so the boost is perpetually dormant. No query-time panic, no data corruption.

The injection path (Finding 3 analog in SR-SEC-01) is correctly gated: instructions are validated at per-file load time, and the merge uses `Option::or` (one-wins, no concatenation), so a malicious instructions string in either file will be caught at load time. No injection bypass through merge.

## Regression Risk

1. The `context_retrospective` → `context_cycle_review` rename is complete across all active Rust, Python, and protocol files. The grep of `*.rs` and `*.py` files for `context_retrospective` returns zero matches. Historical product documentation in `product/research/` and `product/features/col-*/` retain the old name but these are not active code paths — acceptable per the SPECIFICATION.md SR-05 exclusion list.

2. `ConfidenceParams::default()` at all migrated call sites preserves pre-dsn-001 behavior exactly (SR-10 invariant), so no regression for operators with no config file or `collaborative` preset.

3. `CategoryAllowlist::new()` delegates to `from_categories(INITIAL_CATEGORIES)` — existing tests are unaffected.

4. `AgentRegistry::new()` gains a `session_caps: Vec<Capability>` parameter — all existing call sites in tests pass the new signature (compile-enforced).

## Dependency Safety

`toml = "0.8"` (resolved to 0.8.23) with sub-dependencies `toml_datetime 0.6.11`, `toml_edit 0.22.27`, `toml_write 0.1.2`. No known CVEs for these versions. The dependency is confined to `unimatrix-server/Cargo.toml` only per ADR-002. The Cargo.lock pin is confirmed. No other new dependencies are introduced.

## Secrets Check

No hardcoded secrets, API keys, credentials, or tokens were identified in the diff.

## PR Comments

- Posted 1 comment on PR #307 (findings summary).
- Blocking findings: No

## Knowledge Stewardship

Nothing novel to store — all risks are feature-specific to dsn-001 / the config externalization pattern. The recurring observation "the configured ConfidenceParams Arc is resolved at startup and passed to the background tick, but inline serving-path recomputation still uses default params" is a one-time wiring gap specific to this feature, not a generalizable anti-pattern.
