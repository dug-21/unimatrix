# vnc-040 — Per-Slug Configuration Overlay Resolution — Architecture

> Feature A of #785. Delivers capability C6 (per-slug configuration, Unimatrix #5148).
> Designed FROM the APPROVED, LOCKED scope: per-key overlay reusing `merge_configs` /
> `load_single_config` / `validate_config`; integration at the `build_project_server`
> call-site loop (`main.rs:1089-1110`), NOT in `load_config`; both models global; whole
> embedding descriptor locked global-wins; byte-for-byte fallthrough; restart-applies.

## 1. System Overview

Unimatrix runs one daemon that can route many project slugs (vnc-034 gave each slug its own
isolated store at `{base_dir}/{slug}/unimatrix.db` + `vector/`). Configuration, however, has
remained **daemon-global**: `load_config` (`infra/config.rs`) resolves a single
`UnimatrixConfig` from a fixed layering (compiled defaults → global → per-project path-hash →
`UNIMATRIX_CONFIG` env → env bool). crt-056 (#789) then threaded that one resolved config — 9
`Arc`-cloned parity params plus the pre-existing `embed_handle` — into `build_project_server`
so every slug serves at config parity with the daemon. crt-056 ADR-002 explicitly **reserved
the per-slug-override seam for #785** and added no override parameter.

vnc-040 fills exactly that reserved seam. It introduces a **third precedence layer** —
`{base_dir}/{slug}/config.toml` — overlaid per-key onto the daemon's already-resolved global
config, at the `build_project_server` call site, on the same restart that re-attaches routing
(vnc-038 ADR-007; no hot-reload). Two different-domain projects on one daemon can then own
their own categories, domain packs, confidence weights, and inference tuning, while exactly one
NLI model and exactly one embedding model stay loaded in memory.

The change is deliberately confined: a small per-slug resolution helper plus a change to the
call-site loop. `load_config`'s global→project→env layering is untouched. Transport config is
never read at this seam.

## 2. Component Breakdown

| Component | Location (existing/new) | Responsibility |
|-----------|-------------------------|----------------|
| `resolve_slug_config` (NEW helper) | `unimatrix-server` (call-site module; e.g. `http_provision.rs` or a sibling `slug_config.rs`) | Given `base_dir`, `slug`, and the daemon's `&global_resolved: &UnimatrixConfig`, return a per-slug-resolved `UnimatrixConfig`. Probes the per-slug file; on absence returns the global config **unchanged** (fallthrough sentinel); on presence loads + per-file validates + merges + **re-validates the merged result**. Single owner of the overlay decision. |
| `load_single_config` (REUSE) | `infra/config.rs:3783` | Parse one TOML file into a `UnimatrixConfig`. Already used by `load_config` per layer. Reused verbatim for the per-slug file. |
| `validate_config` (REUSE) | `infra/config.rs:3413` | Validate a `UnimatrixConfig` (field-level + cross-field, incl. the fusion/PPR sum-of-six invariant and custom-preset prohibition). Called TWICE in the new path: per-file AND post-merge (SR-01). |
| `merge_configs` (REUSE) | `infra/config.rs` (≈3825) | Per-key replace merge: per-project value if it differs from compiled `Default`, else global; `Option` fields use `.or()`; hash-pin fields (`embedding_model_sha256`, `nli_model_sha256`) are **global-wins** (#4655). Reused as `merge_configs(global_resolved, slug_file)`. |
| `build_project_server` (REUSE, unchanged signature) | `http_provision.rs:132` | Builds one per-slug server from the threaded values. Its signature is **unchanged** — vnc-040 changes only WHAT values the caller derives, not the function. |
| Per-slug loop (MODIFY) | `main.rs:1089-1110` | For each slug: call `resolve_slug_config`, derive the 7 overlayable values (the 6 engine/knowledge `Arc`s + `instructions`) from the resolved config, and `Arc::clone` the 3 global handles + pass `permissive` UNCONDITIONALLY. Pass all into `build_project_server`. |
| `PER_SLUG_CONFIG_CLASSIFICATION` + `is_per_slug_overlayable` (NEW, declarative) | `infra/config.rs` (colocated with `merge_configs`/`validate_config`) | **The single canonical source of truth for the per-slug-overlayable-vs-global-locked split (ADR-004).** A minimal const slice of `ConfigKeyClass { key, disposition }` enumerating each call-site config key/section as `PerSlugOverlayable` or `GlobalLocked`, plus a `is_per_slug_overlayable(key)` predicate. Data-only; no merge logic, no new config knob. The §9 verdict table renders from it, a drift-guard test binds `merge_configs`' real behavior to it, and Feature B's seed annotations render from it. Resolves R-13 / the crt-031 literal-duplication risk: the split has exactly one owner. |

**No new config struct, no new config section, no new tunable field** (scope Non-Goal). The
overlay operates over the EXISTING `UnimatrixConfig` surface.

## 3. Component Interactions / Data Flow at the Call Site

```
main.rs per-slug loop (for slug in &project_slugs):
  │
  ├─ (0) UNCONDITIONAL — outside any overlay branch (SR-03/SR-07):
  │      let nli   = Arc::clone(&nli_handle);     // the ONE NLI model
  │      let embed = Arc::clone(&embed_handle);   // the ONE embedding model
  │      let pool  = Arc::clone(&ml_inference_pool); // shared rayon pool
  │      let permissive = permissive;             // daemon permission flag — GLOBAL-LOCKED
  │      The three handles are NEVER sourced from a merged config. Ever.
  │      `permissive` is a process/daemon-level permission flag, NOT knowledge
  │      config — it is passed unchanged from the global daemon value, never
  │      read from `resolved`.
  │
  ├─ (1) let resolved = resolve_slug_config(base_dir, slug, &config)?;
  │        resolve_slug_config:
  │          let path = base_dir.join(slug.as_str()).join("config.toml");
  │          if !path.exists() {
  │              return Ok(Cow::Borrowed(&global));   // FALLTHROUGH SENTINEL (SR-04)
  │              // byte-for-byte: NO merge, NO re-derive — the global config itself.
  │          }
  │          let slug_file = load_single_config(&path)?;          // parse
  │          validate_config(&slug_file, &path)?;                 // per-file (AC-08)
  │          let merged = merge_configs(&global, &slug_file);      // 3rd layer, reuse
  │          validate_config(&merged, &path)?;                    // POST-MERGE (SR-01)
  │          Ok(Cow::Owned(merged))
  │
  ├─ (2) Derive the 7 OVERLAYABLE values from `resolved` (the 6 engine/knowledge
  │      Arcs + `instructions`; never the model handles, never `permissive`):
  │      instructions       = resolved.server.instructions   // #785 per-slug knob — see below
  │      nli_top_k          = resolved.inference.nli_top_k
  │      nli_enabled        = resolved.inference.nli_enabled
  │      inference_config   = Arc::new(resolved.inference …)   // pins already global-won in merge
  │      confidence_params  = Arc::new(from resolved.confidence.weights)
  │      categories         = Arc::new(from resolved.knowledge.categories + lifecycle)
  │      observation_reg    = Arc::new(from resolved.observation.domain_packs)
  │      boosted_categories = (from resolved.knowledge.boosted_categories)
  │      NOTE: when fallthrough (Cow::Borrowed), these derive from the GLOBAL config —
  │      and MUST equal the daemon's own resolved values byte-for-byte (AC-02).
  │      `instructions` CHANGE (#785): today it is sourced global at main.rs:687
  │      (`let server_instructions = config.server.instructions.clone();`) and
  │      fanned to every slug identically at main.rs:1095. vnc-040 sources it from
  │      `resolved.server.instructions` at this seam instead of the global
  │      `server_instructions` var — so a per-slug file can tune it (#785 names it
  │      explicitly: "operators can tune [knowledge] categories, [server]
  │      instructions, confidence weights … per project").
  │      RESOLVED (code inspection — was the spec/architecture open question of
  │      whether `instructions` is merged config or plumbed separately): it is a
  │      TRIVIAL THREAD-THROUGH, identical to the other overlayable derivations.
  │      `ServerConfig.instructions: Option<String>` is a real config field
  │      (config.rs:428) and `merge_configs` ALREADY merges it project-wins
  │      (config.rs:3862-3864: `instructions: project.server.instructions
  │      .or(global.server.instructions)`) — NO merge_configs change. And
  │      `build_project_server` ALREADY takes `instructions: Option<String>`
  │      (http_provision.rs:137) — NO signature change. The ONLY delivery action
  │      is relocating the source from the pre-loop hoist (main.rs:687) into the
  │      per-slug loop, reading `resolved.server.instructions`.
  │
  └─ (3) build_project_server(base_dir, slug, &embed, permissive, instructions,
                              &pool, &nli, nli_top_k, nli_enabled,
                              &inference_config, &confidence_params,
                              &categories, &observation_reg, &boosted_categories)
         // embed_handle and nli_handle passed as the global clones — never from `resolved`.
         // permissive passed as the global daemon flag — never from `resolved`.
         // instructions NOW sourced from `resolved.server.instructions` (was global).
```

**Fallthrough optimization (SR-04):** the `None`/no-file arm returns the global config by
reference and the loop reuses the same `Arc`s the daemon already built. To make the
byte-for-byte guarantee structural rather than re-derived, the per-slug loop MUST reuse the
daemon's already-constructed parity `Arc`s directly in the no-file arm (the same `Arc::clone`s
crt-056 passes today), and only build fresh `Arc`s in the file-present arm. The no-file
guarantee is asserted as **pointer equality**, not value equality: for the 3 global handles
(`embed_handle` / `nli_handle` / `rayon_pool`) the test MUST assert `Arc::ptr_eq` between the
daemon's handle and the per-slug clone — exactly the assertion crt-056 AC-2 already uses. This
converts "no re-derivation" from a review-only property into a machine-checked one: a regression
that rebuilt or re-derived any of the three would fail `Arc::ptr_eq` even if the rebuilt value
happened to compare equal. The invariant is: no-file ⇒ no merge, no re-derivation, and `ptr_eq`
holds for the 3 handles.

## 4. The Per-Slug Resolution Helper (`resolve_slug_config`)

Single responsibility, single owner of the overlay policy. Contract:

- **Input:** `base_dir: &Path`, `slug: &ProjectSlug`, `global: &UnimatrixConfig`.
- **Output:** `Result<Cow<'_, UnimatrixConfig>, ServerError>` (or an owned `UnimatrixConfig`
  with the no-file arm cloning — spec decides; `Cow` keeps fallthrough allocation-free).
- **No-file arm:** return the global config unchanged. No merge. This is the hard fallthrough
  sentinel for the single-project / local-UDS majority (SR-04, AC-02).
- **File-present arm, in order:**
  1. `load_single_config(&path)` — parse TOML, fail loud naming the slug file (AC-08).
  2. `validate_config(&slug_file, &path)` — per-file validation (AC-08, #2286 discipline).
  3. `merge_configs(global, &slug_file)` — third precedence layer; hash-pin global-wins and
     the embedding descriptor lock both ride inside this existing function (§6).
  4. `validate_config(&merged, &path)` — **post-merge re-validation (SR-01, see §5).**
  5. Return `Cow::Owned(merged)`.
- **Path:** `{base_dir}/{slug}/config.toml` — sibling of `unimatrix.db`/`vector/`, the SAME
  path Feature B will later seed (SR-06; recorded so B builds on A without re-litigation).
- **Errors:** any parse/validation failure returns `ServerError::Config` naming the offending
  slug file; startup fails loud. No `.unwrap()`.

The helper lives at the call-site module so the overlay seam stays at `build_project_server`'s
caller, NOT in `load_config`.

## 5. The Merged-Config Re-Validation Point (SR-01 — HIGH)

**Decision: `validate_config(&merged, &path)` runs inside `resolve_slug_config`, immediately
after `merge_configs` and before the merged config is returned/consumed.** This is exactly the
fix pattern Unimatrix #3905 prescribes for `load_config`, now applied to the third layer.

Why per-file validation is insufficient: #3905 proved that a two-level merge can pass each
file's independent validation yet produce a merged struct that violates a **cross-field**
invariant — the canonical case is `InferenceConfig`'s sum-of-six fusion weights
(`w_sim + w_nli + w_conf + w_coac + w_util + w_prov ≤ 1.0`). A global file setting some weights
non-default and a per-slug file setting *other* weights non-default each validate alone, but the
field-by-field merge combines them into a sum > 1.0. vnc-040 adds a THIRD layer, widening this
surface (global, project, per-slug now all contribute fields to one merged struct).

Cross-field constraints in `UnimatrixConfig` that the post-merge `validate_config` MUST re-check
(spec to enumerate exhaustively from `validate_config`; known classes):
- Fusion-weight sum-of-six ≤ 1.0 (`[inference]` weights).
- PPR / confidence weight constraints.
- Custom-preset cross-level inheritance prohibition (#3923 — enforced in `validate_config`).
- Any category/instruction size or well-formedness bound.

Because the existing `validate_config` already encodes every one of these, calling it on the
merged struct is sufficient — vnc-040 adds **no new validation logic**, only a second invocation
site on a new merge result. Startup-only cost is negligible.

## 6. How Both Model Invariants Hold By Construction (SR-03 / SR-07)

Two distinct mechanisms, both structural — not test-dependent:

### 6a. The three global handles are cloned OUTSIDE any merge branch (SR-07)
Fields 0 (`embed_handle`), 1 (`rayon_pool`), 2 (`nli_handle`) are `Arc::clone`d from the
daemon's single loaded handles **unconditionally**, before and independent of
`resolve_slug_config`. They are NEVER read from the merged config. A merged config cannot rebuild
or reselect a model because the construction path never consults it for these three. This makes
crt-056 AC-2 (exactly one NLI + one embedding model in memory at N≥2 slugs) hold by construction
— there is no code path that loads a second model. (Restates ADR-002's "Arc::clone, never
rebuild" and extends it to the embedding handle.) The no-file fallthrough makes this
machine-checked: the test asserts `Arc::ptr_eq` between the daemon's handle and each per-slug
clone for all three (mirroring crt-056 AC-2), so any re-derivation fails even when the rebuilt
value compares equal.

### 6b. The embedding descriptor is locked global-wins inside the merge (SR-03)
SR-03's concern: even with handles cloned correctly, a per-slug file could set an embedding
**descriptor** key, making the merged config *describe* a model the served handle is not — a
config-vs-handle divergence (Unimatrix #5196). Two facts close this:

1. **There is no separate `[embedding]` config section in the current `UnimatrixConfig`
   surface.** Confirmed by code inspection: the only embedding-model descriptor field is
   `inference.embedding_model_sha256` (the hash pin, `config.rs:515`). There is no
   `embedding.model`, `embedding.dimensions`, or `embedding.model_name` key. The embedding model
   identity is fixed at daemon load (`embed_handle`), not config-selectable beyond the pin. So
   "lock the whole `[embedding]` section" resolves today to: **lock `embedding_model_sha256`** —
   which `merge_configs` ALREADY makes global-wins (#4655, `config.rs:3905-3920`) — and add **no
   new embedding descriptor field** (scope Non-Goal: no new config knobs). The section is locked
   by construction because the only overlayable descriptor is the pin, and the pin is global-wins.
2. **Forward guard (recorded for Feature B / future work):** if any future feature adds an
   `[embedding].model` / `.dimensions` descriptor field, it MUST be added global-wins inside
   `merge_configs` (per #5196), symmetric with the transport lock. vnc-040 does not add it; the
   architecture records the invariant so it is not silently violated later. (See A2 below.)

Together: the served `embed_handle` stays the global model (6a) AND the merged config can never
describe a different embedding model (6b) — AC-04 holds for both models.

### 6c. nli_top_k / nli_enabled are runtime params, not model identity (SR-08)
Fields 3–4 are overlayable. Confirmed (scope OQ-2): `nli_top_k` and `nli_enabled` are runtime
inference parameters consumed by `ServiceLayer`, not model-selection inputs — they tune how the
shared `nli_handle` is queried, never which model loads. A per-slug override changes query
behavior against the one shared handle, consistent with 6a.

## 7. Transport Stays Global (AC-06)

Transport config (TLS / auth / host / `http.enabled`) is resolved once at daemon boot and is
**never read at the per-slug seam**. `resolve_slug_config` produces a `UnimatrixConfig`, but the
per-slug loop derives ONLY the 6 engine/knowledge `Arc`s from it; it never reads transport
fields from the merged config. The HTTP listener is already constructed from the global config
before the per-slug loop runs. No code path threads a per-slug transport value. (Symmetric with
the embedding lock — both are "locked global" boundaries of the C6 `done_when`.)

### 7a. Documented residual: stray GLOBAL-only sections in a per-slug file (known limitation)
A per-slug `config.toml` may technically set a GLOBAL-only section — e.g. `[server.tls]`,
`[http]`, or another transport/daemon field. Because the per-slug loop derives ONLY the 7
overlayable values from `resolved` and never reads transport/daemon fields from it, such a key is
**silently ignored** at the seam: it is parsed and (where it has a cross-field rule) validated,
but never consumed. The only divergence that warns today is a `*_sha256` pin mismatch against the
global pin (AC-05). A stray `[server.tls]`/transport key produces no warning.

**This is a documented design limitation, not a defect.** Section ownership (per-slug-overlayable
vs global-locked) is documented for operators via **Feature B's annotated seed file**, which is
the authoritative place a slug operator learns which sections are honored. A seam-level warn that
flags stray global-section keys in a per-slug file is an **OPTIONAL future enhancement** — vnc-040
deliberately does NOT add field-comparison machinery now (it would require enumerating and diffing
every global-only field, out of scope for A). The residual is recorded here so Feature B / future
work can pick it up without re-discovering it.

## 8. ADR-003 → dsn-001 #2286 Reconciliation (RESTATED so it is not re-litigated)

The issue's "ADR-003 replace semantics" refers to **dsn-001 #2286** (the two-level
global→project TOML merge), **NOT** crt-056's ADR-003 (#5166, the per-slug ServiceLayer handle
set). #2286 established **replace semantics for all field types**: a per-project field absent
falls through to global; present overrides global entirely; list fields
(categories / boosted_categories) **replace, not append**, because they are complete policy
declarations. This is precisely the **per-key (not section-replace)** behavior C6 mandates.

The vnc-040 overlay is a **THIRD precedence layer atop the existing global→project pair, using
the SAME field-level replace discipline** via the SAME `merge_configs` function. No new merge
model, no conflict — C6 extends the established #2286 pattern by adding one more level. The only
exception class to replace semantics (hash-pin global-wins, #4655) is preserved unchanged. This
reconciliation is closed; downstream phases inherit it as settled.

## 9. Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `build_project_server` | `pub async fn build_project_server(base_dir: &Path, slug: &ProjectSlug, embed_handle: &Arc<EmbedServiceHandle>, permissive: bool, instructions: Option<String>, rayon_pool: &Arc<RayonPool>, nli_handle: &Arc<NliServiceHandle>, nli_top_k: usize, nli_enabled: bool, inference_config: &Arc<InferenceConfig>, confidence_params: &Arc<ConfidenceParams>, categories: &Arc<CategoryAllowlist>, observation_registry: &Arc<DomainPackRegistry>, boosted_categories: &HashSet<String>) -> Result<ProjectServerInput, ServerError>` — **UNCHANGED by vnc-040** | `http_provision.rs:132-156` |
| Per-slug loop (call site) | `for slug in &project_slugs { … build_project_server(…) … }` — MODIFY to derive per-slug values | `main.rs:1089-1110` |
| `resolve_slug_config` (NEW) | `fn resolve_slug_config(base_dir: &Path, slug: &ProjectSlug, global: &UnimatrixConfig) -> Result<Cow<'_, UnimatrixConfig>, ServerError>` | NEW — call-site module |
| `merge_configs` | `fn merge_configs(global: &UnimatrixConfig, project: &UnimatrixConfig) -> UnimatrixConfig` (per-key replace; hash-pin global-wins) | `infra/config.rs` ≈3825 |
| `load_single_config` | `fn load_single_config(path: &Path) -> Result<UnimatrixConfig, ConfigError>` | `infra/config.rs:3783` |
| `validate_config` | `pub fn validate_config(config: &UnimatrixConfig, path: &Path) -> Result<(), ConfigError>` — called per-file AND post-merge | `infra/config.rs:3413` |
| `PER_SLUG_CONFIG_CLASSIFICATION` (NEW, ADR-004) | `pub const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass]` where `ConfigKeyClass { key: &'static str, disposition: OverlayDisposition }` and `enum OverlayDisposition { PerSlugOverlayable, GlobalLocked }`; plus `pub fn is_per_slug_overlayable(key: &str) -> bool` — THE canonical per-slug-vs-global split; §9 table renders from it, drift-guard test binds `merge_configs` to it, Feature B renders seed annotations from it | NEW — `infra/config.rs`, colocated with `merge_configs` |
| Drift-guard consistency test (NEW, ADR-004) | for each `ConfigKeyClass`: build global+slug configs differing only on `key`, run `merge_configs`, assert overlayable⇒merged==slug, locked⇒merged==global (incl. `*_sha256` global-wins) | NEW — `infra/config.rs` test module |
| Per-slug config file | `{base_dir}/{slug}/config.toml` — `base_dir = paths.data_dir.parent()` | `main.rs:1087`, `http_provision.rs:159` |
| Embedding descriptor field (locked) | `inference.embedding_model_sha256: Option<String>` — ONLY embedding descriptor; already global-wins | `config.rs:515`, merge at `config.rs:3905-3920` |
| `instructions` (NOW overlayable) | `Option<String>` — `ServerConfig.instructions` (config.rs:428); ALREADY merged project-wins by `merge_configs` (config.rs:3862-3864) and ALREADY accepted by `build_project_server` (http_provision.rs:137). RESOLVED: trivial thread-through, no merge or signature change | new source `main.rs:687` (global) → `resolved.server.instructions`; passed at `main.rs:1095` |
| `permissive` (GLOBAL-locked) | `bool` — daemon/process permission flag; passed unchanged from the global daemon value | `main.rs` daemon flag → `build_project_server` arg 4 |
| `embed_handle` / `nli_handle` / `ml_inference_pool` | `Arc<EmbedServiceHandle>` / `Arc<NliServiceHandle>` / `Arc<RayonPool>` — daemon's single loaded handles; `Arc::clone` only | `main.rs:880-898` scope |
| `ServerError::Config` | `ServerError::Config(String)` — fail-loud variant for invalid slug file | `unimatrix-server` error type |

**Closed call-site overlay verdict (AC-07) — mirror of crt-056 AC-1. The guarantee is NO
CALL-SITE INPUT IS ABSENT FROM THE CHECKLIST, not a count.** This table is the human-readable
**rendering of the canonical `PER_SLUG_CONFIG_CLASSIFICATION` registry (ADR-004)**, not an
independent assertion of the split: the registry in `config.rs` is the single source of truth,
this table renders it, a drift-guard test binds `merge_configs` to it, and Feature B's seed
annotations render from it — so the split has exactly one owner and the three consumers cannot
diverge (R-13 / crt-031 literal-duplication mitigation, owned in A not deferred to B). Every
argument the per-slug loop passes to `build_project_server` gets an explicit verdict row. `build_project_server`'s real
signature takes `base_dir, slug, embed_handle, permissive, instructions, <9 crt-056 params>`; of
these, `base_dir`/`slug` are routing identity (not config), leaving ~12 config-relevant inputs
enumerated below. Reframed from the earlier "10 inputs" framing precisely because `permissive` and
`instructions` were absent from it.

| Input | Verdict | Construction guarantee |
|-------|---------|------------------------|
| `base_dir` | routing identity (not config) | the daemon data dir; identical for all slugs |
| `slug` | routing identity (not config) | the loop variable; the slug being provisioned |
| `embed_handle` (+ embedding descriptor: `embedding_model_sha256`) | **GLOBAL — locked, hard** | `Arc::clone` outside merge (6a); `Arc::ptr_eq`-asserted on the no-file arm; descriptor global-wins in `merge_configs`; no other descriptor field exists (6b) |
| `permissive` | **GLOBAL — locked** | daemon/process permission flag, NOT knowledge config; passed unchanged from the global daemon value; never read from `resolved` |
| `instructions` | **per-slug (NEW, #785)** | sourced from `resolved.server.instructions` at the seam (was global `server_instructions`); #785 names `[server] instructions` an explicit per-slug knob |
| `rayon_pool` | **GLOBAL — locked** | `Arc::clone` outside merge (6a); `Arc::ptr_eq`-asserted on the no-file arm |
| `nli_handle` | **GLOBAL — locked, hard** | `Arc::clone` outside merge (6a); `Arc::ptr_eq`-asserted on the no-file arm |
| `nli_top_k` | overlayable | runtime param, not model identity (6c) |
| `nli_enabled` | overlayable | runtime param, not model identity (6c) |
| `inference_config` | overlayable EXCEPT `*_sha256` pins | `merge_configs` inference arm splits weights (replace) from pins (global-wins) |
| `confidence_params` | **per-slug** | derived from merged `confidence.weights` |
| `categories` | **per-slug** | derived from merged `knowledge.categories` + lifecycle |
| `observation_registry` | **per-slug** | derived from merged `observation.domain_packs` |
| `boosted_categories` | **per-slug** | derived from merged `knowledge.boosted_categories` |

`adapt_service` is built inside `build_project_server` from `AdaptConfig::default()`
(`http_provision.rs:208`) — per-slug-independent state, NOT one of the 10, NOT operator-
configurable. **Decision: leave it default** (scope OQ-5 RESOLVED; recorded for Feature B).

## 10. Assumptions Carried Forward (from SCOPE-RISK-ASSESSMENT)

- **A1:** `merge_configs` global→project semantics transfer cleanly to global→per-slug. The
  function is symmetric in its two `UnimatrixConfig` args (base vs override); the per-slug file
  is just another override layer. SR-02 mitigation: spec/delivery MUST re-audit the
  `merge_configs` `inference` arm (the inline `InferenceConfig { … }` literal, #4070 — the site
  grep-for-spread misses) to confirm every field is handled for the global→per-slug call shape;
  do not assume the project arm covers it identically without inspection.
- **A2:** the per-slug vector index uses `VectorConfig::default()` (`http_provision.rs:182`),
  NOT config-driven dimensions. The embedding-descriptor lock (§6b) depends on this: if a future
  change makes vector dims config-driven, the divergence (SR-03) re-opens and an `[embedding]`
  section lock must be added global-wins. Spec MUST note this dependency.
- **A3:** the crt-056 call-site seam at `main.rs:1089-1110` / `http_provision.rs:132-156` is
  stable (C5 proved on #789). Design is against the live merged signature
  (`base_dir, slug, embed_handle, permissive, instructions, <9 crt-056 params>`), enumerating
  every config-relevant argument — not the issue's "8 fields" shorthand.
