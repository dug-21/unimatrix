# Security Review: nan-007-security-reviewer

## Risk Level: low

## Summary

The nan-007 evaluation harness PR introduces approximately 3,200 lines of new Rust across
`crates/unimatrix-server/src/eval/` and `crates/unimatrix-store/src/db.rs`, plus two new
Python test-harness clients. All identified security requirements from the risk register
(R-06, R-13, R-14, SR-07, SR-08, SR-09, SR-10) were implemented and verified. One
medium-severity concern (string-interpolated SQL in `output.rs`) is present but bounded by
a closed enum; it does not constitute an exploitable injection vector. No blocking findings.

---

## Findings

### Finding 1: SQL built via string interpolation in `scenarios/output.rs` (lines 97–113)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/scenarios/output.rs:97-113`
- **Description**: The `source` filter and `LIMIT` clause are concatenated into the SQL
  string rather than bound via parameterized queries. The code comment correctly notes this
  is safe because `ScenarioSource::to_sql_filter()` only returns static `&'static str`
  literals (`"mcp"` or `"uds"`) derived from a closed `clap::ValueEnum` — no user-supplied
  string reaches the SQL. The `limit` value is a `usize` cast from clap-parsed input, so it
  is an integer, not a string. The query runs against a read-only snapshot opened with
  `SqliteConnectOptions::read_only(true)`.

  The pattern is safe as written, but is an anti-pattern. A future maintainer who extends
  `ScenarioSource` to accept an arbitrary `--source <string>` argument or copies this
  pattern to a mutable connection would have a real injection vulnerability. The comment
  documents the safety invariant but nothing in the type system enforces it.

- **Recommendation**: For defence-in-depth, bind `source` as a parameterised value using
  sqlx's typed query API (`WHERE source = ?` with `.bind(source_str)`). This eliminates
  the category entirely rather than relying on the comment remaining accurate. The `LIMIT`
  clause can stay as an integer interpolation since `usize` has no injection surface.
  This is a quality improvement, not a blocker.
- **Blocking**: no

---

### Finding 2: Snapshot `open_readonly` fallback to read-write connection in `snapshot.rs`

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/snapshot.rs:104-118`
- **Description**: `do_snapshot` attempts to open the source database read-only, and if
  that fails (e.g., sqlx rejects `VACUUM INTO` on a strict read-only connection), it falls
  back to a read-write connection on the same source database. The comment acknowledges this
  and states that the path guard is the actual security boundary. This reasoning is correct
  — `VACUUM INTO` writes to the output path, not the source pool — but the fallback
  silently degrades the defense-in-depth layer without any log output at warning level.

  In a scenario where the daemon is actively writing and the read-only open fails for any
  other reason (permissions configuration, SQLite version incompatibility), the fallback
  proceeds without notifying the operator that the read-only constraint was dropped.

- **Recommendation**: Emit a `tracing::warn!` or `eprintln!` when the read-only open fails
  and the fallback is taken. The security property is not weakened (the path guard is the
  actual barrier), but silent degradation of defense-in-depth deserves observability.
- **Blocking**: no

---

### Finding 3: Markdown injection through profile names and query text in `report/render.rs`

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/report/render.rs:70-80` (and throughout
  `render.rs`)
- **Description**: The Markdown report interpolates `stat.profile_name`, `record.query`,
  and entry IDs directly into Markdown table cells and headings without escaping. A profile
  name containing `|` characters would break the Markdown table; a profile name containing
  backticks, `**`, or heading markers (`#`) would produce unintended formatting.

  The blast radius is cosmetic only: the report is a human-reviewed artifact with no
  automated pass/fail gate (FR-29, C-07). No code execution or data loss is possible.
  Profile TOMLs are developer-authored artifacts. This is a low-severity cosmetic concern.

- **Recommendation**: Escape `|` as `\|` in profile names and query text when they are
  inserted into Markdown table cells. This is a quality improvement that prevents misleading
  report layouts for edge-case input.
- **Blocking**: no

---

### Finding 4: `_recv` in `hook_client.py` resets timeout to `None` in `finally` block

- **Severity**: low
- **Location**: `product/test/infra-001/harness/hook_client.py:218-221`
- **Description**: The `_recv` method sets `self._sock.settimeout(None)` in the `finally`
  block regardless of whether the read succeeded, timed out, or raised `HookTimeoutError`.
  After `HookTimeoutError` is raised, the socket remains in blocking (no-timeout) mode. A
  subsequent `_send` or `_recv` call on the same client would block indefinitely, hanging
  the test. This is not a security vulnerability but is a resource/reliability concern in
  the test harness.

  For the AC-13 scenario (session_start → session_stop), if the first call times out, the
  second call hangs the test runner.

- **Recommendation**: Set the socket back to `self._timeout` rather than `None` in the
  `finally` block, or reset to `None` only on success paths. This is a test-harness
  reliability issue, not a security issue.
- **Blocking**: no

---

## Blast Radius Assessment

The worst-case scenario for a subtle bug in this change set is `VACUUM INTO` writing to
the active database rather than the snapshot path. The path guard in `run_snapshot` uses
`std::fs::canonicalize` on both paths before comparison, and the symlink guard test
(`test_snapshot_path_guard_symlink`) explicitly covers the symlink bypass vector (R-06).
The code correctly handles the new-file case via `canonicalize_or_parent`. Both guards
exist independently in `snapshot.rs` and `eval/scenarios/output.rs`, so both commands
are covered.

The second-worst case is `eval run` writing analytics into the live database via the
drain task. The `SqlxStore::open_readonly` implementation drops the analytics receiver
immediately, making all `enqueue_analytics` calls no-ops. `EvalServiceLayer::from_profile`
stores `AnalyticsMode::Suppressed` and never wires an analytics channel. These two
independent mechanisms ensure no eval replay contaminates the live database.

The `test-support` feature being exposed on `unimatrix-engine` in production builds
(ADR-003) does not introduce runtime risk. `kendall_tau()` and its siblings are pure
computation functions with no I/O or FFI. The concern is solely about feature flag
management, which is mitigated by the doc-comment on the Cargo.toml dependency line.

---

## Regression Risk

**Existing functionality**: The only change to existing crates is `unimatrix-store/src/db.rs`
(new `open_readonly` method, additive), `unimatrix-server/src/lib.rs` (two new `pub mod`
declarations), `unimatrix-server/Cargo.toml` (adding `test-support` feature to
`unimatrix-engine`), and whitespace/formatting changes in `infra/config.rs` tests.

None of these touch any existing function signatures or data flow. The `test-support`
feature addition is the highest-risk Cargo.toml change — if `unimatrix-engine`'s
`test-support` feature gates any code behind `#[cfg(feature = "test-support")]` that has
side effects at link or load time, enabling it in the production binary could introduce
issues. From reading the architecture (ADR-003), these are pure computation functions
with no I/O or initialization. This is low regression risk.

The new `Command::Snapshot` and `Command::Eval` arms in `main.rs` are additive. The clap
dispatch ordering (C-10) is preserved — both new arms are handled in the sync dispatch
block before the tokio runtime. No existing arm ordering changes.

---

## Dependency Safety

No new external crate dependencies are introduced. `unimatrix-engine` is an internal
workspace crate. `toml` (used in `profile/validation.rs`) was already a dependency in
the workspace. No dependency version bumps. The existing rmcp pin to `=0.16.0` (SR-01)
is unchanged.

---

## Secrets Check

No hardcoded credentials, API keys, tokens, or secrets are present in the diff. The
snapshot content-sensitivity warning (NFR-07, C-12) is documented in the Snapshot command
help text and source comments. The `--anonymize` flag was explicitly descoped.

---

## OWASP Checklist

| Concern | Assessment |
|---------|-----------|
| SQL Injection | Low — `source_clause` string interpolation bounded by closed enum; `LIMIT` is `usize`. No parameterized query, but no exploitable injection surface. |
| Path Traversal | Mitigated — `canonicalize`-based path guard in snapshot and eval commands; symlink resolution tested. |
| Command Injection | N/A — no shell invocations introduced. |
| Broken Access Control | N/A — eval commands operate offline against developer snapshots; no privilege boundary crossed. |
| Deserialization | Low — `serde_json::from_str` on JSONL and `toml::from_str` on profiles; both are from developer-owned files. Profile TOML model path validation at construction time (SR-09). |
| Input Validation | Present — `k >= 1` check, profile name collision detection, weight sum invariant, socket path length (103 bytes), payload size guard (1 MiB). |
| Security Misconfiguration | Low — `test-support` feature enabled on production binary is an architectural choice documented in ADR-003, not a misconfiguration. |
| Sensitive Data Exposure | Low — snapshot command help text warns that snapshots contain all operational data including query history and session IDs. No programmatic mitigation (descoped by design). |
| Known Vulnerabilities | N/A — no new external dependencies introduced. |

---

## PR Comments

- Posted 1 comment on PR #322 (findings summary, no blocking items).
- Blocking findings: no

---

## Knowledge Stewardship

- Nothing novel to store — the string-interpolated SQL pattern (Finding 1) is already
  captured in existing conventions as an anti-pattern for query construction. The eval
  harness's use of a closed enum to bound the interpolation is a one-off context that
  does not generalise into a reusable lesson.
