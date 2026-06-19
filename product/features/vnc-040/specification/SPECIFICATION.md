# Specification — vnc-040: Per-Slug Configuration Overlay Resolution (C6 / Feature A)

> Source: `product/features/vnc-040/SCOPE.md` (APPROVED 2026-06-19) and
> `product/features/vnc-040/SCOPE-RISK-ASSESSMENT.md`.
> Delivers capability **C6 — Per-slug configuration** (Unimatrix #5148), Feature A of GH #785.
> Downstream consumers: architect, pseudocode, risk strategist, tester.

## Objective

Resolve a **per-slug** `UnimatrixConfig` at the `build_project_server` call site by overlaying a
slug's own `config.toml` onto the daemon's already-resolved global config using per-key merge, then
threading the per-slug-resolved values through the overlayable `build_project_server` call-site inputs
(the 9 crt-056 params plus `server.instructions`) so each slug's `ServiceLayer` reflects its own
categories, domain packs, confidence weights, inference tuning, and instructions. Both loaded
models (NLI + embedding) and the shared rayon pool stay global by construction (hard invariant);
when no per-slug file exists, behavior is byte-for-byte identical to the current global-only path.

---

## Ubiquitous Language (Domain Model)

| Term | Definition |
|------|-----------|
| **Slug** | The registered project identifier routing requests to an isolated store (`/data/.unimatrix/{slug}/unimatrix.db` + `vector/`, vnc-034). One client maps to one slug/project. |
| **Global resolved config** | The single `UnimatrixConfig` produced by `load_config`'s existing layering (compiled defaults → global `~/.unimatrix/config.toml` → per-project path-hash → `UNIMATRIX_CONFIG` → `UNIMATRIX_HTTP_ENABLED`). C6 does not alter this layering. |
| **Per-slug file** | An operator-hand-placed `{base_dir}/{slug}/config.toml`, a sibling of the path-hash dir, alongside the slug's `unimatrix.db`/`vector/`. Optional. |
| **Overlay** | The third precedence layer C6 introduces: the per-slug file applied **on top of** the global resolved config via per-key merge. An overlay is NOT a section-replace and NOT a new layering inside `load_config`. |
| **Merged config (per-slug-resolved config)** | The `UnimatrixConfig` resulting from `merge_configs(global_resolved, per_slug_file)` — the config that governs ONE slug's `ServiceLayer`. |
| **Fallthrough** | The no-per-slug-file path: the slug is served the global resolved config and global `Arc`s unchanged, never re-derived from a merge. |
| **Model invariant (hard)** | The rule that the loaded NLI model handle, the loaded embedding model handle, and the shared rayon pool are sourced ONLY from global `Arc`s (`Arc::clone`), never from any merged config — so exactly one NLI model and one embedding model exist in memory regardless of slug count. |
| **`[embedding]` section lock** | The whole `[embedding]` section (model + dimensions + sha256) is global-wins inside the per-slug merge, symmetric with transport — so the merged config can never *describe* an embedding model the served handle is not. |
| **Hash-pin carve-out** | The pre-existing `merge_configs` global-wins rule for `embedding_model_sha256` / `nli_model_sha256` (#4655/#4649): a per-project/per-slug file MUST NOT override an operator-set pin. |
| **Call-site verdict** | The closed checklist classifying **every** config-relevant `build_project_server` call-site input (the 9 crt-056 threaded params + pre-existing `embed_handle` + `permissive` + `instructions` — ~12 inputs) plus the `[embedding]` section as global-locked or overlayable. Its purpose (crt-056 AC-1) is to prevent any input being silently dropped; therefore it enumerates the FULL call-site surface, not a subset. |
| **`server.instructions` overlay** | The per-slug-overlayable `[server] instructions` string threaded to each slug's `ServiceLayer`. #785 names it a per-slug knob. Today global (`main.rs:687`, fanned to every slug at `main.rs:1095`). |
| **`permissive` flag** | The daemon-wide permission flag (`build_project_server` arg). A process-level posture, GLOBAL-LOCKED — never per-slug-overlayable. |
| **The seam** | The integration point: the per-slug loop at `main.rs:1089-1110` calling `build_project_server` (`http_provision.rs:132-156`). crt-056 reserved this seam and added no override param. |

### Key Entities & Relationships

- One **daemon** owns one **global resolved config** and exactly one **NLI handle**, one **embed handle**, one **rayon pool**.
- One daemon serves N **slugs**; each slug owns one **store** and (optionally) one **per-slug file**.
- Each slug is served one **merged config** (= global resolved config when no file) and one **`ServiceLayer`** built from it.
- All slugs share the same model handles and pool via `Arc::clone` — the merged config never sources fields 0–2.

---

## Functional Requirements

Each requirement is testable; verification is consolidated in Acceptance Criteria.

- **FR-01 — Per-slug overlay resolution.** At the per-slug loop (`main.rs:1089`), for each slug,
  probe `{base_dir}/{slug}/config.toml`. If present, load it via the existing `load_single_config`,
  validate it per-file via the existing `validate_config`, then produce the merged config via
  `merge_configs(global_resolved, per_slug_file)`. Reuse the existing machinery; do not introduce a
  new merge or load function. (Goal 1, Constraint "Reuse")

- **FR-02 — Per-key merge, not section-replace.** A per-slug file overrides only the keys it sets;
  any unset key falls through to the global resolved value. This is the `merge_configs` field-level
  replace discipline (the dsn-001 #2286 / #2395 precedent), extended as a third precedence layer.
  (Goal 1; AC-03)

- **FR-03 — Thread per-slug values through the 6 overlayable inputs.** The merged config drives the
  6 overlayable `Arc`s — `confidence_params`, `categories`, `observation_registry`,
  `boosted_categories`, and the overlayable portions of `inference_config` (fusion/PPR weights),
  plus `nli_top_k` and `nli_enabled` — so each slug's `ServiceLayer` reflects its own values.
  (`server.instructions` is also per-slug overlayable but is a separate string arg, covered by FR-14,
  not one of these 6 `Arc`s.) (Goal 2; AC-01, AC-03)

- **FR-04 — Model & pool global by construction.** `nli_handle`, `embed_handle`, and `rayon_pool`
  (verdict inputs 0–2) are passed by `Arc::clone` of the global handles **unconditionally**, outside
  any per-slug merge branch. They are never sourced from a merged config under any code path.
  (Goal 3, Constraint "Model invariant"; AC-04)

- **FR-05 — `[embedding]` section locked global.** Inside the per-slug merge, the entire
  `[embedding]` section (model + dimensions + sha256) is global-wins. A per-slug `[embedding]`
  override neither loads a second embedding model nor causes the merged config to *describe* a
  different embedding model than the served global handle. (Goal 3, Constraint "[embedding] lock";
  AC-04; #5196)

- **FR-06 — Hash-pin global-wins preserved.** The existing `merge_configs` global-wins carve-out for
  `embedding_model_sha256` and `nli_model_sha256` (#4655/#4649) is preserved under the global→per-slug
  pairing. A per-slug pin that differs from a global-set pin does NOT override it and emits a
  `tracing::warn` naming the divergence. (Constraint "Security carve-out"; AC-05)

- **FR-07 — Post-merge cross-field re-validation.** After `merge_configs`, the **merged** per-slug
  config is re-validated for cross-field invariants via `validate_config(&merged)`. Per-file
  validation (FR-01) is necessary but not sufficient: non-default fields combining across the
  global and per-slug levels can violate a sum/cross-field invariant (e.g. fusion-weight sums > 1.0)
  that each file passes independently (#3905 precedent). Failure is loud at startup and names the
  offending slug. (SR-01; AC-08b)

- **FR-08 — Byte-for-byte fallthrough.** When no per-slug file exists, the slug is served the global
  resolved config and the global `Arc`s unchanged. The `None` arm passes the global `Arc`s directly;
  it MUST NOT re-derive any value through a merge. (Goal 4, Constraint "Zero-impact fallthrough";
  AC-02)

- **FR-09 — Transport config never read at the seam.** TLS, auth, host, and `http.enabled` are not
  read, merged, or threaded at the per-slug seam. They remain governed solely by the global resolved
  config. (Non-Goal "transport"; AC-06)

- **FR-10 — Loud failure on invalid per-slug file.** An invalid per-slug `config.toml` (unknown
  category, oversized instructions, malformed TOML, world/group-writable permissions) fails loudly at
  startup, naming the offending slug file. (Constraint, Anti-stub; AC-08a)

- **FR-11 — Closed full-call-site verdict.** The implementation asserts the overlay-vs-global verdict
  field-by-field for **every config-relevant `build_project_server` call-site input** — the 9 crt-056
  threaded params, the pre-existing `embed_handle`, `permissive`, and `instructions` (~12 inputs) —
  PLUS the `[embedding]` section. A closed checklist (mirroring crt-056 AC-1) whose whole purpose is
  to prevent silent drops: it enumerates the FULL call-site surface, so no input can be silently
  dropped or mis-classified. The verdict is re-derived from the live `build_project_server` signature,
  never an input subset. (Goal 5; AC-07)

- **FR-12 — Restart-only application.** The overlay is read once, at `build_project_server` time, on
  the restart that already re-attaches routing (vnc-038 ADR-007). No hot-reload / live-reload path is
  introduced. (Non-Goal "hot-reload"; AC-09)

- **FR-13 — `adapt_service` left default.** `adapt_service` remains built from `AdaptConfig::default()`
  inside `build_project_server`; it is not made operator-configurable and not overlaid. The decision
  to leave it default is recorded. (SCOPE OQ-5; design records, not a test)

- **FR-14 — `server.instructions` per-slug overlayable.** The merged per-slug config's
  `server.instructions` is threaded to each slug's `ServiceLayer` via the `instructions`
  `build_project_server` arg. A slug that sets `[server] instructions` in its per-slug file serves
  that value; a slug with no override falls through to the global `server.instructions`. This
  replaces the current global-only fan-out (`main.rs:687` → every slug at `main.rs:1095`). #785 names
  `[server] instructions` an explicit per-slug knob. (Goal 2, #785; AC-10)

  - **RESOLVED (code inspection) — trivial thread-through, NOT seam plumbing.** The earlier open
    question (is `server.instructions` a `UnimatrixConfig` field governed by `merge_configs`, or
    sourced outside the merged config requiring seam plumbing?) is settled in favor of the former.
    `server.instructions` IS a field (`ServerConfig.instructions: Option<String>`, `config.rs:428`)
    and `merge_configs` ALREADY merges it project-wins via `Option::or`
    (`config.rs:3862-3864`: `instructions: project.server.instructions.or(global.server.instructions)`)
    — no new merge code. `build_project_server` ALREADY accepts `instructions: Option<String>`
    (`http_provision.rs:137`) — no signature change. FR-14 is therefore the SAME thread-through
    pattern as the other overlayable fields, sourced from `merged.server.instructions` inside the
    per-slug merge (the `resolve_slug_config` helper). **Sole delivery action:** relocate the
    instructions source from the pre-loop hoist (`main.rs:687`,
    `let server_instructions = config.server.instructions.clone()`, fanned identically to every slug
    at `main.rs:1095`) INTO the per-slug loop, sourcing from each slug's merged config instead of the
    global var. No separate seam-plumbing effort, no merge change, no signature change.

- **FR-15 — `permissive` global-locked.** The `permissive` `build_project_server` arg is a daemon-wide
  permission posture; it is passed unconditionally from the global flag and is NEVER sourced from any
  per-slug merged config. A per-slug file cannot raise or lower a slug's permission posture.
  (Constraint "process-level posture global"; AC-07)

- **FR-16 — Single canonical per-slug-vs-global classification (one authoritative owner in A).**
  Feature A defines exactly ONE declarative source of truth — a canonical classification — naming,
  per config key/section, whether it is **per-slug-overlayable** or **global-locked**. This
  classification is the single owner of the split; no second hand-authored copy of the list may exist.
  The **FR-11 full call-site verdict table IS the human-readable RENDERING of this classification**,
  not an independent second truth — its rows cross-reference the canonical classification rather than
  re-asserting the split. `merge_configs`' actual overlay-vs-lock behavior is the runtime expression of
  the same classification (today encoded implicitly in its field-level `or`/global-wins arms); FR-16
  does NOT require rewriting `merge_configs` — the classification is the explicit declarative anchor
  that `merge_configs`' behavior and the FR-11 table both reduce to. This retires the crt-031
  literal-duplication risk: a downstream consumer (Feature B's annotated seed, FR-11's table) RENDERS
  from this one classification, it never re-derives the split. (Goal 5, Constraint "Reuse";
  anti-divergence; AC-11)

### Full Call-Site Verdict Checklist (FR-11, verified by AC-07)

The verdict is itself a verifiable requirement — **every** config-relevant call-site input has an
explicit row; none dropped. The checklist's purpose (crt-056 AC-1) is to prevent silent drops, so it
covers the FULL `build_project_server` signature, not a subset. This table is the human-readable
**RENDERING** of FR-16's single canonical per-slug-vs-global classification — its "Required verdict"
column reflects that classification, it is not an independent second source of truth. The
machine-checked drift guard (AC-11) pins `merge_configs`' actual behavior to the same classification.

| # | Call-site input | Source | Required verdict | Proof obligation |
|---|-----------------|--------|------------------|------------------|
| 0 | `embed_handle` (pre-existing, line 135) | the ONE embedding model; `[embedding]` section | **GLOBAL-LOCKED** | served handle is global `Arc::clone`; merged `[embedding]` == global (AC-04) |
| 1 | `rayon_pool` | `inference.rayon_pool_size` | **GLOBAL-LOCKED** | global `Arc::clone`; one pool at N≥2 (AC-04) |
| 2 | `nli_handle` | the ONE NLI model | **GLOBAL-LOCKED** | served handle is global `Arc::clone`; one NLI model at N≥2 (AC-04) |
| 3 | `nli_top_k` | `inference.nli_top_k` | **OVERLAYABLE** | runtime inference param, not model identity (AC-03, AC-07) |
| 4 | `nli_enabled` | `inference.nli_enabled` | **OVERLAYABLE** | runtime inference param, not model identity (AC-03, AC-07) |
| 5 | `inference_config` | `[inference]` weights + hash pins | **OVERLAYABLE EXCEPT pins** | weights overlay; `*_sha256` stay global-wins (AC-03, AC-05) |
| 6 | `confidence_params` | `[confidence].weights` | **OVERLAYABLE (per-slug)** | per-slug weights reflected (AC-03) |
| 7 | `categories` | `[knowledge].categories` + lifecycle | **OVERLAYABLE (per-slug)** | per-slug allowlist reflected (AC-01) |
| 8 | `observation_registry` | `[observation].domain_packs` | **OVERLAYABLE (per-slug)** | per-slug packs reflected (AC-03) |
| 9 | `boosted_categories` | `[knowledge].boosted_categories` | **OVERLAYABLE (per-slug)** | per-slug set reflected (AC-03) |
| P | `permissive` | daemon permission flag (process posture) | **GLOBAL-LOCKED** | passed unconditionally from global flag; never sourced from a merged config (AC-07; FR-15) |
| I | `instructions` | `[server] instructions` | **OVERLAYABLE (per-slug)** | per-slug `server.instructions` reflected; global underlies when unset (AC-10; FR-14) |
| — | `[embedding]` section (model + dimensions + sha256) | whole section | **GLOBAL-LOCKED** | merged section == global; no 2nd model loaded/described (AC-04) |

---

## Non-Functional Requirements

- **NFR-01 — Model footprint bound.** At N≥2 registered slugs, exactly **one** NLI model and exactly
  **one** embedding model are resident in memory; the rayon pool is a single shared instance. The
  per-slug overlay adds zero model loads. (Measurable: count of loaded handles == 1 each at any N.)
  (crt-056 AC-2; AC-04)

- **NFR-02 — Zero-regression fallthrough.** The no-file path is byte-for-byte identical to the
  current crt-056 global-only path across every config-relevant call-site input. Blast radius: every
  local UDS / single-project user (the majority). (Measurable: per-slug-resolved == global-resolved
  equality across all call-site inputs; `Arc::ptr_eq` on the 3 global handles per AC-02.)
  (SR-04; AC-02)

- **NFR-03 — DoS-resistant parse.** Per-slug file load enforces the existing 64 KiB size cap before
  `toml::from_str` (#2395). (Reuse via `load_single_config`.)

- **NFR-04 — Permission hardening.** Per-slug file load enforces the existing `#[cfg(unix)]`
  world/group-writable check (`mode() & 0o022 == 0`) via the existing load path. (Reuse.)

- **NFR-05 — Fail-loud, fail-fast.** All per-slug config errors (per-file FR-10, post-merge FR-07)
  surface at startup, never at request time, and name the offending slug. No silent fallback. (#4583)

- **NFR-06 — Workspace rules.** ≤500 lines/file; no stubs / `todo!()` / `unimplemented!()`; no
  `.unwrap()` in non-test code; `tracing` for all logs.

- **NFR-07 — Confined blast radius.** The change is confined to the per-slug call-site loop plus (at
  most) a small per-slug resolution helper. `load_config`'s existing global→project→env layering is
  untouched. (Constraint "Seam is fixed", "Changing layering" non-goal.)

---

## Acceptance Criteria

Verification methods: **B** = behavioral integration test (model-free N=2 harness per #5172 where a
model would otherwise be required); **U** = unit test on the merge/resolution helper; **C** = code
review / construction proof; **R** = byte-equality regression assertion.

- **AC-01** *(FR-03)* — With per-slug files setting distinct `[knowledge].categories` for slug A and
  slug B, A's served `CategoryAllowlist` reflects A's categories and B's reflects B's, with the global
  default underlying both. **Verify: B** (N=2, model-free; distinct populations make the isolation
  non-vacuous per #5172).

- **AC-02** *(FR-08, NFR-02)* — With NO per-slug file present, the slug's resolved config and **every**
  config-relevant call-site input is **byte-for-byte identical** to the global-only crt-056 path.
  Additionally, on the no-file fallthrough arm the 3 global handles — `embed_handle`, `nli_handle`,
  `rayon_pool` — are asserted **same-allocation via `Arc::ptr_eq`** against the global `Arc`s (the
  same machine-checked equality crt-056 AC-2 uses), not merely value/byte-equal: the `None` arm clones
  the global `Arc`s and never re-derives via merge. This is the primary regression sentinel for the
  single-project / local-UDS majority. **Verify: R + U** (`Arc::ptr_eq` on the 3 handles on the
  fallthrough arm; per-slug-resolved == global-resolved value-equality across the remaining inputs).

- **AC-03** *(FR-02, FR-03)* — A per-slug file setting one overlayable field (categories,
  boosted_categories, domain_packs, confidence weights, or an overlayable inference field) overrides
  ONLY that key; unset keys fall through to the global resolved value (per-key merge, not
  section-replace). **Verify: U** (assert overridden key changes, sibling keys unchanged).

- **AC-04** *(FR-04, FR-05, NFR-01)* — At N≥2 slugs, exactly one NLI model and exactly one embedding
  model are in memory. Specifically for embedding: a per-slug `[embedding]` override (model,
  dimensions, or pin) **neither loads a second embedding model nor causes the merged config to
  describe one** — the slug's served `embed_handle` and its merged `[embedding]` section both remain
  the named global embedding model. Symmetric with the NLI one-model assertion. **Verify: B + C**
  (shared unloaded handles in the N=2 harness per #5172; assert merged `[embedding]` == global section;
  construction proof that fields 0–2 are `Arc::clone`d outside any merge branch).

- **AC-05** *(FR-06)* — A per-slug `*_sha256` hash pin that differs from a global-set pin does NOT
  override it (global-wins carve-out preserved, #4655); the divergence emits a `tracing::warn`.
  **Verify: U** (merge with global pin set + differing per-slug pin → merged == global pin; warn
  emitted).

- **AC-06** *(FR-09)* — Transport config (TLS / auth / host / `http.enabled`) is unaffected by any
  per-slug file; it is never read at the per-slug seam. **Verify: U + C** (per-slug file setting
  transport keys leaves served transport == global; review confirms transport not read at seam).

- **AC-07** *(FR-11, FR-15)* — The overlay-vs-global verdict is asserted field-by-field across
  **every** config-relevant call-site input — the 9 crt-056 threaded params, `embed_handle`,
  `permissive`, and `instructions` — PLUS the entire `[embedding]` section (closed checklist,
  mirroring crt-056 AC-1). Global-locked inputs (incl. `permissive`) are proven not overlayable;
  overlayable fields (incl. `instructions`) are proven overlayable. The checklist enumerates the FULL
  call-site surface, never "all 9 threaded params". **Verify: U + B** (one assertion per row of the
  FR-11 table, incl. the `permissive` and `instructions` rows).

- **AC-08a** *(FR-10)* — An invalid per-slug `config.toml` (unknown category, oversized instructions,
  malformed TOML, world/group-writable) fails loud at startup via the existing per-file
  `validate_config`, naming the offending slug file. **Verify: U** (each invalid input → startup error
  naming slug).

- **AC-08b** *(FR-07, SR-01)* — A per-slug file that is valid **per-file** but whose **merge with the
  global resolved config** violates a cross-field invariant (e.g. global fusion-weight + per-slug
  fusion-weight sum > 1.0) fails loud at startup via post-merge `validate_config(&merged)`, naming the
  offending slug. Per-file validation alone does NOT catch this (#3905). **Verify: U** (construct a
  global+per-slug pair each individually valid but whose merge violates a sum constraint → startup
  error).

- **AC-09** *(FR-12)* — The overlay applies on restart only; no live-reload path is introduced.
  **Verify: C** (review confirms read occurs once at `build_project_server` time; no reload watcher /
  endpoint added).

- **AC-10** *(FR-14)* — With per-slug files setting distinct `[server] instructions` for slug A and
  slug B, A's served `ServiceLayer` carries A's instructions and B's carries B's; a slug with no
  `[server] instructions` override falls through to the global `server.instructions`. **Verify: B + U**
  (N=2: A's instructions ≠ B's; merge with unset per-slug instructions → merged == global; threaded
  arg reflects the merged value).

- **AC-11** *(FR-16)* — **Machine-checked drift guard (anti-divergence guarantee).** A test asserts
  that `merge_configs`' **actual** overlay-vs-lock behavior matches the single canonical classification
  (FR-16) for **every** config-relevant `build_project_server` call-site input and the `[embedding]`
  section — i.e. for each input the classification marks **overlayable**, a per-slug override of that
  key is observed in the merged config; for each marked **global-locked**, a per-slug override of that
  key is observed NOT to win in the merged config. The classification and `merge_configs` can never
  silently disagree: if a future edit to `merge_configs` flips an arm (or the classification is
  changed) without the other being updated, this test fails. The test PINS `merge_configs`' existing
  behavior to the classification; it does NOT require rewriting `merge_configs`. **Verify: U + C** (one
  assertion per classified input driving `merge_configs` with a global+per-slug pair and checking the
  merged value against the input's canonical verdict; review confirms the FR-11 table and Feature B's
  seed both render from the same classification, no third hand-authored copy).

---

## User / Operator Workflows

1. **Multi-project tuning (the feature):** Operator running a multi-project (cloud / HTTP) daemon
   hand-places `{base_dir}/{slug}/config.toml` for a slug, sets its own `[knowledge].categories` /
   domain packs / confidence weights / overlayable inference tuning, and restarts the daemon. On the
   restart that re-attaches routing, that slug's `ServiceLayer` reflects its overlaid values; other
   slugs are unaffected. (AC-01, AC-03, AC-09)

2. **Single-project / local UDS (the silent majority):** Operator has no per-slug file. The daemon
   serves the global resolved config exactly as before C6 — no behavior change. (AC-02)

3. **Misconfiguration:** Operator places an invalid or cross-field-violating per-slug file. The daemon
   refuses to start and names the offending slug file, rather than silently degrading. (AC-08a/b)

---

## Constraints (from SCOPE)

- **Model invariant (hard):** `nli_handle`, `embed_handle`, `rayon_pool` sourced ONLY from global
  `Arc`s, never from a per-slug merged config.
- **`[embedding]` section locked global:** whole section (model + dimensions + sha256), not just the
  sha256 pin — symmetric with transport.
- **Process-posture locked global:** `permissive` (daemon permission flag) is passed unconditionally
  from the global flag, never per-slug-overlayable (FR-15).
- **`server.instructions` per-slug overlayable:** #785 names `[server] instructions` a per-slug knob;
  the merged value is threaded to each slug, global underlies when unset (FR-14).
- **Reuse, don't reinvent:** existing `merge_configs`, `load_single_config`, `validate_config`.
- **Security carve-out:** hash-pin fields stay global-wins inside the per-slug merge.
- **Seam is fixed:** integration at `build_project_server` call site (`main.rs:1089-1110`); no
  override param in `load_config`.
- **Zero-impact fallthrough:** no per-slug file → global behavior unchanged byte-for-byte.
- **Restart-applies (vnc-038 ADR-007):** no hot-reload.
- **Rust workspace rules:** ≤500 lines/file, no stubs, no `.unwrap()` in non-test, `tracing` for logs.

---

## Dependencies

- **Existing components (reuse):** `load_config`, `load_single_config`, `validate_config`,
  `merge_configs` (`infra/config.rs`); `build_project_server` (`http_provision.rs:132-156`); the
  per-slug loop (`main.rs:1089-1110`).
- **crt-056 seam (load-bearing, A3):** the 9-param + pre-existing `embed_handle` signature of
  `build_project_server`. Design against the merged live signature, not the issue's "8 fields". If the
  signature shifts, the full call-site verdict (FR-11/AC-07) must be re-derived. Note the live
  signature also carries `permissive` and `instructions` — both classified in the FR-11 table.
- **vnc-034:** per-slug isolated store + `vector/` (the per-slug dir the config file lives in).
- **vnc-038 ADR-007 (#5086):** restart re-attaches routing — the moment the overlay is read.
- **Knowledge precedents:** #2395 (two-level merge), #2286 (replace semantics), #4655/#4649 (hash-pin
  global-wins), #5196 (lock whole section describing a global handle), #3905 (post-merge cross-field
  invariant), #4583 (silent-fallback regression), #5172 (model-free N=2 isolation harness).

---

## NOT in Scope (explicit exclusions)

- **Hot-reload / live config reload** — overlay reads at `build_project_server` time only.
- **Per-slug model selection** — both models + pool are global; forbidden, not merely unset.
- **Per-slug transport config** — TLS / auth / host / `http.enabled` stay global.
- **Per-slug `[embedding]` config** — whole section locked global (model + dimensions + sha256).
- **Feature B (config seeding)** — `register <slug>` writing a seeded annotated `config.toml`, and
  container `serve` seeding `DEFAULT_CONFIG_TOML`. Operator hand-places the file for A.
- **New config sections / new tunable fields** — A overlays the EXISTING `UnimatrixConfig` surface
  only; no new knobs. `adapt_service` stays `AdaptConfig::default()` (FR-13).
- **Changing the existing global→project→env layering** inside `load_config`.

### Known Limitation (documented, not a blocker)

A per-slug `config.toml` that sets a **GLOBAL-only section or key** (e.g. `[server.tls]`, transport,
`permissive`-equivalent posture, or `[embedding]`) is **silently ignored at the seam** — the
global-locked verdict simply wins and the per-slug value is never threaded. The sole exception is the
`*_sha256` hash-pin divergence, which emits a `tracing::warn` (AC-05). An operator can place a
`[server.tls]` block in a slug file and see no effect and no warning.

- **Mitigation (in scope, via Feature B):** Feature B's `register`-time annotated seed documents which
  sections are global-only, steering operators away from setting them per-slug. **Hand-off contract:**
  B's annotated seed **RENDERS** the per-slug-vs-global split from Feature A's single canonical
  classification (FR-16) — it is a *consumer*, not a re-deriver. B MUST NOT hand-author a third copy of
  the split in its seed annotations (the crt-031 literal-duplication risk); it reads A's classification
  as the one source of truth. Dependency direction is strictly one-way: A owns the classification, B
  consumes it. This retires R-13's "unowned per-slug-vs-global split" status — the owner is now A's
  FR-16 classification, and B is a downstream consumer of it.
- **Optional future enhancement (out of scope):** a seam-level `tracing::warn` when a per-slug file
  sets a global-locked section. Not implemented in Feature A; recorded as a deferred improvement.

---

## Assumptions Carried From Risk Assessment

- **A1:** `merge_configs` global→project semantics transfer cleanly to global→per-slug. Re-audit the
  inline `InferenceConfig {…}` literal site (#4070) for the new call shape before reuse (SR-02) —
  flagged to architect.
- **A2:** the per-slug vector index stays `VectorConfig::default()` (not config-driven dims). If a
  later change makes dims config-driven, the `[embedding]` divergence (SR-03/FR-05) re-opens. Noted as
  a standing dependency.
- **A3:** the crt-056 seam signature at `main.rs:1089-1110` is stable (C5/#5190 proved on #789). The
  full call-site verdict is derived from the live signature (incl. `permissive` and `instructions`),
  not the issue's "8 fields".

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #2395 (two-level merge), #4655/#4649 (hash-pin global-wins), #5172 (model-free N=2 isolation harness), #5079 (boot-once routing), #5165 (crt-056 ADR-002 seam), #5086 (vnc-038 ADR-007 restart). #5196 ("lock whole config section describing a global handle") found via #4655 related edges — directly supports the `[embedding]` whole-section lock (FR-05).
- Revision briefing (design-gate correction) — surfaced #5198 (vnc-040 ADR-002: model invariants and byte-for-byte fallthrough hold BY CONSTRUCTION), which underpins the AC-02 `Arc::ptr_eq` strengthening and the AC-07 by-construction `permissive` lock. No new patterns stored (read-only tier).
- Canonical-classification revision (Option 2, human-approved) — context_briefing surfaced #4869 (one-way later-wave-consumes-earlier-wave dependency seam: B impls/renders from A, never A→B reference), #4655/#4649 (hash-pin global-wins arms inside merge_configs that the classification must mirror), #4070 (merge_configs is the hidden field-update site — the runtime expression of the split), and #5148 (C6 capability). These underpin FR-16's single-owner classification, AC-11's drift guard pinning merge_configs to it, and the one-way Feature B hand-off contract. No new patterns stored (read-only tier).
