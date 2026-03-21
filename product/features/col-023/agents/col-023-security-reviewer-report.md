# Security Review: col-023-security-reviewer

## Risk Level: low

## Summary

col-023 generalizes the observation pipeline from a `HookType` enum to string-typed `event_type`/`source_domain` fields, introduces a TOML-configured `DomainPackRegistry`, and extends `OBSERVATION_METRICS` with a nullable `domain_metrics_json` column (schema v14). The diff is a large structural refactor touching 25+ files. Security controls are well-designed with two minor gaps that are informational, not blocking. No injection, deserialization, access control, or secrets concerns were found.

---

## Findings

### Finding 1: Domain pack `categories` strings are not format-validated

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/main.rs:562–564`, `crates/unimatrix-server/src/infra/config.rs:1056–1083`
- **Description**: `validate_config` validates `source_domain` thoroughly (regex `^[a-z0-9_-]{1,64}$`, reserved "unknown" check). However, the `categories` list inside each `[[observation.domain_packs]]` entry is passed directly to `CategoryAllowlist::add_category()` with no format validation. The `[knowledge] categories` section _does_ validate each category string for length ≤ 64 and `[a-z0-9_-]` characters. Domain pack categories bypass this check. An operator who configures a pack with `categories = ["My Category!"]` will successfully pollute the allowlist with a string that a manually-crafted `context_store` call could then accept. The risk is confined to operator-controlled TOML config (not user input), and the worst-case is an oddly-formatted category in the allowlist — no escalation path.
- **Recommendation**: Apply the same character validation (`[a-z0-9_-]{1,64}`) to domain pack `categories` in `validate_config`, consistent with the existing `[knowledge] categories` validation. This closes R-10 more precisely than documented.
- **Blocking**: no

### Finding 2: `_registry` parameter in `parse_observation_rows` is unused (future risk marker)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/observation.rs:353`
- **Description**: `parse_observation_rows` accepts a `_registry: &DomainPackRegistry` parameter (underscore-prefixed to silence the unused warning). Per the architecture, all hook-path records hardcode `source_domain = "claude-code"` — the registry is intentionally unused on this path in W1-5. The comment at line 350–351 documents this explicitly ("available for future non-hook ingress paths"). This is not a current vulnerability. However, the underscore suppression means if a future ingress path is added that should call `registry.resolve_source_domain(event_type)` but a developer instead copies the existing call site without removing the underscore, the resolution will silently be bypassed and all records will receive `source_domain = "claude-code"` regardless of their actual origin — reproducing SEC-05 exactly as described in the risk strategy.
- **Recommendation**: Add a `#[allow(unused_variables)]` annotation with a comment explaining the W3-1 extension contract, or remove the underscore and suppress with `let _ = registry;` inside the function with a `// W3-1: see IR-01` comment. Either approach makes the intentional non-use more explicit and harder to accidentally maintain when the function is extended.
- **Blocking**: no

### Finding 3: Duplicate regex in `validate_config` (dual validation path)

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/infra/config.rs:1059`, `crates/unimatrix-observe/src/domain/mod.rs:206–213`
- **Description**: `validate_config` compiles `regex::Regex::new(r"^[a-z0-9_-]{1,64}$")` and also calls `validate_source_domain_format()` in `DomainPackRegistry::new()`. `validate_source_domain_format` is a manual char check (not regex). The two implementations agree semantically: both enforce length 1–64 and `[a-z0-9_-]` only. However, the config layer validates before registry construction, and the registry validates again. This is defense-in-depth, not a gap — no risk. The informational note is that `regex::Regex::new` uses `unwrap()` on a known-valid pattern; this is standard practice and safe.
- **Recommendation**: None required. The double validation is consistent defense-in-depth.
- **Blocking**: no

---

## OWASP Concerns Evaluated

| Concern | Assessment |
|---------|-----------|
| Injection (command, SQL, path) | No injection risk. SQL queries use parameterized binds (`?1`) throughout. No shell commands. JSON Pointer (`serde_json::Value::pointer`) is read-only navigation with no side effects (SEC-03). `claim_template` substitution (`format_claim`) is a string replace with no format specifiers or shell expansion. |
| Input validation | Strong. Payload size (64 KB raw bytes, pre-parse), JSON depth (≤ 10 levels, recursive with short-circuit), and `source_domain` format (regex at config time, manual check at registry construction) are all present and tested. `field_path` must start with `/` if non-empty, validated at startup. |
| Broken access control | No new access control surfaces. `DomainPackRegistry` has no runtime write path — only `load_from_config()` / `with_builtin_claude_code()` at startup. AC-08 is correctly enforced: no MCP tool can modify the registry after startup. |
| Deserialization | `domain_metrics_json` deserialization uses `serde_json::from_str(...).unwrap_or_else(|_| HashMap::new())` — malformed JSON degrades to empty map, never panics. `MetricVector` uses `#[serde(default)]` on `domain_metrics`. Both are safe. |
| Security misconfiguration | Startup failure on invalid domain pack config (FM-01) is strict and correct. An absent `[observation]` section defaults safely to empty (AC-03). `"unknown"` reserved domain is rejected. |
| Vulnerable components | No new external dependencies. `serde_json::Value::pointer` is an existing transitive dependency. `regex` crate is used at startup only (not hot path). |
| Data integrity | Schema v14 migration is idempotent: pre-checks `pragma_table_info` for `domain_metrics_json` column existence before `ALTER TABLE ADD COLUMN` (FM-05 addressed). |
| Hardcoded secrets | None found in any new or modified file. |

---

## Blast Radius Assessment

**Worst case if the fix has a subtle bug:**

The critical blast radius is silent retrospective regression. If any of the 21 rewritten detection rules has a subtle string comparison error (e.g., `"PreToolUse"` vs `"pre_tool_use"`), that rule silently produces no findings for real Claude Code sessions. There is no error, no panic, and no log entry — operators receive an empty findings set and conclude the session was clean. This is the R-02 scenario.

The second worst case is if `compute_universal()` lost its `source_domain == "claude-code"` guard. In that case, non-claude-code domain records would inflate metric counts. The guard is present and tested.

The `domain_metrics_json` NULL path for claude-code sessions is correct and tested — no data corruption risk there.

The migration is a pure additive `ALTER TABLE ADD COLUMN TEXT NULL` with idempotency via `pragma_table_info` pre-check. Worst case on partial migration: the column already exists, the pre-check detects it, and migration proceeds without double-applying. Downgrade from v14 to v13 would encounter an extra column; since all queries use named columns, this is safe.

---

## Regression Risk

**Medium-low.** The 25-file HookType enum removal is the highest-regression-risk element, and it is protected by the four-wave compilation gate discipline. Every detection rule and extraction rule has had its `source_domain == "claude-code"` guard added as the first filter. The test fixtures in `detection/agent.rs` have all been updated to supply both `event_type` and `source_domain`. The `compute_universal()` function correctly guards on `source_domain` before all metric accumulation.

The `_registry` parameter being unused means the `DomainPackRegistry` injection into `parse_observation_rows` is a no-op in W1-5 — but this does not affect current behavior since `source_domain = "claude-code"` is hardcoded for all hook-path records.

Category allowlist poisoning (Finding 1) is the only regression risk with external impact: a misconfigured domain pack category could cause `context_store` to accept entries with unusual category strings. This is operator-controlled and low-likelihood.

---

## PR Comments

- Posted 1 comment on PR #332 summarizing findings.
- Blocking findings: no

---

## Knowledge Stewardship

- Stored: nothing novel to store — the category validation asymmetry (Finding 1) is specific to this feature's config extension pattern. If domain pack configuration is extended in future features (W3-1, col-xxx) and the same gap recurs, it would warrant a lesson-learned entry. No recurrence yet.
