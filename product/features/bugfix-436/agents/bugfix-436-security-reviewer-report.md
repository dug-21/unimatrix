# Security Review: bugfix-436-security-reviewer

## Risk Level: low

## Summary

PR #440 removes two stale categories (`duties`, `reference`) from `INITIAL_CATEGORIES` and
eliminates a duplicate constant that required manual lockstep updates. The change is purely
additive-restrictive: it tightens the allowlist (two fewer valid values) rather than expanding
it. There are no new inputs from external sources, no deserialization changes, no new
dependencies, and no secrets. One pre-existing inconsistency (`outcome` still in
`builtin_claude_code_pack`) is correctly called out as out-of-scope; it carries no security
consequence. Two informational findings are noted; neither is blocking.

---

## Findings

### Finding 1: README example config still lists retired categories
- **Severity**: low / informational
- **Location**: `README.md:244`
- **Description**: The README shows a sample `[knowledge]` config block that includes
  `"duties"` and `"reference"` in the `categories` list. An operator who copies this example
  verbatim will get two extra categories beyond the new defaults. This is not dangerous —
  the categories are accepted without error because `from_categories()` does not validate
  against `INITIAL_CATEGORIES`; arbitrary strings are accepted. But it creates silent drift:
  operators think they are using defaults when they are not, and newly ingested entries
  tagged `duties` or `reference` will succeed where they should now fail.
- **Recommendation**: Update `README.md:241-244` to remove `"duties"` and `"reference"` from
  the sample config (or update the comment from "7-category list" to "5-category list").
- **Blocking**: no

### Finding 2: builtin_claude_code_pack still includes `outcome` (pre-existing, noted for completeness)
- **Severity**: low / informational
- **Location**: `crates/unimatrix-observe/src/domain/mod.rs:56`
- **Description**: `builtin_claude_code_pack()` still lists `"outcome"` in its `categories`
  vec. Per ADR-005 (crt-025), `outcome` was already retired from `INITIAL_CATEGORIES`. The
  gate report acknowledges this as pre-existing and out of scope. It does not affect the
  correctness of this fix because the domain-pack categories field contributes to the
  `CategoryAllowlist` via `add_category()` at startup (main.rs line 567), but `outcome` is
  not in `INITIAL_CATEGORIES`, so it can only be injected if the domain pack registers it.
  In the current startup wiring, domain-pack categories are added to the allowlist after
  the initial `from_categories()` call, meaning `outcome` would become valid again at
  runtime if the claude-code pack still lists it. This should be tracked.
- **Recommendation**: Verify whether `outcome` in `builtin_claude_code_pack` actually re-adds
  it to the live `CategoryAllowlist` at server startup, and if so, open a follow-up issue.
  This is a correctness concern for ADR-005, not a security vulnerability.
- **Blocking**: no

---

## OWASP Evaluation

| Check | Verdict |
|-------|---------|
| Injection | Not applicable — no format strings, shell commands, or SQL changes |
| Broken access control | Not applicable — no permission checks changed |
| Security misconfiguration | Low — README sample config now misleads operators (Finding 1) |
| Vulnerable components | Not applicable — no new dependencies |
| Data integrity failures | Not applicable — change tightens allowlist; no removal of existing data |
| Deserialization risks | Not applicable — no new deserialized inputs |
| Input validation gaps | Neutral — allowlist is made stricter; no validation removed |
| Secrets / credentials | None present in diff |
| Unsafe code | None — confirmed by gate report and independent review |

---

## Blast Radius Assessment

If this fix has a subtle bug, the worst realistic case is that new attempts to store
entries with category `duties` or `reference` are accepted where they should be rejected
(allowlist accidentally left permissive). This would be a usability regression, not a
security vulnerability. The failure mode is bounded: only the two retired category strings
are affected; the other five categories, all ingestion paths, all search paths, and all
existing data are untouched.

The structural fix (eliminating the duplicate constant) removes a future blast-radius
amplifier: previously, a developer updating one constant without the other would silently
produce divergent behavior between the config-time default and the runtime allowlist.
That divergence is now compile-time impossible.

---

## Regression Risk

Minimal. The only behavioral change visible to callers is:
- `CategoryAllowlist::validate("duties")` now returns `Err` instead of `Ok`.
- `CategoryAllowlist::validate("reference")` now returns `Err` instead of `Ok`.

Any caller that was previously storing entries under `duties` or `reference` will now
receive an `InvalidCategory` error. The integration tests confirm this is the intended
behavior. The gate report confirms 3846 unit tests and 84/84 integration tests pass.

Existing data in the SQLite store with `category = "duties"` or `category = "reference"` is
NOT deleted. Queries against existing entries are unaffected. Only new ingestion is blocked.
This matches the precedent set by the `outcome` retirement in ADR-005.

---

## Dependency Safety

No new crate dependencies introduced. `cargo audit` was not available in the environment
(pre-existing gap noted by gate report). All changed files are pure logic with no
dependency additions.

---

## Minimal Change Verification

All 7 changed files are directly on the required fix path. No unrelated changes detected.
The architectural cleanup (duplicate constant -> import) is on the required fix path per
ADR-003 (single source of truth), not a scope addition.

---

## PR Comments

- Posted 1 comment on PR #440 (non-blocking, informational)
- Blocking findings: no

---

## Knowledge Stewardship

- Stored: nothing novel to store — the pattern of "README doc samples not updated when
  constants change" is too project-specific to generalize into a lesson. The gate report
  correctly noted cargo audit unavailability; that gap is already tracked as a pre-existing
  environment issue.
