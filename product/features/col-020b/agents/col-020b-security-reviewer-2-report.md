# Security Review: col-020b-security-reviewer-2

## Risk Level: low

## Summary
The changes are limited to type renames, serde annotation additions, a new private normalizer function, and debug tracing. No new external inputs, no new dependencies, no schema changes, no access control modifications. All data flows remain internal (trusted store data and Claude Code hook events). The change surface is minimal and well-tested.

## Findings

### Finding 1: Formatter-only changes in tools.rs inflate diff
- **Severity**: low
- **Location**: crates/unimatrix-server/src/mcp/tools.rs (throughout)
- **Description**: Approximately 60% of the tools.rs diff is rustfmt reformatting (line breaks on method chains, import reordering) with no behavioral change. While not a security concern, it makes the diff harder to audit and increases the chance of an unnoticed behavioral change hiding in formatting noise.
- **Recommendation**: Informational only. Verified by reading the full diff that no behavioral changes are hidden in the formatting.
- **Blocking**: no

### Finding 2: serde(alias) backward compatibility is unidirectional only
- **Severity**: low
- **Location**: crates/unimatrix-observe/src/types.rs (lines with `#[serde(alias = ...)]`)
- **Description**: The aliases allow deserialization of old field names into new struct fields, but serialization always uses the new names. If any downstream system or log parser expects old field names (`knowledge_in`, `knowledge_out`, `tier1_reuse_count`, `knowledge_reuse`), it will break. This is documented and accepted in ADR-003. The RISK-TEST-STRATEGY explicitly acknowledges this as R-13 (low severity).
- **Recommendation**: Accepted risk per ADR-003. Reports are ephemeral (not persisted long-term).
- **Blocking**: no

### Finding 3: Debug tracing does not leak sensitive data
- **Severity**: low
- **Location**: crates/unimatrix-server/src/mcp/tools.rs (lines with `tracing::debug!`)
- **Description**: Four debug log points added to `compute_knowledge_reuse_for_sessions`. They log counts only (session ID count, query_log record count, injection_log record count, delivery/cross-session counts). No entry content, no session IDs, no user data. Debug level means they are off by default.
- **Recommendation**: No action needed.
- **Blocking**: no

## Blast Radius Assessment

**Worst case**: If `normalize_tool_name` had a bug (e.g., incorrectly stripping prefixes from non-Unimatrix tools), tool classification would miscount categories in the retrospective report. Impact: incorrect statistics in an internal analytics report. No data corruption, no privilege escalation, no denial of service. The retrospective report is ephemeral and informational.

**Failure mode**: Safe. If the knowledge reuse computation fails, the error is caught and logged (`tracing::warn`), and the report field is set to `None`. The overall retrospective report still returns successfully.

## Regression Risk

1. **Serde backward compatibility**: Old JSON with `knowledge_in`/`knowledge_out`/`tier1_reuse_count` fields will deserialize correctly via aliases. Verified by dedicated tests. New JSON uses new field names. Risk: low.

2. **Semantic change in delivery_count**: Previously counted entries in 2+ sessions. Now counts all distinct delivered entries. This is an intentional fix. The old metric is preserved as `cross_session_count`. Existing tests updated, new regression tests added. Risk: low.

3. **New `curate` category in tool_distribution**: Additive. HashMap-based, no fixed schema. Consumers already handle arbitrary string keys. Risk: low.

4. **Re-export rename**: `KnowledgeReuse` to `FeatureKnowledgeReuse`. Compile-time checked. If missed anywhere, `cargo build` fails. Risk: none (compiler-enforced).

## Dependency Safety
- No new crate dependencies added (no Cargo.toml changes)
- No version bumps
- No new feature flags

## Secrets Check
- No hardcoded credentials, tokens, API keys, or secrets in the diff
- No `.env` file changes

## OWASP Assessment
- **Injection**: Not applicable. No shell commands, SQL, or format strings with untrusted input.
- **Access control**: Unchanged. All existing capability checks preserved.
- **Deserialization**: serde with aliases and defaults on trusted internal data. No deserialization of external/untrusted input.
- **Input validation**: `normalize_tool_name` operates on trusted tool names from Claude Code hooks. No external input surface.
- **Path traversal**: No file path operations added.
- **Data integrity**: Entry deduplication logic (HashSet-based) is correct. `delivery_count` uses `resolved_entries.len()` after category lookup, correctly excluding deleted entries.

## PR Comments
- Posted approval comment on PR #195
- Blocking findings: no
