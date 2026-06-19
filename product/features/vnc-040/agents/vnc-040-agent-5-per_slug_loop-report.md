# Agent Report — vnc-040-agent-5-per_slug_loop

## Component
`per_slug_loop` (Wave 2) — MODIFY `crates/unimatrix-server/src/main.rs:1089-1110` (per-slug provisioning loop) + RELOCATE the `instructions` source from the `main.rs:687` hoist.

## Files Modified
- `crates/unimatrix-server/src/main.rs` — per-slug loop rewrite + `#[cfg(test)] #[path="per_slug_loop_tests.rs"] mod per_slug_loop_tests;` declaration.
- `crates/unimatrix-server/src/per_slug_loop_tests.rs` — NEW (468 lines, under the 500-line cap), 10 component tests.

## Tests
Component suite: **10 passed / 0 failed** (`cargo test -p unimatrix-server --bin unimatrix per_slug_loop_tests`).
Full binary suite (regression): **98 passed / 0 failed** (includes Wave 1 `slug_config_tests` + existing `main_tests`).
Clippy on the binary target (incl. tests): no warnings in edited regions.

| Test | Risk / AC |
|------|-----------|
| `test_no_file_arm_ptr_eq_on_three_global_handles` | AC-02 / R-03 — `Arc::ptr_eq` on the 3 handles, no-file arm |
| `test_no_file_arm_overlayable_values_equal_global` | AC-02 value half — instructions/nli_top_k/nli_enabled/categories/boosted/confidence equal global |
| `test_n2_exactly_one_nli_and_one_embed_handle_resident` | AC-04 / R-04 — one model each at N=2 (#5172), `[embedding]` attempt global-won |
| `test_fields_0_2_cloned_unconditionally_on_file_present_arm` | R-04 construction proof — ptr_eq holds even with a file present |
| `test_n2_instructions_per_slug_isolated` | AC-10 / R-12 — per-slug instructions overlay, no leakage |
| `test_instructions_absent_falls_through_to_global` | AC-10 / R-12 — absent instructions fall through to global |
| `test_n2_categories_per_slug_isolated` | AC-01 — per-slug categories overlay N=2, no leakage |
| `test_permissive_passed_from_global_flag_never_from_resolved` | AC-07 — `permissive` construction-lock (both directions) |
| `test_transport_keys_in_per_slug_file_do_not_affect_served_transport` | AC-06 / R-09 — transport never threaded at the seam |
| `test_per_slug_vector_index_uses_vectorconfig_default_not_merged_dims` | R-06 — `VectorConfig::default()` forward guard |

## Confirmation (per RETURN FORMAT item 3)
- **Fields 0–2 + permissive cloned/passed UNCONDITIONALLY outside any overlay branch:** YES. `let embed = Arc::clone(&embed_handle); let pool = Arc::clone(&ml_inference_pool); let nli = Arc::clone(&nli_handle);` are bound at the TOP of each loop iteration, textually ahead of and outside the `resolve_slug_config` call. `permissive` is passed from the global daemon flag (`config.agents.default_trust == "permissive"`, main.rs:688), never read from `resolved`. None of the 4 is ever sourced from `resolved` on any path.
- **`instructions` sourced from `resolved`:** YES — `let instructions = r.server.instructions.clone();` inside the loop (where `r = &*resolved`).
- **Did the `main.rs:687` hoist have other consumers?** YES — line 935 (the daemon's OWN `UnimatrixServer::new`) still uses `server_instructions.clone()`. Per the pseudocode caution, I relocated ONLY the per-slug-loop source (formerly line 1095) and LEFT the hoist + its line-935 consumer intact. The stdio path (lines 1377/1615, a separate function) is untouched.
- **`Arc::ptr_eq` holds on the no-file arm:** YES — `test_no_file_arm_ptr_eq_on_three_global_handles` asserts it for all 3 handles; `build_project_server` does `Arc::clone(embed_handle)` internally so the served handle is the same allocation as the daemon's.

## Other findings
- `build_project_server` signature UNCHANGED; `resolve_slug_config`/`merge_configs`/`load_single_config`/`validate_config` reused as-is.
- The 7 overlayable values are derived by re-sourcing the EXACT constructor expressions the daemon uses (`resolve_confidence_params`, `CategoryAllowlist::from_categories_with_policy`, `domain_pack_from_config` → `DomainPackRegistry::new`, `r.inference.clone()`, boosted-set collect) from `r` instead of `config`, so the no-file arm is byte-for-byte equal by reuse.
- Wave 1's `#[allow(dead_code)]` on `resolve_slug_config`/`config_err`/`PROJECT_CONFIG_NAME` in `http_provision.rs` are now no-ops (the items are used by the loop); left as-is — out of my file scope.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (category=pattern) -- surfaced ADR-001/002 (#5209/#5206), the crt-056 by-value-move pre-clone pattern (#5169), the resolve_slug_config gotchas (#5212), and the construction-locked classification pattern (#5211). Applied all.
- Stored: entry #5213 "Per-slug loop construction-lock testing: embedding_model_sha256 global-wins is CONDITIONAL on the global pin being Some" via /uni-store-pattern (4 runtime-invisible gotchas: conditional sha256 global-wins, post-merge validate rejecting fixtures, child-test-module layout + #5172 sentinels, and the dual-consumer instructions hoist).
