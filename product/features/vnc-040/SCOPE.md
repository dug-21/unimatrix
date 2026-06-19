# vnc-040 — Per-Slug Configuration Overlay Resolution (C6 / Feature A)

> Feature A of GH issue #785 (two-part split, uni-zero 2026-06-19). Delivers capability
> **C6 — Per-slug configuration** (Unimatrix #5148). Prerequisite C5 (#5190) proved on the
> crt-056 / #789 merge (2026-06-19), so C6 is unblocked.

## Problem Statement
Configuration is **daemon-global**, not per-slug. `load_config` (`infra/config.rs:3225`)
resolves a single `UnimatrixConfig` from a fixed layering — compiled defaults → global
(`~/.unimatrix/config.toml`) → per-project (`{path-hash}/config.toml`) → `UNIMATRIX_CONFIG`
env file → `UNIMATRIX_HTTP_ENABLED` env bool. That one resolved config governs **every**
routed slug. Each registered slug owns an isolated store (`/data/.unimatrix/{slug}/unimatrix.db`
+ `vector/`, shipped vnc-034) but **no config file** of its own. `ProjectConfigEntry`
(`config.rs:124`) is intentionally `slug`-ONLY — the per-slug overlay was explicitly deferred
to this issue (D2). crt-056 (#789) then threaded 9 resolved fields into `build_project_server`
and **reserved this exact seam**: ADR-002 states *"#785 later adds overrides; this ADR must
not introduce that seam."* Two different-domain projects on one daemon cannot have their own
categories, domain packs, confidence weights, or instructions.

Who is affected: operators running multi-project (cloud / HTTP multi-tenant) deployments.
Why now: C5 (per-slug analytics) just proved — config tunes an engine that must already run,
and that engine now runs per-slug.

## Goals
1. Resolve a **per-slug** `UnimatrixConfig` at the `build_project_server` call site
   (`main.rs:1089-1110`) by overlaying the slug's own config file onto the daemon's already-
   resolved global config, per-key (merge, not section-replace).
2. Thread the per-slug-resolved values through the call-site config inputs so each slug's
   `ServiceLayer` reflects its own categories / domain packs / confidence / inference tuning /
   **instructions** (the latter sourced from the merged `resolved.server.instructions`, today
   global at `main.rs:687` and fanned to every slug at `main.rs:1095`).
3. Hold the **hard model invariant** for BOTH models: per-slug config NEVER selects a different
   model. The one loaded `nli_handle`, the one loaded `embed_handle` (embedding model), and the
   shared `rayon_pool` stay global (`Arc::clone`, never rebuilt) — preserving crt-056 AC-2 (one
   model of each kind in memory, not N).
4. Guarantee **zero behavior change** when no per-slug file exists: byte-for-byte fallthrough
   to the global resolved config (crt-056 `None`-arm discipline; no impact to local UDS /
   single-project).
5. Own a **single canonical per-slug-vs-global key classification** — one declarative source of
   truth in code (not prose) that enumerates the overlay-vs-global verdict for every call-site input
   — none dropped (the full `build_project_server` surface: `embed_handle` + `permissive` +
   `instructions` + the 9 crt-056 threaded params) plus the entire `[embedding]` section. The
   verdict table (Background Research) is a RENDERING of this classification, not an independent
   definition; `merge_configs`' actual overlay-vs-lock arms RENDER from / are pinned to it; and
   Feature B's future seeded `config.toml` annotations RENDER from it (B consumes the split, A owns
   it). Its integrity mechanism is a **drift-guard test** that pins `merge_configs`' actual
   overlay-vs-lock behavior to the classification, so the split cannot silently diverge across its
   copies (crt-031 literal-duplication pattern; cf. #4585/#3771 lockstep-list duplication, #4915
   completeness-over-code-derived-set). Prove the verdict field-by-field against the classification.

## Non-Goals
- **Hot-reload / live config reload** — overlay is read at `build_project_server` time and
  applies on the restart that already re-attaches routing (vnc-038 ADR-007). Explicit non-goal.
- **Per-slug model selection** — both models (NLI + embedding) + pool are global; forbidden, not
  merely unset.
- **Per-slug transport config** — TLS / auth / host / `http.enabled` stay global. The C6
  `done_when` boundary stops at engine/knowledge config.
- **Per-slug `permissive` (daemon permission flag)** — the `permissive` call-site input stays
  global, symmetric with transport; it is daemon-level posture, not engine/knowledge config.
  Named here, not silently absent. (`instructions`, by contrast, is IN scope — see Goal 2.)
- **Per-slug embedding config** — the entire `[embedding]` section (model + dimensions + sha256)
  is locked global, symmetric with transport. The C6 `done_when` boundary does not overlay it.
- **Feature B (config seeding)** — `register <slug>` writing a seeded annotated `config.toml`,
  and container `serve` seeding `DEFAULT_CONFIG_TOML`. Defaulted OUT of scope (see Breadth Call
  under Open Questions). An operator hand-places the per-slug file for A. NOTE the boundary: A owns
  the single canonical per-slug-vs-global classification (Goal 5); B, when built, RENDERS its seed
  `config.toml` annotations FROM that classification rather than restating the split — B consumes,
  A owns. Writing the seed file (B itself) stays out of scope here; only A's classification that B
  will later render from is in scope.
- **New config sections / new tunable fields** — A overlays the EXISTING `UnimatrixConfig`
  surface per-slug; it does not add config knobs.
- **Changing the existing global→project→env layering** inside `load_config`.

## Background Research

### The crt-056 seam (the integration point — LOCKED)
`build_project_server` (`http_provision.rs:132-156`) takes 9 config-parity params appended at
the end (params-at-end convention) PLUS a pre-existing `embed_handle: &Arc<EmbedServiceHandle>`
(line 135) threaded BEFORE the crt-056 block. The per-slug loop (`main.rs:1089-1110`) passes
`Arc::clone`s of the daemon's single resolved config (`main.rs:880-898` scope). C6 makes that
per-slug set **resolved against the slug's overlay file** at the same call site. crt-056
deliberately threaded exactly the 9 and added NO override parameter (ADR-002 boundary, SR-06).

### Overlay-vs-global verdict for the FULL `build_project_server` call-site surface (~12)
Every call-site input gets an explicit verdict — none dropped. The issue calls these "8 fields",
but `build_project_server`'s real signature is `build_project_server(base_dir, slug, embed_handle,
permissive, instructions, <9 crt-056 params>)`. Beyond the **9 crt-056 params** (which split
`boosted_categories` out from `categories`) the call site threads THREE more config-relevant
inputs: `embed_handle` (the embedding model), `permissive` (the daemon permission flag), and
`instructions` (the `[server] instructions` knob, which #785 names per-slug-overlayable). The
verdict checklist therefore covers the full call-site surface — `embed_handle` + `permissive` +
`instructions` + the 9 crt-056 params — plus the entire `[embedding]` config section (model +
dimensions + sha256). The table below is a **rendering of the single canonical in-code
classification** (Goal 5) — the single source of truth — NOT an independent definition; the
in-code classification, `merge_configs`' overlay-vs-lock arms, and Feature B's future seed
annotations all render from it, kept in sync by the drift-guard test. Verdict table (confirm at
design):

| # | Call-site input (`build_project_server`) | Source | C6 treatment |
|---|------------------------------------------|--------|--------------|
| 0 | `embed_handle: &Arc<EmbedServiceHandle>` (pre-existing) | the ONE loaded EMBEDDING model; `[embedding]` section (model + dimensions + sha256) | **GLOBAL — locked, hard invariant**; entire `[embedding]` section locked global (symmetric with transport) |
| P | `permissive: bool` (daemon permission flag) | daemon permission flag | **GLOBAL — locked** (daemon-level posture, not engine/knowledge config; symmetric with transport) |
| I | `instructions` (`[server] instructions`) | merged `resolved.server.instructions` (today global `main.rs:687`, fanned to every slug `main.rs:1095`) | **per-slug** ✅ — #785 names `[server] instructions` a per-slug knob; sourced from the merged config |
| 1 | `rayon_pool: &Arc<RayonPool>` | `inference.rayon_pool_size` | **GLOBAL — locked** (shared inference pool) |
| 2 | `nli_handle: &Arc<NliServiceHandle>` | the ONE loaded NLI model | **GLOBAL — locked, hard invariant** |
| 3 | `nli_top_k: usize` | `inference.nli_top_k` | overlayable ⚠️ — verify not model-coupled |
| 4 | `nli_enabled: bool` | `inference.nli_enabled` | overlayable ⚠️ — verify not model-coupled |
| 5 | `inference_config: &Arc<InferenceConfig>` | `[inference]` (fusion/PPR weights + **hash pins**) | overlayable ⚠️ EXCEPT `*_sha256` pins MUST stay global-wins (#4655/#4649) |
| 6 | `confidence_params: &Arc<ConfidenceParams>` | `[confidence].weights` | **per-slug** ✅ |
| 7 | `categories: &Arc<CategoryAllowlist>` | `[knowledge].categories` + lifecycle | **per-slug** ✅ (primary domain knob) |
| 8 | `observation_registry: &Arc<DomainPackRegistry>` | `[observation].domain_packs` | **per-slug** ✅ |
| 9 | `boosted_categories: &HashSet<String>` | `[knowledge].boosted_categories` | **per-slug** ✅ |

### The `[embedding]` section is locked GLOBAL — whole section, not just the sha256 pin
`embed_handle` (line 135) is the ONE loaded embedding model, `Arc::clone`d per slug exactly like
`nli_handle`. The sha256 carve-out (`embedding_model_sha256`, below) is necessary but **NOT
sufficient**: where no global `embedding_model_sha256` pin is set, nothing otherwise stops a
per-slug file from setting `[embedding].model = "other"`. The served handle stays the global
model by construction (the loop `Arc::clone`s the global `embed_handle` and never sources it from
the merged config — good), but the MERGED CONFIG would then *describe* an embedding model the
handle is not — a config-vs-handle divergence. It is only defused today by luck: the per-slug
vector index uses `VectorConfig::default()` (`http_provision.rs:182`), NOT config-driven dims, so
a per-slug `[embedding].dimensions` override has no effect on the index. Locking the ENTIRE
`[embedding]` section global (model + dimensions + sha256) — symmetric with how transport
(TLS/auth/host) is locked — closes this by construction: the merged config can never describe a
different embedding model than the one served.

Note (#4): `adapt_service` is built INSIDE `build_project_server` from `AdaptConfig::default()`
(`http_provision.rs:206-208`) — already per-slug-independent state, but NOT operator-configurable
today and NOT one of the 9. Out of scope unless a per-slug `[adapt]` overlay is in scope (it is
not — no new fields). The crt-056 comment there explicitly tags it as a #785 candidate; design
should record the decision to leave it default.

### Merge semantics — there is ALREADY a per-key merge to reuse
`merge_configs` (`config.rs:3825`) is the canonical per-key merge: for each field, "project value
if it differs from the compiled default, else global." `Option` fields use `.or()`. This is
**exactly the merge model C6 needs** — global resolved config as base, per-slug file overrides
only the keys it sets, unset keys fall through. C6 should reuse this same merge (likely
`merge_configs(global_resolved, per_slug_file)`), NOT invent a new one.

### ADR-003 reconciliation (the issue's #3 — RESOLVED in research)
"ADR-003 replace semantics" refers to **dsn-001 #2286** (the two-level global→project TOML
merge), NOT crt-056's ADR-003 (#5166, the per-slug ServiceLayer handle set). #2286's "replace"
governs **list-field merge** (categories etc. are complete policy declarations — present replaces,
absent falls through), which is precisely the per-key (NOT section-replace) behavior the issue
mandates. The C6 overlay is a THIRD precedence layer atop the existing global→project pair, using
the SAME field-level replace discipline. No conflict — C6 extends the established pattern.

### Security carve-out (load-bearing — #4655 / #4649)
`merge_configs` already enforces **global-wins** for hash-pin fields (`embedding_model_sha256`,
`nli_model_sha256` at `config.rs:3903+`): a per-project override MUST NOT bypass an operator hash
pin. The per-slug overlay MUST preserve this carve-out — pins inside `inference_config` (field 5)
stay global-wins even though the rest of `inference` is overlayable. Reinforces the model invariant
(a per-slug model pin can't smuggle in a different model). NOTE the carve-out is necessary but
NOT sufficient for `[embedding]`: it only guards the sha256 pin, not the `[embedding].model` /
`[embedding].dimensions` keys when no pin is set — see "[embedding] section locked GLOBAL" above
for why the WHOLE section is locked, not just the pin.

### Per-slug config file location
`build_project_server` already computes `data_dir = base_dir.join(slug.as_str())`
(`http_provision.rs:159`) where `base_dir = paths.data_dir.parent()` (`main.rs:1087`). The slug dir
is a sibling of the path-hash dir, both under `/data/.unimatrix/` (cloud) or `~/.unimatrix/`
(local; the human's open `~/.unimatrix/0d62f3bf1bf46a0a/config.toml` is the daemon-global path-hash
file). The natural per-slug file is `{base_dir}/{slug}/config.toml` — alongside `unimatrix.db` +
`vector/`. Reuses the existing `load_single_config` + `validate_config` machinery.

### No-restart-asymmetry context (vnc-038, #5079)
`[[projects]]` routing is read once at boot; "registered != routable" until restart. The per-slug
overlay reads on the same restart that re-attaches routing — consistent with ADR-007, no new
reload mechanism. (Reinforces the hot-reload non-goal.)

## Proposed Approach
At the per-slug loop (`main.rs:1089`), for each slug: (1) probe `{base_dir}/{slug}/config.toml`;
(2) if present, `load_single_config` + per-file `validate_config`, then `merge_configs(global_
resolved, per_slug_file)` to produce a per-slug-resolved `UnimatrixConfig` — preserving the
hash-pin global-wins carve-out AND treating the entire `[embedding]` section as global-wins;
(3) derive the per-slug values for the overlayable inputs (fields 6–9 + nli_top_k/nli_enabled/
inference-non-pin + `instructions` from `resolved.server.instructions`) from the merged config;
(4) ALWAYS `Arc::clone` the global `nli_handle`, `embed_handle`, and `rayon_pool` (fields 0–2) and
pass the global `permissive` (field P) unchanged, regardless of the file. If no file: pass the
global values unchanged (byte-for-byte fallthrough). Transport config is never read here.

Rationale: reuses the proven `merge_configs` + `load_single_config` + `validate_config` path;
keeps both model invariants enforceable by construction (fields 0–2 are never sourced from the
merged config, and the `[embedding]` section is locked global so the merged config can never
*describe* a model the handle is not); confines the change to the call-site loop + (likely) a
small per-slug resolution helper, leaving `load_config`'s global layering untouched.

## Acceptance Criteria
- AC-01: With a per-slug `config.toml` setting `[knowledge].categories`, slug A's served
  `CategoryAllowlist` reflects A's categories; slug B (different file) reflects B's; the daemon
  global default underlies both — proven behaviorally without loading N models.
- AC-02: With NO per-slug file present, the slug's resolved config and every threaded value are
  **byte-for-byte identical** to the global-only crt-056 path (regression sentinel for local UDS /
  single-project).
- AC-03: A per-slug file that sets a per-slug field (categories, boosted_categories, domain_packs,
  confidence weights, overlayable inference fields) overrides ONLY that key; unset keys fall
  through to the global resolved value (per-key merge, not section-replace).
- AC-04: BOTH the one loaded NLI model AND the one loaded embedding model are shared across all
  slugs — exactly one NLI model and exactly one embedding model in memory at N≥2 registered slugs.
  Specifically for embedding: a per-slug `[embedding]` override (model, dimensions, or pin) neither
  LOADS a second embedding model nor DESCRIBES one in the slug's merged config — the slug's served
  `embed_handle` and its merged `[embedding]` section both remain the named global embedding model.
  Symmetric with the NLI one-model assertion (hard invariant; preserves crt-056 AC-2).
- AC-05: A per-slug `*_sha256` hash pin does NOT override a global-set pin (global-wins carve-out
  preserved per #4655); divergence logs a `tracing::warn`.
- AC-06: Transport config (TLS / auth / host / `http.enabled`) is unaffected by any per-slug
  file — it is never read at the per-slug seam.
- AC-07: The overlay-vs-global verdict is asserted field-by-field across the ENTIRE
  `build_project_server` call-site surface — every input has an explicit verdict, none dropped:
  the 9 crt-056 threaded params PLUS `embed_handle` (field 0), `permissive` (field P), and
  `instructions` (field I), plus the entire `[embedding]` config section (closed checklist,
  mirroring crt-056 AC-1, which exists precisely to prevent a silently-dropped input) —
  global-locked inputs (`embed_handle`, `permissive`, pool, NLI handle, `[embedding]`) proven not
  overlayable, per-slug fields (categories, domain packs, confidence, overlayable inference,
  `instructions`) proven overlayable. The checklist names the full call-site surface, not just
  "the 9 threaded params".
- AC-08: An invalid per-slug `config.toml` (bad category, oversized instructions, malformed TOML)
  fails loud at startup via the existing `validate_config`, naming the offending slug file.
- AC-09: Overlay applies on restart only; no live reload path is introduced.

## Constraints
- **Model invariant (hard):** `nli_handle`, `embed_handle` (embedding model), and `rayon_pool`
  sourced ONLY from global Arcs; never from a per-slug merged config. crt-056 AC-2 must keep
  holding for BOTH models.
- **`[embedding]` section locked GLOBAL:** the whole section (model + dimensions + sha256), not
  just the sha256 pin, is global-wins inside the per-slug merge — symmetric with transport. This
  closes the config-vs-handle divergence by construction (the merged config can never describe an
  embedding model the served `embed_handle` is not).
- **Reuse, don't reinvent:** use existing `merge_configs`, `load_single_config`, `validate_config`.
- **Security carve-out:** hash-pin fields stay global-wins inside the per-slug merge (necessary
  but not sufficient for `[embedding]` — see the section lock above).
- **Seam is fixed:** integration happens at `build_project_server` call site (`main.rs:1089-1110`);
  crt-056 reserved it and added no override param — A adds it here, not in `load_config`.
- **Zero-impact fallthrough:** no per-slug file → global behavior unchanged byte-for-byte.
- **Restart-applies (vnc-038 ADR-007):** no hot-reload.
- **Rust workspace rules:** ≤500 lines/file, no stubs, no `.unwrap()` in non-test, `tracing` for logs.

## Open Questions
All six open questions were RESOLVED at the 2026-06-19 human gate (human approved the researcher's
recommendation on every one). Retained here as a resolution ledger so they are not re-litigated.

1. **Per-slug file path/name — RESOLVED.** `{base_dir}/{slug}/config.toml` (sibling of the
   path-hash dir, alongside `unimatrix.db`/`vector/`). CONFIRMED — `config.toml` for operator
   familiarity + reuse of `load_single_config`. NOTE: this is the SAME path Feature B will later
   seed, so A and B share one file location.
2. **`nli_top_k` / `nli_enabled` verdict (fields 3–4) — RESOLVED.** Overlayable; confirmed NOT
   model-coupled (runtime inference params, not model identity).
3. **`inference_config` partial overlay (field 5) — RESOLVED.** Weights (fusion/PPR) are
   overlayable; the hash pins (`*_sha256`) are global-wins. The `merge_configs` `inference` arm
   already splits these correctly — reuse it.
4. **Breadth call — RESOLVED: A only.** A flips C6 without seeding (operator hand-places the
   file); its seam is the call-site loop and does NOT touch the vnc-038 register path. Feature B
   (seeding — `register <slug>` writing a seeded annotated `config.toml`) stays a tracked
   follow-up, OUT of scope for vnc-040.
5. **`adapt_service` (`AdaptConfig`) — RESOLVED: out of scope.** Left default/per-slug-independent
   (not one of the 9 threaded params, no new fields).
6. **Validation timing for per-slug files — RESOLVED: yes.** Validate each per-slug file
   independently before merge (mirrors `load_config` "both files validated independently before
   merge", #2286). Captured as AC-08.

## Tracking
GH Issue #785 (Feature A). This SCOPE.md feeds the design session (architecture + spec).
