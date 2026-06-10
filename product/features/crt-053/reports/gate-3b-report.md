# Gate 3b Report: crt-053

> Gate: 3b (Code Review)
> Date: 2026-06-10
> Result: PASS (with one environment-conditional WARN — see Vacuous-Pass Guard)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | Production edit is exactly `.filter(\|(e, _)\| e.status == Status::Active)` on the `seed_ids` build, byte-identical to Stage 3a pseudocode (`search.rs:915-919`). |
| Architecture compliance | PASS | Single edit site inside `if self.ppr_expander_enabled`; off-path bit-for-bit identical by lexical scope; typed-enum predicate. Matches ADR-001 / ARCHITECTURE. |
| C-01 single production change | PASS | Production diff = the filter clause + a descriptive comment only. `test_support.rs` + `pipeline_e2e.rs` are test-only (cumulative). No new symbol, no `find_terminal_active` on injected entries, no `penalty_map` mutation, no config flag, no edge-write, quarantine gate untouched. |
| Interface implementation | PASS | No new signature. `Status` already imported; `graph_expand` signature unchanged; `seed_ids: Vec<u64>` reused. |
| Test case alignment | PASS | All 9 planned crt-053 tests present and named per test plan; AC-01..AC-05 + R-04 control arms + edge cases all implemented. |
| Tests actually PASS when executed | PASS | With the model present, all 9 crt-053 tests run for real (2.68s) and pass — including both R-04 differential control arms and the Proposed-exclusion GATE-05 test. |
| GATE-04 (no eval-harness gate / R-01) | PASS | Zero p@5/MRR/soft-GT/eval-harness assertions in crt-053 tests. |
| R-02 (positive retention / AC-04) | PASS | `test_seed_filter_retains_terminal_active_head` asserts both H and its neighbor Z are present — proves filter RETAINS, not just drops. |
| R-04 (differential control arms) | PASS | AC-01 and AC-05 each have a paired `_control` arm that forces the deprecated seed Active and asserts the previously-absent neighbor reappears. |
| GATE-05 (typed `== Status::Active`) | PASS | Predicate is typed enum; `test_proposed_seed_excluded` proves a non-Active non-Deprecated status (Proposed) is excluded → `== Active`, not `!= Deprecated`. |
| ANTI-AC-01 | PASS | No test asserts deprecated entries absent from Flexible; absence assertions target neighbor IDs (X/V), not the deprecated seed. |
| Code quality (compile, stubs, unwrap) | PASS | `cargo build --workspace` clean; no `todo!`/`unimplemented!`/TODO/FIXME/unsafe added; no `.unwrap()` added to production. |
| File size <= 500 lines | WARN | `search.rs` is 5890 lines (pre-existing; crt-053 added 8). Splitting it would violate C-01. Not crt-053-remediable. See findings. |
| Security | PASS | No new external input; predicate reads in-memory typed enum. No secrets, no path/command injection, no deser added. Quarantine gate `:956` unchanged. |
| Knowledge stewardship | PASS | Dev agent report has `## Knowledge Stewardship` with `Queried:` + `Stored: #4918`. |
| **Vacuous-pass guard (env-conditional)** | **WARN** | In THIS environment all crt-053 tests SILENTLY SKIP due to a pre-existing `skip_if_no_model()` path mismatch (`--` vs `_`). A default green run is a vacuous pass. Tests proven correct only after manual model-path fix. See dedicated section. |

## Detailed Findings

### Pseudocode fidelity / Architecture compliance / C-01
**Status**: PASS
**Evidence**: The entire production diff (`git diff` of impl commit `0e9fc3b5` on `search.rs`) is:
```rust
-                let seed_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
+                // crt-053: seed graph_expand from ACTIVE entries only ...
+                let seed_ids: Vec<u64> = results_with_scores
+                    .iter()
+                    .filter(|(e, _)| e.status == Status::Active)
+                    .map(|(e, _)| e.id)
+                    .collect();
```
This is byte-identical to the Stage-3a pseudocode and the ARCHITECTURE "The Change" block.
- **Single edit site**: lives inside `if self.ppr_expander_enabled` (block opens ~`:911`).
- **No new symbol / no scope creep**: diff grep for `is_quarantined`, `find_terminal_active`,
  `penalty_map`, `config`, `edge` on added/removed lines returns only the comment word
  "Quarantined". No `find_terminal_active` call on injected entries, no `penalty_map` mutation,
  no config flag, no edge-write change.
- **Quarantine gate (R-11)**: `SecurityGateway::is_quarantined(&entry.status)` at `search.rs:956`
  (the architecture's "~:950") is NOT in the diff — unchanged.
- **GATE-02 (#4495 trip-wire)**: impl commit touches zero files under `crates/unimatrix-engine/**`;
  existing `graph_expand` negative tests are unedited.
- **Import**: `Status` already in scope; no import edit needed (C-01 tightened).
- **Off-path equivalence (C-02)**: filter binding `seed_ids` is lexical-scope-local to the enabled
  branch; the OFF path never evaluates it. Confirmed by review + `test_off_path_identical_to_baseline`.

### Test case alignment + behavior coverage
**Status**: PASS
**Evidence**: `pipeline_e2e.rs` adds the full planned set:
- AC-01 `test_seed_filter_excludes_deprecated_only_neighbor` (+ `_control`, R-04)
- AC-05 `test_supersession_false_positive_guard` (+ `_control`, R-04)
- AC-04 `test_seed_filter_retains_terminal_active_head` (R-02 retention — asserts H AND Z present)
- AC-02 `test_off_path_identical_to_baseline` (expander OFF → neither X nor Y injected)
- `test_proposed_seed_excluded` (GATE-05 / R-12)
- `test_all_seeds_deprecated_no_panic` (empty-seed boundary)
- `test_superseded_but_active_is_retained` (discriminator is status, not `superseded_by`)

**R-04 control arms verified individually**: each `_control` test rebuilds the identical fixture/edges
but forces the deprecated seed Active, then asserts the previously-absent neighbor (X) reappears AND
the active-seed neighbor (Y) stays present — proving the real-arm absence is filter-caused, not
unreachability (#4902). Isolation uses a topic filter (seeds in "k8s" = HNSW-eligible; neighbors in
"ref" = graph-injection-only) so a neighbor's presence is attributable solely to seed survival.

**GATE-05**: `test_proposed_seed_excluded` inserts a `Status::Proposed` seed P with edge P->V and
asserts V is NOT injected — proving the predicate excludes a non-Deprecated non-Active status, i.e.
it is genuinely `== Active`, not merely `!= Deprecated`.

**ANTI-AC-01**: no assertion that a deprecated entry is absent from Flexible. The `!ids.contains(&x)`
/ `!ids.contains(&v)` assertions target the injection-only *neighbor*, and the comments explicitly
note "A itself (Deprecated) MAY still appear in results from the HNSW path — that is correct (C-03)."

### Tests actually pass when executed
**Status**: PASS (after manual model-path fix; see WARN below for the default-env caveat)
**Evidence**: After symlinking the model dir to the `--` name `skip_if_no_model()` expects, a real run
of the crt-053 filter yields:
```
test result: ok. 9 passed; 0 failed; ... finished in 2.68s
```
with zero "ONNX model not found" lines — i.e. the model loaded and the full pipeline executed. The
filter behavior is therefore genuinely verified: deprecated-only neighbors excluded, active-seed
neighbors retained, control arms reverse the exclusion, Proposed excluded, off-path inert.

### Code quality
**Status**: PASS
**Evidence**: `cargo build --workspace` finishes clean. No `todo!()`/`unimplemented!()`/`TODO`/
`FIXME`/`unsafe` added by the impl commit. No `.unwrap()` added to production code (the only
`.expect()` additions are in test support / tests, which is acceptable). Clippy on
`unimatrix-server` shows only pre-existing warnings (unused imports across the crate), none
attributable to the 8-line change.

### File size
**Status**: WARN
**Evidence**: `search.rs` = 5890 lines, far over the 500-line limit; `pipeline_e2e.rs` = 1168 lines.
Both conditions are pre-existing. crt-053 added only 8 production lines. Splitting `search.rs` is a
multi-hundred-line refactor that would directly violate the C-01 "filter clause is the ONLY
production change" constraint and the locked-scope mandate. **This is not crt-053-remediable** and is
recorded as a WARN, not a blocking FAIL, because the binding feature constraint forbids the fix.
Recommend tracking the `search.rs` size as separate technical debt.

### Security
**Status**: PASS
**Evidence**: Per RISK-TEST-STRATEGY Security section — the change accepts no new external input; the
predicate reads an already-loaded in-memory `EntryRecord.status` typed enum on candidates already
admitted by HNSW. No path, query param, deserialization, secret, or command invocation added. The
`:956` quarantine enforcement is unchanged, so defense-in-depth is intact (the seed predicate
dropping Quarantined seeds supplements, not replaces, the `:956` gate).

### Knowledge stewardship
**Status**: PASS
**Evidence**: `agents/crt-053-agent-3-search-seed-filter-report.md` contains a `## Knowledge
Stewardship` block with `Queried:` (context_briefing/search/get — ADR-001 #4917, #3637, #3992) and
`Stored: entry #4918` (the topic-filter isolation pattern + the skip_if_no_model gotcha). Obligation
satisfied.

## Vacuous-Pass Guard (REQUIRED FINDING — record explicitly)

**The new AC-01..AC-05 tests DO NOT EXECUTE in the default environment — they silently SKIP.**

Root cause (pre-existing, NOT introduced by crt-053):
- `skip_if_no_model()` (`test_support.rs:87`) builds the model dir with
  `config.model.model_id().replace('/', "--")` → `sentence-transformers--all-MiniLM-L6-v2`.
- The model downloader / `cache_subdir()` (`model.rs:70`) uses `.replace('/', "_")` →
  `sentence-transformers_all-MiniLM-L6-v2`.
- On disk in this environment ONLY the underscore dir exists. The dash path
  `skip_if_no_model()` checks does **not** exist, so it returns `true` and every pipeline_e2e
  test early-returns.

Empirical proof:
- Default run: `running 10 tests` then 10x `ONNX model not found ... skipping pipeline_e2e test`,
  `test result: ok ... finished in 0.03s` — all "ok" but every body skipped (a vacuous pass).
- After symlinking the underscore dir to the dash name: same tests run in **2.68s**, zero skip
  lines, 9 passed — proving they (a) are correct and (b) were genuinely no-ops before.

Assessment: the **code and tests are correct**; the **test harness silently skips them in this
environment**, so an unguarded green `cargo test` for crt-053 is a vacuous pass (the #4902 trap at
the harness level). This does not invalidate the production change — validated to pass on real
execution — but it is a genuine gate/CI-integrity concern: CI or any reviewer relying on a default
green run would certify nothing.

Disposition: recorded as **WARN**, not a blocking FAIL, because (1) the tests are proven correct on
real execution, (2) the root cause is pre-existing infra unrelated to the crt-053 production change,
and (3) fixing `skip_if_no_model()` is out of crt-053's locked scope (C-01). Strongly recommend a
separate infra fix to align the two path conventions (or make `skip_if_no_model()` check the
`_` form) before crt-053 is relied upon in CI, and that Gate 3c / RISK-COVERAGE-REPORT assert the
crt-053 tests actually RAN (non-skip), not merely that the suite was green.

## Rework Required

None blocking. Two follow-ups (neither crt-053-scoped, neither blocks 3b):
| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `skip_if_no_model()` `--` vs `_` path mismatch causes silent skip of all pipeline_e2e tests | infra (separate ticket) | Align `skip_if_no_model()` dir convention with `cache_subdir()` (`_`), or check both. Make Gate 3c assert crt-053 tests executed (non-skip). |
| `search.rs` 5890 lines (>500) | deferred / separate refactor | Pre-existing; cannot be fixed under crt-053 C-01. Track as tech debt. |

## Scope Concerns

None. Production change is exactly the locked single-edit filter; no scope creep into any of the five
locked exclusions.

## Knowledge Stewardship
- Stored: nothing novel to store -- the recurring patterns this gate relied on are already captured:
  the harness-level silent-skip vacuous-pass is now documented in the dev agent's stored pattern
  (#4918), and the scope-creep / vacuous-pass / unmeasurability lessons (#4495, #4902, #4888) already
  exist. No new 2+-feature validation pattern emerged.
