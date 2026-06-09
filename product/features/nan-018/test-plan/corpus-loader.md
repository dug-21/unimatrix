# Test Plan — Fixture corpus loader + property assertions (`eval/corpus/`)

**Component**: `eval/corpus/loader.rs` (loads fixture entry-graphs → snapshot DB + alias→id map), `eval/corpus/assertions.rs` (property resolution). Materializes a DB the existing `EvalServiceLayer::from_profile` consumes unchanged.
**Wave**: 1. **Primary risks**: R-09 (literal/null regress, High), R-10 (alias resolution, High), R-14 (Wave independence), security (path traversal).

## Unit test expectations

### R-09 — loader rejection of forbidden `expected` forms — AC-05 — **(static audit is in corpus-fixtures.md)**
- `test_loader_rejects_null_expected_primary`: a primary-corpus fixture with **null `expected`** is rejected (or is unrepresentable in the format).
- `test_loader_rejects_literal_id_expected_primary`: a primary-corpus fixture with a **literal-ID `expected`** is rejected. Codifies crt-013 #703 "assert outcomes, never constants"; bans the ASS-037/ASS-039 self-consistency trap.

### R-10 — alias→id resolution (no path degrades to silent pass) — AC-05
- `test_alias_renumber_survival`: load the same fixture **twice with deliberately different id assignment**; assert every alias-based assertion resolves to the **same logical entry** and yields the **same pass/fail verdict** (the whole point of alias indirection — assertions survive re-snapshot/renumber).
- `test_alias_missing_is_hard_load_error`: an assertion referencing an **undefined alias** is a **hard load error**, never a silent vacuous pass.
- `test_alias_duplicate_rejected`: a fixture defining the **same alias twice** (or two fixtures sharing an alias) is rejected — assert global uniqueness or scoped resolution per the chosen rule.

### Property-anchor resolution — AC-05
- `test_property_anchor_resolves_chain_head`: redirect-to-head resolves "the chain head" to the terminal-active entry via `find_terminal_active` against the loaded graph.
- `test_property_anchor_resolves_weakest_active`: rank-below "weakest active" anchor resolves correctly against the loaded graph.

### Reuse of existing replay (cumulative infra)
- `test_corpus_loads_into_eval_service_layer`: the materialized snapshot DB is consumed by `EvalServiceLayer::from_profile(db_path, ..)` **unchanged** — corpus is just another snapshot source; replay/metric machinery reused, not re-implemented.

## Security (RISK-TEST-STRATEGY §Security Risks)
- `test_corpus_loader_writes_only_under_controlled_path`: the loader materializes the snapshot DB + vector dir **only under a controlled temp/eval path**.
- `test_corpus_fixture_path_traversal_rejected`: a fixture referencing an absolute or `../` file path **must not escape** the controlled path (path-traversal check on author-supplied references).
- `test_corpus_malformed_toml_errors_cleanly`: a malformed/oversized fixture TOML **errors cleanly**, does not panic or hang.

## R-14 — Wave independence (NFR-04) — owned here for the Wave-1-alone gate
- `test_wave1_suite_green_without_wave2_artifacts`: the Wave-1 acceptance suite (AC-01…09 + AC-14) passes with **zero Wave-2 artifacts present** — no Band-2 docs, no `RECOMMENDATION-band3-protocol.md`, no `convention`/`procedure` entries. Build/test Wave-1 with the `docs/` Band-2 guides and recommendation paths **absent**. A Wave-2 artifact that becomes a Wave-1 code dependency is a defect.

## R-16 — boundary breach (git-diff gate) — AC-13 — **(static audit, owned here)**
- `test_no_protocol_edits`: `git diff --name-only origin/main -- .claude/protocols/` is **empty** (zero `.claude/protocols/` file edited).
- `test_no_eval_gate_wiring`: review-checklist + mechanical check that no CI/PR hook makes eval **results** a standing decision gate (the one-time migration-validation run is allowed; a standing gate is not).
- `test_recommendation_doc_recommendation_only`: assert `product/features/nan-018/RECOMMENDATION-band3-protocol.md` exists and is recommendation-only (Wave-2 — runs when the doc lands; the git-diff half is Wave-1-applicable).

## Edge cases
- A chain whose head is itself deprecated (dead-end, optional 5th shape): redirect-to-head has no valid head ⇒ defined behavior (**fail, not panic**) — shared with `trust-metric.md`.
