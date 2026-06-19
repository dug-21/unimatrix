## ADR-004: A single canonical per-slug-vs-global key classification owned by Feature A; the verdict table, `merge_configs` behavior, and Feature B's seed annotations all DERIVE from it

### Context
The per-slug-overlayable-vs-global-locked split is the central policy of vnc-040 (C6).
Today that split is expressed in TWO non-authoritative places:

1. The **design verdict table** (ARCHITECTURE §9 / ADR-001) — a hand-maintained doc that
   asserts which `build_project_server` inputs are overlayable vs locked.
2. The **implicit per-field arms of `merge_configs`** (`config.rs:≈3825`) — the actual,
   executable behavior: replace-if-non-default for overlayable fields, the hash-pin
   global-wins carve-out (#4655) for locked descriptors.

Feature B (per-slug seed file, SR-06) is slated to ship a THIRD copy: hand-authored
annotations in the seed `config.toml` telling operators which sections are honored vs
ignored. ADR-002 (#5206) already names that annotated seed file the *authoritative* place a
slug operator learns section ownership — which, as written, would make B the third
independent restatement of the split.

Three hand-maintained copies of one conceptual value is exactly the **crt-031
literal-duplication anti-pattern**: a value that conceptually belongs in operator/config
gets copied to N sites and silently diverges. The verdict table can drift from
`merge_configs`' real arms (a field added to the merge but not the table, or vice versa);
B's annotations can drift from both. R-13's mitigation today is "documented but unowned,
deferred to B" — no single owner, no machine check.

This ADR makes the split have **exactly one owner** so it cannot diverge.

It does NOT rewrite `merge_configs`. That function is existing, proven, shared code that
already enforces the split correctly (including the hash-pin global-wins carve-out). ADR-001
LOCKS its reuse; ADR-002 builds the model invariants on its current shape. Re-architecting it
into a data-driven/generic merge engine is explicitly out of scope and rejected here.

### Decision
Feature A defines ONE canonical, declarative classification of the per-slug-vs-global split
as the single source of truth. The verdict table, `merge_configs`' behavior, and Feature B's
future seed annotations all DERIVE from it rather than independently restating it.

**Chosen shape — a minimal declarative registry colocated with config**, in `infra/config.rs`
beside `merge_configs`/`validate_config`:

- A const slice enumerating every config key/section that participates in the per-slug seam,
  each tagged with its disposition:

  ```rust
  pub enum OverlayDisposition { PerSlugOverlayable, GlobalLocked }

  pub struct ConfigKeyClass {
      /// Stable identifier for the key/section, e.g. "knowledge.categories",
      /// "inference.embedding_model_sha256", "server.instructions", "server.tls".
      pub key: &'static str,
      pub disposition: OverlayDisposition,
  }

  /// THE canonical per-slug-vs-global classification. Single source of truth.
  pub const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass] = &[ /* … */ ];

  pub fn is_per_slug_overlayable(key: &str) -> bool { /* lookup */ }
  ```

The slice enumerates each call-site-relevant config key/section from ARCHITECTURE §9's verdict
rows: `knowledge.categories`, `knowledge.boosted_categories`, `confidence.weights`,
`observation.domain_packs`, the overlayable `inference.*` weights, `server.instructions`,
`nli_top_k`, `nli_enabled` as `PerSlugOverlayable`; `inference.embedding_model_sha256`,
`inference.nli_model_sha256`, `permissive`, and the transport/daemon sections
(`server.tls`, `http`, auth/host) as `GlobalLocked`.

**Three consumers bind to it:**

1. **Verdict table (doc).** ARCHITECTURE §9's table is documented as the human-readable
   *rendering* of `PER_SLUG_CONFIG_CLASSIFICATION`, not an independent assertion. Its framing
   changes from "here is the split" to "here is the split, as rendered from the canonical
   classification in `config.rs`."

2. **`merge_configs` drift-guard test (machine-checked anti-divergence guarantee).** A new
   consistency test asserts that `merge_configs`' ACTUAL overlay-vs-lock behavior matches
   `PER_SLUG_CONFIG_CLASSIFICATION` for EVERY classified key. `merge_configs` keeps its current
   hand-written per-field arms unchanged — the test, not a rewrite, is the binding. For each
   classified key the test constructs a global config and a per-slug config that differ ONLY on
   that key, runs `merge_configs(&global, &slug)`, and asserts:
   - `PerSlugOverlayable` ⇒ the merged value equals the per-slug value (overlay won);
   - `GlobalLocked` ⇒ the merged value equals the global value (lock held — covers the
     `*_sha256` global-wins carve-out and any future locked descriptor).
   A field whose merge arm disagrees with its classification fails the test. The registry and
   the merge therefore can never silently disagree: a new field added to the merge but not the
   registry (or classified one way but merged the other) breaks the build. This is R-13's
   mitigation turned from prose into a machine check.

3. **Feature B seed annotations (hand-off contract).** Feature B's annotated seed `config.toml`
   RENDERS its "honored per-slug / ignored-because-global" annotations from
   `PER_SLUG_CONFIG_CLASSIFICATION` rather than hand-restating them. The classification, owned in
   A, is the contract B consumes; B adds no fourth copy. (This refines ADR-002's residual: the
   seed file remains where the operator READS ownership, but it is no longer the SOURCE of that
   ownership — A is.)

The registry is data-only and minimal; it introduces no new config knobs and no new merge
logic. It is colocated with `merge_configs`/`validate_config` so the source of truth, the merge
it constrains, and the drift-guard test all live together.

### Consequences
- **Easier:** the split has exactly one owner (A). The verdict table, `merge_configs`, and B's
  seed annotations are bindings to it, not independent copies — the crt-031 literal-duplication
  divergence is structurally prevented.
- **Easier:** R-13 graduates from "documented but unowned, deferred to B" to "owned in A,
  machine-checked." A regression that diverges the merge from the classification (or omits a new
  field from the registry) fails the drift-guard test rather than shipping a silent overlay/lock
  bug.
- **Easier:** Feature B builds on a typed contract instead of re-deriving the split — the hand-off
  is a render, not a re-litigation (extends SR-06 / ADR-001's shared-path contract).
- **Bounded cost (intentional):** delivery owns one new declarative slice + predicate and one new
  drift-guard test. `merge_configs` keeps its proven hand-written arms — no generic merge engine,
  no behavior change to the shipped merge (honors ADR-001's reuse lock and ADR-002's
  by-construction invariants).
- **New maintenance obligation:** adding a future config field that crosses the per-slug seam now
  requires a classification entry; forgetting one is caught by the drift-guard test (the entry and
  the merge arm are co-required). This is the intended forcing function, not incidental cost.
- **Drift-guard test obligation (for the tester/delivery):** one consistency test, asserting
  `merge_configs`' real overlay-vs-lock outcome equals the disposition of EVERY entry in
  `PER_SLUG_CONFIG_CLASSIFICATION`, including the `*_sha256` global-wins carve-out. This is the
  anti-divergence guarantee and is mandatory.
- Cross-references ADR-001 (the overlay seam and verdict table this classification backs),
  ADR-002 (whose §6b embedding-descriptor lock and documented section-ownership residual this
  classification now formalizes and owns), and ADR-003 (post-merge re-validation — orthogonal:
  `validate_config` checks merged values are *valid*; this classification checks they came from
  the *right layer*).
```
