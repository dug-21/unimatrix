# vnc-040 Pseudocode — OVERVIEW

> Per-Slug Configuration Overlay Resolution (C6 / Feature A of #785).
> Source of truth: ARCHITECTURE.md §2–§9, ADR-001..004, SPECIFICATION.md FR-01..FR-16,
> RISK-TEST-STRATEGY.md R-01..R-14. This overview defines the component boundaries, the data
> that crosses them, shared types, the `Cow` fallthrough, and the module-home decision.

## Components (3)

| # | Component | File (impl target) | Kind |
|---|-----------|--------------------|------|
| 1 | `slug_config_classification` — ADR-004 registry | `infra/config.rs` (colocated with `merge_configs`) | NEW, DATA-ONLY |
| 2 | `resolve_slug_config` — overlay helper | `http_provision.rs` (EXISTING module; see decision below) | NEW |
| 3 | `per_slug_loop` — call-site modification | `main.rs:1089-1110` (+ relocate `main.rs:687`) | MODIFY |

`merge_configs`, `load_single_config`, `validate_config` (all in `infra/config.rs`) and
`build_project_server` (`http_provision.rs:132`) are REUSED UNCHANGED — no per-component file.

## Module-home decision for `resolve_slug_config` — PLACE IN `http_provision.rs`

**Decision: `resolve_slug_config` lives in the EXISTING `http_provision.rs` module, NOT a new
`slug_config.rs` file.** Rationale (per the brief's module-wiring note):

- `main.rs` declares exactly ONE local module: `mod http_provision;` (main.rs:6). A new
  `slug_config.rs` would require adding `mod slug_config;` at the crate root in `main.rs` — the
  SAME module-root region the `per_slug_loop` agent edits. Two agents editing main.rs's top-of-file
  `mod` lines in parallel is a needless merge-collision hazard (the swarm-shared-worktree git
  hazard).
- `http_provision.rs` already owns the call-site (`build_project_server`), already imports
  `ProjectSlug`, `UnimatrixConfig`, `ServerError`, `Arc`, and `std::path::Path`, and already
  derives `base_dir.join(slug.as_str())` (http_provision.rs:159). The overlay helper is the
  call-site's own concern; colocating it needs ZERO new `mod` wiring.
- File-size guard: `http_provision.rs` plus `resolve_slug_config` (~50–70 lines incl. the
  no-file/file-present arms + tests live in a `#[cfg(test)]` mod) must stay ≤500 lines. If the
  combined file would exceed 500 lines, THEN split — and only then add `mod slug_config;`,
  documenting that the `per_slug_loop` agent must NOT also touch that `mod` block. As of the
  fact-check `http_provision.rs` is well under budget; expect colocation to hold. **Flag for
  Gate 3b: re-confirm the ≤500-line budget after the helper lands.**

## Shared types (all EXISTING except the two ADR-004 types)

| Type | Origin | Used by |
|------|--------|---------|
| `UnimatrixConfig` | `infra/config.rs` (existing struct; NO new fields) | all 3 |
| `Cow<'a, UnimatrixConfig>` | std; `resolve_slug_config` return | 2 → 3 |
| `ProjectSlug` (`.as_str()`, allowlist-validated) | `http/router/seam.rs:67` | 2, 3 |
| `ServerError::Config(String)` | `error.rs:119` | 2 (and propagated by 3) |
| `ConfigError` | `infra/config.rs` (returned by reused fns) | 2 (mapped → `ServerError::Config`) |
| `Arc<EmbedServiceHandle>` / `Arc<NliServiceHandle>` / `Arc<RayonPool>` | daemon handles (`main.rs:880-901` scope) | 3 (Arc::clone only) |
| `Arc<InferenceConfig>` / `Arc<ConfidenceParams>` / `Arc<CategoryAllowlist>` / `Arc<DomainPackRegistry>` / `HashSet<String>` | derived per slug from resolved | 3 |
| `enum OverlayDisposition { PerSlugOverlayable, GlobalLocked }` | NEW (ADR-004), `infra/config.rs` | 1 |
| `struct ConfigKeyClass { key: &'static str, disposition: OverlayDisposition }` | NEW (ADR-004) | 1 |
| `const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass]` | NEW (ADR-004) | 1 (drift-guard test, Feature B, §9 table all RENDER from it) |
| `fn is_per_slug_overlayable(key: &str) -> bool` | NEW (ADR-004) | 1 |

## Data flow across boundaries (per slug, in the loop)

```
                  daemon-global (built ONCE before loop)
   embed_handle:Arc, nli_handle:Arc, ml_inference_pool:Arc, permissive:bool, config:UnimatrixConfig
        │
   per_slug_loop (component 3), for slug in &project_slugs:
        │
   (0) UNCONDITIONAL clones — OUTSIDE any overlay branch (ADR-002 §1, FR-04):
        embed = Arc::clone(&embed_handle)        ── fields 0-2 NEVER read from `resolved`
        pool  = Arc::clone(&ml_inference_pool)
        nli   = Arc::clone(&nli_handle)
        permissive (P) passed unconditionally from the global daemon flag (FR-15)
        │
   (1) resolved: Cow<UnimatrixConfig> = resolve_slug_config(base_dir, slug, &config)?   (component 2)
        │            ├ no file  → Cow::Borrowed(&config)   (NO merge, NO re-derive — ADR-002 §4)
        │            └ file     → Cow::Owned(merged)       (load→validate→merge→validate)
        │
   (2) derive 7 OVERLAYABLE values from &*resolved (fields 3-9 + instructions):
        instructions (I) = resolved.server.instructions.clone()   (relocated from main.rs:687, FR-14)
        nli_top_k, nli_enabled, inference_config(Arc), confidence_params(Arc),
        categories(Arc), observation_registry(Arc), boosted_categories(HashSet)
        │
   (3) build_project_server(base_dir, slug, &embed, permissive, instructions,
                            &pool, &nli, nli_top_k, nli_enabled,
                            &inference_config, &confidence_params,
                            &categories, &observation_registry, &boosted_categories)?   (REUSED, UNCHANGED)
```

## The `Cow` fallthrough (ADR-002 §4 / AC-02 / R-03)

- **No-file arm** of `resolve_slug_config` returns `Cow::Borrowed(global)`. The loop's step (2)
  derivations then read from the GLOBAL config — and MUST produce values byte-for-byte equal to the
  daemon's own resolved values. The 3 global handles in step (0) are the daemon's already-built
  parity `Arc`s; the test asserts `Arc::ptr_eq` between daemon handle and per-slug clone (machine-
  checked, not value-equality). NO `merge_configs` runs on this arm.
- **File-present arm** returns `Cow::Owned(merged)`. Step (2) derives fresh `Arc`s from the owned
  merged config. The 3 global handles are STILL cloned in step (0), unchanged — the file arm never
  touches fields 0–2.
- Construction obligation (both arms): the loop reads fields 0–2 only from the daemon handles, never
  from `&*resolved`. This is the R-04 / SR-07 by-construction guarantee.

## CRITICAL upstream-signature correction (GAP — flag to rust-dev + architect)

The IMPLEMENTATION-BRIEF and ARCHITECTURE §9 both state
`merge_configs(global: &UnimatrixConfig, project: &UnimatrixConfig) -> UnimatrixConfig`
(by-reference). **The LIVE signature takes OWNED values:**
`fn merge_configs(global: UnimatrixConfig, project: UnimatrixConfig) -> UnimatrixConfig`
(`config.rs:3825`). Likewise `validate_config(config: &UnimatrixConfig, path: &Path)` and
`load_single_config(path: &Path) -> Result<UnimatrixConfig, ConfigError>` are by-value-returning /
by-ref-taking exactly as the brief states. Consequence for `resolve_slug_config`: it holds `global`
by reference, so it must `global.clone()` to feed `merge_configs` (which consumes its first arg);
the slug file is already owned from `load_single_config`. See `resolve_slug_config.md` step detail.
This is a one-clone-per-slug-with-a-file cost, startup-only, negligible — NOT a signature change to
`merge_configs`. Reuse stays intact.

## Sequencing constraints (Stage 3b build order)

1. **Component 1 (classification registry)** can be built first/independently — pure data + a
   predicate + the AC-11 drift-guard test against `merge_configs`. No dependency on 2 or 3.
2. **Component 2 (`resolve_slug_config`)** depends only on the EXISTING reused fns. Independent of 1
   and 3. Build in parallel with 1.
3. **Component 3 (`per_slug_loop`)** depends on component 2's signature (`resolve_slug_config`) and
   on the relocated `instructions` source. Build LAST (or stub the call against 2's locked
   signature). Components 2 and 3 BOTH live near the call site but in DIFFERENT files
   (`http_provision.rs` vs `main.rs`) given the module-home decision — no shared-line collision.

## Reuse obligations carried into the pseudocode (do NOT rewrite)

- `merge_configs` — REUSED. SR-02/R-02 re-audit obligation: the inline `InferenceConfig {…}` literal
  (#4070) is confirmed to list every field explicitly with NO `..Default()` tail
  (`config.rs:3895-4260`). rust-dev MUST re-confirm this still holds at implementation time before
  trusting reuse for the global→per-slug call shape. **No change expected; flag only.**
- `load_single_config` — REUSED; carries the 64 KiB cap (#2395) + `#[cfg(unix)]` `mode()&0o022==0`
  check (NFR-03/04, R-10). Exercised on the per-slug path, not assumed.
- `validate_config` — REUSED; called TWICE in component 2 (per-file + post-merge, ADR-003).
- `build_project_server` — REUSED, signature UNCHANGED. Component 3 changes only WHICH values the
  caller derives.
