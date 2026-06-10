# Agent Report — nan-018-agent-3-corpus-loader

**Component**: Fixture corpus loader + shared property-assertion types (`eval/corpus/`)
**Wave**: 1
**Status**: Complete. 21 corpus tests pass; 193 eval lib tests green.

## Files created
- `crates/unimatrix-server/src/eval/corpus/mod.rs` — submodule + re-exports
- `crates/unimatrix-server/src/eval/corpus/assertions.rs` — on-disk fixture parse types (`RawFixture/RawEntry/RawScenario`), path-traversal guard (`safe_join`)
- `crates/unimatrix-server/src/eval/corpus/loader.rs` — `load_fixture_corpus`, `AliasMap`, `LoadedCorpus`, `CorpusError`, snapshot materialization, head-member precompute
- `crates/unimatrix-server/src/eval/corpus/tests.rs` — 13 loader/integration tests

## Files modified (additive)
- `crates/unimatrix-server/src/eval/scenarios/types.rs` — OWNS shared types: `pub type EntryRef`, `pub struct ExpectedAssertions { redirect_to_head, forbidden_absent, rank_below }`, additive `ScenarioRecord.assertions: Option<ExpectedAssertions>` (`#[serde(default)]`); `expected` untouched
- `crates/unimatrix-server/src/eval/scenarios/mod.rs` — re-export `EntryRef`, `ExpectedAssertions`
- `crates/unimatrix-server/src/eval/scenarios/extract.rs`, `runner/tests_metrics.rs` — `assertions: None` at the two `ScenarioRecord` construction sites
- `crates/unimatrix-server/src/eval/mod.rs` — `pub mod corpus;` (single line)

## Invariants honored
- R-09/C-04: `LiteralIdExpected` + `NullExpected` hard errors (empty assertion set also rejected as null)
- R-10: `DuplicateAlias` (global), `MissingAlias` for assertion + `superseded_by` refs — never a vacuous pass; `AliasMap::resolve` total at eval time because load validated existence
- Path traversal: lexical guard rejects absolute / `..` before any FS access
- head_members precomputed via `find_terminal_active` (graph.rs:547)
- Snapshot reuse: materialized DB consumed by UNCHANGED `EvalServiceLayer::from_profile` (verified by integration test; rebuilt graph retains the 3 fixture entries)

## Ownership resolution
Sole definer of the shared on-disk types (per spawn note). Did NOT define `TrustOutcome` / `evaluate_trust` (Wave-2 trust-metric). `AliasMap` exposes `resolve` + `head_members` accessors the trust evaluator consumes.

## Key design decision (flag for trust-metric agent)
`AliasMap::resolve(&str) -> Option<u64>` returns `Option` (not the pseudocode's total `-> u64`) to stay panic-free per the no-unwrap rule. Resolution is still logically total — load proved every assertion alias exists — so the trust evaluator can `.expect()`/unwrap-with-context safely, or treat `None` as an internal invariant violation. `head_members(&str) -> &BTreeSet<u64>` is total (returns a shared empty set for unknown heads).

## Notes / non-blockers
- ONNX vector embedding is intentionally NOT wired into the loader: `from_profile` falls back to an empty vector index when no vector dir exists, so DB materialization alone yields a consumable snapshot and keeps loader unit tests model-free. If the AC-14 sweep needs populated vectors, that embedding pass belongs in the runner/fixtures wiring, not the loader's unit-testable core.
- Fixture TOML assets (`eval/corpus/fixtures/` + manifest stamp) are owned by `corpus-fixtures.md` (separate agent), not produced here.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search -- surfaced #2652 (EvalServiceLayer read-only wrapper), #2806 (eval profile->snapshot->replay pattern), #2676 (vector snapshot round-trip), #4064 (dual-default trap). Confirmed the corpus = snapshot-source reuse boundary.
- Stored: entry #4901 "Materializing a snapshot DB for eval: set entries.supersedes column, not graph_edges rows" via /uni-store-pattern -- captures the non-obvious Pass-2a authoritative-column gotcha (graph_edges Supersedes rows are skipped), edge direction, the counter-collision trap, and the embedding-decoupling note.
