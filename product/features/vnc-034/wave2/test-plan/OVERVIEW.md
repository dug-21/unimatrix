# vnc-034 Wave 2 — Test Plan OVERVIEW

> Multi-project routing on the merged Wave-1 `StoreResolver` seam (issue #727).
> Strategy, risk→test mapping, and the integration harness plan for the three
> Wave-2 components. Component test plans: `project-router.md`,
> `project-registry-cli.md`, `projects-config.md`.
>
> **Locked decisions that drive this plan (WAVE2-DELIVERY-BRIEF D1/D2/D3 +
> Stage 3a refinements D4/D5/D6 + funnel-honesty record):**
> - **D1** — slug allowlist is EXACTLY `^[a-z0-9][a-z0-9-]{0,62}$` (1–63 chars,
>   lowercase alnum + hyphen, leading alnum). NOT the drifted issue-body
>   `^[a-z0-9][a-z0-9_-]{0,63}$`. The discriminator tests below turn a drifted
>   impl red.
> - **D2** — config-overlay is OUT of scope; a negative assertion guards against
>   it being introduced.
> - **D3** — per-slug health is registry/CLI-side only; a negative test asserts
>   NO per-slug network endpoint exists.
> - **D4** — `delete` is de-register-only (data dir preserved); `--purge` is the
>   only destroy and is loud (slug-name confirmation); de-register → re-register
>   **RE-ATTACHES** to the preserved hash chain, never clobbers it. (registry-CLI)
> - **D5** — `register` rejects reserved route segments `{v1, health, observe,
>   tools}` — a check SEPARATE from the D1 charset. `tools` is the critical
>   shadowing case (default-project alias). (registry-CLI / projects-config)
> - **D6** — `register` idempotence is TWO-STATE: already-routing → loud error;
>   dir-exists-but-de-registered → re-attach success (NOT an error). (registry-CLI)
> - **Funnel-honesty** — Wave 2 eliminates the Wave-1 `let _store` discard /
>   residual fixed-adapter path; every request dispatches through `adapter_for(key)`.
>   (project-router)

---

## 1. Overall Test Strategy

Wave 2 is **one trait-impl swap behind the merged seam, plus the registry/config
that backs it.** The seam (`StoreResolver`, `SlugRouter`, `ProjectKey`,
`ProjectSlug`, `RouteError`, `DefaultResolver`) is already on `main` and unit-tested.
Wave 2 adds:

| Component | Source | Test plan |
|-----------|--------|-----------|
| `ProjectRouter` as `StoreResolver` impl (slug → per-slug store, hot caches, drop-in swap at `SlugRouter::new`) | `crates/unimatrix-server/src/http/router.rs` | `project-router.md` |
| `ProjectRegistry` + lifecycle CLI (`register`/`list`/`delete`; creates `/data/.unimatrix/{slug}/`) | `crates/unimatrix-server/src/projects.rs` *(new)* | `project-registry-cli.md` |
| `[[projects]]` config + slug validation (D1 regex) | `crates/unimatrix-server/src/infra/config.rs` | `projects-config.md` |

Three test tiers:

1. **Unit (cargo, `#[tokio::test]`/`#[test]`)** — the load-bearing tier for Wave 2.
   The slug allowlist, the `ProjectRouter::resolve_store` swap behavior, per-slug
   store-map lookup, registry lifecycle, config parse/validate, and the D2/D3
   negative assertions all live here. Extends the existing conventions in
   `crates/unimatrix-server/src/http/router/tests.rs` (Mock adapter, `collect_body`,
   `Request::builder`) and `seam.rs`'s slug-parse tests — **cumulative, no isolated
   scaffolding.**
2. **Rust HTTP integration (`crates/unimatrix-server/tests/`)** — per-slug routing
   and **per-slug isolation** are only observable through the HTTP `/v1/{slug}/`
   edge. infra-001 cannot reach this edge (see §4). New file
   `tests/project_routing_integration.rs` drives `PathRouter` with a real
   `ProjectRouter` resolver over two real per-slug stores and asserts routing +
   isolation + backward-compat.
3. **infra-001 smoke (MANDATORY gate)** — proves the single-project default path
   (`serve --stdio`, `ProjectKey::Default`) is unchanged: the AC-W2-R2 / AC-CT-C4
   "additive, zero behavior change" guarantee, exercised end-to-end. infra-001
   does NOT exercise slug routing (it spawns `serve --stdio --project-dir` —
   single-project, no HTTP slug path); it is the **backward-compat regression
   gate**, not the routing test.

### Source-grade (grep/structural) assertions
Several ACs are unrepresentability / no-bypass / no-new-surface guarantees that a
behavioral test cannot fully prove. These get explicit source-grade checks:
- AC-CT-C4 / R-01: the resolver is the SOLE Wave1↔Wave2 swap point; `PathRouter::new`
  still takes `Arc<dyn StoreResolver>` (a reverted bypass would not compile against
  the test) — structural assertion (per pattern #4963).
- AC-CT-C6: `BearerValidator` / `TlsConfig` / slug seams present and not collapsed.
- D3 / AC-W1-S6: no unauthenticated route beyond `GET /health`; no per-slug HTTP
  health endpoint — negative route-probe + grep.
- D2: no config-overlay merge surface added by Wave 2 — grep + config-struct review.

---

## 2. Risk → Test Mapping (Wave 2 + cross-wave)

From `../RISK-TEST-STRATEGY.md`. Wave 2 owns R-01 (swap), R-03 (slug allowlist),
R-06 (transport 1:1), R-13 (additive addressing); it co-owns R-04 (seam parity) and
R-10/R-12 (seam preservation / no-new-surface).

| Risk | Priority | Wave-2 scenario | Test(s) | Component plan |
|------|----------|-----------------|---------|----------------|
| **R-01** | Critical | Trait swap doesn't break the funnel; `ProjectRouter` injected THROUGH the seam, hot routing INSIDE `resolve_store` not a new edge | `test_projectrouter_swaps_at_slugrouter_callsite`, `test_pathrouter_new_takes_resolver_trait_object` (structural), `test_projectrouter_routing_inside_resolve_store` | project-router |
| **R-03** | High (fix-before-merge) | Slug allowlist rejects traversal/encoded separators; D1 regex EXACT; no fs escape | Security table T-SEC-01..14 incl. **64-char-reject** & **underscore-reject** discriminators | projects-config |
| **R-04** | High | One seam, two resolvers; Default path unchanged under `ProjectRouter`; slug never leaks into local path, path-hash never into slug | `test_projectrouter_default_key_unchanged`, `test_slug_and_pathhash_disjoint` | project-router |
| **R-06** | High | N:1 clients share one slug store, attributed by `session_id`; identity transport-only | `test_n_clients_one_slug_shared_store`, `test_no_payload_project_field` (structural) | project-router |
| **R-10** | Medium | `BearerValidator`/`TlsConfig`/slug seams not collapsed (AC-CT-C6) | `test_auth_scope_transport_seams_intact` (structural) | project-router |
| **R-12** | Medium | No unauth endpoint beyond `/health`; no per-slug network health (D3, AC-W1-S6) | `test_no_per_slug_health_endpoint`, `test_only_health_unauthenticated` (negative) | project-router |
| **R-13** | Low | `[[projects]]`-absent ⇒ `/v1/tools/…` unchanged; additive `/{slug}` | `test_projects_absent_backward_compat`, infra-001 smoke | projects-config / project-router |

### Stage 3a refinement → test mapping (D4/D5/D6 + funnel)

| Decision | Scenario | Test(s) | Risk / AC | Component plan |
|----------|----------|---------|-----------|----------------|
| **D5** | `register tools` → REJECT (shadows `/v1/tools/…` default alias) — reserved check SEPARATE from charset | `test_register_rejects_reserved_tools_shadowing`, `test_register_reserved_is_separate_from_charset`, `test_register_rejects_reserved_route_segments`, `test_register_reserved_exact_match_only`; mirror `test_reserved_check_is_separate_from_charset` | R-03 (allowlist family), AC-W2-R4 | registry-CLI §A.2 / projects-config |
| **D4** | `delete` de-registers + preserves dir; `--purge` destroys loudly (slug confirmation); re-register RE-ATTACHES preserved chain | `test_delete_deregisters_and_preserves_data_dir`, `test_purge_requires_slug_confirmation_or_no_destroy`, `test_purge_with_confirmation_removes_dir_and_deregisters`, **`test_deregister_reregister_reattaches_to_preserved_chain`** (highest-value), `test_purge_then_register_is_fresh_store` | R-04 (per-slug dirs), R-11 (fail-loud), AC-W2-R4 | registry-CLI §C |
| **D6** | `register` two-state: already-routing → loud error; de-registered-dir-exists → re-attach success | `test_register_already_routing_errors_loud`, `test_register_dir_exists_deregistered_reattaches`, `test_register_two_states_distinct_messages` | R-11, AC-W2-R4 | registry-CLI §A.3 |
| **Funnel** | No residual fixed-adapter bypass; every request dispatches via `adapter_for(key)`; ≥2 slugs + Default each serviced by their own adapter/store | `test_no_residual_fixed_adapter_path` (structural), `test_dispatch_through_adapter_for_no_fixed_bypass` (HTTP integration) | R-01, AC-CT-C4 | project-router §A + integration |

### AC → primary verification

| AC-ID | Verification | Where |
|-------|--------------|-------|
| AC-W2-R1 | `/v1/{slug}/…` routes to per-slug store (two slugs → two stores) | HTTP integration |
| AC-W2-R2 | `[[projects]]`-absent ⇒ `/v1/tools/…` unchanged | unit + infra-001 smoke |
| AC-W2-R3 | Write slug A → unreadable/unwritable from slug B | HTTP integration |
| AC-W2-R4 | register/list/delete lifecycle, incl. D5 reserved refusal, D6 two-state register, D4 de-register/`--purge`/re-attach integrity | registry CLI unit |
| AC-W2-R5 | N clients : 1 slug, per-`session_id` attribution; each bound to one slug | project-router unit + HTTP integration |
| AC-W2-R6 | slug allowlist rejects traversal/encoded; no fs escape (SR-09) | projects-config security table |
| AC-CT-C4 | additive seam swap; Wave-1 Default path unchanged; no client re-point; **no residual fixed-adapter bypass** (every request via `adapter_for`) | project-router structural + HTTP integration + infra-001 smoke |
| AC-CT-C6 | token/slug/cert seams not collapsed | project-router structural |

---

## 3. Cross-component test dependencies

- **Slug grammar is shared.** `ProjectSlug::TryFrom` (in `seam.rs`, merged) is the
  single allowlist. `projects-config.md` owns the exhaustive security table against
  it; `project-registry-cli.md` reuses the SAME newtype for `register <slug>`
  validation (register must reject the same corpus the router rejects — no second,
  drifting validator). A test asserts register and route reject an identical corpus.
- **Registry → Router.** `ProjectRouter` is constructed from the `ProjectRegistry` /
  `[[projects]]` map. Router tests use a registry/map fixture of ≥2 slugs.
- **Config → Registry.** `[[projects]]` parse feeds the registry at boot; a malformed
  slug in config must fail loud at load (config plan), not reach the router.
- **Default path invariance is multi-owner.** AC-W2-R2 / AC-CT-C4 is asserted in
  config (absence → Default), router (Default key returns the one store), and
  infra-001 smoke (end-to-end unchanged).

---

## 4. Integration Harness Plan (MANDATORY — server feature)

### 4.1 infra-001 (Python, stdio MCP) — role: BACKWARD-COMPAT GATE

infra-001 spawns `serve --stdio --project-dir <dir>` — a **single-project, no-HTTP**
process. It exercises `ProjectKey::Default` only; the `/v1/{slug}/` HTTP edge is
unreachable from it. Therefore infra-001's job in Wave 2 is to prove the
single-project default path is **unchanged** by the resolver swap (AC-W2-R2 /
AC-CT-C4 regression), NOT to test slug routing.

- **Smoke (`pytest -m smoke`) — MANDATORY minimum gate.** Must pass green:
  store→get→search→correct→briefing→restart all behave exactly as on `main`.
  Any smoke red after the resolver swap = the swap regressed the Default path.
- **Recommended suites:** `tools`, `lifecycle` (store/retrieval + restart
  persistence; schema/storage-adjacent), `protocol` (handshake unaffected).
- **No new infra-001 tests** are added for slug routing — infra-001 has no HTTP
  slug transport. Adding one would require new harness infrastructure → out of
  scope; file a follow-up GH Issue only if HTTP-transport harness coverage is
  later wanted (per USAGE-PROTOCOL "significant harness infrastructure" rule).

### 4.2 Rust HTTP integration (`crates/unimatrix-server/tests/`) — role: ROUTING + ISOLATION

This is where AC-W2-R1 / R-3 / R-5 are actually proven, because slug routing is
HTTP-path-only. New file: **`tests/project_routing_integration.rs`**, following the
existing `tests/` conventions (`pipeline_e2e.rs`, `client_bundle_e2e.rs`) and the
router unit-test helpers (`collect_body`, `Request::builder`, `BoxBody` test body).

Scenarios (detailed assertions in `project-router.md`):

| Test | AC | What it proves |
|------|----|----|
| `test_two_slugs_route_to_distinct_stores` | AC-W2-R1 | `/v1/alpha/…` and `/v1/beta/…` land in different `Arc<Store>` instances |
| `test_slug_a_write_unreadable_from_slug_b` | AC-W2-R3 | store an entry via slug A; identical query via slug B returns nothing (read isolation) |
| `test_slug_a_write_does_not_appear_in_slug_b` | AC-W2-R3 | write isolation: B's store + hash chain untouched by A's write |
| `test_v1_tools_default_unchanged_with_projects` | AC-W2-R2/CT-C4 | with `[[projects]]` present, `/v1/tools/…` still resolves Default |
| `test_unregistered_slug_returns_unknown_project` | R-01 sc.3 | `/v1/ghost/…` → `RouteError::UnknownProject` (404), never a default-store fallback, never a panic |
| `test_n_clients_one_slug_share_store` | AC-W2-R5 | two sessions on `/v1/alpha/` see each other's writes; `session_id` attribution preserved |
| `test_only_health_unauthenticated` | R-12/D3 | probe `/v1/{slug}/health`-style + arbitrary paths unauthenticated → only `GET /health` answers; no per-slug health |

**Fixture note:** build two real `UnimatrixServer`/`Store` instances over temp dirs,
wrap in a `ProjectRouter` resolver keyed by `{alpha, beta}`, inject into
`PathRouter::new(resolver, project_router, observe_ctx)`. This reuses the merged
seam wiring (pattern #4963) — no new edge.

### 4.3 Suite selection summary (per USAGE-PROTOCOL table)

| Feature touches | Suites run |
|-----------------|-----------|
| Any server tool logic | `tools`, `protocol` |
| Store/retrieval + schema/storage (per-slug stores) | `lifecycle`, `volume` |
| Security (slug validation boundary) | covered by Rust security table + `security` smoke subset |
| Any change | `smoke` (mandatory) |

---

## 5. Self-check

- [x] Risks mapped from RISK-TEST-STRATEGY (R-01,03,04,06,10,12,13) to scenarios
- [x] Integration harness plan: infra-001 role (backward-compat gate) + new Rust HTTP
      integration tests for slug routing/isolation; smoke is the mandatory gate
- [x] Component plans map 1:1 to the brief's component map
- [x] D1 discriminators (64-char-reject, underscore-reject) called out and routed to
      `projects-config.md`
- [x] D2 (no config-overlay) and D3 (no per-slug network health) negative tests planned
- [x] D5 reserved-slug discriminators (`tools` shadowing + separate-from-charset) planned
- [x] D4 delete/`--purge`/re-attach integrity tests planned (re-attach is highest-value)
- [x] D6 register two-state (loud error vs re-attach) planned, states distinct
- [x] Funnel no-bypass: `adapter_for` dispatch correctness for ≥2 slugs + Default; discard path gone
- [x] All output under `wave2/test-plan/`
