# Agent Report — nan-018-agent-3-shape-hash (Wave 2 / drift guard)

## Files modified
- `crates/unimatrix-server/src/eval/shape/manifest.rs` (new) — ordered/versioned/enumerated manifest + live builder
- `crates/unimatrix-server/src/eval/shape/hash.rs` (new) — deterministic canonical serialize + SHA-256 hex
- `crates/unimatrix-server/src/eval/shape/guard.rs` (new) — `check_drift` severity split + dimension-naming + `ShapeDriftError`
- `crates/unimatrix-server/src/eval/shape/mod.rs` (new) — module wiring + re-exports
- `crates/unimatrix-server/src/eval/shape/tests.rs` (new) — full test suite
- `crates/unimatrix-server/src/eval/mod.rs` (modified, +1 line `pub mod shape;`)

Committed on `feature/nan-018` as `impl(shape-hash): ... (#716)`.

## Tests pass/fail
24/24 passing (verified at rc=0 against a consistent checkout before a concurrent wave agent broke the test build of `infra/config.rs`). Coverage:
- R-03 determinism: N=200 stable, permuted-order unchanged, cross-process self-spawn equal, fixed int-format golden string.
- R-04 sensitivity matrix: per declared entry column, edge-type add/remove/rename, per confidence dim, embed dim, manifest_version; display-only insensitivity; migration_number not hashed.
- R-05 embed live-source: model_id/dimension propagate; embed_sha256 participates; no-literal grep guard over module sources.
- R-06 severity split: primary HardAbort (non-zero), snapshot WARN+continue, message names diverged dimension (`embedding-identity`/`edge-types`), match = no fire, per-class attribution, unknown manifest_version = clear error.

## Issues / blockers
1. **Concurrent-wave checkout race (NOT my code):** `crates/unimatrix-server/src/infra/config.rs` was edited mid-flight by the penalty-config wave agent and currently breaks the LIB TEST compile (references a missing `mod graph_penalty_config_tests;` file, a missing `ConfigError::GraphPenaltyFieldOutOfRange` variant, and a missing `UnimatrixConfig.graph_penalty` field initializer). The LIB build (non-test) passes rc=0, so my module compiles fully. The penalty-config agent will resolve this when their files land. I did not touch config.rs (out of scope; confirmed byte-identical to their version).
2. **Type-name deviation (documented):** pseudocode/test-plan name the embed type `EmbedModel`; the LIVE exported type is `unimatrix_embed::EmbeddingModel`. Used the live type. `EmbeddingModel::model_id()`/`dimension()` are the read-only hash inputs as specified.
3. **R-04 named human delivery gate (open, by design):** declared sets (`RETRIEVAL_RELEVANT_COLUMNS`, `RETRIEVAL_EDGE_TYPES`, `confidence_dimension_names`) are commented as the human-review surface. Tests prove sensitivity to the declared set only — completeness is the named human gate (ARCHITECTURE §7.3), not closable in code. Confidence dims enumerated from the live `ConfidenceWeights` struct (base/usage/fresh/help/corr/trust).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern: deterministic hash/HashMap non-determinism) + context_get #4895 (ADR-002). Findings: confirmed branch (b) embed-in-hash, severity split, structural-determinism (sorted vectors, no HashMap, fixed format) per #2610/#3752 lineage. Applied directly.
- Stored: entry #4903 "Cross-process determinism test via test-binary self-spawn (current_exe + env-gated emitter test)" via context_store (pattern) — the in-process N-loop cannot catch per-process RandomState seed drift; the self-spawn emitter test can. Novel reusable testing gotcha not present in source.
