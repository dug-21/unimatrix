# Test Plan: `build_project_server` (config-parity threading)

> Component: `crates/unimatrix-server/src/http_provision.rs:125-204` (today's signature `125-131`).
> Change (ADR-002): append 8 config-parity params (params-at-end); build the config-driven
> `ServiceLayer`; pass `Some(service_layer)` to the constructor. Replace per-slug
> `AdaptConfig::default()` / `CategoryAllowlist::new()` defaults at `180-181` with threaded operator
> values (adapt stays per-slug independent state).
> Risks: **R-05** (partial threading), **R-12** (N model copies / unloaded handle), R-11 (adapt).
> ACs: **AC-1** (8-field parity), **AC-2** (shared model). FR-1..FR-6, FR-9, FR-10.

This is the hand-threaded 8-field boundary — the #2398-class "new fields not propagated to all call
sites" risk. **Field-by-field AC-1 is the only adequate boundary test;** a representative subset is a
coverage gap and must be rejected.

---

## Unit test expectations

### AC-1 — 8-field field-by-field equality vs the daemon's RESOLVED config
- `test_build_project_server_config_parity_all_fields`
  - **Arrange:** resolve a daemon config with **non-default** values for every field (NLI on,
    custom `nli_top_k`, non-default `InferenceConfig`, non-default `ConfidenceParams`, a populated
    `CategoryAllowlist`, a non-built-in `observation_registry` domain pack set, rayon pool size > 1).
  - **Act:** `build_project_server(..., <8 threaded params>)`; inspect the resulting per-slug
    `ServiceLayer`.
  - **Assert — all 8, individually (no subset):**
    1. `nli_enabled` == daemon resolved flag
    2. `nli_top_k` == daemon resolved value
    3. `nli_handle` is the daemon's shared loaded handle (`Arc::ptr_eq`, see AC-2)
    4. `inference_config` == daemon resolved (field by field)
    5. `confidence_params` == daemon resolved (field by field)
    6. `category_allowlist` == daemon resolved set
    7. `observation_registry` (domain packs) == daemon resolved set
    8. effective rayon pool size == daemon resolved pool size
  - **`session_capabilities` is OUT (ADR-006/FR-10) — NOT asserted here or anywhere.**

### AC-1 — NLI flag both directions (FR-2)
- `test_build_project_server_nli_enabled_when_config_on`
- `test_build_project_server_nli_disabled_when_config_off`
  - Proves the flag is **threaded**, not hardcoded either way. (The pre-crt-056 defect hardcoded it
    off.)

### AC-1 — global-config-only guard (FR-9, keeps #785/C6 out)
- `test_build_project_server_resolves_global_config_only`
  - **Assert:** there is no per-slug override parameter/path; all slugs built from the same global
    config resolve to identical values. (R-09 adjacent — no per-slug config overlay surface.)

### AC-2 — shared loaded model, no copies (FR-6, R-12)
- `test_build_project_server_uses_shared_nli_handle`
  - **Assert:** the per-slug `nli_handle` is `Arc::ptr_eq` to the daemon's loaded handle; the per-slug
    embedding handle likewise. No `NliServiceHandle::new()` is constructed on this path.
- **Source audit (R-12):** grep `http_provision.rs` (and the per-slug path) for
  `NliServiceHandle::new(` — MUST be absent on the per-slug build path (the pre-crt-056 defect at
  `server.rs:306-333` constructed a fresh unloaded handle).

### R-11 — adapt_service per-slug independent, threaded operator categories
- `test_build_project_server_adapt_service_per_slug_independent`
  - **Assert:** each per-slug build gets its own `AdaptationService` instance (independent state,
    same config) — `adapt_service` is NOT shared across slugs. Cross-slug bleed proven absent
    behaviorally adjacent to AC-4 (`multi-slug-harness.md`).
- `test_build_project_server_threads_operator_categories`
  - **Assert:** the `180-181` defaults (`AdaptConfig::default()` is the resolved adapt value;
    `CategoryAllowlist::new()` empty) are replaced by the threaded operator `CategoryAllowlist` —
    not an empty allowlist.

---

## Integration boundary note

The per-slug call site is `main.rs:1085-1092` (covered in `daemon-http-boot.md`). The
params-at-end signature is the propagation boundary: every appended param must be `Arc::clone`d from
the daemon's resolved values (`main.rs:880-898`). A field silently keeping a default is R-05's exact
failure — AC-1's all-8-fields assertion is the detector.

## Edge cases / failure modes

- **Config field missing/unresolved at boot:** the build MUST fail loudly, not fall back to a test
  default (a silent fallback recreates the original defect). *Testable:* a build with an unthreaded
  field should not silently degrade to defaults — caught by AC-1 (the field would mismatch).
- **NLI config-disabled:** per-slug NLI off, downstream tick NLI op no-ops, no spurious rayon work
  (the NLI-off direction of FR-2).

## Coverage requirement

AC-1 asserts **every** field of the closed 8-field checklist against the resolved config — a subset
assertion is a coverage gap and must be rejected in review (R-05). AC-2 = `Arc::ptr_eq` + absence of
`NliServiceHandle::new()` on the per-slug path (R-12 source audit).
