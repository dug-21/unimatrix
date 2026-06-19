# Agent Report — vnc-040-agent-4-resolve_slug_config

## Component
`resolve_slug_config` — NEW per-slug config overlay helper (vnc-040 C6, ADR-001/002/003).

## Files Modified
- `crates/unimatrix-server/src/http_provision.rs` — added `pub fn resolve_slug_config`, the
  `config_err` slug-naming helper, the `PROJECT_CONFIG_NAME` const, and the
  `#[cfg(test)] mod slug_config_tests;` declaration. Added `Cow` import + config-fn imports.
  (371 lines, under the 500-line cap.)
- `crates/unimatrix-server/src/http_provision/slug_config_tests.rs` — NEW. 13 component
  unit tests. (367 lines.)
- `crates/unimatrix-server/src/infra/config.rs` — VISIBILITY-ONLY change: `load_single_config`
  and `merge_configs` made `pub` (logic untouched) so the binary-crate helper can reuse them
  across the bin/lib boundary. `validate_config` was already `pub`.

## Contract Confirmation
- **No-file arm runs NO merge:** `std::fs::metadata(&path).is_file()` probe; if absent, returns
  `Ok(Cow::Borrowed(global))` — the global config itself, no `merge_configs`, no load, no
  re-derivation. Proven by `test_resolve_no_file_returns_cow_borrowed_global_no_merge`
  (asserts `std::ptr::eq` to `&global`).
- **Post-merge validate runs AFTER merge, before return:** order is
  `load_single_config -> validate_config(&slug_file) -> merge_configs(global.clone(), slug_file)
  -> validate_config(&merged) -> Ok(Cow::Owned(merged))`. The #3905 third-layer fix (ADR-003).
- **Owned merge_configs signature honored:** `merge_configs(global.clone(), slug_file)` — one
  clone per slug-with-a-file, startup-only.
- **Reuse unchanged:** `load_single_config` (64 KiB cap #2395 + `#[cfg(unix)]` 0o022 check),
  `validate_config`, `merge_configs` reused as-is (only visibility raised). No new
  load/merge/validate logic.
- **Fail-loud, slug-named, no `.unwrap()`:** every failure → `ServerError::Config` naming the
  slug AND the file path; no panic/unwrap/expect on the non-test path.

## Tests — 13 passed / 0 failed (component-level, `cargo test --bin unimatrix slug_config_tests`)
Covers: R-01 (post-merge fusion-weight sum-of-six violation fails loud naming slug, AC-08b, +
the load-bearing negative proving per-file validation alone returns Ok), R-01 construction proof
(post-merge validate runs inside helper), valid-merge no-false-positive, R-03 no-file
`Cow::Borrowed`, empty-file degenerate fallthrough, AC-03 single-key overlay, R-10 oversized
(>64 KiB) + `#[cfg(unix)]` group/world-writable rejection on the per-slug path, R-11 slug-named
startup-fatal for malformed TOML / unknown category / oversized instructions, full-order proof.

## Self-Check
- `cargo build --bin unimatrix` passes (zero errors). Pre-existing lib/anndists warnings only.
- `cargo clippy --bin unimatrix --tests` — zero warnings on my files.
- No `todo!`/`unimplemented!`/TODO/FIXME/HACK in non-test code.
- `#[allow(dead_code)]` on the helper + const + `config_err` until Wave 2 (`per_slug_loop` in
  `main.rs`) wires the call — the helper ships one wave before its only caller. Documented at
  each site.

## Coordination Notes / Flags
- Ran NO git commands (Delivery Leader owns git).
- Did NOT touch the per-slug loop in `main.rs` (Wave 2) or integration tests (Stage 3c).
- **Visibility change to `infra/config.rs`** (`load_single_config`, `merge_configs` → `pub`):
  the sibling Wave 1 agent (classification registry, ADR-004) also edits `infra/config.rs`. My
  edits are on the `fn`-keyword lines of two existing functions (plus doc comments), distinct
  from the registry's new items near `merge_configs` — but FLAG for the combined wave-commit
  build: confirm no overlap.
- SR-02/R-02 re-audit (the inline `InferenceConfig {…}` merge literal, #4070): the fusion-weight
  arms are explicit per-field replaces (verified at `config.rs` ~3984), no `..Default()` tail on
  the inference arm — reuse is safe for the global→per-slug call shape.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced ADR-001 (#5209),
  ADR-003 (#5199), ADR-002 (#5206), #3905 (per-file-validate insufficiency lesson), #4655
  (hash-pin global-wins), #4070 (hidden merge literal), #5175 (bin-crate provisioning test
  reachability). Applied all.
- Stored: entry #5212 "resolve_slug_config: child test module avoids main.rs mod collision;
  per-field-replace merge means global non-default weights only survive when project leaves
  them default" via context_store (pattern) — captures the cross-crate visibility requirement,
  the child-test-module-without-main.rs-collision technique, the per-field-replace merge
  subtlety, and the R-01 valid-each/invalid-merged construction recipe.
