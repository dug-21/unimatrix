# Risk-Based Test Strategy: crt-038

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `effective()` short-circuit omitted or placed after the re-normalization branch, producing skewed weights (w_sim'≈0.588, w_conf'≈0.412) in production | High | Med | Critical |
| R-02 | AC-12 eval run executed before AC-02 is implemented — baseline comparison against the wrong scoring path | High | Med | Critical |
| R-03 | ASS-039 MRR=0.2913 baseline was measured on the re-normalized path (nli_enabled=false, no short-circuit), not on conf-boost-c direct weights — AC-12 gate has no valid baseline | High | Med | Critical |
| R-04 | Shared helpers (`write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs`) accidentally deleted — `nli_detection_tick.rs` fails to compile | High | Low | High |
| R-05 | `write_edges_with_cap` retained as dead code after `run_post_store_nli` removal — clippy -D warnings fails | Med | Med | High |
| R-06 | Residual references to removed symbols cause compile failure rather than a test failure — silent gap in AC-09 verification | Med | Med | High |
| R-07 | `NliStoreConfig` partial deletion — struct deleted from `store_ops.rs` but import in `mod.rs` or construction site left behind | Med | Med | High |
| R-08 | `process_auto_quarantine` call site in `maintenance_tick` still passes the two dropped parameters — compile error exposed only by full workspace build | Med | Med | Med |
| R-09 | Formula test `test_fusion_weights_default_sum_unchanged_by_crt030` not updated — assertion message references wrong feature, causing misleading CI output | Low | High | Med |
| R-10 | `w_util=0.00` / `w_prov=0.00` operator config overrides silently lost — production instance had non-default config active | Low | Low | Low |
| R-11 | `background.rs` sequencing comment ("Must remain after maybe_run_bootstrap_promotion") retained as stale documentation | Low | High | Low |

---

## Risk-to-Scenario Mapping

### R-01: effective() short-circuit omitted or misplaced

**Severity**: High
**Likelihood**: Med
**Impact**: Every `context_search` and `context_briefing` query silently applies w_sim≈0.588, w_conf≈0.412 instead of the evaluated conf-boost-c formula. No runtime error surfaces; ranking is degraded in an unmeasured way.

**Test Scenarios**:
1. `test_effective_short_circuit_w_nli_zero_nli_available_false`: construct `FusionWeights` with `w_nli=0.0`, `w_sim=0.50`, `w_conf=0.35`; call `effective(false)`; assert returned weights equal input exactly (no scaling).
2. `test_effective_short_circuit_w_nli_zero_nli_available_true`: same construction; call `effective(true)`; assert unchanged (existing fast path also unchanged).
3. `test_effective_renormalization_still_fires_when_w_nli_positive`: construct with `w_nli=0.20`; call `effective(false)`; assert re-normalization occurs (guard must not suppress the `w_nli > 0` path).

**Coverage Requirement**: Short-circuit guard must be unit-tested with both `nli_available` values and must NOT suppress the re-normalization path for positive `w_nli`. Both the new behavior and the preserved behavior must be covered before AC-12 can be run.

---

### R-02: Eval run before AC-02 is implemented

**Severity**: High
**Likelihood**: Med
**Impact**: MRR is compared against 0.2913 baseline using a different scoring formula. A pass would be a false positive; a fail would be undiagnosable. Either invalidates the gate.

**Test Scenarios**:
1. Delivery ordering check: `cargo test --workspace` must pass (confirming AC-02 unit tests pass) before eval is invoked. Document in PR checklist.
2. Eval output must include the git commit hash at time of run. Reviewer confirms hash is post-AC-02 commit.

**Coverage Requirement**: The PR description must confirm the eval was run after the AC-02 commit. A commit hash in the eval output satisfies this.

---

### R-03: ASS-039 baseline measured on wrong scoring path

**Severity**: High
**Likelihood**: Med
**Impact**: The MRR=0.2913 gate value applies to a formula that crt-038 does not actually implement. Passing AC-12 would be meaningless; failing it would be a false alarm.

**Test Scenarios**:
1. Delivery must inspect the ASS-039 eval configuration: confirm whether the harness was invoked with `nli_enabled=true, w_nli=0.0` (effective(true) — no re-normalization, weights unchanged) or `nli_enabled=false` on a build without the short-circuit.
2. ADR-001 documents that ASS-039 was run with `nli_enabled=true` and `w_nli=0.0`. Delivery must verify this against the ASS-039 harness configuration or commit hash before accepting the baseline as valid.
3. If the baseline was measured on the re-normalized path: a new baseline eval must be run on the direct conf-boost-c formula before AC-12 can gate anything.

**Coverage Requirement**: The PR description must state which scoring path the ASS-039 baseline was measured on, with supporting evidence (harness config or commit hash). Merge is blocked without this determination.

---

### R-04: Shared helpers accidentally deleted

**Severity**: High
**Likelihood**: Low
**Impact**: `nli_detection_tick.rs` fails to compile; `cargo build --workspace` fails; the entire server binary is unshippable.

**Test Scenarios**:
1. `cargo build --workspace` must succeed after all removals (AC-13 verification).
2. `grep -r "write_nli_edge\|format_nli_metadata\|current_timestamp_secs" crates/unimatrix-server/src/services/nli_detection.rs` must return three matches (pub(crate) definitions).
3. Search `nli_detection_tick.rs` line 34 import compiles without modification.

**Coverage Requirement**: Post-removal build must be verified before opening the PR. AC-13 grep check is non-optional.

---

### R-05: write_edges_with_cap retained as dead code

**Severity**: Med
**Likelihood**: Med
**Impact**: `cargo clippy --workspace -- -D warnings` fails with "function is never used" warning. AC-11 blocks PR merge.

**Test Scenarios**:
1. After removing `run_post_store_nli`, verify `write_edges_with_cap` has zero callers: `grep -r "write_edges_with_cap" crates/` returns zero matches outside `nli_detection.rs` itself.
2. Delete `write_edges_with_cap` alongside the three dead functions (ADR-004 decision).
3. `cargo clippy --workspace -- -D warnings` passes clean.

**Coverage Requirement**: `write_edges_with_cap` must be explicitly listed in the deletion checklist and grep-verified absent before AC-11 is claimed.

---

### R-06: Residual removed-symbol references cause compile failure

**Severity**: Med
**Likelihood**: Med
**Impact**: PR opens with a broken build, or a partial deletion leaves one file compiling while another has a dangling import. Historical pattern: entry #2758 confirms incomplete symbol removal is a recurrent gate-3c failure mode.

**Test Scenarios**:
1. Grep-verify each symbol in the Architecture's Symbol Checklist returns zero matches in compiled source before `cargo test` is run.
2. `cargo build --workspace` after each independent group (Group A, then each Group B component) to catch partial-deletion compile errors early.
3. `cargo test --workspace` final run catches any test-only residual references.

**Coverage Requirement**: The Symbol Checklist from ARCHITECTURE.md (deleted functions, struct, enum, parameters) must be grep-verified to zero before marking AC-09 complete. No single-file build shortcut.

---

### R-07: NliStoreConfig partial deletion — mod.rs import or constructor left behind

**Severity**: Med
**Likelihood**: Med
**Impact**: Compile error in `services/mod.rs` ("unresolved import" or "unknown field"). Would surface on `cargo build` but only if that file is compiled — silently missed in incremental builds during development.

**Test Scenarios**:
1. `grep -r "NliStoreConfig" crates/` returns zero matches (AC-14 verification command).
2. `grep -r "nli_store_cfg" crates/` returns zero matches (constructor argument removed).
3. Full `cargo build --workspace` (not incremental) passes after Group A + all Group B removals complete.

**Coverage Requirement**: AC-14 grep must be run against the full workspace, not just `store_ops.rs`. The `mod.rs` import and construction site are the most likely residuals.

---

### R-08: process_auto_quarantine call site not updated

**Severity**: Med
**Likelihood**: Med
**Impact**: Compile error in `background.rs` at `maintenance_tick` call site. Surfaces only during full workspace build, not file-local compilation.

**Test Scenarios**:
1. `cargo build --workspace` passes — this is the definitive check.
2. Inspect `maintenance_tick` (~line 946) directly after signature change to confirm argument count matches updated signature.
3. `cargo clippy` will also flag extra arguments before the compile error — run before pushing.

**Coverage Requirement**: Full workspace build is required. Partial builds (single crate) may not cover this cross-function call within the same file.

---

### R-09: Formula sum test assertion message not updated

**Severity**: Low
**Likelihood**: High
**Impact**: CI output references "crt-030" formula context in a post-crt-038 build — misleading to future debuggers, but not a functional failure.

**Test Scenarios**:
1. `test_fusion_weights_default_sum_unchanged_by_crt030` assertion message updated to reference "crt-038".
2. Expected sum value `0.92` is unchanged — only the message string changes.

**Coverage Requirement**: String update is a one-line change; verify in diff review.

---

### R-10: Operator config overrides for w_util / w_prov silently lost

**Severity**: Low
**Likelihood**: Low
**Impact**: Any production instance with `w_util` or `w_prov` overrides in a config file would now have those signals zeroed by default. No data loss, but ranking silently shifts.

**Test Scenarios**:
1. Delivery confirms in PR description that no production config files contain `w_util` or `w_prov` overrides (SR-05 resolution).
2. Config loading test: verify `InferenceConfig` deserialization with explicit `w_util=0.05` still overrides the default (field retained, only default changes).

**Coverage Requirement**: PR description confirmation is sufficient. No new automated test required beyond confirming override deserialization still works.

---

### R-11: Stale sequencing comment retained in background.rs

**Severity**: Low
**Likelihood**: High
**Impact**: The comment "Must remain after maybe_run_bootstrap_promotion" (~line 781) references a deleted function, confusing future readers.

**Test Scenarios**:
1. After removing the bootstrap promotion call site, grep for the comment text and confirm it is removed: `grep -n "maybe_run_bootstrap_promotion" crates/unimatrix-server/src/background.rs` returns zero matches.
2. Any sequencing rationale that remains valid should be rewritten without reference to the deleted function.

**Coverage Requirement**: Comment removal included in the diff; verified in PR review.

---

## Integration Risks

**Scoring pipeline path**: `config.rs` defaults flow through `FusionWeights::from_config` into `FusionWeights::effective`, then into `compute_fused_score`. The AC-02 short-circuit is a guard in `effective()` — if it is not executed (e.g., placed after the `nli_available` branch rather than before), the formula silently diverges. The integration test for R-01 specifically verifies the guard fires on the `effective(false)` path.

**Cross-module import at removal boundary**: `nli_detection_tick.rs` imports from `nli_detection.rs` via an explicit use statement at line 34. This is the sole cross-module dependency that survives the removals. The three retained symbols (`write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs`) must remain `pub(crate)` at their current locations. Any accidental visibility change (e.g., made private during a cleanup pass) would produce a compile error in `nli_detection_tick.rs` that may not surface until `cargo build --workspace`.

**`process_auto_quarantine` signature boundary**: The function is called from `maintenance_tick` within the same file (`background.rs`). Signature changes are intra-file but span ~300 lines between definition and call site. Both the definition and the call site must be updated atomically or the file will not compile. A partial edit (signature updated, call site not, or vice versa) produces a compile error that incremental builds may suppress.

---

## Edge Cases

**`w_nli == 0.0` exact float equality**: The short-circuit uses `self.w_nli == 0.0` (ADR-001). This is an exact f64 equality check. Since `w_nli` is set by a literal `0.00` in `default_w_nli()`, the comparison is safe. A risk exists if any code path produces `w_nli` via floating-point arithmetic that evaluates to a tiny non-zero value (e.g., `0.35 - 0.35 = ~1e-17`). This is not a current code path, but should be verified — the default is a constant literal, not a computed value.

**All-zero weights pathological case**: `FusionWeights::effective()` retains a zero-denominator guard for the pathological all-zero case. With the short-circuit at the top, this guard is only reachable when `w_nli > 0.0` and `nli_available=false` and all remaining weights are also zero. Verify the existing guard is still reachable in tests after the short-circuit is added.

**Deleted tests and retained tests coexist in the same `#[cfg(test)]` module**: `nli_detection.rs` has 13 tests to delete and some tests to retain. A line-range deletion that accidentally removes a `#[cfg(test)]` or `mod tests {` boundary will silently disable all remaining tests in the module without a compile error. Verify test count post-deletion against the spec's retained symbol list.

**`nli_enabled=false` and `run_graph_inference_tick`**: The background tick gates `run_graph_inference_tick` behind `if inference_config.nli_enabled`. With the default now `false`, this tick will not run in production unless explicitly configured. This is the intended behavior per SCOPE.md non-goals, but delivery should confirm the gate is on `InferenceConfig.nli_enabled` (retained field) and not on the removed `NliStoreConfig.enabled`.

---

## Security Risks

**No new untrusted input surfaces introduced**: This feature changes defaults and removes dead code. No new external input pathways are created.

**Eval harness input (`scenarios.jsonl`)**: The eval harness reads 1,585 scenario records from a static file in the repository. This file is not user-controlled at eval time. Blast radius if malformed: eval produces incorrect MRR and the gate comparison is invalid — not a security concern, but a data integrity concern (R-03 already covers the baseline validity risk).

**Retained NLI inference path (`run_graph_inference_tick`)**: The tick-based NLI path is retained and gates on `nli_enabled`. With the default now `false`, this path is inactive by default. If an operator enables it via config, the existing path runs unchanged. The removal of `NliStoreConfig` does not affect the tick path's security posture.

**Dead NLI paths removed reduce attack surface**: `run_post_store_nli` accepted serialized entry content and passed it to the NLI cross-encoder. Its removal eliminates that inference path entirely, shrinking the code surface that processes stored content through an external model.

---

## Failure Modes

**Eval gate fails (MRR < 0.2913)**: This is a blocking gate. If R-03 is resolved (baseline is valid) and MRR still fails, the scoring formula change has a regression. Delivery must diff the effective weights against the ASS-039 evaluated formula and identify the divergence. The short-circuit (AC-02) must be verified as actually executing in the production code path.

**`cargo clippy` fails after removal**: Most likely cause is `write_edges_with_cap` retained as dead code (R-05) or a stale import left behind after symbol deletion. Run clippy incrementally after each Group B component removal to isolate the source.

**`cargo test --workspace` fails after formula change (AC-01/AC-02)**: Most likely cause is a test asserting old default values (`w_nli=0.35`, `nli_enabled=true`) that was not updated. The spec's Modified Test Symbols list is authoritative. Any test not in that list but still asserting old values is an untracked residual — find with `grep -r "w_nli.*0.35\|nli_enabled.*true"` in test source.

**Cross-module compile failure after AC-13 removal**: If `nli_detection_tick.rs` fails to compile, the retained symbols were accidentally deleted or their visibility changed. Restore from the deleted functions diff rather than re-typing — the functions' surrounding code may have been removed in the same edit.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — effective(false) re-normalization diverges from conf-boost-c | R-01 | ADR-001 mandates the `w_nli==0.0` short-circuit before the nli_available branch. Two new unit tests (AC-02) verify both effective(true) and effective(false) return weights unchanged when w_nli=0.0. |
| SR-02 — eval run before AC-02 produces invalid baseline comparison | R-02 | ADR-003 mandates Step 1 (AC-02) → Step 2 (AC-01) → Step 3 (eval). Ordering is enforced by delivery sequence in SPECIFICATION.md. PR must include commit hash confirming eval ran post-AC-02. |
| SR-03 — residual test references to removed symbols cause compile failure | R-06 | Symbol checklist in ARCHITECTURE.md must be grep-verified to zero before claiming AC-09. Lesson #2758 (gate-3c symbol retention failure) elevates this to mandatory. |
| SR-04 — NliStoreConfig contradiction between Background and AC-14 | R-07 | ADR-002 resolves: AC-14 is authoritative; struct deleted entirely. Grep `NliStoreConfig` across full workspace, not just store_ops.rs. |
| SR-05 — w_util/w_prov zeroing silently drops operator signal | R-10 | Accepted as low-risk. PR description must confirm no production config overrides. Config deserialization test verifies explicit override still works. |
| SR-06 — shared helpers in nli_detection.rs must survive removal | R-04, R-05 | Architecture specifies exactly three retained symbols (write_nli_edge, format_nli_metadata, current_timestamp_secs) and one to delete (write_edges_with_cap). AC-13 grep verifies retained; R-05 scenario verifies write_edges_with_cap is absent. |
| SR-07 — background.rs pre-existing 500-line violation is not a gate condition | — | Accepted by architecture. NFR-05 explicitly exempts pre-existing over-limit files from gate failure. No architecture-level risk created. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 8 scenarios |
| High | 4 (R-04, R-05, R-06, R-07) | 10 scenarios |
| Med | 2 (R-08, R-09) | 4 scenarios |
| Low | 2 (R-10, R-11) | 3 scenarios |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for "lesson-learned failures gate rejection" — entry #2758 (gate-3c symbol retention failure) and #3579 (missing mandatory tests) directly informed R-06 severity elevation.
- Queried: `mcp__unimatrix__context_search` for "risk pattern" category=pattern — entry #4003 (w_nli=0.0 re-normalization pattern) confirmed the R-01/R-02/R-03 risk chain; entry #3206 (FusionWeights additive field dual exemption) confirmed no additive-term interaction risk.
- Queried: `mcp__unimatrix__context_search` for "FusionWeights effective scoring formula weight normalization" — entry #4005 (ADR-001 crt-038) and #4003 confirmed architecture-risk alignment.
- Queried: `mcp__unimatrix__context_search` for "dead code removal surgical deletion compilation breakage shared symbols" — entry #3256 (compile isolation pattern) and #366 (cfg-split shared module merge procedure) confirmed R-04 as a non-trivial shared-symbol risk.
- Stored: nothing novel — R-01/R-02/R-03 risk chain (eval gate depends on correct scoring path being implemented first) is already captured in entry #4003 and the crt-038 ADRs. The pattern of "eval gate invalidated by wrong scoring path" may warrant a cross-feature pattern entry after delivery confirms the resolution.
