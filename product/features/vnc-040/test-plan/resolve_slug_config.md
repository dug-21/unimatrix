# Test Plan — `resolve_slug_config` (NEW helper, call-site module)

> Component: `fn resolve_slug_config(base_dir: &Path, slug: &ProjectSlug, global: &UnimatrixConfig)`
> `-> Result<Cow<'_, UnimatrixConfig>, ServerError>`. Single owner of the overlay decision:
> probe → (no-file ⇒ `Cow::Borrowed`) | (file ⇒ load → per-file validate → merge → POST-MERGE validate
> → `Cow::Owned`). Owns: **R-01 (AC-08b post-merge re-validation, Critical)**, R-03 (no-file arm half),
> R-10 (DoS/perm hardening), R-11 (slug-named, startup-fatal). Tests in the call-site crate's test
> module; file-present tests write a temp `{base_dir}/{slug}/config.toml`.

## Unit Test Expectations

### AC-08b / R-01 — Post-merge cross-field re-validation (CRITICAL, the highest-rated risk)

**Prereq:** the cross-field invariant enumeration in OVERVIEW §4 — one merged-only violation PER
invariant class. For each enumerated class write a pair:

`test_resolve_merged_violation_fails_loud_naming_slug__<invariant>`
- **Arrange:** a `global` and a per-slug `config.toml` EACH individually valid (each passes
  `validate_config` alone), whose MERGE violates the invariant (canonical: fusion-weight sum-of-six
  > 1.0 — global sets some weights non-default, per-slug sets OTHER weights non-default).
- **Act:** `resolve_slug_config(base_dir, slug, &global)`.
- **Assert:** returns `Err(ServerError::Config(msg))` at startup; `msg` NAMES the offending slug file;
  failure occurs BEFORE any value is consumed (fail-fast, not request-time).

`test_per_file_validation_alone_does_not_catch_merged_violation__<invariant>`
- **Assert:** `validate_config(&slug_file, &path)` (per-file) on the SAME slug file returns `Ok` —
  proving per-file validation is necessary-but-insufficient (#3905). This is the load-bearing negative:
  it demonstrates WHY the post-merge call exists.

`test_resolve_runs_post_merge_validate_inside_helper_after_merge`
- **Assert (construction proof, recorded C):** `validate_config(&merged, &path)` runs INSIDE
  `resolve_slug_config`, AFTER `merge_configs`, BEFORE return — not only the per-file call. Verified by
  the ordering observable in a passing merged-violation test (the error fires from the helper, post-merge).

`test_resolve_valid_merge_passes_no_false_positive`
- **Arrange:** a global+per-slug pair whose merged sums are all valid.
- **Assert:** returns `Ok(Cow::Owned(merged))`; no spurious startup failure.

### AC-02 / R-03 — No-file fallthrough (helper half; pointer half in `per_slug_loop`)

`test_resolve_no_file_returns_cow_borrowed_global_no_merge`
- **Arrange:** `{base_dir}/{slug}/config.toml` does NOT exist.
- **Act:** `resolve_slug_config(...)`.
- **Assert:** returns `Ok(Cow::Borrowed(&global))` — `matches!(result, Cow::Borrowed(_))`; the returned
  reference IS `&global` (same address). NO `merge_configs` runs, NO re-derivation. (The `Arc::ptr_eq`
  on the 3 handles is asserted in `per_slug_loop`, which clones outside the helper.)

`test_resolve_empty_file_merges_to_global_equivalent`
- **Arrange:** file present but EMPTY / all-default.
- **Assert:** merged == global value-equality across all overlayable keys (degenerate fallthrough; must
  not differ from the no-file semantics in served values, though this arm returns `Cow::Owned`).

### AC-08a / R-10 — DoS + permission hardening on the per-slug path (MANDATORY, not assumed)

`test_resolve_rejects_oversized_file_before_parse`
- **Arrange:** a per-slug `config.toml` > 64 KiB.
- **Assert:** rejected via `load_single_config`'s existing 64 KiB cap (#2395) BEFORE `toml::from_str`;
  `Err(ServerError::Config)` naming the slug; daemon refuses to start.

`#[cfg(unix)] test_resolve_rejects_world_or_group_writable_file`
- **Arrange:** a per-slug file with `mode() & 0o022 != 0` (world/group-writable).
- **Assert:** rejected at load via the existing `#[cfg(unix)]` 0o022 check; `Err` naming the slug at
  startup. (Trust-boundary: the per-slug file is a NEW untrusted input surface.)

### AC-08a / R-11 — Slug-named, startup-fatal error for every invalid class

`test_resolve_invalid_class_fails_loud_naming_slug__<class>` for each:
- malformed TOML → `Err(ServerError::Config)` naming the slug at startup.
- unknown `[knowledge].category` → per-file `validate_config` error naming the slug.
- oversized `instructions` → per-file `validate_config` error naming the slug.
- **Assert across all:** the error message contains the slug file path; failure is at STARTUP, not a
  request-time fallback.

`test_resolve_no_unwrap_on_per_slug_path` (review-grade, recorded C)
- **Assert:** no `.unwrap()` / `panic!` on the per-slug path; all failure modes return
  `ServerError::Config`. (NFR-06.)

## File-Present Order Proof (integration of the helper's steps)

`test_resolve_file_present_executes_full_order`
- **Assert:** on a valid file the helper executes, in order: `load_single_config` → per-file
  `validate_config` → `merge_configs(global, slug_file)` → post-merge `validate_config(&merged)` →
  `Ok(Cow::Owned(merged))`. A failure at any step short-circuits with a slug-named error.

## Integration Test Expectations (MCP interface)

**Regression-only.** No MCP test asserts the helper directly (not reachable on the single-server
harness, OVERVIEW §5b). The smoke/`tools`/`confidence`/`lifecycle` suites corroborate that the
single-project NO-FILE path (the helper's `Cow::Borrowed` arm) still serves identically — a new failure
there on the single-server path signals a fallthrough regression (R-03) and is triaged
caused-by-this-feature.

## Edge Cases (from Risk Strategy)

- Per-slug file at the path but unreadable (permissions) → fail-loud naming slug (R-10/R-11).
- Per-slug file setting `[adapt]` (FR-13: left default) → parses, but `adapt_service` stays
  `AdaptConfig::default()` (not threaded; verified in `per_slug_loop`/review, recorded here as expected
  no-effect).
- N=0 / N=1 slugs → fallthrough path only; no merge (covered by no-file test).

## Assertions Summary (concrete)

- `resolve_slug_config(no file)` ⇒ `Cow::Borrowed(&global)`, no merge, ptr-identity to `&global`.
- merged-only sum-violation ⇒ `Err(ServerError::Config)` naming slug at startup; per-file `Ok`.
- >64 KiB and `mode()&0o022!=0` files ⇒ `Err` naming slug.
- every invalid class ⇒ slug-named startup error; no `.unwrap()`.
- valid merge ⇒ `Ok(Cow::Owned)`, post-merge `validate_config` ran inside the helper.
