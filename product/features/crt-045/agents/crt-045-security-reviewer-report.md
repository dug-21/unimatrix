# Security Review: crt-045-security-reviewer

## Risk Level: low

## Summary

crt-045 is a single-file production change that wires `TypedGraphState::rebuild()` into
`EvalServiceLayer::from_profile()`. The eval harness is an internal offline tool with no
user-facing attack surface. The change introduces no new dependencies, no deserialization
of untrusted data, and no external write paths. One low-severity code-quality concern was
identified: error-type discrimination by substring match (`reason.contains("cycle")`) in
production code. No blocking findings. Accessor visibility is correctly bounded to
`pub(crate)`. No hardcoded secrets, injections, or access control violations found.

---

## Findings

### Finding 1 — Cycle detection by error string substring match

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/profile/layer.rs:201`
- **Description**: The production code in `from_profile()` branches on error type by calling
  `reason.contains("cycle")` on the stringified `StoreError::InvalidInput` message. The
  expected string `"supersession cycle detected"` is hard-coded in `typed_graph.rs:130` and
  the `Display` implementation for `StoreError::InvalidInput` formats as `"invalid input for
  'supersedes': supersession cycle detected"`, so the substring match is stable. However,
  this pattern ties error-type discrimination to a string literal rather than the error's
  structural identity. If `typed_graph.rs` ever changes the reason string (e.g., to
  "supersession cycle found"), the match silently fails and cycle errors fall into the
  generic warn branch — yielding the same `use_fallback=true` degraded-mode outcome, so
  correctness is preserved but the log message will be misleading. This is not a security
  concern (the eval tool is not attacker-reachable), but is a fragility that should be
  addressed in a follow-up: use a dedicated error variant (e.g., `StoreError::CycleDetected`)
  rather than substring matching.
- **Recommendation**: Introduce `StoreError::CycleDetected` as a dedicated variant in
  `unimatrix-store/src/error.rs` and match on it structurally in `from_profile()`. This
  is a follow-up item; the current behavior is correct (both branches produce `use_fallback=true`)
  and poses no security risk.
- **Blocking**: no

### Finding 2 — Snapshot database path is caller-supplied but read-only

- **Severity**: low (acknowledged in RISK-TEST-STRATEGY.md as SR-SEC-01)
- **Location**: `crates/unimatrix-server/src/eval/profile/layer.rs:96–116`
- **Description**: `from_profile()` opens a SQLite database at a path supplied by the eval
  runner (profile TOML via CLI). The live-DB path guard (`Step 1`) correctly rejects any
  path that canonicalizes to the active production database. The snapshot is opened via
  `SqlxStore::open_readonly()` (Step 5) with a `read_only(true)` pool (Step 4), providing
  two independent read-only enforcement layers. A malformed SQLite file at the snapshot path
  could trigger SQLite parser errors, but these are surfaced as `StoreError` and cause
  `from_profile()` to return `Err` with a clear message — not a panic, not information
  disclosure, not a write to the production DB. The blast radius of a crafted snapshot file
  is limited to aborting the eval run.
- **Recommendation**: No code change needed. The dual read-only enforcement and error
  propagation are adequate for this internal tool. Document the path-guard pattern in
  Unimatrix if not already present, for future eval features.
- **Blocking**: no

### Finding 3 — `write_pool_server()` in test fixture writes to snapshot DB

- **Severity**: low (test-only, no production impact)
- **Location**: `crates/unimatrix-server/src/eval/profile/layer_graph_tests.rs:80–83, 175–181`
- **Description**: The integration test fixture seeds the snapshot database using
  `store.write_pool_server()` for raw SQL INSERTs into `graph_edges` and UPDATE of
  `entries.supersedes`. This is correct and necessary to seed graph edges that cannot be
  inserted through the public store API. The method name `write_pool_server` could suggest
  concern, but this is a `#[cfg(test)]` block and writes to a temp-dir database — not the
  live production DB. The live-DB guard in `from_profile()` would catch any accidental
  collision. No production code path uses `write_pool_server` from test fixtures.
- **Recommendation**: No change needed.
- **Blocking**: no

---

## OWASP Checklist

| Control | Assessment |
|---------|-----------|
| A01 Broken Access Control | Not applicable. `typed_graph_handle()` is `pub(crate)` — verified in source. No external promotion. |
| A02 Cryptographic Failures | Not applicable. No new cryptographic operations. |
| A03 Injection | Not applicable. Graph rebuild reads from SQLite via sqlx parameterized queries only. No user-supplied query strings in the new code paths. |
| A04 Insecure Design | Not applicable. Post-construction write pattern is sound (Arc clone chain verified at `services/mod.rs:419`). |
| A05 Security Misconfiguration | Not applicable. Snapshot opened as read-only at both pool and store layers. |
| A06 Vulnerable Components | Not applicable. No new dependencies introduced. Cargo.toml and Cargo.lock unchanged. |
| A07 Auth / Identification | Not applicable. Eval harness is an internal offline tool, not network-accessible. |
| A08 Data Integrity Failures | Not applicable. The write-back swaps pre-built graph state; it does not write to the snapshot DB or the production DB. |
| A09 Logging Failures | Acceptable. Errors in `tracing::warn!` include the error message (`error = %e`), not internal state or secrets. Cycle detection branch omits the error object (by design — error message is redundant when reason is known). |
| A10 SSRF | Not applicable. No outbound HTTP or network calls introduced. |

---

## Blast Radius Assessment

**Worst case**: `TypedGraphState::rebuild()` hangs indefinitely on a corrupted `GRAPH_EDGES`
table (R-07 from RISK-TEST-STRATEGY.md). The RISK-TEST-STRATEGY.md correctly identifies this
as an accepted residual risk: sqlx query timeout provides an implicit guard, and an explicit
`tokio::time::timeout` is deferred. Impact: `from_profile()` hangs, eval run produces no
output. Blast radius is strictly bounded to the offline eval tool — no production server,
no production database, no user data.

**Typical bad case**: Snapshot database path resolves to the live DB (bypassing the path
guard via e.g. a symlink). The live-DB path guard compares canonicalized paths and returns
`Err(EvalError::LiveDbPath{..})` before any database access occurs. The snapshot pool and
store are both read-only in any case.

**Subtle regression case**: Post-construction write-back fails to propagate to `SearchService`
if the Arc chain is broken by a future `with_rate_config()` refactor. Behavioral impact:
eval produces bit-identical results across profiles (the pre-crt-045 bug returns silently).
Not a security risk; a correctness regression. ADR-001 documents this fragility explicitly.

No data corruption, privilege escalation, or information disclosure paths were identified.

---

## Regression Risk

**Low.** The change is additive: a new `.await` call in `from_profile()` followed by a
conditional write-back. The zero-graph path (no edges in snapshot) degrades to `use_fallback=true`
and is functionally identical to the pre-fix state. Existing tests in `layer_tests.rs` and
`eval/profile/tests.rs` exercise non-graph profiles and must pass unchanged (AC-07, AC-08).

The `ppr-expander-enabled.toml` fix (setting `distribution_change = false`) removes a parse
failure that blocked the eval run before any new code was reached. This is a config-only
regression fix with no code-level behavior change in the runner.

No changes to `SearchService`, `ServiceLayer::with_rate_config()`, `TypedGraphState::rebuild()`,
`ScenarioResult`, `ProfileResult`, or any runner/report types. Regression surface is narrow.

---

## Dependency Safety

No new crate dependencies introduced. `Cargo.toml` and `Cargo.lock` are unchanged on this
branch. All existing crates (`tokio`, `sqlx`, `tracing`, `tempfile`) are pre-existing
workspace dependencies with no new version pins required.

---

## Secrets Scan

No hardcoded secrets, API keys, credentials, tokens, or private keys found in any changed
file. The TOML file contains only numeric metric thresholds (`mrr_floor`, `p_at_5_min`),
which are public eval gate values.

---

## PR Comments

- Posted 1 comment on PR #507 (findings summary + cycle detection recommendation).
- Blocking findings: no

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for eval harness security patterns — found entry #4085
  (eval ground truth must be pinned to snapshot — confirms live-DB guard is correct and
  necessary). Entry #4097 (Arc::clone post-construction write pattern — directly confirms
  SR-01 resolution is architecturally sound). Entries were helpful.
- Stored: nothing novel to store. The `reason.contains("cycle")` anti-pattern is
  feature-specific to this one call site and the recommended fix (dedicated error variant)
  is already established Rust practice. The eval security boundary observations (read-only
  snapshot, path guard) are feature-specific. No cross-feature generalization warrants a
  new lesson-learned entry — the snapshot-must-be-read-only constraint is already covered
  by entry #4085.
