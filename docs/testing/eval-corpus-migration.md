# Schema-Migration Runbook — Re-Stamping the Fixture Corpus

Band-2 runbook (nan-018, AC-11). What to do when a schema/shape change trips the
drift guard: re-stamp the corpus, bump the migration number, revalidate
assertions. Sufficient to migrate the corpus **from this page alone**. Pairs with:

- [Fixture-corpus authoring guide](./eval-fixture-authoring.md)
- [Two-corpus model](./eval-two-corpus-model.md)
- [Config-knob reference](./eval-config-knobs.md)
- [Evaluation harness overview](./eval-harness.md)

This runbook mirrors the canonical Unimatrix procedure
("Migrate the eval fixture corpus and author a fixture scenario") and the
convention "a change that alters the retrieval-shape hash MUST trigger a
fixture-corpus migration." Both surface in agent briefings.

---

## What the drift guard is

The fixture corpus carries a **retrieval-shape hash** stamp. At the start of a
fixture sweep, the harness computes the **running schema's** shape hash from live
inputs and compares it to the stamp (`eval/shape/guard.rs::check_drift`):

- **Primary fixture corpus** mismatch ⇒ **HARD ERROR** (`ShapeDriftError::HardAbort`,
  abort, non-zero exit). The corpus is the durable yardstick whose numbers feed
  downstream spikes → product ranking; silent drift would propagate to product
  behavior, so an invalid yardstick aborts the run.
- **Production snapshot** mismatch ⇒ **WARN** and continue (it is ephemeral by
  contract — re-snapshot when it drifts).

The error/warning **names which input class diverged** so the fix is obvious:
`entry-columns`, `edge-types`, `confidence-dims`, `embedding-identity`, or
`manifest-version`.

### What feeds the hash (ADR-002 enumerated manifest)

The hash is computed (SHA-256, lowercase hex) over an **ordered, versioned,
enumerated** manifest (`eval/shape/manifest.rs`). The four input classes:

1. **Entry columns** — the *retrieval-relevant* `entries` columns
   (`RETRIEVAL_RELEVANT_COLUMNS`: `status`, `supersedes`, `superseded_by`,
   `category`, `trust_source`, `access_count`, `last_accessed_at`, `created_at`,
   `helpful_count`, `unhelpful_count`). Display-only columns (e.g. `content`,
   `summary`) are deliberately **excluded** — so "does a display-only column count?"
   is answered by the manifest, not by reviewer judgment.
2. **Edge types** — the retrieval-participating `RelationType` variants
   (`RETRIEVAL_EDGE_TYPES`), by `as_str()`.
3. **Confidence dimensions** — the `ConfidenceWeights` field names
   (`base`, `usage`, `fresh`, `help`, `corr`, `trust`).
4. **Embedding identity** — `EmbeddingModel::model_id()`,
   `EmbeddingModel::dimension()`, and `InferenceConfig.embedding_model_sha256` when
   set. **This is load-bearing (OQ-3 branch (b)):** an embed-model change moves the
   hash and trips the guard, so the durable reference is protected against ONNX
   embed-model drift **without** a frozen vector sidecar.

Determinism is structural: every collection is a `Vec` sorted at build time, never
a `HashMap` — source-declaration order never affects the hash.

---

## The corpus stamp

`crates/unimatrix-server/src/eval/corpus/fixtures/manifest.toml`:

```toml
manifest_version = 1       # bumped ONLY when the hash INPUT SET changes (new class)
migration_number = 47      # human legibility ONLY — NOT a hash input
shape_hash = "25b2a18c...252fc0d8"   # SHA-256 lowercase hex (64 chars)
```

Three fields, three distinct jobs:

| Field | Job | When it changes |
|-------|-----|-----------------|
| `shape_hash` | The machine guard. | Re-stamped whenever the running hash changes. |
| `migration_number` | Human label for legibility. **Never a hash input.** | Bumped on every migration, monotonically. |
| `manifest_version` | The hash *definition* version. | Bumped **only when the input SET changes** — a new input class added/removed — distinct from `migration_number`. A new *value* of an existing input does **not** bump it. |

> If the running manifest declares a `manifest_version` this binary does not
> understand, the guard returns `ShapeDriftError::UnknownManifestVersion` and
> **refuses to hash** — a clear error, not a silent mis-hash. A `manifest_version`
> bump is a code change (the `MANIFEST_VERSION` const in `manifest.rs`) plus a
> stamp update, done together.

---

## PART A — Migrate the corpus + re-stamp (when a change alters the hash)

1. **Identify what diverged.** Run the fixture sweep; the hard-error message names
   the diverged input class (columns vs edge types vs confidence dims vs embedding
   identity vs manifest version). That tells you which schema change tripped it.

2. **Update the corpus to the new shape.** Adjust `[[entries]]` (columns, statuses,
   `superseded_by` edges) as the schema change requires. If the change is a new
   *retrieval-relevant column* or *edge type*, it must also be added to the
   declared manifest lists in `eval/shape/manifest.rs` (`RETRIEVAL_RELEVANT_COLUMNS`
   / `RETRIEVAL_EDGE_TYPES`) — and that is a **`manifest_version` bump** (see step
   4 and the R-04 note below).

3. **Recompute and re-stamp `shape_hash`.** Set `shape_hash` to the new running
   hash. The fixture sweep's hard-error message prints both `stamped=` and `live=`
   hashes; the `live=` value is the new stamp. Bump `migration_number` (legibility
   only).

4. **Bump `manifest_version` ONLY if the input SET changed** — i.e. a new input
   *class* (a new declared column, edge type, confidence dim, or embedding field)
   was added or removed. A new *value* of an existing input (e.g. a different
   embedding `model_id`, an added entry) does **NOT** bump `manifest_version` — it
   only re-stamps `shape_hash`. The `manifest_version` const and the stamp must
   move together.

5. **Revalidate assertions against the new shape** (PART B discipline). Update any
   property assertion the shape change affects — e.g. if a status semantics change
   alters which entry is the terminal head, fix the `redirect_to_head` /
   `rank_below` anchors.

6. **Validate once.** Run the corpus a single time (a migration-validation run) and
   confirm it **loads** AND the **drift guard passes** (primary = hard error on
   mismatch; snapshot = warn). This one-time validation run is allowed.
   **Do NOT turn eval *results* into a standing gate** — that is an explicit
   Non-Goal (eval is the instrument, not the referee).

### ⚠️ R-04 — manifest completeness is a named human review (not routine review)

A test can only prove the hash is sensitive to the columns the manifest
**declares**. No test can prove the *declared* column set is **complete**. If a
schema change adds a column the live retrieval/ranking path reads but you forget to
add it to `RETRIEVAL_RELEVANT_COLUMNS`, the guard gives **false confidence** while
the corpus silently drifts.

So: when a migration touches the declared lists, a **named human reviewer** must
confirm the lists cover **every** column / edge / confidence dimension the live
retrieval/ranking path reads — that no retrieval-relevant input was mis-classified
as display-only and omitted. This is a delivery obligation distinct from automated
tests and routine code review.

---

## PART B — Revalidating (and authoring) assertions

Assertion discipline is in the
[fixture-corpus authoring guide](./eval-fixture-authoring.md). The migration-time
essentials:

- **Property assertions only**, authored against **stable aliases** — never literal
  ids. Alias discipline is exactly what makes assertions shape-stable across a
  migration: ids renumber, aliases don't.
- **`rank_below` is asymmetric:** A-absent ⇒ pass; B-absent (while A present) ⇒
  **fail**. Re-check this on every migration — a shape change that drops an entry
  from results can flip a `rank_below` from pass to fail (or mask a real
  regression).
- Keep the corpus **small and curated**; breadth grows over time, not in a single
  migration.

After authoring a new scenario, re-stamp **only if** the new scenario changed the
shape inputs — usually it does **not** (adding entries within the existing shape
leaves the hash unchanged). Adding a new retrieval-relevant *column* or *edge type*
does.
