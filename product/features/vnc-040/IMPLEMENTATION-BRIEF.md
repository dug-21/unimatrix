# Implementation Brief — vnc-040: Per-Slug Configuration Overlay Resolution (C6 / Feature A)

> Feature A of GH #785. Delivers capability **C6 — Per-slug configuration** (Unimatrix #5148).
> Compiled from the APPROVED, LOCKED Session 1 design (revised at the design-gate correction).
> All vision checks PASS; zero variances. Downstream consumers: pseudocode, rust-dev, tester, validators (Session 2).

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-040/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-040/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-040/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-040/architecture/ARCHITECTURE.md |
| ADR-001 (overlay at call site, #5209) | product/features/vnc-040/architecture/ADR-001-per-slug-overlay-at-call-site.md |
| ADR-002 (invariants + fallthrough by construction, #5206) | product/features/vnc-040/architecture/ADR-002-model-invariants-and-fallthrough-by-construction.md |
| ADR-003 (post-merge re-validation, #5199) | product/features/vnc-040/architecture/ADR-003-post-merge-revalidation.md |
| ADR-004 (canonical per-slug classification, #5210) | product/features/vnc-040/architecture/ADR-004-canonical-per-slug-classification.md |
| Risk-Based Test Strategy | product/features/vnc-040/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-040/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-040/ACCEPTANCE-MAP.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| resolve_slug_config (NEW helper) | pseudocode/resolve_slug_config.md | test-plan/resolve_slug_config.md |
| Per-slug loop (MODIFY, main.rs:1089-1110) | pseudocode/per_slug_loop.md | test-plan/per_slug_loop.md |
| Per-slug classification registry (NEW, declarative — `PER_SLUG_CONFIG_CLASSIFICATION` + `is_per_slug_overlayable`, ADR-004) | pseudocode/slug_config_classification.md | test-plan/slug_config_classification.md |

> `build_project_server`, `merge_configs`, `load_single_config`, `validate_config` are REUSED unchanged — no
> per-component pseudocode/test files; the new behavior lives in the three components above. The classification registry
> (ADR-004) is DATA-ONLY — it does NOT rewrite `merge_configs`; its test is the AC-11 drift-guard binding `merge_configs`'
> real behavior to the registry. Pseudocode and test-plan paths are produced in Session 2 Stage 3a; this map lists the
> expected components from the architecture.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Resolve a **per-slug** `UnimatrixConfig` at the `build_project_server` call site by overlaying a slug's own
`{base_dir}/{slug}/config.toml` onto the daemon's already-resolved global config via per-key merge, then threading the
per-slug-resolved values through the overlayable call-site inputs (the 9 crt-056 params plus `server.instructions`) so
each slug's `ServiceLayer` reflects its own categories, domain packs, confidence weights, inference tuning, and
**instructions**. Both loaded models (NLI + embedding), the shared rayon pool, and the daemon `permissive` flag stay
global **by construction** (hard invariants); when no per-slug file exists, behavior is byte-for-byte identical to the
current global-only crt-056 path. The per-slug-overlayable-vs-global-locked split has **exactly one owner** — a
declarative classification registry in `infra/config.rs` (ADR-004) — and a machine-checked drift-guard test pins
`merge_configs`' real behavior to it so the split can never silently diverge across its consumers.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Where the overlay seam lives | New `resolve_slug_config` helper at the call-site module, invoked in the per-slug loop (`main.rs:1089-1110`); NOT in `load_config`. Reuses `merge_configs`/`load_single_config`/`validate_config` unchanged. | ADR-001 (#5209) | architecture/ADR-001-per-slug-overlay-at-call-site.md |
| Model invariants (NLI + embedding + pool) | Held BY CONSTRUCTION: fields 0–2 `Arc::clone`d from the daemon's single handles UNCONDITIONALLY, outside any merge branch; never sourced from a merged config on any path. | ADR-002 (#5206) | architecture/ADR-002-model-invariants-and-fallthrough-by-construction.md |
| `[embedding]`-section lock | Resolves to **pin-global-wins + forward guard** (R-06): no `[embedding]` section exists today — the only descriptor is `inference.embedding_model_sha256`, already global-wins (#4655). No new descriptor field is added (scope Non-Goal). A documented simplification, NOT a variance. | ADR-002 (#5206) | architecture/ADR-002-model-invariants-and-fallthrough-by-construction.md |
| `permissive` verdict | **GLOBAL-LOCKED** — daemon/process permission flag, NOT engine/knowledge config. Passed unconditionally from the global daemon flag; never sourced from a merged config (FR-15). Symmetric with transport. | ADR-001 (#5209) | architecture/ADR-001-per-slug-overlay-at-call-site.md |
| `instructions` verdict | **PER-SLUG OVERLAYABLE** (#785, FR-14). RESOLVED a trivial thread-through: `merge_configs` ALREADY merges `server.instructions` project-wins (`config.rs:3862-3864`); `build_project_server` ALREADY accepts `instructions: Option<String>` (`http_provision.rs:137`). No merge change, no signature change. | ADR-001 (#5209) | architecture/ADR-001-per-slug-overlay-at-call-site.md |
| `nli_top_k` / `nli_enabled` verdict | Overlayable — runtime inference params that tune how the shared `nli_handle` is queried, not model identity (OQ-2). | ADR-002 (#5206) | architecture/ADR-002-model-invariants-and-fallthrough-by-construction.md |
| Byte-for-byte fallthrough | Structural AND machine-checked: no-file arm returns `Cow::Borrowed(&global)` / reuses the daemon's already-built parity `Arc`s — no merge runs, nothing re-derived. AC-02 asserts `Arc::ptr_eq` on the 3 global handles. | ADR-002 (#5206) | architecture/ADR-002-model-invariants-and-fallthrough-by-construction.md |
| Single canonical per-slug-vs-global classification | ONE declarative source of truth — `PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass]` + `enum OverlayDisposition { PerSlugOverlayable, GlobalLocked }` + `fn is_per_slug_overlayable(key)`, colocated with `merge_configs` in `infra/config.rs`. **Data-only — does NOT rewrite `merge_configs`** (no generic merge engine). The verdict table RENDERS from it; Feature B's seed annotations RENDER from it (one-way: A owns, B consumes). Integrity = AC-11 drift-guard test pinning `merge_configs`' real overlay-vs-lock behavior to the registry (crt-031 anti-divergence). Retires R-13's "unowned split" status. | ADR-004 (#5210) | architecture/ADR-004-canonical-per-slug-classification.md |
| Post-merge cross-field re-validation | `validate_config(&merged, &path)` runs INSIDE `resolve_slug_config`, after `merge_configs`, before return — in addition to per-file validation (#3905 third-layer fix). | ADR-003 (#5199) | architecture/ADR-003-post-merge-revalidation.md |
| ADR-003 → dsn-001 #2286 reconciliation | The issue's "ADR-003 replace semantics" = dsn-001 #2286 (field-level replace), NOT crt-056 ADR-003. C6 is a THIRD precedence layer using the SAME replace discipline. No conflict. | ADR-001 (#5209) | architecture/ADR-001-per-slug-overlay-at-call-site.md |
| `adapt_service` | Left `AdaptConfig::default()`; not overlaid, not operator-configurable. Recorded for Feature B (OQ-5). | ADR-001 (#5209) | architecture/ADR-001-per-slug-overlay-at-call-site.md |
| Feature A/B breadth | A only — operator hand-places the file. Feature B (seeding) is OUT of scope; shared `{base_dir}/{slug}/config.toml` path recorded so B builds on A (SR-06). | ADR-001 (#5209) | architecture/ADR-001-per-slug-overlay-at-call-site.md |

## Files to Create / Modify

| File | Change | Summary |
|------|--------|---------|
| `unimatrix-server` call-site module (`http_provision.rs` or new sibling `slug_config.rs`) | CREATE | `resolve_slug_config(base_dir, slug, &global) -> Result<Cow<UnimatrixConfig>, ServerError>` — sole owner of the overlay decision (probe → load → per-file validate → merge → post-merge validate). |
| `main.rs:1089-1110` (per-slug loop) | MODIFY | Per slug: `Arc::clone` the 3 global handles + pass `permissive` UNCONDITIONALLY; call `resolve_slug_config`; derive the 7 overlayable values (6 `Arc`s + `instructions`) from the resolved config; pass all into unchanged `build_project_server`. |
| `main.rs:687` (instructions hoist) | RELOCATE | Move the `instructions` source from the pre-loop hoist (`let server_instructions = config.server.instructions.clone()`, fanned to every slug at `main.rs:1095`) INTO the per-slug loop, sourcing `resolved.server.instructions`. Bounded carry-item — no merge/signature change (FR-14). |
| `infra/config.rs` (`merge_configs` inference arm, ≈3825) | RE-AUDIT (no change expected) | SR-02/R-02: confirm the inline `InferenceConfig {…}` literal (#4070) handles every field for the global→per-slug call shape before trusting reuse. `merge_configs` is NOT rewritten by ADR-004. |
| `infra/config.rs` (classification registry, colocated with `merge_configs`) | CREATE | ADR-004: `enum OverlayDisposition`, `struct ConfigKeyClass`, `const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass]`, `fn is_per_slug_overlayable(key)`. Data-only; the single owner of the per-slug-vs-global split. |
| `infra/config.rs` test module (drift-guard, ADR-004) | CREATE | AC-11: for EVERY registry entry, drive `merge_configs` with a global+per-slug pair differing only on that key; assert `PerSlugOverlayable`⇒slug value wins, `GlobalLocked`⇒global wins (incl. the `*_sha256` global-wins carve-out). Mandatory anti-divergence guarantee. |
| Test module(s) for the above | CREATE | 32 scenarios across 14 risks; N=2 model-free harness (#5172). |

## Data Structures

- **`UnimatrixConfig`** (existing) — the config struct merged/threaded; no new fields added.
- **`Cow<'_, UnimatrixConfig>`** — `resolve_slug_config` return; `Borrowed(&global)` on no-file (allocation-free
  fallthrough), `Owned(merged)` on file-present.
- **Threaded `Arc`s** (existing, derived per slug): `Arc<InferenceConfig>`, `Arc<ConfidenceParams>`,
  `Arc<CategoryAllowlist>`, `Arc<DomainPackRegistry>`, `HashSet<String>` (boosted_categories).
- **`instructions: Option<String>`** (existing) — `ServerConfig.instructions` (`config.rs:428`); now sourced per-slug
  from `resolved.server.instructions` (was global `main.rs:687`). Already merged project-wins by `merge_configs`
  (`config.rs:3862-3864`).
- **Global handles** (existing, `Arc::clone` only): `Arc<EmbedServiceHandle>`, `Arc<NliServiceHandle>`,
  `Arc<RayonPool>`.
- **`permissive: bool`** (existing) — daemon permission flag; passed unchanged from the global value.
- **`inference.embedding_model_sha256: Option<String>`** (`config.rs:515`) — the ONLY embedding descriptor; already
  global-wins in `merge_configs` (`config.rs:3905-3920`).
- **`enum OverlayDisposition { PerSlugOverlayable, GlobalLocked }`** (NEW, ADR-004) — the disposition of one config key.
- **`struct ConfigKeyClass { key: &'static str, disposition: OverlayDisposition }`** (NEW, ADR-004) — one classified key
  (stable id, e.g. `"knowledge.categories"`, `"inference.embedding_model_sha256"`, `"server.instructions"`).
- **`const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass]`** (NEW, ADR-004) — THE canonical per-slug-vs-global split.
  Single source of truth; the verdict table, `merge_configs`' behavior (via the drift-guard test), and Feature B's seed
  annotations all reduce to it. `PerSlugOverlayable`: `knowledge.categories`, `knowledge.boosted_categories`,
  `confidence.weights`, `observation.domain_packs`, overlayable `inference.*` weights, `server.instructions`,
  `nli_top_k`, `nli_enabled`. `GlobalLocked`: `inference.embedding_model_sha256`, `inference.nli_model_sha256`,
  `permissive`, and transport/daemon sections (`server.tls`, `http`, auth/host).

## Function Signatures

```rust
// NEW — call-site module
fn resolve_slug_config(
    base_dir: &Path,
    slug: &ProjectSlug,
    global: &UnimatrixConfig,
) -> Result<Cow<'_, UnimatrixConfig>, ServerError>;

// NEW — infra/config.rs (declarative, ADR-004; colocated with merge_configs). DATA-ONLY.
pub enum OverlayDisposition { PerSlugOverlayable, GlobalLocked }
pub struct ConfigKeyClass { pub key: &'static str, pub disposition: OverlayDisposition }
pub const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass] = &[ /* … */ ]; // single source of truth
pub fn is_per_slug_overlayable(key: &str) -> bool;

// REUSED — infra/config.rs (unchanged)
fn load_single_config(path: &Path) -> Result<UnimatrixConfig, ConfigError>;            // :3783
pub fn validate_config(config: &UnimatrixConfig, path: &Path) -> Result<(), ConfigError>; // :3413, called per-file AND post-merge
fn merge_configs(global: &UnimatrixConfig, project: &UnimatrixConfig) -> UnimatrixConfig;  // ≈3825; hash-pin global-wins; instructions project-wins :3862-3864

// REUSED — http_provision.rs:132-156 (UNCHANGED by vnc-040)
pub async fn build_project_server(
    base_dir: &Path, slug: &ProjectSlug, embed_handle: &Arc<EmbedServiceHandle>,
    permissive: bool, instructions: Option<String>, rayon_pool: &Arc<RayonPool>,
    nli_handle: &Arc<NliServiceHandle>, nli_top_k: usize, nli_enabled: bool,
    inference_config: &Arc<InferenceConfig>, confidence_params: &Arc<ConfidenceParams>,
    categories: &Arc<CategoryAllowlist>, observation_registry: &Arc<DomainPackRegistry>,
    boosted_categories: &HashSet<String>,
) -> Result<ProjectServerInput, ServerError>;
```

### `resolve_slug_config` contract

1. `path = {base_dir}/{slug}/config.toml`.
2. **No file** → `Ok(Cow::Borrowed(global))` — byte-for-byte fallthrough; NO merge, NO re-derivation.
3. **File present**, in order: `load_single_config(&path)` → `validate_config(&slug_file, &path)` (per-file, AC-08a) →
   `merge_configs(global, &slug_file)` (third layer; hash-pin global-wins + `instructions` project-wins ride inside) →
   `validate_config(&merged, &path)` (post-merge, SR-01/AC-08b) → `Ok(Cow::Owned(merged))`.
4. Any failure → `ServerError::Config` naming the offending slug file; startup fails loud. No `.unwrap()`.

### Per-slug loop derivation

- Fields 0–2 — `Arc::clone` of global `embed_handle`, `rayon_pool`, `nli_handle` outside any overlay branch.
- `permissive` (P) — passed unconditionally from the global daemon flag, never read from `resolved`.
- `instructions` (I) — sourced from `resolved.server.instructions` (was global `server_instructions` var) (FR-14).
- Fields 3–9 — derived from `resolved`: `nli_top_k`, `nli_enabled`, `inference_config` (pins already global-won),
  `confidence_params`, `categories` (+lifecycle), `observation_registry`, `boosted_categories`.
- No-file arm SHOULD reuse the daemon's already-built parity `Arc`s directly (pointer-grade AC-02 sentinel via `Arc::ptr_eq`).

## Full Call-Site Verdict (centerpiece — AC-07, closed checklist over the LIVE `build_project_server` surface)

The verdict is derived from the live `build_project_server` signature — every config-relevant call-site input has an
explicit row, **none dropped**. The framing is "no call-site input is absent from the checklist," NOT a count.
`base_dir`/`slug` are routing identity (not config). **This table is the human-readable RENDERING of the single
canonical `PER_SLUG_CONFIG_CLASSIFICATION` registry (ADR-004 / FR-16), not an independent second source of truth** —
its verdicts reduce to the registry, the AC-11 drift-guard test pins `merge_configs`' real behavior to the same
registry, and Feature B's seed annotations render from it (one-way: A owns, B consumes). The remaining ~12
config-relevant inputs:

| # | Call-site input | Source | Verdict | Construction guarantee |
|---|-----------------|--------|---------|------------------------|
| 0 | `embed_handle` (+ `embedding_model_sha256` descriptor) | the ONE embedding model; `[embedding]` descriptor | **GLOBAL — locked, hard** | `Arc::clone` outside merge; `Arc::ptr_eq` on no-file arm; descriptor global-wins; no other descriptor field exists |
| 1 | `rayon_pool` | `inference.rayon_pool_size` | **GLOBAL — locked** | `Arc::clone` outside merge; `Arc::ptr_eq` on no-file arm |
| 2 | `nli_handle` | the ONE NLI model | **GLOBAL — locked, hard** | `Arc::clone` outside merge; `Arc::ptr_eq` on no-file arm |
| 3 | `nli_top_k` | `inference.nli_top_k` | overlayable | runtime param, not model identity |
| 4 | `nli_enabled` | `inference.nli_enabled` | overlayable | runtime param, not model identity |
| 5 | `inference_config` | `[inference]` weights + hash pins | overlayable EXCEPT `*_sha256` pins | merge splits weights (replace) from pins (global-wins) |
| 6 | `confidence_params` | `[confidence].weights` | **per-slug** | derived from merged weights |
| 7 | `categories` | `[knowledge].categories` + lifecycle | **per-slug** | derived from merged allowlist |
| 8 | `observation_registry` | `[observation].domain_packs` | **per-slug** | derived from merged packs |
| 9 | `boosted_categories` | `[knowledge].boosted_categories` | **per-slug** | derived from merged set |
| P | `permissive` | daemon permission flag (process posture) | **GLOBAL — locked** | passed unconditionally from the global flag; never read from `resolved` (FR-15) |
| I | `instructions` | `[server] instructions` | **per-slug (NEW, #785)** | sourced from `resolved.server.instructions`; already merged project-wins (`config.rs:3862-3864`) + already accepted by `build_project_server` (`http_provision.rs:137`) — trivial thread-through (FR-14) |
| — | `[embedding]` section (resolves to the sha256 pin today) | whole section | **GLOBAL — locked** | merged section == global; no 2nd model loaded/described |

## Constraints

- **Model invariant (hard):** `nli_handle`, `embed_handle`, `rayon_pool` sourced ONLY from global `Arc`s, never from a
  merged config. crt-056 AC-2 (one of each model in memory at any N) must keep holding.
- **`[embedding]` locked global:** the only descriptor (`embedding_model_sha256`) stays global-wins; no new descriptor
  field added. Forward guard: any future `[embedding].model`/`.dimensions` field MUST be added global-wins (#5196).
- **`permissive` locked global:** the daemon permission flag is passed unconditionally from the global value; a per-slug
  file cannot raise or lower a slug's permission posture (FR-15).
- **`instructions` per-slug overlayable:** #785 names `[server] instructions` a per-slug knob; the merged value is
  threaded to each slug, global underlies when unset (FR-14). Trivial thread-through — no merge/signature change.
- **Single canonical classification (ADR-004/FR-16):** the per-slug-vs-global split has exactly ONE owner —
  `PER_SLUG_CONFIG_CLASSIFICATION` in `infra/config.rs`. No second hand-authored copy of the split may exist; the verdict
  table and Feature B's seed annotations RENDER from it. `merge_configs` is NOT rewritten — the registry is data-only and
  the AC-11 drift-guard test is the binding. A field merged but not classified (or classified one way, merged the other)
  breaks the build.
- **Reuse, don't reinvent:** existing `merge_configs`, `load_single_config`, `validate_config` — no new merge model.
- **Security carve-out:** hash-pin fields stay global-wins inside the per-slug merge; divergence logs `tracing::warn`.
- **Seam is fixed:** integration at the `build_project_server` caller (`main.rs:1089-1110`); no override param in
  `load_config`; `build_project_server` signature UNCHANGED.
- **Zero-impact fallthrough:** no per-slug file → global behavior unchanged byte-for-byte across every call-site input;
  `Arc::ptr_eq` asserted on the 3 global handles.
- **Restart-applies (vnc-038 ADR-007):** no hot-reload.
- **DoS / permission hardening:** reuse the 64 KiB cap (#2395) + `#[cfg(unix)]` `mode() & 0o022 == 0` check via
  `load_single_config` — exercised on the per-slug path, not assumed (R-10).
- **Rust workspace rules:** ≤500 lines/file, no stubs/`todo!()`/`unimplemented!()`, no `.unwrap()` in non-test code,
  `tracing` for all logs, fail-loud-at-startup (never request-time).

## Dependencies

- **Existing components (reuse):** `load_config`, `load_single_config`, `validate_config`, `merge_configs`
  (`infra/config.rs`); `build_project_server` (`http_provision.rs:132-156`); per-slug loop (`main.rs:1089-1110`).
- **crt-056 seam (load-bearing, A3):** the live signature `build_project_server(base_dir, slug, embed_handle,
  permissive, instructions, <9 crt-056 params>)`. Design against the merged live signature, NOT the issue's "8 fields";
  if it shifts, the full call-site verdict must be re-derived.
- **vnc-034:** per-slug isolated store + `vector/` (the dir the config file lives in).
- **vnc-038 ADR-007 (#5086):** restart re-attaches routing — the moment the overlay is read.
- **No new crates / external services.**
- **Knowledge precedents:** #2395 (two-level merge + 64 KiB cap), #2286 (replace semantics), #4655/#4649/#4648
  (hash-pin global-wins + parallel-path flaw exposure), #5196 (lock whole section describing a global handle), #3905
  (post-merge cross-field invariant), #4583 (silent-fallback regression), #5172 (model-free N=2 isolation harness),
  #4070 (hidden `merge_configs` inline literal), crt-031 (literal-duplication anti-pattern — drives ADR-004's single
  owner + AC-11 drift guard), #4869 (one-way later-wave-consumes-earlier-wave seam — B renders from A, never A→B).

## Cross-Feature Dependency — Feature B Hand-Off (ONE-WAY CONTRACT: A owns, B consumes)

**R-13 (accepted residual) — what changed under ADR-004 (Option 2).** A per-slug `config.toml` that sets a GLOBAL-only
section/key (`[server.tls]`, transport, `permissive`-posture, `[embedding]`) is **silently ignored at the seam** — the
global-locked verdict wins and the per-slug value is never threaded. The ONLY divergence that warns today is a `*_sha256`
pin mismatch (AC-05); every other global-only key is dropped with no diagnostic. The **no-runtime-warn silent-ignore is
the deferred residual that remains** — Feature A deliberately does NOT add seam-level field-comparison machinery (out of
scope). **But the per-slug-vs-global SPLIT is no longer unowned or hand-duplicated:** ADR-004 makes Feature A's
`PER_SLUG_CONFIG_CLASSIFICATION` registry the single owner (FR-16). R-13's "documented but unowned" status is RETIRED;
the owner is A's classification.

**One-way hand-off contract (MUST be carried into Feature B's scope when scoped):** Feature B's `register`-time annotated
seed `config.toml` **RENDERS** its section-ownership annotations (per-slug-overlayable: `[knowledge]`, `[confidence]`,
`[observation]`, overlayable `[inference]`, `[server] instructions`; global-locked: `permissive`, transport/`[server.tls]`,
`[embedding]`) **FROM Feature A's `PER_SLUG_CONFIG_CLASSIFICATION` registry**. Dependency direction is strictly one-way:
**A owns the classification, B consumes/renders it.** B MUST NOT hand-author a third copy of the split in its seed
annotations (the crt-031 literal-duplication risk). The classification is a typed contract B reads, not a split B
re-derives. The integrity of A's owner-side classification is machine-guaranteed by the AC-11 drift-guard test (pinning
`merge_configs` to the registry); B's render parity is the seam where B must read A's registry rather than restate it. A
seam-level `tracing::warn` flagging stray global-locked keys in a per-slug file remains an OPTIONAL future enhancement,
NOT in scope for Feature A.

## Delivery Carry-Items (FLAG — must be honored in Session 2)

1. **SR-01 / R-01 (Critical) — post-merge `validate_config(&merged)`.** The #3905 third-layer fix MUST run inside
   `resolve_slug_config` after `merge_configs`, before return. Per-file validation alone is provably insufficient for
   cross-field invariants (fusion-weight sum-of-six > 1.0). Enumerate every sum/cross-field constraint in
   `validate_config` and prove a merged-only violation fails loud at startup naming the slug.
2. **SR-02 / R-02 / A1 — re-audit the inline `InferenceConfig {…}` literal in `merge_configs` (#4070).** This is the one
   site grep-for-spread misses. Confirm it lists every field explicitly or ends `..InferenceConfig::default()`, and that
   the global→per-slug call shape exercises the SAME arm as global→project — do NOT assume identical coverage. Record
   the audit as a checked obligation before trusting reuse.
3. **R-06 (forward guard) — `VectorConfig::default()` dependency.** The `[embedding]` lock depends on the per-slug vector
   index staying `VectorConfig::default()` (`http_provision.rs:182`), not config-driven dims. Add a standing guard test
   that fails loudly if a future change wires per-slug dims through (A2 re-opens SR-03 if violated). Any future
   `[embedding].model`/`.dimensions` field MUST be added global-wins (#5196).
4. **Both model invariants + `Arc::ptr_eq` fallthrough held BY CONSTRUCTION.** Fields 0–2 (`embed_handle`, `rayon_pool`,
   `nli_handle`) are `Arc::clone`d UNCONDITIONALLY outside any merge branch — never read from `resolved`. The no-file arm
   asserts `Arc::ptr_eq` on the 3 global handles (machine-checked, matching crt-056 AC-2), not mere value-equality. Prove
   exactly one NLI model and one embedding model at N≥2 (model-free #5172 harness) AND construction-review the clone site
   (R-04).
5. **`instructions` per-slug overlay (FR-14 / AC-10) — BOUNDED thread-through.** Relocate the `instructions` source from
   the pre-loop hoist (`main.rs:687`) into the per-slug loop, sourcing `resolved.server.instructions`. NO `merge_configs`
   change (it already merges `server.instructions` project-wins, `config.rs:3862-3864`), NO `build_project_server`
   signature change (it already accepts `instructions: Option<String>`, `http_provision.rs:137`). Prove per-slug overlay
   AND absent-file fallthrough to the global value (R-12).
6. **`permissive` GLOBAL-LOCKED (FR-15).** Pass `permissive` unconditionally from the global daemon flag; never source it
   from `resolved`. Include it as an explicit verdict row (AC-07).
7. **R-13 (accepted residual) + one-way Feature B hand-off.** The silent-ignore of stray global-locked sections (no
   runtime warn) is the documented, accepted residual — no field-comparison machinery is built. But the per-slug-vs-global
   split is now OWNED by A's classification (no longer unowned). Confirm the residual is recorded and that the one-way
   Feature B hand-off (B renders seed annotations FROM A's `PER_SLUG_CONFIG_CLASSIFICATION`; B MUST NOT hand-author a third
   copy) is surfaced. No behavioral guard beyond the R-09 transport-ignored assertion + R-05 pin warn — the split's
   correctness-against-code is now machine-checked by carry-item 9 (R-14/AC-11).
8. **A-only breadth.** Feature B (seeding — `register <slug>` writing a seeded `config.toml`, container `serve` seeding
   `DEFAULT_CONFIG_TOML`) stays OUT. Operator hand-places the file for A. The shared `{base_dir}/{slug}/config.toml` path
   and the leave-`adapt`-default decision are recorded so B builds on A without re-litigation (SR-06).
9. **ADR-004 / FR-16 / R-14 / AC-11 (High proof obligation) — canonical classification + mandatory drift-guard test.**
   Implement the declarative registry (`enum OverlayDisposition`, `struct ConfigKeyClass`,
   `const PER_SLUG_CONFIG_CLASSIFICATION`, `fn is_per_slug_overlayable`) colocated with `merge_configs` in
   `infra/config.rs`. **Leave `merge_configs`' arms intact — DATA-ONLY, no generic merge engine.** The **AC-11 drift-guard
   test is mandatory**: for every registry entry, drive `merge_configs` with a global+per-slug pair differing only on that
   key and assert `PerSlugOverlayable`⇒slug value wins, `GlobalLocked`⇒global wins (incl. the `*_sha256` carve-out). This
   is the anti-divergence guarantee (crt-031 pattern, R-14): a field merged but not classified — or classified one way,
   merged the other — breaks the build. **Delivery/tester MUST also confirm the registry key list is EXHAUSTIVE against
   `validate_config`'s field set** (architect's open question — no seam-relevant key omitted).

## NOT in Scope

- **Hot-reload / live config reload** — overlay reads at `build_project_server` time only (restart-applies).
- **Per-slug model selection** — both models + pool are global; forbidden, not merely unset.
- **Per-slug transport config** — TLS / auth / host / `http.enabled` stay global; never read at the seam.
- **Per-slug `permissive`** — daemon permission flag stays global, symmetric with transport (FR-15).
- **Per-slug `[embedding]` config** — descriptor locked global (pin global-wins + no new descriptor field).
- **Feature B (config seeding)** — operator hand-places the file for A. (But see the cross-feature hand-off note above.)
- **Seam-level warn for stray global-locked sections** — optional future enhancement; R-13 is an accepted residual.
- **New config sections / new tunable fields** — overlays the EXISTING `UnimatrixConfig` surface only;
  `adapt_service` stays `AdaptConfig::default()`.
- **Changing the existing global→project→env layering** inside `load_config`.

## Alignment Status

**All vision checks PASS; 0 VARIANCE, 0 FAIL** (ALIGNMENT-REPORT.md, 2026-06-19).

- Advances #4946 (personal-cloud — one isolation seam across local AND cloud) and #4678 (domain-agnostic — configured
  not rebuilt); delivers C6 (#5148).
- One documented **design refinement, not a variance**: the `[embedding]`-section lock resolves to **pin-global-wins +
  forward guard**. No `[embedding]` section exists today (only `inference.embedding_model_sha256`, already global-wins),
  so the "whole-section lock" language was defensive over-specification against a section that does not exist. The
  vision guardian explicitly confirmed this **does NOT weaken the model invariant**: the served `embed_handle` is
  `Arc::clone`d outside any merge (no second model can load) AND the only descriptor field is already global-wins (the
  merged config cannot describe a different model). AC-04 still asserts the behavioral guarantee; the only residual is
  the A2 `VectorConfig::default()` dependency, pinned by the R-06 guard test (carry-item 3).
- `permissive` (global-locked) and `instructions` (per-slug overlayable) are now explicit verdict rows, closing the
  "silently-dropped call-site input" gap (R-07) that materialized twice at the design gate.
- No hash-chain, audit-log, or capability principle touched — transport/auth/`permissive` stay global, correctly outside
  the C6 `done_when`.

**Post-alignment design change (ADR-004, #5210 — single canonical per-slug-vs-global classification, Option 2).** The
ALIGNMENT-REPORT.md on file predates ADR-004 (it records "11 risks / 27 scenarios" and the "10-input verdict" framing).
ADR-004 is a **strengthening, not a variance**: it gives the per-slug-vs-global split a single owner
(`PER_SLUG_CONFIG_CLASSIFICATION`) and a machine-checked drift guard (AC-11), directly tightening the same #4946
isolation-seam and #4678 configured-not-rebuilt goals the report PASSed on. It RETIRES R-13's "unowned split" residual
(crt-031 mitigation moves from prose to a build-breaking test) and turns the one-way Feature B hand-off into a typed
contract (#4869). It adds NO new config knob and does NOT rewrite `merge_configs`. Current source counts (RISK-TEST-
STRATEGY.md): **14 risks / 32 scenarios** (adds R-14 High proof obligation; reclassifies R-13). No new VARIANCE or FAIL is
introduced; a re-run of vision alignment against the updated sources is expected to remain PASS.
