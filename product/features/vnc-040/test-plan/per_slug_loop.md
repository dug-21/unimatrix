# Test Plan — `per_slug_loop` (MODIFY, `main.rs:1089-1110` + instructions hoist `main.rs:687`)

> Component: the per-slug provisioning loop. Per slug it (a) `Arc::clone`s the 3 global handles +
> passes `permissive` UNCONDITIONALLY, outside any overlay branch; (b) calls `resolve_slug_config`;
> (c) derives fields 3–9 + `instructions` from the resolved config; (d) passes all into the unchanged
> `build_project_server`. Owns: **AC-02 `Arc::ptr_eq` (R-03)**, **AC-04 model invariants (R-04)**,
> **AC-10 instructions overlay (R-12)**, **AC-01 categories overlay**, AC-06 transport-locked (R-09),
> R-06 forward guard, AC-07 threading half. Tested via the **#5172 model-free N=2 harness** (no real
> model loaded — handles are unloaded sentinels) + Rust construction-proof tests.

## Behavioral Test Expectations (N=2 model-free harness, #5172)

### AC-01 — Per-slug categories overlay (isolation non-vacuous)

`test_n2_categories_per_slug_isolated`
- **Arrange:** N=2 slugs A, B; A's `config.toml` sets distinct `[knowledge].categories`, B's sets a
  DIFFERENT distinct set; a global default underlies both. Populations are distinct (non-vacuous).
- **Assert:** A's served `CategoryAllowlist` == A's merged categories; B's == B's; neither leaks into
  the other; the global default underlies where a slug leaves a key unset.

### AC-10 / R-12 — Per-slug instructions overlay + absent-file fallthrough

`test_n2_instructions_per_slug_isolated`
- **Arrange:** A's `config.toml` sets `[server] instructions = "A..."`, B's sets `"B..."`.
- **Assert:** A's served `ServiceLayer` carries A's instructions; B's carries B's; A's ≠ B's; neither
  leaks. The threaded `instructions` arg reflects `resolved.server.instructions` (relocated from the
  `main.rs:687` hoist into the loop).

`test_instructions_absent_falls_through_to_global`
- **Arrange:** a slug with NO `[server] instructions` override; global `server.instructions` set.
- **Assert:** the slug's served instructions == global `resolved.server.instructions` (the
  `main.rs:687`/`1095` value), NOT empty/default. (Merge half also asserted in `slug_config_classification`
  via `Option::or`; this is the threaded-value behavioral half.)

### AC-04 / R-04 — One model each at N≥2 (the hard invariant)

`test_n2_exactly_one_nli_and_one_embed_handle_resident`
- **Arrange:** N=2 slugs with DISTINCT per-slug configs (incl. a slug attempting `[embedding]` /
  model-identity keys); #5172 harness so no real model loads.
- **Assert:** exactly ONE NLI handle and ONE embedding handle resident across both slugs; the slug that
  attempted `[embedding].model` leaves the served handle AND merged `[embedding]` descriptor as the
  GLOBAL model (no load, no describe). Mirrors crt-056 AC-2 / NFR-01.

## Construction-Proof / Pointer-Identity Tests (Rust, machine-checked)

### AC-02 / R-03 — `Arc::ptr_eq` fallthrough sentinel (THE primary regression sentinel)

`test_no_file_arm_ptr_eq_on_three_global_handles`
- **Arrange:** a slug with NO per-slug file; capture the daemon's `embed_handle`, `nli_handle`,
  `rayon_pool` `Arc`s.
- **Assert:** `Arc::ptr_eq(&daemon_handle, &per_slug_clone)` holds for ALL THREE — same allocation, NOT
  merely value-equal (matches crt-056 AC-2). A regression that rebuilt/re-derived any handle fails this
  even if the rebuilt value compares equal. Plus: value-equality across the remaining ~12 threaded
  inputs vs the global-only crt-056 path (byte-for-byte fallthrough).

### AC-04 / R-04 — Unconditional clone site (construction review, recorded C)

`test_fields_0_2_cloned_unconditionally_outside_overlay_branch`
- **Assert (construction proof):** fields 0–2 (`embed_handle`, `rayon_pool`, `nli_handle`) are
  `Arc::clone`d UNCONDITIONALLY, ahead of / outside the `resolve_slug_config` call and any overlay
  branch — NEVER read from `resolved` on ANY path (file-present or no-file). Reviewed at the loop's
  clone site; the `Arc::ptr_eq` test above is the machine-checked corroboration for the no-file arm.

### AC-07 — `permissive` GLOBAL-LOCKED + threading half of the verdict (recorded C + B)

`test_permissive_passed_from_global_flag_never_from_resolved`
- **Assert:** `permissive` is passed unconditionally from the global daemon flag; a per-slug file
  setting a `permissive`-equivalent does NOT change the threaded value. (The row-set/closed-checklist
  guard lives in `slug_config_classification`; this is the call-site threading proof.)

### AC-06 / R-09 — Transport never read at the seam

`test_transport_keys_in_per_slug_file_do_not_affect_served_transport`
- **Arrange:** a per-slug `config.toml` setting `[server.tls]` / auth / host / `http.enabled`.
- **Assert:** served transport == global; the loop never reads a transport field from `resolved`
  (construction review); the HTTP listener is built from global config BEFORE the per-slug loop runs.

### R-06 — Forward guard on `VectorConfig::default()` (standing guard test)

`test_per_slug_vector_index_uses_vectorconfig_default_not_merged_dims`
- **Assert:** the per-slug vector index is constructed from `VectorConfig::default()`
  (`http_provision.rs:182`), NOT from merged-config dims. The test FAILS LOUDLY if a future change wires
  per-slug dims through `resolved` — defusing the SR-03/A2 `[embedding]` divergence re-open. Plus:
  merged `[embedding]` section (today `embedding_model_sha256`) == global for any per-slug input.

### AC-09 — Restart-only (review-grade, recorded C)

`test_overlay_read_once_at_build_time_no_reload_path`
- **Assert (review):** the per-slug read occurs once at `build_project_server` time; no reload watcher /
  endpoint / file-watch added (vnc-038 ADR-007 restart-applies).

## Integration Test Expectations (MCP interface)

**Regression-only, no new tests.** True multi-slug per-slug-config behavior is NOT reachable on the
single-server infra-001 harness (one `--project-dir`, no multi-slug fixture — OVERVIEW §5b). The N=2
isolation, `Arc::ptr_eq`, and one-model-each proofs are the in-crate #5172 harness + Rust tests above.
infra-001 `smoke`/`tools`/`confidence`/`lifecycle` corroborate the single-project no-file path is
unchanged at the MCP surface; a NEW single-server failure there ⇒ suspect the no-file fallthrough arm
(R-03) first. Multi-slug HTTP harness uplift → file a GH Issue (do not build in this PR).

## Edge Cases (from Risk Strategy)

- N=0 / N=1 slugs → fallthrough path only, no behavior change (R-03).
- Per-slug `[adapt]` set → no effect; `adapt_service` stays `AdaptConfig::default()` (FR-13), not threaded.
- Per-slug GLOBAL-only section (`[server.tls]`, `permissive`) → silently ignored at seam except
  `*_sha256` warn (R-13 accepted residual); served transport/permissive unchanged.
- Per-slug `instructions` set in global, unset per-slug → global retained (R-12 fallthrough).

## Assertions Summary (concrete)

- `Arc::ptr_eq` holds for the 3 global handles on the no-file arm; value-equality on the remaining inputs.
- fields 0–2 cloned unconditionally outside the overlay branch (review + ptr_eq corroboration).
- N=2: exactly one NLI + one embed handle; per-slug `[embedding]` attempt ⇒ served handle == global.
- N=2: A's categories/instructions ≠ B's, no leakage; absent instructions ⇒ global fallthrough.
- `permissive` from global flag only; transport never read at seam; vector index == `VectorConfig::default()`.
