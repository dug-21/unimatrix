# C1 — Seed-Write Primitive Test Plan (`write_if_absent`, `write_default_config_if_absent` delegate)

> File: `infra/config.rs`. ADR-001 (#5235). Risks: **R-03 (Critical)**, R-09, R-10, R-11.
> ACs: **AC-05** (no-clobber). Tests live in `infra/config.rs` mod-tests, alongside the
> existing `write_default_config_if_absent` tests (config.rs:11262–11346).

## What this component is

Extract a content-parameterized no-clobber helper:
```rust
fn write_if_absent(path: &Path, content: &str); // parent-create + OpenOptions::create_new(true)
                                                 // + AlreadyExists-is-noop + warn-and-continue
```
`write_default_config_if_absent(path, force=false)` delegates to it with `DEFAULT_CONFIG_TOML`.
**Critical delivery note**: the existing `force=true` arm uses `fs::write` (overwrite, config.rs:4858);
the `force=false` arm uses `create_new` (4873). `write_if_absent` IS the `force=false` body. The
`force=true` overwrite path is NOT a seed and keeps its semantics — do not route it through
`write_if_absent`.

## Unit tests

### R-03 / AC-05 — no-clobber is atomic (the core proof)
- `test_write_if_absent_creates_file_when_absent_returns_content`
  Arrange: temp dir, non-existent path. Act: `write_if_absent(path, "hello")`. Assert: file exists,
  content == `"hello"` byte-for-byte.
- `test_write_if_absent_does_not_overwrite_existing_file`
  Arrange: pre-write `"operator content\n"` to path. Act: `write_if_absent(path, "SEED BODY")`.
  Assert: content == `"operator content\n"` byte-for-byte (the seed body never written).
- `test_write_if_absent_already_exists_is_silent_noop`
  Arrange: pre-write a file. Act: `write_if_absent` again. Assert: no panic, function returns `()`,
  content unchanged, **and mtime unchanged** (no truncate/rewrite touched the inode). This is the
  `AlreadyExists`-is-a-no-op contract (R-03 scenario 4).
- `test_write_if_absent_idempotent_second_call_no_change`
  Act: call twice with different bodies. Assert: first body wins, second is a no-op (content + mtime
  unchanged after first write). Idempotency (R-03 scenario 3).

### R-10 — best-effort, never panics (C-09, NFR-07)
- `test_write_if_absent_swallows_write_failure_no_panic`
  Arrange: parent dir chmod `0o555` (read-only), as the existing
  `test_write_default_config_succeeds_even_if_write_fails` does. Act: `write_if_absent` into it.
  Assert: returns without panic; no `.unwrap()` reached. Restore perms for cleanup.

### Parent-directory creation
- `test_write_if_absent_creates_missing_parent_dirs`
  Arrange: path = `tmp/a/b/c/config.toml` where `a/b/c` does not exist. Act: `write_if_absent`.
  Assert: file created (parents created first).

## TOCTOU / structural assertion (R-03 scenario 5 — note for delivery + 3c verification)

There must be **no `path.exists()` precheck** gating the `write_if_absent` write — the `create_new(true)`
open IS the existence guard (one syscall, no TOCTOU window). This is verified structurally at Stage 3c
(grep the helper body for an `.exists()` precheck; assert absent) — it is a code-shape invariant, not a
runtime assertion. Document it in the RISK-COVERAGE-REPORT as a structural check.

## R-09 / SR-08 — regression ripple (existing tests must stay green)

The four pre-existing tests at config.rs:11262–11346 MUST pass unchanged after the extraction:
- `test_write_default_config_creates_file_when_absent`
- `test_write_default_config_does_not_overwrite_without_force`
- `test_write_default_config_overwrites_with_force` ← exercises the `force=true` `fs::write` arm; confirm
  the extraction did not route force through the no-clobber helper.
- `test_write_default_config_succeeds_even_if_write_fails`

Add one delegation proof:
- `test_write_default_config_delegates_to_write_if_absent_for_force_false`
  Assert (behaviorally): `write_default_config_if_absent(path, false)` on a pre-existing file leaves it
  unchanged (same no-clobber semantics as `write_if_absent`) — proves delegation, not a forked copy.

## Coverage requirement (from RISK-TEST-STRATEGY R-03)

Byte-for-byte survival of pre-placed content; the no-clobber primitive is `create_new`-based and
single-sourced (one `write_if_absent`); `AlreadyExists` is a silent no-op; no `path.exists()` precheck;
best-effort failure swallowed with no panic. All four legacy tests stay green.

## Notes for C3 / C4 (consumers)

C3 (per-slug seed) and C4 (global seed) both route their writes through this primitive. Their
end-to-end byte-survival proofs (AC-05 on real (a) and (b) paths) live in their own plans, but they
depend on C1's no-clobber guarantee holding here first.
