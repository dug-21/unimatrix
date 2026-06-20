# Gate 3b Report: vnc-041

> Gate: 3b (Code Review)
> Date: 2026-06-20
> Result: PASS
> Branch: feature/vnc-041 @ d5d6ad2b (wave 3)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | C1–C5 implemented as designed; delegation + legend render + dual State B/C seed match pseudocode |
| Architecture compliance | PASS | A→B one-way contract honored; gate is `http.enabled` (ADR-004); (b)-only writes; signatures unchanged |
| Interface implementation | PASS | `write_if_absent` (pub(crate)), `render_per_slug_seed_toml` (pub), `resolve_slug_config` signature unchanged |
| Test case alignment | PASS | Every component test plan scenario has a corresponding test; 56 vnc-041 tests pass |
| Risk coverage | PASS | R-01..R-14 each map to passing tests (see Detailed Findings) |
| Code quality | PASS | Builds clean; clippy clean; no stubs/`todo!()`; no `.unwrap()` in non-test new code; tracing-only |
| Security | PASS | Slug traversal rejected at newtype; content-free WARN; `create_new` no-clobber blast radius |
| Load-bearing invariants C1–C5 | PASS | All five confirmed in code (see below) |
| Knowledge stewardship | PASS | All 3 impl agents have `## Knowledge Stewardship` with Queried + Stored |
| 500-line file rule | WARN | main.rs/config.rs pre-existing legacy over cap (not introduced); projects.rs now exactly 500 |

## Detailed Findings

### Load-bearing invariants (C1–C5)

**C1 — `write_if_absent` no-clobber (infra/config.rs)** — PASS
- Uses `OpenOptions::new().write(true).create_new(true).open(path)` (O_EXCL). No `path.exists()` precheck before the seed write — the single open IS the guard (confirmed by grep; lines 4919/4949 are doc/comment, 3240/3258 are unrelated `load_config` paths, rest are test assertions).
- `AlreadyExists` arm is a silent no-op (skip-if-exists), operator content survives.
- `write_default_config_if_absent` `force=true` overwrite arm preserved (`fs::write` with own parent handling at line 5002+); `force=false` delegates to `write_if_absent`.

**C2 — `render_per_slug_seed_toml` (infra/config.rs)** — PASS
- Renders from `PER_SLUG_CONFIG_CLASSIFICATION` iterated in order via `render_legend_line`.
- `render_legend_line` matches `OverlayDisposition` EXHAUSTIVELY (PerSlugOverlayable / GlobalLocked) — NO catch-all `_ =>` arm (confirmed); a future variant is an intended compile break (R-06 forcing function).
- Keyed purely on `entry.key` + `entry.disposition`; never dereferences a `UnimatrixConfig` field, so field-less locks (`permissive`, `tls`, `http`, `*_sha256`) render a "managed globally" legend line with no editable knob and cannot panic (R-12/SR-03).

**C3 — per-slug seed writer in `register` (projects.rs)** — PASS
- `write_per_slug_seed` called at BOTH State B (re-attach, line 312) and State C (genesis, line 351) after `ensure_project_stanza`.
- Writes ONLY (b) via `per_slug_data_dir(&self.base_dir, slug).join(PROJECT_CONFIG_NAME)` (line 370) — the single validated join site (SR-09); never touches (a)≡(c).
- Best-effort: `config::write_if_absent` returns `()`, warn-and-continue; no signature changes to `register`/`ensure_project_stanza`/`per_slug_data_dir`.

**C4 — global serve seed (main.rs)** — PASS
- `write_default_config_if_absent(&paths.data_dir.join("config.toml"), false)` placed lexically INSIDE `if config.http.enabled` (line ~1022). The local `else` branch has NO seed call site (AC-06 by construction). Gate is `http.enabled`, not `base_dir`.

**C5 — locked-key seam WARN (http_provision.rs)** — PASS
- `warn_locked_keys` derives the locked surface from `is_per_slug_overlayable(key) == false` over keys the file SETS (`flatten_present_keys`) — no hand-list.
- WARN-only: separate `std::fs::read_to_string` (line ~342), raw parse degrades silently on failure, leaves `load_single_config`'s typed-load error semantics untouched; resolution output identical.
- Content-free: logs `slug` + `key` only, never the operator's value (#4749 pattern).

### Pseudocode fidelity — PASS
Each component's code matches its pseudocode file: C1 delegation + shared primitive; C2 legend-block + reused `DEFAULT_CONFIG_TOML` (no new serializer); C3 dual-branch seed via shared join; C4 structural `http.enabled` gate; C5 raw-table flatten + per-key WARN. No undocumented departures.

### Architecture compliance — PASS
A→B one-way contract intact: render (C2) and WARN (C5) both consume `PER_SLUG_CONFIG_CLASSIFICATION`/`is_per_slug_overlayable` at runtime; B restates nothing. `ensure_project_stanza` (a/c writer) unchanged. ADR-001..005 followed.

### Test case alignment + Risk coverage — PASS
- C1: 8 `write_if_absent`/delegation tests (R-03 no-clobber, R-10 swallow-failure, R-11 idempotent).
- C2: legend coverage, overlayable→editable, locked→managed-globally, flip test, field-less render (R-04/R-06/R-12/R-14).
- C3 (projects/tests.rs): `test_register_writes_b_at_per_slug_data_dir_path`, `..._state_c_genesis_writes_b`, `..._state_b_reattach_writes_b` (R-13), `..._does_not_modify_shared_a_c_file` (R-05), `..._does_not_clobber_pre_placed_b` (R-03/AC-05), `..._seed_write_failure_does_not_fail_register` (R-10), `..._seeded_b_path_is_the_resolver_formula` + `..._seeded_b_is_resolver_loadable` (AC-02 in-scope seam proof).
- C4 (global_serve_seed_tests.rs): `..._fires_with_http_enabled_and_base_dir_none` (R-01.4), `..._local_serve_writes_zero_new_config_files` + negative control (R-01/R-02), `..._seed_call_is_inside_http_enabled_block` (structural), `..._failure_does_not_abort_startup` (R-10), `..._init_then_container_serve_a_written_once` (R-11).
- C5 (slug_config_tests.rs): 14 new tests — WARN on locked, no-WARN on overlayable, content-free, flip, unknown-key, output-identical (R-07), uninspectable-file-no-error, no-file-arm-unchanged, once-per-boot, per-slug isolation (R-08), table-shaped lock, sha256 duplicate-acceptable.

### Code quality — PASS
- `cargo build -p unimatrix-server`: clean.
- `cargo clippy -p unimatrix-server --lib --bins`: clean (no warnings/errors).
- `cargo test -p unimatrix-server --lib --bins`: 4279 passed, 1 failed, 1 ignored. The single failure — `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` — is in the `eval` module (UNTOUCHED by vnc-041), passes in isolation on HEAD; a pre-existing flaky non-deterministic sweep test, NOT a vnc-041 regression. The 56 vnc-041-specific tests all pass deterministically.
- No `todo!()`/`unimplemented!()`/`FIXME` introduced. The two `TODO(W2-4)` in main.rs are pre-existing (outside the vnc-041 diff). `panic!`/`unwrap_or_else` occurrences in config.rs are pre-existing or in vnc-041 test code (acceptable).
- No `.unwrap()`/`.expect()` in non-test new code (C3 writer, C5 WARN, C2 renderer, C1 primitive).
- `tracing` only — no `println!`/`eprintln!` in the WARN/seed paths.

### Security — PASS
- Path traversal: C3 reuses the single validated `per_slug_data_dir` join site, inheriting `ProjectSlug` newtype validation; `test_register_validates_slug_via_newtype` rejects `../etc`, `a/b`, `a%2fb`.
- Serialization: C5 raw `toml::from_str` degrades silently (no panic, no new error) on malformed input; `test_resolve_warn_pass_does_not_add_error_on_uninspectable_file` proves it.
- Seed blast radius: `create_new` cannot overwrite; failed write swallowed; worst case is a missing convenience file.
- Content-free logging: WARN names bounded key + validated slug, never the set value.
- No hardcoded secrets; no command injection surface.

### Knowledge stewardship — PASS
All three wave-3 implementation agent reports contain `## Knowledge Stewardship` with `Queried:` (context_briefing/search, ADR lookups) and `Stored:` entries (#5241 seam-WARN pattern, #5242 renderer test traps, #5243 crate-split round-trip trap).

## Notes / Deferred (Stage 3c concerns — NOT failed here)
- AC-02 empirical `register → resolve_slug_config` round-trip is deferred to 3c (resolver is bin-crate-only). The in-scope seam proof EXISTS and passes: `test_register_seeded_b_path_is_the_resolver_formula` (literal path-formula) + `test_register_seeded_b_is_resolver_loadable`.
- Full `cargo test --workspace` OOMs while linking `import_integration`; validated via per-crate `cargo test -p unimatrix-server --lib --bins` per the known constraint.

## WARN detail — 500-line file rule
| File | pre-vnc-041 | HEAD | Assessment |
|------|-------------|------|------------|
| main.rs | 2137 | 2156 | Pre-existing legacy, far over cap before this feature; +19 lines (C4 seed call + test mod). Not introduced here. |
| infra/config.rs | 12306 | 12863 | Pre-existing legacy; C1/C2 additions. Not introduced here. |
| projects.rs | 475 | 500 | Now exactly at the boundary (does not exceed 500). Watch — next addition crosses the cap; consider extracting the seed writer if projects.rs grows further. |
| global_serve_seed_tests.rs | — | 345 | New file, under cap. |
| projects/tests.rs | (test file) | 1226 | Test file. |

No vnc-041 code change pushed a previously-compliant source file over 500. main.rs/config.rs are long-standing tech debt that prior gates (vnc-040 #799) shipped against. Recorded as WARN, not FAIL, because the feature introduced no new violation; projects.rs sitting at exactly 500 is flagged for monitoring.
