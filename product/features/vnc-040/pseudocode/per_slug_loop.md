# Component 3 — `per_slug_loop` (call-site MODIFY)

> MODIFY `main.rs:1089-1110` (the `for slug in &project_slugs` loop) + RELOCATE the `instructions`
> source from `main.rs:687`. `build_project_server` signature is UNCHANGED.
> ADR-001 (#5209 derivation), ADR-002 (#5206 by-construction invariants + fallthrough). FR-03,
> FR-04, FR-08, FR-14, FR-15. AC-01, AC-02, AC-04, AC-07, AC-10. R-03, R-04, R-07, R-12.

## Purpose

For each routed slug: hold the 3 global model/pool handles + `permissive` GLOBAL by construction;
call `resolve_slug_config` to get the per-slug-resolved config; derive the 7 overlayable values from
it (6 engine/knowledge `Arc`s + `instructions`); pass all into the UNCHANGED `build_project_server`.
On the no-file arm, the resolved config IS the global config, so derivations equal the daemon's own
values byte-for-byte.

## Current code (baseline, to be replaced) — main.rs:1089-1110

```
for slug in &project_slugs {
    let input = http_provision::build_project_server(
        base_dir, slug, &embed_handle, permissive,
        server_instructions.clone(),                    // ← global hoist from main.rs:687, fanned to all
        &ml_inference_pool, &nli_handle,
        config.inference.nli_top_k, config.inference.nli_enabled,
        &inference_config, &confidence_params, &categories,
        &observation_registry, &boosted_categories,
    ).await?;
    slug_servers.push(input);
}
```

## Relocation (FR-14, carry-item 5) — `main.rs:687`

- **DELETE / NEUTRALIZE** the pre-loop hoist `let server_instructions = config.server.instructions.clone();`
  as the SOURCE fanned to every slug. (If `server_instructions` is used elsewhere — e.g. the daemon's
  own ServiceLayer — leave THAT use intact; only the per-slug fan-out moves.)
  **rust-dev: grep `server_instructions` usages before deleting — relocate ONLY the per-slug source.**
- Per-slug `instructions` is now sourced INSIDE the loop from `resolved.server.instructions`.

## Modified loop pseudocode

```
for slug in &project_slugs {

    // (0) UNCONDITIONAL — OUTSIDE/AHEAD of any overlay branch (ADR-002 §1, FR-04, R-04, SR-07).
    //     Fields 0-2 + permissive are NEVER read from `resolved` on ANY path.
    let embed = Arc::clone(&embed_handle)         // the ONE embedding model
    let pool  = Arc::clone(&ml_inference_pool)    // the shared rayon pool
    let nli   = Arc::clone(&nli_handle)           // the ONE NLI model
    // `permissive` (P): pass the global daemon flag UNCONDITIONALLY (FR-15) — used directly at (3).

    // (1) Resolve per-slug config (component 2). `config` is the daemon global resolved config.
    let resolved: Cow<UnimatrixConfig> = resolve_slug_config(base_dir, slug, &config)?
    //     no file → Cow::Borrowed(&config)  (NO merge — derivations below read the GLOBAL config)
    //     file    → Cow::Owned(merged)
    //     `?` propagates ServerError::Config to startup (fail-loud, names the slug).

    // (2) Derive the 7 OVERLAYABLE values from &*resolved (fields 3-9 + instructions, FR-03/FR-14).
    //     Mirror EXACTLY how the daemon builds its own ServiceLayer values (main.rs:880-901) so the
    //     no-file arm is byte-for-byte (AC-02). Read ONLY from resolved — never the model handles.
    let r = &*resolved
    let instructions      = r.server.instructions.clone()              // (I) #785 per-slug knob (FR-14)
    let nli_top_k         = r.inference.nli_top_k                      // (3)
    let nli_enabled       = r.inference.nli_enabled                   // (4)
    let inference_config  = Arc::new(/* InferenceConfig snapshot from r.inference */)   // (5) pins already global-won in merge
    let confidence_params = Arc::new(/* ConfidenceParams from r.confidence.weights */)  // (6)
    let categories        = Arc::new(/* CategoryAllowlist from r.knowledge.categories + lifecycle */) // (7)
    let observation_reg   = Arc::new(/* DomainPackRegistry from r.observation.domain_packs */)         // (8)
    let boosted_categories: HashSet<String> = /* from r.knowledge.boosted_categories */                // (9)
    //     rust-dev: REUSE the SAME constructor expressions the daemon uses at main.rs:880-901 for
    //     these 7 values, sourced from `r` instead of the global `config`. On the no-file arm
    //     r == &config, so each value equals the daemon's own (AC-02 value-equality holds by reuse).

    // (3) Build the per-slug server — UNCHANGED build_project_server signature.
    let input = http_provision::build_project_server(
        base_dir, slug,
        &embed,                       // field 0 — global clone (NEVER from resolved)
        permissive,                   // P — global daemon flag (NEVER from resolved, FR-15)
        instructions,                 // I — from resolved.server.instructions (FR-14)
        &pool,                        // field 1 — global clone
        &nli,                         // field 2 — global clone
        nli_top_k, nli_enabled,       // fields 3-4 — from resolved
        &inference_config,            // field 5 — from resolved
        &confidence_params,           // field 6 — from resolved
        &categories,                  // field 7 — from resolved
        &observation_reg,             // field 8 — from resolved
        &boosted_categories,          // field 9 — from resolved
    ).await?

    slug_servers.push(input)
}
```

## The closed call-site verdict, AS WIRED (AC-07 / R-07 — every arg classified, none dropped)

| Arg (build_project_server) | Source in loop | Verdict |
|----------------------------|----------------|---------|
| `base_dir`, `slug` | loop identity | routing identity (not config) |
| `embed_handle` (field 0) | `Arc::clone(&embed_handle)` step (0) | GLOBAL-LOCKED (never from resolved) |
| `permissive` (P) | global daemon flag, step (3) | GLOBAL-LOCKED (FR-15) |
| `instructions` (I) | `resolved.server.instructions` | OVERLAYABLE (FR-14) |
| `rayon_pool` (field 1) | `Arc::clone(&ml_inference_pool)` | GLOBAL-LOCKED |
| `nli_handle` (field 2) | `Arc::clone(&nli_handle)` | GLOBAL-LOCKED |
| `nli_top_k`/`nli_enabled` (3-4) | `resolved.inference.*` | OVERLAYABLE |
| `inference_config` (5) | from `resolved.inference` | OVERLAYABLE except `*_sha256` pins |
| `confidence_params` (6) | from `resolved.confidence.weights` | OVERLAYABLE |
| `categories` (7) | from `resolved.knowledge.categories` | OVERLAYABLE |
| `observation_registry` (8) | from `resolved.observation.domain_packs` | OVERLAYABLE |
| `boosted_categories` (9) | from `resolved.knowledge.boosted_categories` | OVERLAYABLE |

Every arg the live call passes has a row. This verdict RENDERS from the component-1 registry
(ADR-004) — it is not an independent split.

## Data flow

- **Inputs (loop-scope, daemon-global):** `project_slugs`, `base_dir`, `config` (global resolved),
  `embed_handle`, `nli_handle`, `ml_inference_pool`, `permissive`, `slug_servers` (accumulator).
- **Per-iteration output:** one `ProjectServerInput` pushed to `slug_servers`.
- **Boundary in:** `Cow<UnimatrixConfig>` from `resolve_slug_config`.
- **Boundary out:** the 14-arg `build_project_server` call.

## Error handling

- `resolve_slug_config(...)?` and `build_project_server(...).await?` both propagate via `?` to the
  startup path — any bad slug file fails the daemon LOUD at startup naming the slug (R-11, NFR-05).
  No partial-serve, no request-time degradation.

## By-construction obligations (carry-item 4, R-04, R-03 — construction proofs, not just behavioral)

- Fields 0–2 are `Arc::clone`d in step (0), textually OUTSIDE and AHEAD of the
  `resolve_slug_config` call. There is NO code path in the loop reading `embed`/`pool`/`nli` from
  `resolved`. Reviewer (AC-04/C) and `Arc::ptr_eq` test (AC-02) confirm this.
- `permissive` is passed from the global flag in step (3); it is never assigned from `resolved`.
- On the no-file arm, `&*resolved == &config`, so step (2) derivations equal the daemon's own
  ServiceLayer values byte-for-byte (AC-02) — achieved by REUSING the daemon's constructor
  expressions, just re-sourced from `r`.

## Key test scenarios (hints for tester)

1. **AC-02 / R-03 #1,#3** — no-file slug: all ~12 threaded inputs == global-only crt-056 path
   (value-equality); `Arc::ptr_eq` on `embed`/`nli`/`pool` against the daemon handles (pointer
   identity, not value-equality).
2. **AC-04 / R-04 #1-2** — N=2 model-free harness (#5172): exactly one NLI + one embedding handle
   resident at N≥2 with distinct slug configs; construction review that fields 0–2 are cloned
   unconditionally outside the overlay branch.
3. **AC-01** — N=2: distinct `[knowledge].categories` per slug → A's allowlist reflects A, B's
   reflects B, global underlies both.
4. **AC-10 / R-12** — N=2: distinct `[server] instructions` per slug → A's served instructions ≠
   B's; a slug with no override falls through to global `resolved.server.instructions` (NOT
   empty/default). Both arms.
5. **AC-07 / R-07** — one verdict assertion per call-site arg; closed-checklist guard that the
   row-set == the live arg-set (a future added arg breaks the test loudly).
6. **AC-06 / R-09** — a per-slug file setting transport keys: served transport == global; the loop
   never reads a transport field from `resolved`.

## Anti-patterns guarded

- DO NOT read `embed`/`pool`/`nli`/`permissive` from `resolved` (R-04, FR-04/FR-15).
- DO NOT change `build_project_server`'s signature (ADR-001) — only the derived values change.
- DO NOT re-derive Arcs through a merge on the no-file arm (R-03) — `Cow::Borrowed` means the loop
  reads the global config directly.
- DO NOT leave the old `main.rs:687` per-slug fan-out of `server_instructions` (FR-14).
