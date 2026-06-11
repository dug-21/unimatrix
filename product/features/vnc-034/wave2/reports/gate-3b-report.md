# Gate 3b Report: vnc-034 Wave 2

> Gate: 3b (Code Review)
> Feature: vnc-034 Wave 2 (issue #727)
> Date: 2026-06-11
> Branch / HEAD: feature/vnc-034-wave2 @ 3a06e611
> Result: **PASS**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | Resolver / registry / config match OVERVIEW + per-component pseudocode; per-key dispatch lives inside the seam (ADR-003 SR-07). |
| Architecture compliance | PASS | Drop-in `StoreResolver` swap at the same `SlugRouter::new` call site; ADR-003/004/005 honored; no new edge. |
| Interface implementation | PASS | `adapter_for` is the sanctioned seam extension (no default impl). No signature drift on `resolve_store`. |
| Test case alignment | PASS | Every test-plan scenario has a corresponding test; security corpus + reserved + two-state + re-attach + funnel-elimination all present. |
| Code quality | PASS | Builds clean; no stubs/`todo!`/`unimplemented!`; no `.unwrap()` in non-test code; no unsafe; all Wave-2 files ≤500 lines; router.rs shrank 562→525. |
| Security | PASS | D1 allowlist at parse edge before any path join; escape unrepresentable; `--purge` re-type confirmation; no hardcoded secrets. |
| **D1** slug allowlist exact + reused | PASS | `^[a-z0-9][a-z0-9-]{0,62}$`; single `ProjectSlug::TryFrom` reused by config + CLI; discriminators pass. |
| **D4** delete=de-register / purge / re-attach | PASS | Default preserves data dir + chain; `--purge --confirm <slug>` required; re-attach opens existing store; OQ-CLI-7 resolved (non-destructive open + `data_exists` genesis gate). |
| **D5** reserved-slug refusal | PASS | `RESERVED_SLUGS = {v1, health, observe, tools}` single source; separate check from charset; `tools` charset-valid yet rejected. |
| **D6** register two-state | PASS | already-routing → loud error; data-exists-but-de-registered → re-attach; distinct messages. |
| **D3** list status operator-side | PASS | Config-driven (not directory scan); `store_open` is filesystem-only; no per-slug HTTP/network health surface (AC-W1-S6 intact). |
| **D2** no config-overlay | PASS | No overlay/precedence surface introduced. |
| **FUNNEL** elimination | PASS | Wave-1 `let _store` discard + fixed `ProjectRouter<ReqBody>` fallback removed; `adapter_for` sole dispatch route; no-bypass + two-store dispatch isolation tests present and passing. |
| Build | PASS | `cargo build -p unimatrix-server` — Finished, no errors (pre-existing warnings only). |
| Tests | PASS | `cargo test -p unimatrix-server --lib` — 4002 passed; 0 failed; 1 ignored. Known flakes did not trip. |

## Detailed Findings

### D1 — Slug allowlist exact regex, reused not re-implemented
**Status**: PASS
**Evidence**:
- `seam.rs:71-104` `ProjectSlug::TryFrom<&str>` enforces 1..=63 length, first char `is_ascii_lowercase() || is_ascii_digit()`, remainder lowercase-alnum or `-`. This is exactly `^[a-z0-9][a-z0-9-]{0,62}$`. The issue-body drift (underscore + 64) is NOT implemented.
- `config.rs:2318` calls `ProjectSlug::try_from(...)` — comment explicitly states "config does NOT re-implement the regex".
- `projects.rs:199` CLI `validate_slug` also calls `ProjectSlug::try_from`. Single source, three reuse sites, zero re-implementation.
- Discriminator tests: `projects_config_tests.rs:158-203` reject `my_project` (underscore), 64-char, and the full `../ /%2f /%2e` / uppercase / empty / backslash / absolute corpus (T-SEC-01..16); accept 63-char (T-SEC-17) and canonical valids. `projects/tests.rs:165` `test_register_rejects_overlength_slug` (64-char). Escape is structurally impossible — rejected at parse edge before any `per_slug_data_dir` join.

### D4 — delete de-registers + preserves; --purge destroys loudly; re-attach
**Status**: PASS
**Evidence**:
- `projects.rs:398-419` default `delete` prints "de-registered ... data preserved", no filesystem destruction.
- `projects.rs:421-451` `--purge` requires `confirm == Some(slug.as_str())`; bare `--purge` → loud refusal (`projects.rs:425-432`); only then `remove_dir_all`.
- Re-attach: `register` State B (`projects.rs:282-306`) opens the existing store (`Store::open`), never re-creates; State C dir-creation (`projects.rs:308-336`) is gated on `!data_exists` so genesis can never run over preserved data (OQ-CLI-7 resolved via both non-destructive `open` AND the `data_exists` gate — documented `projects.rs:24-33`).
- Integrity test present + passing: `test_deregister_reregister_reattaches_to_preserved_chain` (`projects/tests.rs:456-494`) asserts prior entries survive and `content_hash` chain head is IDENTICAL after re-attach. Contrast guard `test_purge_then_register_is_fresh_store` (line 497) confirms purge severs the chain.

### D5 — reserved-slug refusal, separate from charset, single source
**Status**: PASS
**Evidence**:
- `config.rs:2285` `pub const RESERVED_SLUGS: [&str; 4] = ["v1", "health", "observe", "tools"]` — single source; `is_reserved_slug` (line 2292) exact-match. CLI imports this constant (`projects.rs:44`), never a second list.
- Separate check: config `validate_projects_config` runs charset (step 1) then reserved (step 2) (`config.rs:2318-2331`); CLI `validate_slug` likewise (`projects.rs:199-213`).
- `tools` charset-valid yet rejected: `test_reserved_check_is_separate_from_charset` (`projects_config_tests.rs:260`), `test_register_reserved_is_separate_from_charset` (`projects/tests.rs:214`), and exact-match-only guards (`toolsx`/`v1-prod` accepted).

### D6 — register two-state distinct messages
**Status**: PASS
**Evidence**: `register` branches on `(data_exists, is_routed)` (`projects.rs:268-336`): State A (both) → loud "already registered and routing" error; State B (data only) → re-attach path (not error); State C (neither) → fresh. `test_register_already_routing_errors_loud` and `test_register_two_states_distinct_messages` (`projects/tests.rs:248,280`) confirm the two states are not collapsed.

### D3 — list status operator-side only; no network surface
**Status**: PASS
**Evidence**: `scan_registered` (`projects.rs:364-378`) is config-driven via `configured_slugs` (reads `config.toml` `[[projects]]`, NOT a directory scan — explicitly to avoid mis-classifying path-hash sibling dirs and leaking path-hash dir names). `store_open` derived from local `std::fs::File::open` only. No HTTP/network probe anywhere. `test_list_is_config_driven_not_dir_scan` (`projects/tests.rs:361`) confirms. AC-W1-S6 (no unauthenticated endpoint beyond `/health`) intact — no per-slug endpoint added.

### D2 — no config-overlay surface
**Status**: PASS
**Evidence**: Wave-2 config additions are only `projects: Vec<ProjectConfigEntry>` (`config.rs:104,115`) — a flat slug list. No precedence/merge/overlay machinery.

### FUNNEL — Wave-1 discard + fixed adapter eliminated
**Status**: PASS
**Evidence**:
- `seam.rs:259-330` `route_mcp`: resolved store is USED (`let store = ...resolve_store`), dispatch goes through `self.resolver.adapter_for(&key)` only — no fixed-adapter fallback; unknown key → 404. `debug_assert!(adapter.wraps_store(&store))` proves resolve/dispatch read the same map.
- Fixed `ProjectRouter<ReqBody>` removed: `router.rs:331-344` documents the generic dispatcher is GONE; `SlugRouter::new(resolver)` takes only the resolver (`seam.rs:247`), no fixed-adapter param.
- `adapter_for` has NO trait default (`seam.rs:118-138`) — deliberately preventing a `{ None }` bypass.
- DefaultResolver Default path byte-identical: `main.rs:934-941` single-project uses `DefaultResolver::with_adapter`; `test_no_projects_default_byte_identical` + `test_default_path_unchanged_with_projects` (`project_resolver/tests.rs:305,274`) assert same `Arc<Store>`, no re-open, no re-init (AC-W2-R2/AC-CT-C4).
- No-bypass test `test_no_residual_fixed_adapter_path` (`project_resolver/tests.rs:195`) + two-store dispatch isolation (`adapter_for(alpha)` never wraps beta/default) present and passing.

### Code quality / files / unsafe / unwrap
**Status**: PASS
**Evidence**: Line counts — `project_resolver.rs` 226, `seam.rs` 331, `default_resolver.rs` 121, `projects.rs` 456, all ≤500. `router.rs` 525 (was 562 pre-Wave-2 — Wave 2 SHRANK it, the funnel removal). No `.unwrap()` in non-test Wave-2 code (grep clean; only doc-comment mentions). No `todo!`/`unimplemented!`/`unsafe`. `main.rs:947` uses `unwrap_or` (safe), not `.unwrap()`.

## Notes / Non-blocking observations

1. **Resolver naming (OQ-PR-2, documented)**: design docs call the Wave-2 resolver `ProjectRouter`; in code it is `MultiProjectRouter` to avoid shadowing the removed generic `ProjectRouter<ReqBody>`. The divergence is intentional and documented (`project_resolver.rs:8-12`). Not a fidelity gap.
2. **Integration test instrument**: the brief/ADR reference `tests/project_routing_integration.rs` as the two-store HTTP instrument; no such file exists under `crates/unimatrix-server/tests/`. The two-store isolation and dispatch-isolation guarantees are instead proven at the resolver/unit level (`project_resolver/tests.rs`: store isolation lines 95-99, dispatch isolation 245-255, no-bypass 195-267) — structurally equivalent coverage of AC-W2-R3 and the funnel invariant. WARN-level note only; does not block (the locked FUNNEL checks all have passing tests). Recommend Wave 3 / follow-up add the over-the-wire two-store integration test to exercise `session_id` attribution end-to-end (AC-W2-R5 currently asserted at resolver level, noted as deferred to HTTP integration in `project_resolver/tests.rs:388`).

## Rework Required

None.

## Knowledge Stewardship
- Queried: gate-3b check set against locked decisions D1–D6 + FUNNEL (no Unimatrix query needed — all source documents were in-context).
- nothing novel to store -- this is a clean PASS with no new cross-feature gate-failure pattern; the resolver-naming and missing-integration-file observations are feature-specific and already documented in code.
