# Component: Drift guard / retrieval-shape hash — `shape-hash.md`

**Wave**: 1
**Location**: `crates/unimatrix-server/src/eval/shape/{manifest.rs, hash.rs, guard.rs, mod.rs}` (new).
**ADR**: ADR-002 (#4895). **Risks**: R-03 (critical — determinism), R-04 (incomplete manifest),
R-05 (embed-model live source), R-06 (mismatch path fires).

## Purpose

Compute a deterministic retrieval-shape hash over an ordered, versioned, ENUMERATED manifest;
stamp it on the fixture corpus; at eval start recompute the running schema's hash and compare —
HARD ERROR (abort) on the primary corpus, WARN (continue) on the snapshot. This is the triple
linchpin: drift guard + OQ-5 trigger predicate + embed-model-dependence settle on ONE hash.

## Manifest (`manifest.rs`) — ordered, versioned, enumerated

```
pub const MANIFEST_VERSION: u32 = 1;     // bumped ONLY when the input SET changes (not per corpus)

pub struct ShapeManifest {
    pub manifest_version: u32,                         // = MANIFEST_VERSION
    pub entry_columns: Vec<(String, String)>,          // (column_name, type) — retrieval-relevant subset, SORTED
    pub edge_types: Vec<String>,                       // RelationType::as_str() participating in retrieval, SORTED
    pub confidence_dims: Vec<String>,                  // ConfidenceWeights/ConfidenceParams field names, SORTED
    pub embedding_model_id: String,                    // EmbedModel::model_id() — LIVE read
    pub embedding_dimension: usize,                    // EmbedModel::dimension() — LIVE read
    pub embedding_model_sha256: Option<String>,        // InferenceConfig.embedding_model_sha256 when set
}
```

### Building the manifest from the LIVE schema

```
pub fn build_running_manifest(embed: &EmbedModel, inference: &InferenceConfig) -> ShapeManifest {
    ShapeManifest {
        manifest_version: MANIFEST_VERSION,
        entry_columns: RETRIEVAL_RELEVANT_COLUMNS.iter().sorted().cloned().collect(),  // see DECLARED SET below
        edge_types:    RETRIEVAL_EDGE_TYPES.iter().map(RelationType::as_str).sorted().collect(),
        confidence_dims: confidence_dimension_names().sorted(),                         // from the live ConfidenceWeights struct
        embedding_model_id: embed.model_id().to_string(),        // *** LIVE — R-05, NOT a literal ***
        embedding_dimension: embed.dimension(),                  // *** LIVE — R-05, NOT 384 literal ***
        embedding_model_sha256: inference.embedding_model_sha256.clone(),
    }
}
```

### DECLARED entry-column set (R-04 — and the residual human gate)

`RETRIEVAL_RELEVANT_COLUMNS` is the explicitly enumerated list of `entries` columns that feed
retrieval/penalty/ranking — e.g. `status`, `supersedes`/`superseded_by`, `category`, and the
confidence-bearing columns. Display-only columns are deliberately EXCLUDED, named in the manifest
so "does a display-only column count" is answered by the manifest, not by judgment.

> **GAP / NAMED HUMAN DELIVERY GATE (R-04, LOCKED §7.3)**: a test can only prove the hash is
> sensitive to the DECLARED set. It CANNOT prove the declared set is COMPLETE. Before delivery, a
> NAMED human reviewer must certify that `RETRIEVAL_RELEVANT_COLUMNS` covers every column the live
> retrieval/ranking path reads — no retrieval-relevant column mis-classified as display-only. This
> is a delivery gate, not codeable. The pseudocode flags it; it cannot close it.

> **GAP**: the exact `confidence_dimension_names()` field list must be read from the live
> `ConfidenceWeights`/`ConfidenceParams` struct at delivery. Pseudocode fixes the access pattern
> (sorted field names); the concrete names are a delivery read against the live type.

## Hash (`hash.rs`) — deterministic serialization (R-03)

```
pub fn compute_shape_hash(m: &ShapeManifest) -> String {
    let mut buf = String::new();
    // FIXED canonical order; every collection ALREADY sorted; fixed float/int formatting.
    write!(buf, "manifest_version={}\n", m.manifest_version);
    for (name, ty) in &m.entry_columns       { write!(buf, "col:{}={}\n", name, ty); }   // sorted vec
    for et in &m.edge_types                  { write!(buf, "edge:{}\n", et); }            // sorted vec
    for cd in &m.confidence_dims             { write!(buf, "conf:{}\n", cd); }            // sorted vec
    write!(buf, "embed_model_id={}\n", m.embedding_model_id);
    write!(buf, "embed_dim={}\n", m.embedding_dimension);                                 // usize — fixed integer format
    if let Some(sha) = &m.embedding_model_sha256 { write!(buf, "embed_sha={}\n", sha); }
    // NEVER iterate a HashMap; NEVER use {:?} Debug float formatting (R-03).
    sha256_lowercase_hex(buf.as_bytes())     // 64-hex
}
```

Determinism is STRUCTURAL: sorted vectors (no map iteration), fixed string/int serialization, no
locale-dependent or Debug float formatting. (No raw f64 currently in the manifest — dimension is
`usize`; if any f64 enters later it must use a fixed `{:.N}` format, not `{}`/`{:?}`.)

## Guard (`guard.rs`) — compare + severity split (R-06, LOCKED §7.2)

```
pub enum CorpusKind { PrimaryFixture, ProductionSnapshot }

pub fn check_drift(
    running: &ShapeManifest,
    stamped_hash: &str,
    kind: CorpusKind,
) -> Result<(), ShapeDriftError> {
    let live = compute_shape_hash(running);
    if live == stamped_hash { return Ok(()); }

    let diverged = name_diverged_dimensions(running, stamped_hash);   // which input class differs (for triage)
    let msg = format!("retrieval-shape drift: diverged dimension(s) = {diverged}; stamped={stamped_hash} live={live}");

    match kind {
        CorpusKind::PrimaryFixture     => Err(ShapeDriftError::HardAbort(msg)),   // *** abort, non-zero exit ***
        CorpusKind::ProductionSnapshot => { tracing::warn!("{msg}"); Ok(()) }     // *** warn, continue ***
    }
}
```

- `name_diverged_dimensions` recomputes per-class sub-hashes (column set / edge types / confidence
  dims / embedding identity) and reports WHICH class diverged, so the migration fix is obvious (FR-22).
- The HardAbort on the PRIMARY corpus DELIBERATELY overrides the eval `report` exit-0 convention:
  the guard protects corpus VALIDITY (a precondition), distinct from the body-only quality verdict.
- Manifest with an unknown/future `manifest_version` ⇒ clear error, NOT a silent mis-hash.

## Stamping (corpus authoring / migration)

```
pub fn stamp_corpus(running: &ShapeManifest) -> String { compute_shape_hash(running) }
// written into eval/corpus/fixtures/manifest.toml (corpus-fixtures.md); migration_number bumped by author.
```

## Branch (b) — embed-model-in-hash (OQ-3, R-05)

Embedding model-id + dimension are FIRST-CLASS hash inputs, read LIVE from `EmbedModel`. Therefore
embed-at-load is safe and NO frozen vector sidecar is shipped. A model/dimension change flips the
hash and trips the guard. If a future delivery removes embed identity from the hash, branch (a)
(frozen sidecar) becomes mandatory and MUST be stated explicitly (durability reversal).

## Call site

At eval start, before replaying scenarios: build the running manifest, load the corpus stamp,
`check_drift(.., kind)`. On `HardAbort`, abort the run with non-zero exit.

## Data flow

- **Inputs**: `EmbedModel` (read-only), `InferenceConfig`, the live schema's retrieval columns +
  edge types + confidence dims; the corpus manifest stamp.
- **Output**: `Ok(())` or `ShapeDriftError::HardAbort` (primary) / WARN (snapshot).

## Error handling

- Primary mismatch ⇒ `HardAbort` (abort, non-zero exit), dimension-naming message.
- Snapshot mismatch ⇒ WARN, continue.
- Unknown `manifest_version` ⇒ clear error, never silent mis-hash.

## Key test scenarios

- **Determinism (R-03, NFR-03)**: compute the hash N≥100 times ⇒ all identical;
  permute edge-type / confidence-dim input order ⇒ hash UNCHANGED (proves sorting);
  cross-process: compute in two process invocations ⇒ equal (catches HashMap seed randomization);
  float/int format: `dimension` (384) serializes via fixed locale-independent format.
- **Per-input sensitivity (R-04.1)**: mutate EACH declared input (each entry column, edge-type set,
  confidence dim, embed dim, model-id) ⇒ hash CHANGES. One assertion per declared input.
- **Manifest-version (R-04.2)**: changing `MANIFEST_VERSION` changes the hash.
- **Embed-model sensitivity + live source (R-05)**: change `model_id`/`dimension` ⇒ hash changes;
  assert the hash reads from `EmbedModel::model_id()`/`dimension()`, NOT a hardcoded literal.
- **Deliberate mismatch (R-06, AC-08b)**: stamp a corpus, mutate one input ⇒ guard fires;
  message names the diverged dimension; PRIMARY ⇒ hard abort (non-zero exit),
  SNAPSHOT ⇒ warn-and-continue (severity split).
- **Unknown manifest-version**: clear error, not a silent mis-hash.
- **(Review gate, not a test)** R-04 named human column-completeness review at delivery.
