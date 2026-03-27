# Security Review: bugfix-421-security-reviewer

## Risk Level: low

## Summary

The fix is narrow and correctly scoped to `select_source_candidates` in `nli_detection_tick.rs`. It introduces `rand 0.9` (already present in the workspace lock file) to shuffle tier 1 and tier 2 candidate vectors before selection, and builds a `HashSet<u64>` of embedded IDs from `vector_index.contains()` calls to exclude no-embedding entries from candidate selection. No external inputs, trust boundaries, or privilege levels are touched. All data operated on is fully internal (DB-sourced `u64` entry IDs and metadata). OWASP risk surface is negligible for this change.

## Findings

### Finding 1: `vector_index.contains()` called synchronously inside async context
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/nli_detection_tick.rs:96-100`
- **Description**: The `embedded_ids` HashSet is built by calling `vector_index.contains(e.id)` (the synchronous `VectorIndex::contains` method, which acquires an internal `RwLock` read guard) inside a `.filter()` iterator within the async `run_graph_inference_tick` function. The existing comment at line 116 acknowledges this pattern ("VectorIndex::search and get_embedding are synchronous (internal RwLock, no Tokio I/O)"). This is consistent with how Phase 4 uses the same index synchronously. The Unimatrix knowledge base (lesson #3672) specifically flags that `embedded_ids` must be built in async context, _not_ inside the rayon closure — the fix correctly satisfies this constraint. No violation.
- **Recommendation**: No action required. The pattern is consistent with documented Phase 4 usage. If the RwLock contention becomes a concern at scale, it can be addressed as a separate performance issue, not a security one.
- **Blocking**: no

### Finding 2: Non-cryptographic PRNG used for shuffle
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/nli_detection_tick.rs:345-347`
- **Description**: `rand::rng()` in rand 0.9 returns the default OS-seeded thread-local RNG (ChaCha12). This is a cryptographically strong PRNG. The shuffle is used solely to break deterministic re-selection bias across ticks — not for security-sensitive operations such as token generation, session IDs, or secret derivation. The CSPRNG quality here is more than sufficient for the purpose; there is no downgrade risk.
- **Recommendation**: No change needed. `rand::rng()` is the correct rand 0.9 API (confirmed: `rand::thread_rng()` was removed in 0.9, and the knowledge base entry #3671 records this explicitly).
- **Blocking**: no

### Finding 3: Dual rand versions in dependency tree
- **Severity**: low
- **Location**: `Cargo.lock`
- **Description**: The workspace now carries both `rand 0.8.5` (pulled in by transitive dependencies) and `rand 0.9.2` (new direct dependency). This is a normal Cargo semver-compatibility situation — these are separate crates in the lock file and do not conflict. The new `rand 0.9` dependency is the only version directly depended on by `unimatrix-server`. `cargo audit` is not installed in this environment but no known CVEs exist for rand 0.8.5 or 0.9.2 as of the knowledge cutoff.
- **Recommendation**: No action required for this fix. A routine dependency audit during the next maintenance window will confirm clean status. Pin `rand = "0.9"` (already done in Cargo.toml) is correct practice.
- **Blocking**: no

### Finding 4: No input validation gap introduced
- **Severity**: informational
- **Location**: entire diff
- **Description**: The `embedded_ids` set is constructed from `u64` entry IDs fetched from internal DB state — no external user input enters this path. The `HashSet::contains` filter operates on integer keys only; no string parsing, path operations, or deserialization of untrusted data occurs. No injection surface (SQL, path traversal, command injection) is introduced or modified.
- **Recommendation**: None. Informational only.
- **Blocking**: no

### Finding 5: No hardcoded secrets
- **Severity**: informational
- **Location**: entire diff
- **Description**: Diff contains no API keys, tokens, passwords, or credentials.
- **Recommendation**: None.
- **Blocking**: no

## OWASP Checklist

| Concern | Assessment |
|---|---|
| Injection (SQL/command/path) | Not applicable — only internal integer IDs processed |
| Broken access control | Not applicable — no trust boundary touched |
| Security misconfiguration | Not applicable — no config surface changed |
| Vulnerable components | rand 0.9.2 — no known CVEs; dual-version situation is benign |
| Data integrity failures | Not applicable — no serialization/write path changed |
| Deserialization of untrusted data | Not applicable |
| Input validation | No new external inputs introduced |
| Hardcoded secrets | None present |

## Blast Radius Assessment

Worst case if the fix contains a subtle regression: all active entries happen to have embeddings (the common case), so `embedded_ids` == all active IDs. In this scenario the filtering has no effect and behavior reverts to pre-fix: deterministic tier 2 ordering by `created_at`. This is a no-harm degradation — the bug being fixed (permanent starvation of entries that later gain embeddings) would persist, but no data corruption, information disclosure, or privilege escalation can result.

If `vector_index.contains` produces a false negative (reports an entry has no embedding when it does), that entry is silently excluded from candidate selection for that tick. It will be reconsidered on subsequent ticks. No edge is incorrectly written; no data is corrupted.

If `vector_index.contains` produces a false positive (reports an embedding when none exists), that entry enters Phase 4 where `get_embedding` returns `None` and the entry is skipped (lines 124-133 — existing guard). The downstream path is already defended.

The shuffle using `rand::rng()` cannot produce duplicates (`SliceRandom::shuffle` is a permutation). The existing duplicate-guard at lines 167-170 remains untouched and provides a backstop.

## Regression Risk

**Low.** The tests have been correctly updated:

- Tests that previously asserted a specific deterministic order (e.g., `test_select_source_candidates_remainder_by_created_at`) have been correctly relaxed to assert set membership and length — appropriate since shuffle makes order non-deterministic.
- The isolated-entry priority invariant test (`test_select_source_candidates_isolated_second`) continues to assert that both isolated IDs appear in the result — valid because the cap (2) equals the number of isolated entries, so regardless of shuffle order both are included.
- `test_select_source_candidates_priority_ordering_combined` asserts that isolated entries occupy the first 3 positions as a set — valid since all 3 isolated entries fit within the cap of 5, guaranteeing tier 1 is fully drained before tier 2 entries appear. The shuffle within each tier does not affect inter-tier priority.
- Two new tests cover the RC-1 and RC-2 fix scenarios specifically.

One minor observation: `test_select_source_candidates_priority_ordering_combined` continues to assert `result[..3]` contains the isolated set. After shuffling, tier 1 entries are guaranteed to appear in positions 0..3 (since `tier1.len() == 3` and the chain is `tier1.iter().chain(tier2.iter())`), so the positional assertion is still correct. No regression.

## PR Comments
- Posted 1 comment on PR #422
- Blocking findings: no

## Knowledge Stewardship
- nothing novel to store — the rand 0.9 API change is already recorded as entry #3671, and the async-context constraint for `embedded_ids` is recorded as entries #3669 and #3672
