# Security Review: vnc-048-security-reviewer

## Risk Level: low

## Summary
`--slug` on `export`/`import` resolves a filesystem path from an operator-supplied string, but path traversal is structurally closed: the raw slug crosses `ProjectSlug::try_from` (charset `^[a-z0-9][a-z0-9-]{0,62}$`) + reserved-slug refusal before any FS/DB access, and the single join site `per_slug_data_dir` accepts `&ProjectSlug` only. The destructive import path is gated pre-write by a force-proof live-PID refusal and a force-proof non-empty-audit refusal. No blocking findings.

## Findings

### F1 — Path traversal closed by construction
- **Severity**: informational (verified safe)
- **Location**: `projects/slug_store.rs:63-113`, `projects.rs:123-125,206-231`, `http/router/seam.rs:96-119`, `main.rs:580-604`
- **Description**: `--slug` (raw `String`) is forwarded untouched to the funnel. `resolve_slug_store` runs `validate_slug` (= `ProjectSlug::try_from` charset, then `is_reserved_slug`) as step 1, before base derivation, join, or existence check. The charset scan uses `is_ascii_lowercase()/is_ascii_digit()`, rejecting `.` `/` `\` `%` whitespace NUL uppercase and all multi-byte/non-ASCII bytes; length bound 1..=63 is exact (ASCII-only). Explicit reject tests cover `../x`, `..`, `%2e%2e`, `/abs`, `a/b`, `a\b`, `a\0b`, plus a zero-FS-side-effect assertion. Exactly one join site (`per_slug_data_dir(base, &ProjectSlug)`); a raw `&str` cannot compile through it. Both commands reach it only via the funnel; the sole other `per_slug_data_dir` call is a `#[cfg(test)]` seam.
- **Recommendation**: none.
- **Blocking**: no.

### F2 — Destructive import gates fire pre-write and are force-proof
- **Severity**: informational (verified safe)
- **Location**: `import/mod.rs:145-171,188,209-221,294-346,357-372`
- **Description**: Order is funnel existence-gate → `preflight_live_pid_refusal` (slug-mode, pre-`open`, no `force` param, keys on `kill -0` + `/proc` identity not file presence) → `SqlxStore::open` → `check_preflight` non-empty-audit refusal (slug-mode, no `force` param) → `drop_all_data`. `SqlxStore::open` (auto-creates+migrates) is reached only after `db_path.exists()` returned true in slug mode, so it never mints a store past the gate. `--force` bypasses only the intended entry-count check; the PID and audit refusals have no override. A first import's provenance audit row makes re-import self-refuse (ADR-005).
- **Recommendation**: none.
- **Blocking**: no.

### F3 — TOCTOU between existence gate and open
- **Severity**: low (informational)
- **Location**: `slug_store.rs:97` vs `export.rs:111` / `import/mod.rs:166`
- **Description**: A file removed between `db_path.exists()` and `SqlxStore::open` would let `open` auto-create an empty store. Benign in the local single-operator personal-cloud threat model; the existence gate is fail-loud UX, the security boundary is `validate_slug`.
- **Recommendation**: accept; document only.
- **Blocking**: no.

### F4 — Symlink following at resolved db_path
- **Severity**: low (informational)
- **Description**: `exists()`/`open` follow a symlink planted at `{base}/<slug>/unimatrix.db`. Requires prior local write access to the operator's own `.unimatrix` tree — out of threat model.
- **Blocking**: no.

## Blast Radius Assessment
Worst case is a wrong-store resolve: export of the wrong store = cross-project information disclosure; import into the wrong store = data loss. Mitigated by the single funnel (one base derivation, one join, one validation edge) plus the pre-open existence gate and fail-loud-with-resolved-absolute-path on any miss — the silent wrong-store resolve that motivated the feature cannot recur silently. A host bind-mount base mismatch surfaces as a named-path error, not a wrong-but-real store.

## Regression Risk
No-`--slug` path is byte-for-byte unchanged (`slug=None` never enters the funnel; `paths.db_path` path-hash flow preserved). Signature additions thread `None` at existing call sites. Visibility raises are `fn`→`pub(crate)` only (`per_slug_data_dir`, `validate_slug`); crate-internal, no widening to `pub`, join site still demands `&ProjectSlug`. SQL stays parameterized; existing SQL-injection tests retained and passing in the diff.

## Dependency & Secret Safety
No new dependencies (no `Cargo.toml`/`Cargo.lock` changes) — no new CVE surface. No hardcoded credentials, tokens, or keys in the diff. No `unwrap`/`expect` in production funnel code (uses `unwrap_or_else` fallback idiom).

## PR Comments
- Posted 1 review comment on PR #959 (state COMMENTED, 2026-07-19).
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store — the traversal-closure-by-typed-newtype and force-proof-pre-write-gate patterns are already the established vnc-04x slug discipline (ADR-001/003/005, pattern #4972, lesson #5507); no new generalizable anti-pattern surfaced in this PR.
