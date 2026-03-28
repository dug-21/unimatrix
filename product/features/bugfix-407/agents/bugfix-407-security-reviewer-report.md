# Security Review: bugfix-407-security-reviewer

## Risk Level: low

## Summary

PR #431 replaces 7 byte-index string slice sites (`&s[..N]`) with char-safe
truncation (`s.chars().take(N).collect()`) in a local CLI eval report harness.
No MCP network path, no database writes, no trust boundaries, and no external
inputs are touched. The fix is minimal, correct, and has no security
implications beyond eliminating a panic on malformed (multi-byte UTF-8) input.
No blocking findings.

---

## Findings

### Finding 1: Double chars() traversal in aggregate/mod.rs (minor inefficiency, not a security concern)

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/eval/report/aggregate/mod.rs` — the
  `compute_cc_at_k_scenario_rows` function
- **Description**: The fix traverses `result.query` with `chars()` twice: once
  to build `truncated` (`chars().take(60).collect()`) and once to evaluate the
  condition (`chars().count() > 60`). For very long strings this doubles the
  iteration. In a display-only local harness with bounded input this is
  negligible. It is not a security issue but is noted for completeness.
  The other 6 sites in `render.rs` do not have the condition check and are
  single-traversal — they simply collect and use the truncated string directly,
  which is the more efficient pattern.
- **Recommendation**: Replace the two-traversal pattern with a single
  `chars().take(60).collect()` + check `truncated.chars().count() == 60`
  (or compare character counts once). Not blocking — a follow-up improvement
  only.
- **Blocking**: no

### Finding 2: Test coverage for render.rs truncation sites (existing gap, not introduced by this fix)

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/eval/report/tests_distribution.rs`
- **Description**: The new test `test_cc_at_k_scenario_rows_unicode_query_no_panic`
  covers the aggregate/mod.rs truncation site. The six truncation sites in
  `render.rs` (two title truncations in Section 2, two query truncations in
  Section 7 improvement/degradation rows, two title truncations in Section 4
  entry analysis) do not have dedicated unicode-boundary tests. The render.rs
  sites use `chars().take(N).collect()` directly without a guard condition, so
  they cannot panic (correct), but if the pattern drifted in a future edit,
  there is no test to catch it. This is a pre-existing gap in test coverage
  for the render functions, not introduced by this fix.
- **Recommendation**: Low-priority follow-up: add a render-level unicode test.
  Not blocking.
- **Blocking**: no

---

## OWASP Assessment

| Concern | Verdict | Rationale |
|---------|---------|-----------|
| Injection (path traversal, shell, SQL, format string) | Not applicable | The truncated strings are written into Markdown table cells. No shell execution, no SQL, no path construction uses these values. |
| Broken access control | Not applicable | No access control paths changed. |
| Security misconfiguration | Not applicable | No configuration, env vars, or feature flags changed. |
| Deserialization of untrusted data | Not applicable | Deserialization paths not touched. |
| Input validation gaps | No gap introduced | The fix removes a panic on multi-byte input; it does not bypass any validation. The eval harness reads local JSON result files — inputs are developer-controlled, not user-supplied over a network. |
| Vulnerable components | Not applicable | No new dependencies added. Cargo.toml and Cargo.lock are unchanged. |
| Hardcoded secrets | None | Diff contains no keys, tokens, or credentials. |
| Unsafe code | None | No `unsafe` blocks introduced. Gate report confirms grep clean. |

---

## Blast Radius Assessment

Worst case if the fix has a subtle bug:

- The fix is in display-only Markdown report generation for a local CLI eval harness.
- Affected output: the `query` and `title` cells in eval report tables.
- Worst case regression: a truncated string is one character longer or shorter than intended, or the ellipsis is appended when it should not be (or vice versa). Report tables would show slightly incorrect display lengths.
- Data corruption risk: none — no database writes, no stored state, no network output.
- Denial of service risk: none — the old code panicked; the new code is strictly safer.
- Information disclosure risk: none — this is a local report file, not a network response.
- Privilege escalation risk: none.

The blast radius is bounded entirely to cosmetic display formatting in a local Markdown file.

---

## Regression Risk

- The 7 changed sites are display-only. All other behavior (metrics, deltas, sorting, scenario selection) is unchanged.
- The new `truncated` variable in aggregate/mod.rs is used in both the `then` and `else` branches, eliminating the original `result.query.clone()` in the `else` branch. This is functionally equivalent: a string <= 60 chars that has been through `chars().take(60).collect()` is identical to the original (all chars are preserved; no truncation occurs). No regression.
- The `render.rs` fixes drop the intermediate `title_len` / `query_len` variable and use `chars().take(N).collect()` directly. Functionally equivalent for ASCII and strictly safer for multi-byte input. No regression.
- 3842 tests passed (confirmed by gate report). No regressions detected.

---

## Site Coverage Verification

The gate report claims 7 sites. Verified against the diff:

| # | File | Location | Pattern removed | Pattern added |
|---|------|----------|-----------------|---------------|
| 1 | aggregate/mod.rs | `compute_cc_at_k_scenario_rows` | `&result.query[..60]` | `chars().take(60).collect()` |
| 2 | render.rs | Section 2 baseline entry | `&e.title[..title_len]` | `chars().take(30).collect()` |
| 3 | render.rs | Section 2 candidate entry | `&e.title[..title_len]` | `chars().take(30).collect()` |
| 4 | render.rs | Section 7 improvement rows | `&row.query[..query_len]` | `chars().take(40).collect()` |
| 5 | render.rs | Section 7 degradation rows | `&row.query[..query_len]` | `chars().take(40).collect()` |
| 6 | render.rs | Section 4 promoted entries | `&title[..title_len]` | `chars().take(40).collect()` |
| 7 | render.rs | Section 4 demoted entries | `&title[..title_len]` | `chars().take(40).collect()` |

All 7 sites confirmed replaced. No byte-index slicing patterns remain in the changed files or in the untouched render helpers (render_distribution_gate.rs, render_phase.rs, render_zero_regression.rs — all clean).

---

## Ellipsis Guard Logic Verification (aggregate/mod.rs)

The specific pattern used is:

```rust
let truncated: String = result.query.chars().take(60).collect();
let query = if result.query.chars().count() > 60 {
    format!("{}…", truncated)
} else {
    truncated
};
```

Logic is correct:
- `chars().take(60).collect()` always produces a valid UTF-8 string of at most 60 Unicode scalar values.
- The condition `chars().count() > 60` correctly tests whether the original query exceeded 60 characters (not bytes), and only then appends the ellipsis to `truncated`.
- When `chars().count() <= 60`, `truncated` equals the original string (take(N) on a string with N-or-fewer chars is identity), so the `else` branch is equivalent to the removed `result.query.clone()`.
- The double traversal is inefficient but not incorrect.

---

## PR Comments

- Posted 1 comment on PR #431 (see below)
- Blocking findings: no

---

## Knowledge Stewardship

- Stored: nothing novel to store — this is a well-known Rust pattern (byte vs char indexing). The investigator's lesson ("byte-index slicing `&s[..N]` is not char-safe; use `chars().take(N).collect()`") should be stored by the Bugfix Leader via `/uni-store-lesson`. No new security anti-pattern discovered.
