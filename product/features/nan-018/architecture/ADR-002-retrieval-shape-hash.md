# ADR-002 (nan-018): Retrieval-Shape Hash — Ordered Versioned Manifest, Enumerated Inputs, OQ-3 Branch (b)

### Context

Curated eval data goes stale: schema/shape evolution (new columns, edge types, confidence dimensions) silently invalidates a curated corpus, and — separately — ONNX embed-model drift silently invalidates embed-at-load baselines (Unimatrix #4085 burned on KB-snapshot drift producing fake MRR deltas; #500 snapshot-drift caution). The fixture corpus is meant to be the **durable yardstick**; a yardstick that silently changes length is worthless.

This hash is the **triple linchpin** (SR-01): one definition of "retrieval shape" must serve (1) the mechanical drift guard, (2) the OQ-5 protocol-trigger predicate ("your change alters the retrieval-shape hash"), and (3) the OQ-3 embed-model-dependence question. If its enumerated inputs are wrong or its serialization is non-deterministic, all three break together. Hashing is fragile to ordering/serialization non-determinism (SR-01).

OQ-3 (resolved, architecture, with binding caveat): the durable reference must **not** silently become embed-model-dependent. Branch (a) = frozen vector sidecar; branch (b) = embedding model-id/dimensionality in the hash so embed-at-load is safe. OQ-3 and OQ-4 are coupled; OQ-4/OQ-5 unify on this one hash.

### Decision

**Choose OQ-3 branch (b): embedding model-id and dimensionality are first-class hash inputs.** Embed-at-load is therefore safe — if the ONNX model or its output dimension changes, the running shape hash diverges from the corpus stamp and the guard fires. No frozen vector sidecar (branch (a)) is shipped. Rationale: branch (b) collapses two staleness axes (schema shape + embed-model drift) into one guard and one OQ-5 trigger definition (the SR-01/SR-03 design recommendation); a sidecar would add a second, independently-maintained durability artifact and a second drift definition.

**The hash is computed over an explicit, ordered, versioned manifest.** Determinism is structural, not incidental:

1. A `manifest_version: u32` constant (bumped only when the *input set* changes — distinct from the per-corpus `migration_number`).
2. The input set, serialized in a **fixed canonical order** (never `HashMap` iteration; sorted vectors of `(key, value)`):
   - **Entry columns** — the ordered list of retrieval-relevant `entries` columns (name + type), sourced from the migration-defined schema, **excluding display-only columns** that do not affect retrieval (the manifest names which columns are in scope, so "does a display-only column count" is answered by the manifest, not by a delivery-leader judgment — this is what makes OQ-5's predicate deterministic).
   - **Edge types** — the sorted set of `RelationType::as_str()` values (16 today) that participate in retrieval (the Supersedes-penalty + PPR-positive set), per `graph.rs:86-122`.
   - **Confidence dimensions** — the ordered confidence-signal field set feeding scoring (the `ConfidenceWeights`/`ConfidenceParams` dimension names).
   - **Embedding identity** — `EmbedModel::model_id()` (`&'static str`), `EmbedModel::dimension()` (`384`), and `InferenceConfig.embedding_model_sha256` when set (`unimatrix-embed/src/model.rs:26,40`; `infra/config.rs:279`). This is the load-bearing OQ-3 input.
3. The canonical serialization is hashed (SHA-256, lowercase hex) → `shape_hash`.

**Corpus manifest stamp:**

```toml
# alongside the fixture corpus
manifest_version = 1
migration_number = 47          # human legibility only; NOT a hash input
shape_hash = "ab12…"           # 64-hex
```

**Guard behavior** (in `eval/shape/`):
- At eval start, compute the running schema's shape hash from the live inputs.
- Compare to the corpus stamp. **Primary/fixture corpus: hard error (abort)** on mismatch. **Snapshot (realism layer): warn** on mismatch (it is ephemeral by contract). **(LOCKED, human-ratified — ARCHITECTURE §7.2.)** Rationale: the primary corpus is the durable yardstick whose numbers feed ass-073 → crt-053 ACs → product ranking, so silent drift propagates to product behavior, not just a dashboard; breaking the eval exit-0 convention here is correct because the drift guard protects *corpus validity* (a precondition), a different class from the body-only quality verdict (which stays advisory).
- The error/warning names *which* input class diverged (column set vs edge types vs confidence dims vs embedding identity) so the fix is obvious.

**Mandatory tests (AC-08):**
- **Determinism/stability:** computing the hash twice over the same inputs yields the identical string; map-ordering perturbation does not change it (guards SR-01 serialization non-determinism).
- **Deliberate mismatch:** mutate one input (e.g. add an edge type, change `dimension()` to 768, change `model_id`) and assert the hash changes AND the guard fires with the correct diverged-class message.
- **Embedding-drift case (OQ-3 branch (b) proof):** changing `model_id`/`dimension` alone flips the hash — proving embed-at-load is protected.

### Consequences

**Easier:** Staleness becomes loud even when the (future, recommended) protocol trigger is missed — the mechanical guard is the live, code-level backstop nan-018 actually ships. One hash serves drift guard + OQ-5 predicate + embed-model protection (SR-01/SR-03 unified). Embed-at-load with no sidecar keeps corpus authoring low-tax (the OQ-3 lean) without sacrificing durability. The diverged-class message makes migrations self-documenting.

**Harder:** The manifest's "retrieval-relevant column" list is a curated judgment encoded once; if a future change adds a retrieval-affecting column but the manifest author forgets to include it, the guard gives false confidence (SR-01 residual risk) — mitigated by the deliberate-mismatch test discipline and the Band-2 migration runbook. The `manifest_version` bump is a manual step on any input-set change (documented in the runbook). Embedding identity in the hash means a legitimate model upgrade forces a corpus re-stamp + re-embed — intended (it *is* a shape change), but it is real maintenance.

**Coupling:** OQ-4 (this hash) and OQ-5 (the trigger predicate, ADR-005) are one definition. The enumerated input set above is exactly what the Band-3 recommendation documents as "what feeds the hash" — not a separate enumerated trigger list.
