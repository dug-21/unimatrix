# Security Review: bugfix-381-security-reviewer

## Risk Level: low

## Summary

The fix adds structured debug-level log statements to the UDS dispatch path and corrects the tracing-subscriber filter initialization to honour `RUST_LOG`. Both changes are purely observability additions with no mutations to data, access control, or trust boundaries. No blocking security concerns were found. One low-severity information-disclosure note and one low-severity log-injection note are documented below.

---

## Findings

### Finding 1: LP-2 entries field allocates titles at debug call site
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:1273`
- **Description**: The `entries` field in the LP-2 `tracing::debug!` call evaluates a full iterator map — calling `truncate_at_utf8_boundary` on each entry title — before the tracing filter can suppress the line. In Rust's tracing crate, structured field expressions are evaluated unconditionally at the call site (they are not lazy unless wrapped in a closure). This means one heap allocation per injection event even when `unimatrix_server::obs` is filtered off. At O(k=5) default injection depth on a non-hot-path this is negligible, but it is a minor inconsistency with the design rationale. This was acknowledged in the design review report (Finding 1) and accepted. Not a security concern, but noted for completeness.
- **Recommendation**: If this path ever becomes hot, wrap the `entries` field in a `tracing::field::display(...)` closure or switch to a lazy field format. No action required now.
- **Blocking**: no

### Finding 2: query_preview content in log file
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:954`, `listener.rs:1256`
- **Description**: LP-3 (SubagentStart received, line 954) and LP-1 (ContextSearch executed, line 1256) log a `query_preview` field truncated to 120 bytes via `truncate_at_utf8_boundary`. This is user-controlled input (the agent's prompt text). The design reviewer explicitly raised and addressed this (design-reviewer-report.md, Finding 6): the field was renamed `query_preview` and bounded to 120 bytes. The data is already persisted in the `QUERY_LOG` and `OBSERVATION` SQLite tables with full content. The log file is no more sensitive than the database. The truncation bound is appropriate and consistent with the existing `goal_preview` field (50 bytes) pattern already in the file.

  One nuance: `RUST_LOG` now being respected means an operator could inadvertently enable `debug` level globally (`RUST_LOG=debug`) and see these previews in log aggregation tooling. The fix does not introduce this risk — the data was always reachable at `--verbose` — but the new `RUST_LOG` mechanism makes it easier to enable at runtime without a redeploy. This is by design and is the stated purpose of the fix.
- **Recommendation**: No action required. The 120-byte truncation and `query_preview` field naming are adequate mitigations. Document in operator runbook that `RUST_LOG=debug` will include query previews in log output.
- **Blocking**: no

### Finding 3: Log injection via entry titles
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:1273`
- **Description**: The `entries` field in LP-2 logs entry titles sourced from the `EntryRecord.title` field. These are not user-controlled at UDS dispatch time — they are titles of knowledge entries already stored in the Unimatrix database by project team members via MCP tools. Entry titles go through the store write path (validated and persisted) before reaching this log call; they are not echoed directly from UDS request payloads. No log injection risk from this field.
- **Recommendation**: None required.
- **Blocking**: no

### Finding 4: RUST_LOG=debug enables debug-level output globally (deliberate design)
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/main.rs:410,798,1194` (all three tokio_main_* sites)
- **Description**: The previous behaviour silently ignored `RUST_LOG`. The fix correctly makes `RUST_LOG` effective. A consequence is that `RUST_LOG=debug` will now enable debug-level output from all tracing subscribers in the process, including third-party crates (rmcp, tokio, etc.), potentially producing verbose output including internal request/response data from the MCP transport layer. This is expected behaviour for `debug` level and is not a regression from the pre-fix state (it was achievable via `--verbose`). The operator guide comment at each site correctly documents the silencing mechanism (`RUST_LOG=info,unimatrix_server::obs=off`).
- **Recommendation**: No code change needed. The three comments at each `tokio_main_*` site document the control mechanism. The operator runbook should note that `RUST_LOG=debug` is equivalent to `--verbose`.
- **Blocking**: no

---

## Blast Radius Assessment

The worst-case scenario if this fix has a subtle bug is a tracing initialization failure. In all three `tokio_main_*` functions, `.init()` is called on the tracing subscriber. If `EnvFilter::try_from_default_env()` panicked or returned an unusable filter, the subscriber init could fail. However: `try_from_default_env` returns `Result`, the `unwrap_or_else` fallback is unconditional (it cannot fail — `EnvFilter::new("info")` and `EnvFilter::new("debug")` are valid static strings known at compile time), and the logging calls themselves are pure side effects with no data mutation. If every log statement were silently dropped, the server would still function correctly — it would just be unobservable. Logging failures in Rust's tracing crate do not propagate as errors.

The `source` parameter threading through `handle_context_search` is the only structural change to existing code. The parameter is `Option<String>`, used only in the debug log call, and never mutated. If it were accidentally `None` where it should be `Some`, the log field would show `None` — a cosmetic observability gap, not a functional regression.

---

## Regression Risk

Low. The changes are:

1. `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level))` replaces a static string. When `RUST_LOG` is unset (the standard runtime environment for this server), the behaviour is byte-for-byte identical to before.

2. Four `tracing::debug!` calls added. All are pure reads of already-computed local variables. No mutations, no new async calls, no new fallible operations.

3. `source: Option<String>` added to `handle_context_search` signature. One call site updated. The parameter is forwarded from the outer match arm where it was already bound. This is a safe mechanical threading.

The existing test suite (2047 unit + 16+16 integration + smoke tests) passes at zero failures. The design reviewer confirmed the `tracing_test::traced_test` tests for the SubagentStart goal-present branch are unaffected by log level changes (those tests capture all levels).

---

## OWASP Scan Summary

| Category | Finding |
|----------|---------|
| Injection | No injection vectors introduced. Query and title text is logged via structured fields (`%` Display), not string interpolation. Structured logging does not allow format string injection. |
| Broken Access Control | No access control changes. The `uds_has_capability(Capability::Search)` check at line 929 is unchanged and still guards the entire dispatch arm. |
| Security Misconfiguration | `RUST_LOG` exposure is an intentional design outcome. Defaulting to `info` when `RUST_LOG` is unset is safe and correct. |
| Vulnerable Dependencies | No new dependencies introduced. Uses `tracing_subscriber::EnvFilter` already present in the dependency tree. |
| Data Integrity | No data mutation paths touched. All changes are read-only log operations. |
| Deserialization | No deserialization changes. The `HookRequest` deserialization path is unchanged. |
| Input Validation | Validated: `query_preview` uses `truncate_at_utf8_boundary(query, 120)` — bounded, Unicode-safe. `session_id` is already sanitized at line 936 via `sanitize_session_id` before reaching these log points. Entry titles are from the internal store, not from UDS input. |
| Secrets | No hardcoded credentials, tokens, or keys in the diff. |

---

## PR Comments

- Posted 1 comment on PR #387 (general findings)
- Blocking findings: no

---

## Knowledge Stewardship

- Stored: nothing novel to store — the design reviewer and investigator already captured the relevant anti-patterns (EnvFilter lesson #3453, obs target pattern #3457). The information-disclosure note on query_preview in logs is a context-specific observation that belongs in this PR comment, not in Unimatrix knowledge. No generalizable new anti-pattern emerged.
