# Risk Coverage Report: crt-053 — Active-Only PPR Expansion Seeds

**Feature**: crt-053 (GH #717) — single-edit `seed_ids` filter in `SearchService::search` (`crates/unimatrix-server/src/services/search.rs:919`).
**Stage**: 3c (Test Execution).
**Acceptance surface (OQ-1, BINDING)**: Rust full-pipeline harness `crates/unimatrix-server/tests/pipeline_e2e.rs` (live `SearchService::search` via `TestHarness`). The nan-018 corpus and the Python MCP suite cannot host AC-01/AC-05 (corpus authors only `Supersedes` edges excluded from positive BFS; MCP has expander default-OFF + no positive-deprecated-edge authoring + no per-request toggle). See OVERVIEW.md §4.
**R-04 control-arm form used**: deprecated seed **forced `Status::Active`** (no second code path), per the test plan's preferred form.

---

## CRITICAL: Non-Skip Evidence for the Rust Acceptance Tests (#723 vacuous-pass guard)

Pre-existing bug **GH#723** (OPEN): `skip_if_no_model()` (`test_support.rs:87`) builds the model dir name with `.replace('/', "--")` → `sentence-transformers--all-MiniLM-L6-v2`, but the downloader/`cache_subdir()` uses `.replace('/', "_")` → `sentence-transformers_all-MiniLM-L6-v2`. On disk only the `_` dir exists, so a default `cargo test` makes every `pipeline_e2e` test **silently early-return** ("ok" in ~0.01–0.03s) — a vacuous pass at the harness level (#4902 trap).

**Workaround applied (NOT a #723 fix — out of scope; documented):** created a symlink
`…/.cache/unimatrix/models/sentence-transformers--all-MiniLM-L6-v2 → sentence-transformers_all-MiniLM-L6-v2`
so the `--` path `skip_if_no_model()` checks resolves to the real model. `ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so`. #723 itself was left unmodified.

**Differential proof the crt-053 tests actually RAN (not skipped):**

| Condition | Run time | Skip lines ("ONNX model not found … skipping") | Result |
|-----------|----------|-----------------------------------------------|--------|
| WITHOUT workaround (default env, `--` dir absent) | **0.01s** | **present** (every test prints the skip line) | "ok" but **vacuous** — bodies skipped |
| WITH workaround (`--` symlink in place) | **2.36s** | **0** (zero skip lines) | **9 passed; 0 failed** — genuine execution |

Exact non-skip evidence (targeted run, `--nocapture --test-threads=1`):
```
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 2.36s
```
Zero "skipping pipeline_e2e test" / "ONNX model not found" lines. Same 9 tests also ran non-skip inside the full `cargo test --workspace` run (pipeline_e2e binary: 16 tests, 0.88s, no skip lines). The 0.01s-vs-2.36s and skip-line-present-vs-absent contrast proves the green is real, not vacuous.

The 9 crt-053 tests, all PASS, non-skip:
`test_seed_filter_excludes_deprecated_only_neighbor`, `…_control`,
`test_supersession_false_positive_guard`, `…_control`,
`test_seed_filter_retains_terminal_active_head`,
`test_off_path_identical_to_baseline`,
`test_proposed_seed_excluded`,
`test_all_seeds_deprecated_no_panic`,
`test_superseded_but_active_is_retained`.

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) / Gate | Result | Coverage |
|---------|-----------------|----------------|--------|----------|
| R-01 | Tester chases an unmeasurable metric gate (P@5/MRR/soft-GT) | GATE-04 grep over `pipeline_e2e.rs` → zero eval-harness gates; all ACs assert entry IDs | PASS | Full |
| R-02 | Filter over-drops a legitimate active seed | `test_seed_filter_retains_terminal_active_head` (asserts H AND Z present); AC-01 positive arm (Y present) | PASS | Full |
| R-03 | Scope creep into the five locked exclusions | GATE-01 diff review: prod diff = 8 lines in `search.rs` only (commit 0e9fc3b5); GATE-02: zero `unimatrix-engine/**` changes | PASS | Full |
| R-04 | Vacuous absence pass (AC-01/AC-05) | `…_control` arms force A→Active; previously-absent neighbor X REAPPEARS; fixture precondition (A→X edge, X no active in-edge) asserted | PASS | Full |
| R-05 | Off-path identity drift (`ppr_expander_enabled = false`) | `test_off_path_identical_to_baseline` (neither X nor Y injected, expander OFF); lexical-scope confirmed (filter inside `if` block, `search.rs:911-921`) | PASS | Full |
| R-06 | Anti-AC violation (deprecated-absence-in-Flexible) | ANTI-AC-01 grep: all `!ids.contains()` target injection-only neighbors (X/Y/V), never the deprecated seed; AC-03 keeps deprecated present + penalized | PASS | Full |
| R-07 | Reverse-walk direction mis-framing | AC-01/AC-05 fixtures state forward edges concretely (A→X, B→Y); verified by neighbor-ID outcome, no `Direction::` inspection | PASS | Full |
| R-08 | Fixture lacks positive-edge revision → AC silently skipped | Resolved by OQ-1: ACs hosted on `pipeline_e2e.rs` with edges authored via `insert_graph_edge` + `rebuild_typed_graph`; non-skip proven above | PASS | Full |
| R-09 | `results_with_scores` not sole seed source | OQ-2 review: `:917` is the only seed collection inside the enabled branch; AC-04 exercises a 6b-injection query and confirms the head reaches the seed set | PASS | Full |
| R-10 | #406 reproduces and is "fixed" as retrieval bug | Did NOT reproduce in the delivery fixture; AC-05 / `_control` pass. #406 remains pre-existing xfail (lifecycle:704, GH#406) — not patched | PASS | Full |
| R-11 | Quarantine gate `:950/:956` conflated/edited | GATE-03 diff review: `SecurityGateway::is_quarantined` at `search.rs:956` UNCHANGED; `test_proposed_seed_excluded` shows seed predicate is defense-in-depth, not a replacement | PASS | Full |
| R-12 | String compare instead of typed enum | GATE-05 source review: predicate is `e.status == Status::Active` (`search.rs:919`); `test_proposed_seed_excluded` proves `== Active`, not `!= Deprecated` | PASS | Full |

All 12 risks covered; zero gaps.

---

## Test Results

### Unit / Rust workspace tests
Hardened convention: `setsid -w timeout 600 cargo test --workspace` (rc=0).
- **Total: 6022**
- **Passed: 6022**
- **Failed: 0**

Includes the `unimatrix-server` lib unit suite (`search.rs mod tests` penalty/ranking helpers) — passed UNCHANGED (AC-03 ranking-path proof) — and the `pipeline_e2e` binary (16 tests incl. all 9 crt-053 tests, non-skip, 0.88s).

### crt-053 Rust acceptance tests (subset, non-skip targeted run)
- **Total: 9 · Passed: 9 · Failed: 0 · Skipped: 0** (2.36s; see Non-Skip Evidence above).

### Integration tests (infra-001 Python MCP harness)
Release binary `target/release/unimatrix`, `ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so`.

| Run | Suites | Result | Time |
|-----|--------|--------|------|
| Smoke (mandatory gate) | all suites `-m smoke` | **23 passed, 0 failed**, 351 deselected | 199s |
| Regression baseline (per OVERVIEW.md §5) | protocol, tools, lifecycle, edge_cases | **289 passed, 9 xfailed, 0 failed, 0 errors** (298 collected; rc=0) | ~14 min |

- Smoke gate: **PASS** (Stage 3c minimum gate met).
- Regression baseline: all green; pre-existing xfails behave as marked (see Triage).

**Integration test totals (infra-001, MCP layer):**
- **Total executed: 312** (23 smoke + 289 non-xfail regression).
- **Passed: 312 · Failed: 0 · xfailed: 9** (all pre-existing, GH-tracked).
- The 9 xfails are NOT crt-053-caused; behaved exactly as marked. Authoritative composition: `--collect-only` reports 298 tests in the 4 regression suites, 9 of them `@pytest.mark.xfail`; full run returned rc=0.

---

## Integration Failure Triage

No crt-053-caused integration failures. The expander is default-OFF at the MCP layer (`default_ppr_expander_enabled() = false`), so by C-02/AC-02 the suites are unchanged-green — any regression here would itself be an AC-02/C-02 violation; none observed.

Pre-existing `xfail` markers encountered (already filed, NOT crt-053-caused, NOT modified):

| Marker | GH Issue | Note |
|--------|----------|------|
| `test_tools.py:475` (deprecated confidence flake) | GH#405 | The `x` seen in the tools suite; explicitly split-out per the four-issue cluster disposition |
| `test_lifecycle.py:704` (multi-hop terminal-active) | GH#406 | Does NOT reproduce in crt-053 fixture; out of scope per brief |
| `test_edge_cases.py:285` (rate-limit rapid stores) | GH#111 | Unrelated pre-existing |
| further `test_lifecycle.py` xfails | (pre-existing GH issues) | Unrelated pre-existing |

**GH Issues filed this session: none.** The one bug surfaced (#723 silent-skip) was **already OPEN** before this session; the spawn prompt scoped it out of crt-053. No new xfail markers added; no integration test deleted or commented out.

---

## Gaps

**None.** All twelve risks (R-01..R-12) and all five acceptance criteria (AC-01..AC-05) plus the anti-AC and the five review/grep gates are covered with passing evidence. The one knowingly-accepted residual (a 6b head whose neighbor is reachable only via the vnc-017 >50-edge redirect ceiling — Locked Decision 4/5) is, per the test plan, deliberately NOT a test target and is not a gap.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_seed_filter_excludes_deprecated_only_neighbor`: Y (active seed B's neighbor) present, X (deprecated seed A's neighbor) absent; `_control` arm forces A→Active and X REAPPEARS (R-04 non-vacuous). Non-skip, 2.36s run. |
| AC-02 | PASS | `test_off_path_identical_to_baseline` (expander OFF): no Phase 0 injection — neither X nor Y appears; filter is lexically inside `if self.ppr_expander_enabled` (`search.rs:911-921`); infra-001 MCP suites (expander OFF) unchanged-green. |
| AC-03 | PASS | `search.rs mod tests` + `pipeline_e2e::test_active_above_deprecated` pass UNCHANGED; deprecated still present in Flexible and ranked below comparable active (no deprecated-absence assertion — ANTI-AC-01). |
| AC-04 | PASS | `test_seed_filter_retains_terminal_active_head`: 6b terminal-active head H AND its neighbor Z both present — filter RETAINS active anchors, not just drops. |
| AC-05 | PASS | `test_supersession_false_positive_guard`: BFS expands from B's path (Y present), not from deprecated A's path (X absent); `_control` arm (A→Active) makes X reappear (R-04). #406 did not reproduce. |
| ANTI-AC-01 | PASS (confirmed absent) | Grep: no assertion that a deprecated entry is absent from Flexible/search results; all negative asserts target injection-only neighbor IDs. |
| GATE-01 | PASS | Prod diff (commit 0e9fc3b5) = +8 lines in `search.rs` only; `test_support.rs` + `pipeline_e2e.rs` are test-only/cumulative. |
| GATE-02 | PASS | Zero changes under `crates/unimatrix-engine/**`; existing `graph_expand` negative tests unedited. |
| GATE-03 | PASS | `search.rs:956` `is_quarantined` enforcement unchanged. |
| GATE-04 | PASS | No P@5/MRR/soft-GT/eval-harness gate in crt-053 tests. |
| GATE-05 | PASS | Predicate is typed `e.status == Status::Active`; `test_proposed_seed_excluded` proves `== Active`, not `!= Deprecated`. |

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced delivery-process lessons on tests named-but-not-implemented (#2656, #4202, #3935) and the silent-skip vacuous-pass concern; applied the #723 non-skip differential discipline directly. The crt-053-specific silent-skip vacuous-pass pattern is already stored as #4918 (dev agent).
- Stored: nothing novel to store — the non-skip differential (run-time + skip-line absence) is a re-application of the existing #4918 pattern + the #4902 vacuous-pass lesson; no new 2+-feature pattern emerged. #723 is already an OPEN GH issue.
