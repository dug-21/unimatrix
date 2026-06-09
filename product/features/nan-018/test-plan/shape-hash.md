# Test Plan — Drift guard / retrieval-shape hash (`eval/shape/`)

**Component**: `eval/shape/` — ordered versioned manifest, SHA-256 hash computation, drift-guard compare. Reads `EmbedModel::model_id()`/`dimension()` (read-only) + `RelationType` taxonomy + entry columns + confidence dims.
**Wave**: 1. **Primary risks**: R-03 (non-determinism, **Critical**), R-04 (incomplete manifest → silent staleness, High), R-05 (embed-model-dependence, High), R-06 (mismatch under-tested, Med).

## Unit test expectations

### R-03 — hash determinism (NFR-03) — AC-08(c) — **Wave-1 backstop component**
- `test_shape_hash_stable_n100`: compute the hash **N≥100 times** on the same schema; assert all identical.
- `test_shape_hash_permuted_input_order_unchanged`: feed the edge-type taxonomy and confidence-dimension set in **shuffled order**; assert the hash is **unchanged** (proves the manifest serializer sorts/orders before hashing — the #2610 / #1099 / #3752 lineage).
- `test_shape_hash_cross_process_equal`: compute the hash in **two separate process invocations**; assert equality (catches `HashMap` seed randomization, the #2610 failure mode). Implement via a tiny binary/test-harness spawn or a `setsid` sub-invocation comparing stdout hashes.
- `test_shape_hash_float_format_fixed`: the embedding dimension (`384`) and any f64 manifest member serialize via a **fixed, locale-independent** format — assert no `{}` Debug float drift (golden-string compare on the serialized manifest).

### R-04 — per-input sensitivity matrix (hash changes iff a DECLARED input changes) — AC-08(e) — **Wave-1 backstop test #2**
A coverage **matrix**: one assertion per enumerated manifest input proves completeness **against the declared manifest**.
- `test_shape_hash_sensitive_to_entry_column_{col}`: for **each** declared retrieval/penalty entry column (`status`, `supersedes`, `category`, confidence-bearing columns — the architect's enumerated list), mutate it and assert the hash **changes**.
- `test_shape_hash_sensitive_to_edge_type_set`: add/remove/rename a `RelationType` variant ⇒ hash changes.
- `test_shape_hash_sensitive_to_confidence_dimension`: change a `ConfidenceWeights` dimension ⇒ hash changes.
- `test_shape_hash_sensitive_to_embedding_dim`: change `dimension` ⇒ hash changes (overlaps R-05).
- `test_shape_hash_insensitive_to_display_only_column`: mutate a **display-only** (declared out-of-scope) column ⇒ hash **unchanged** (proves the manifest excludes non-retrieval columns by design — the negative half of the matrix).
- `test_shape_hash_sensitive_to_manifest_version`: bumping the `manifest_version` integer ⇒ hash changes (the hash definition is itself migratable, ADR-002).
- `test_migration_number_not_hashed`: changing `migration_number` (legibility only) ⇒ hash **unchanged**.

> **R-04 completeness is NOT closable by test.** These prove sensitivity to the *declared* set only. The **named human delivery gate** (ARCHITECTURE §7.3) certifies the declared set is *complete* — flagged in OVERVIEW §5, not a test here.

### R-05 — embed-model live-source (no regression to embed-dependence) — AC-08(d)
- `test_shape_hash_sensitive_to_model_id`: mutate the value from the embedding-identity source (`model_id`/`dimension`) ⇒ hash changes (the binding constraint — branch (b) holds only if these genuinely feed the hash).
- `test_shape_hash_reads_embed_model_live_not_literal`: assert the hash sources `EmbedModel::model_id()` / `dimension()` (or `InferenceConfig.embedding_model_sha256`) — **not** a hardcoded `"all-MiniLM-L6-v2"` / `384` literal in the shape module. Implement by asserting a changed embed identity propagates, or a grep guard that the shape module has no model-id string literal.

### R-06 — deliberate-mismatch path (the guard must actually fire) — AC-08(b)
- `test_drift_guard_fires_on_mismatch_primary_aborts`: stamp the **primary fixture** corpus, mutate one manifest input (or corrupt the stamp), run `eval run` ⇒ guard triggers a **HARD ERROR (abort, non-zero exit)**.
- `test_drift_guard_warns_on_mismatch_snapshot_continues`: same mutation on the **production snapshot** ⇒ **WARN and continue** (severity split, ARCHITECTURE §7.2 LOCKED).
- `test_drift_guard_message_names_diverged_dimension`: the failure/warn message **names which shape dimension diverged** (FR-22) — assert the message contains the dimension label.
- `test_drift_guard_passes_on_match`: matching hashes ⇒ no fire (the guard isn't a permanent failure).

## Edge cases (RISK-TEST-STRATEGY §Edge Cases)
- Manifest with an unknown/future `manifest_version` integer ⇒ **clear error**, not a silent mis-hash.
- A stamp matching a deliberately-stale corpus on the *same* schema is a trust-of-source issue (mitigated by version control, not testable here) — note in the report, do not test.

## Boundary note
**shape module ↔ embed crate** is the R-05 seam: a literal-embedded value instead of a live read silently severs branch (b) and reintroduces the #4085 fake-MRR-drift class. The live-source assertion is the seam guard.
