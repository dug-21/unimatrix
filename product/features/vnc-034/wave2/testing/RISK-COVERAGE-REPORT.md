# Risk Coverage Report: vnc-034 Wave 2 (#727)

> Multi-project routing on the merged Wave-1 `StoreResolver` seam. Stage 3c
> execution. This report maps every Wave-2 AC + the D1/D4/D5/D6 discriminators +
> the funnel no-bypass invariant + OQ-CLI-7 chain-preservation to its passing test
> (unit or HTTP integration), records the infra-001 backward-compat gate result and
> its role, and states honestly any case that cannot be driven at the HTTP edge.
>
> Test names below are the **as-implemented** names (some differ from the Stage-3a
> plan's working names; the plan's intent is preserved). Integration tests live in
> `crates/unimatrix-server/tests/project_routing_integration.rs`; unit tests in
> `src/http/router/project_resolver/tests.rs`, `src/http/router/seam.rs`,
> `src/projects/tests.rs`, and the `http`/`auth`/`tls` modules.

## Test Results

### Unit Tests (cargo, `-p unimatrix-server --lib`)
- Total: 4002
- Passed: 4002
- Failed: 0
- Source: leader-verified GREEN at Gate 3b (not re-run here per finish-task scope).
  This report cites the specific named unit tests that back each AC/discriminator;
  all were confirmed present in source.

### HTTP Integration Tests (`tests/project_routing_integration.rs`)
- Total: 10
- Passed: 10
- Failed: 0

Command:
```
cargo test -p unimatrix-server --test project_routing_integration
# result: ok. 10 passed; 0 failed; 0 ignored; finished in 0.20s
```

The 10:
`test_two_slugs_route_to_distinct_stores`, `test_slug_a_write_unreadable_from_slug_b`,
`test_slug_a_write_does_not_appear_in_slug_b`, `test_v1_tools_default_unchanged_with_projects`,
`test_non_v1_path_routes_default`, `test_unregistered_slug_returns_unknown_project`,
`test_invalid_slug_path_rejected_at_edge`, `test_n_clients_one_slug_share_store`,
`test_dispatch_through_adapter_for_no_fixed_bypass`,
`test_default_and_slug_interleaved_no_cross_contamination`.

### infra-001 Smoke (Python stdio MCP) — BACKWARD-COMPAT GATE
- Total: 23 (smoke subset; 351 deselected)
- Passed: 23
- Failed: 0

Command:
```
cd product/test/infra-001
UNIMATRIX_BINARY=target/release/unimatrix python -m pytest suites/ -m smoke --timeout=60
# result: 23 passed, 351 deselected in 199.40s
```

**Role (per OVERVIEW §4.1):** infra-001 spawns `serve --stdio` — single-project,
no HTTP. It exercises `ProjectKey::Default` ONLY; it **cannot reach the `/v1/{slug}/`
HTTP edge**, so it does NOT and structurally CANNOT test slug routing. Its job in
Wave 2 is the **regression gate**: prove the resolver swap left the single-project
Default path byte-for-byte unchanged (store→get→search→correct→briefing→restart
across protocol/tools/lifecycle/security/confidence/contradiction/edge/volume/adaptation
smoke paths). Green here = the swap did not regress the Default path (AC-W2-R2 /
AC-CT-C4 end-to-end). Slug routing/isolation is proven by the Rust HTTP integration
file above, which IS able to reach the edge.

---

## Coverage Summary — Wave 2 Risks

| Risk ID | Description | Test(s) | Result | Coverage |
|---------|-------------|---------|--------|----------|
| R-01 | Trait swap doesn't break the funnel; routing INSIDE `resolve_store`, single edge | `test_swaps_at_slugrouter_callsite`, `test_resolves_slug_to_its_store`, `test_unknown_slug_returns_unknown_project`, `test_path_router_mcp_edge_is_the_slug_router_seam` (unit); `test_two_slugs_route_to_distinct_stores`, `test_unregistered_slug_returns_unknown_project` (integration) | PASS | Full |
| R-03 | Slug allowlist rejects traversal/encoded/uppercase/over-length; D1 regex EXACT | `test_projectslug_rejects_traversal_corpus`, `test_slug_reject_64_char_discriminator`, `test_slug_reject_underscore_discriminator`, `test_slug_accept_63_char_boundary`, `test_projectslug_over_length_boundary` (unit); `test_invalid_slug_path_rejected_at_edge` (integration) | PASS | Full |
| R-04 | One seam, two resolvers; Default path unchanged under projects; slug ⟂ path-hash | `test_default_path_unchanged_with_projects`, `test_slug_never_leaks_into_default_resolution`, `test_default_key_returns_default_store` (unit); `test_v1_tools_default_unchanged_with_projects` (integration) | PASS | Full |
| R-06 | N:1 clients share one slug store; identity transport-only, no payload project field | `test_n_clients_one_slug_shared_store`, `test_per_request_slug_rejected_at_funnel_not_default_store` (unit); `test_n_clients_one_slug_share_store` (integration) | PASS | Full |
| R-10 | `BearerValidator`/`TlsConfig`/slug seams not collapsed (AC-CT-C6) | `test_storeresolver_seam_types_present`, `test_bearer_validator_trait_valid_token`/`_invalid_token`, `test_valid_cert_and_key_returns_tls_acceptor`, `test_default_resolver_is_the_same_trait_as_wave2_resolver` (unit, structural) | PASS | Full |
| R-12 | No unauth endpoint beyond `GET /health`; no per-slug network health (D3) | `test_get_health_routes_to_health_handler`, `test_health_post_no_bypass`, `test_healthz_no_bypass`, `test_route_v1_slug_tools_parses_to_slug`, `test_per_request_slug_rejected_at_funnel_not_default_store` (unit, negative) | PASS | Full |
| R-13 | `[[projects]]`-absent ⇒ `/v1/tools/…` unchanged; additive `/{slug}` | `test_no_projects_default_byte_identical`, `test_route_non_v1_paths_map_to_default` (unit); `test_v1_tools_default_unchanged_with_projects`, `test_non_v1_path_routes_default` (integration); infra-001 smoke | PASS | Full |

---

## Coverage Summary — Wave 2 Acceptance Criteria

| AC-ID | Description | Test(s) | Result | Evidence |
|-------|-------------|---------|--------|----------|
| AC-W2-R1 | `/v1/{slug}/…` routes to the per-slug store (two slugs → two stores) | `test_two_slugs_route_to_distinct_stores` (integration) + `test_resolves_slug_to_its_store` (unit) | PASS | alpha/beta dispatch (non-404/400) to distinct `Arc<Store>` instances; `Arc::ptr_eq` distinctness; OQ-PR-4 wraps_store debug_assert proves dispatch is to the slug's OWN store |
| AC-W2-R2 | `[[projects]]`-absent ⇒ `/v1/tools/…` unchanged | `test_v1_tools_default_unchanged_with_projects` (integration) + `test_no_projects_default_byte_identical` (unit) + infra-001 smoke | PASS | Default-path status IDENTICAL with vs without projects registered; smoke green end-to-end |
| AC-W2-R3 | Per-slug isolation: no cross-project read or write | `test_slug_a_write_unreadable_from_slug_b`, `test_slug_a_write_does_not_appear_in_slug_b`, `test_default_and_slug_interleaved_no_cross_contamination` (integration) | PASS | A's write absent from B's `get`/`query_all_entries`; B + Default entry counts unchanged by A's writes; interleaved sequence shows no cross-contamination |
| AC-W2-R4 | register/list/delete lifecycle; D5 reserved; D6 two-state; D4 delete/purge/re-attach | `test_register_list_delete_roundtrip`, `test_register_adds_slug_to_registry`, `test_list_returns_registered_slugs`, plus the D4/D5/D6 tests below (unit, `src/projects/tests.rs`) | PASS | CLI lifecycle exercised unit-side; this is registry/CLI scope, not HTTP-edge driveable |
| AC-W2-R5 | N clients : 1 slug, per-`session_id` attribution; each bound to one slug | `test_n_clients_one_slug_share_store` (integration) + `test_n_clients_one_slug_shared_store`, `test_per_request_slug_rejected_at_funnel_not_default_store` (unit) | PASS | Two distinct `mcp-session-id` requests on `/v1/alpha/` both DISPATCH to alpha's adapter (shared store); identity is URL-path-derived only — no payload project field (see cannot-drive note) |
| AC-W2-R6 | Slug allowlist rejects traversal/encoded/uppercase/over-length; no fs escape | `test_invalid_slug_path_rejected_at_edge` (integration) + `test_projectslug_rejects_traversal_corpus`, `test_slug_reject_64_char_discriminator`, `test_slug_reject_underscore_discriminator` (unit) | PASS | 7 edge cases (percent-encoded traversal, encoded relative traversal, encoded dot-dot, uppercase, underscore, leading hyphen, 64-char over-length) each → 400 `invalid project slug` at the parse edge; no store touched |

---

## Coverage Summary — Cross-wave Contracts (Wave-2 relevant)

| AC-ID | Description | Test(s) | Result | Evidence |
|-------|-------------|---------|--------|----------|
| AC-CT-C4 | Additive seam swap; Wave-1 Default unchanged; no client re-point; no residual fixed-adapter bypass | `test_no_projects_default_byte_identical`, `test_local_and_cloud_single_project_byte_identical_route`, `test_observe_text_entries_byte_identical`, `test_no_residual_fixed_adapter_path`, `test_swaps_at_slugrouter_callsite` (unit); `test_v1_tools_default_unchanged_with_projects`, `test_dispatch_through_adapter_for_no_fixed_bypass` (integration); infra-001 smoke | PASS | Default `/v1/tools/…` byte-identical with/without projects; every per-slug + Default request dispatches via `adapter_for(key)`; per-key writes land ONLY in the matching store; Wave-1 `let _store` discard path gone |
| AC-CT-C6 | token/slug/cert seams not collapsed | `test_storeresolver_seam_types_present`, `test_bearer_validator_trait_valid_token`/`_invalid_token`, `test_valid_cert_and_key_returns_tls_acceptor` (unit, structural) | PASS | `BearerValidator` / `TlsConfig` / `StoreResolver` slug seam each present as distinct interfaces; resolver does not fold auth/TLS into slug resolution |

---

## D1 / D4 / D5 / D6 Discriminators + Funnel + OQ-CLI-7

| Discriminator | Intent | Test(s) | Result |
|---------------|--------|---------|--------|
| **D1** length bound | 63 accepted, 64 rejected (`^[a-z0-9][a-z0-9-]{0,62}$`, NOT the drifted `_` variant) | `test_slug_accept_63_char_boundary`, `test_slug_reject_64_char_discriminator`, `test_projectslug_over_length_boundary` (unit); 64-char `over_len` case in `test_invalid_slug_path_rejected_at_edge` (integration) | PASS |
| **D1** charset | underscore NOT in charset → reject | `test_slug_reject_underscore_discriminator` (unit); `al_pha` case (integration) | PASS |
| **D4** delete = de-register only | dir preserved on `delete`; `--purge` is loud (slug confirmation); re-register RE-ATTACHES preserved chain | `test_delete_deregisters_and_preserves_data_dir`, `test_purge_requires_slug_confirmation_or_no_destroy`, `test_purge_with_confirmation_removes_dir_and_deregisters`, `test_purge_then_register_is_fresh_store` (unit) | PASS |
| **D5** reserved-segment refusal | `register tools` → REJECT (shadows `/v1/tools/…`); separate from charset | `test_register_rejects_reserved_tools_shadowing`, `test_register_rejects_reserved_route_segments`, `test_register_reserved_is_separate_from_charset`, `test_register_reserved_exact_match_only`; `test_route_reserved_tools_never_a_slug`, `test_route_v1_tools_maps_to_default` (unit) | PASS |
| **D6** register two-state | already-routing → loud error; dir-exists-but-de-registered → re-attach success | `test_register_already_routing_errors_loud`, `test_register_dir_exists_deregistered_reattaches`, `test_register_two_states_distinct_messages` (unit) | PASS |
| **Funnel no-bypass** | every request via `adapter_for(key)`; ≥2 slugs + Default each on own store; Wave-1 discard path gone | `test_no_residual_fixed_adapter_path` (unit); `test_dispatch_through_adapter_for_no_fixed_bypass` (integration) | PASS |
| **OQ-CLI-7** chain-preservation | de-register → re-register re-attaches to the preserved hash chain, never clobbers | `test_deregister_reregister_reattaches_to_preserved_chain` (unit, highest-value) | PASS |

---

## Cannot-Drive Cases (Stated Honestly)

These are intentionally NOT driven through the HTTP integration file; each is covered
elsewhere (unit / structural) for a structural reason, not a coverage gap.

1. **AC-W2-R4 CLI lifecycle (register/list/delete/purge, D4/D5/D6, OQ-CLI-7)** —
   cannot drive at the HTTP `/v1/{slug}/` layer because these are **registry/CLI
   operations**, not request-routing behavior. There is no MCP/HTTP verb that
   registers a project. Covered by `src/projects/tests.rs` unit tests (all PASS).

2. **No-payload-project-field (R-06 / FR-X2 unrepresentability)** — cannot drive a
   *positive* "mis-target a second slug" request at this layer because the property is
   that **no such request is representable**: `ProjectKey` is constructed ONLY from the
   transport path via `parse_project_key`; there is no payload field naming a project.
   This is a source/structural invariant, asserted by
   `test_per_request_slug_rejected_at_funnel_not_default_store` and the construction
   surface of `ProjectKey`. The integration file proves the *positive* path-derived
   binding (session headers do not change the resolved slug — both `client-1` and
   `client-2` on `/v1/alpha/` bind to alpha purely by URL).

3. **AC-CT-C6 / R-10 seam-not-collapsed** — structural (type/trait presence), not a
   runtime behavior reachable through a single MCP request. Covered by the structural
   unit tests above.

4. **infra-001 slug routing** — infra-001 CANNOT reach the `/v1/{slug}/` edge (it is
   single-project stdio). This is by design; slug routing is proven by the Rust HTTP
   integration file. infra-001's role is strictly the Default-path backward-compat gate.

No silent stubs and no TODOs were introduced. The two `route_mcp`-direct requests in
`test_n_clients_one_slug_share_store` are fully driven (custom session headers,
bodies collected via the shared `collect_resp` helper).

---

## xfail / Pre-existing Flakes

No `xfail` markers were added in Wave 2 (no GH issues required).

Two **pre-existing** unit-test flakes are noted as NON-regressions (they pass in
isolation; not introduced by Wave 2, not in scope to fix here):
- `http::token::tests::test_concurrent_creation_no_corruption`
- `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous`

These are documented for the validator; they are unrelated to the resolver swap and
do not affect any Wave-2 AC mapping above.

---

## Gaps

None at the Wave-2 AC level. Every Wave-2 AC (R1–R6), both Wave-2-relevant
cross-wave contracts (CT-C4, CT-C6), all D1/D4/D5/D6 discriminators, the funnel
no-bypass invariant, and OQ-CLI-7 chain-preservation map to at least one PASSING
test. The cannot-drive cases above are covered by unit/structural tests by design,
not gaps.

(Wave-1 ACs AC-W1-*, AC-CT-C2/C3/ROT are out of Wave-2 scope and tracked in the
Wave-1 reports.)
