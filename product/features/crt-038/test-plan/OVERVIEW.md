# crt-038 Test Plan Overview

## Test Strategy

crt-038 has two distinct test concerns with different risk profiles:

**Group A — Formula correctness (high risk, blocking)**
: `FusionWeights::effective()` short-circuit and `InferenceConfig` default changes.
These are pure unit tests: deterministic, fast, run in `cargo test`. They MUST pass before
the eval gate (AC-12) is run. Any failure here invalidates the eval baseline.

**Group B — Dead-code removal (medium risk, completeness)**
: Deletions across `nli_detection.rs`, `store_ops.rs`, `background.rs`, `mod.rs`.
Testing is structural: grep-verify each symbol is absent, then confirm `cargo build
--workspace` succeeds. Compilation is the primary gate. Unit tests for removed code
paths are deleted alongside the code.

**Mandatory sequencing (ADR-003)**
: Step 1 → effective() short-circuit tests pass; Step 2 → config defaults tests pass;
Step 3 → eval gate AC-12; Step 4 → dead-code removal and structural verification;
Step 5 → full `cargo test --workspace` + `cargo clippy --workspace -- -D warnings`.

---

## Risk-to-Test Mapping

| Risk | Priority | Test Type | Test Location | Test Names / Verification |
|------|----------|-----------|---------------|--------------------------|
| R-01 — effective() short-circuit omitted/misplaced | Critical | Unit | search.rs | `test_effective_short_circuit_w_nli_zero_nli_available_false`, `test_effective_short_circuit_w_nli_zero_nli_available_true`, `test_effective_renormalization_still_fires_when_w_nli_positive` |
| R-02 — eval run before AC-02 implemented | Critical | Procedural | PR checklist | `cargo test` passes before eval; PR includes commit hash confirming eval is post-AC-02 |
| R-03 — ASS-039 baseline measured on wrong scoring path | Critical | Procedural | PR description | Delivery confirms baseline was on effective(true) path (nli_enabled=true, w_nli=0.0); ADR-001 documents this |
| R-04 — shared helpers accidentally deleted | High | Build | workspace | `cargo build --workspace` succeeds; grep for three retained symbols returns 3 matches in nli_detection.rs |
| R-05 — write_edges_with_cap retained as dead code | High | Clippy | workspace | `cargo clippy --workspace -- -D warnings` exits 0; grep returns 0 matches after deletion |
| R-06 — residual removed-symbol references | High | Build + Grep | workspace | Symbol checklist from ARCHITECTURE.md grep-verified to zero before `cargo test` |
| R-07 — NliStoreConfig partial deletion | High | Build + Grep | workspace | `grep -r "NliStoreConfig" crates/` = 0; `grep -r "nli_store_cfg" crates/` = 0 |
| R-08 — process_auto_quarantine call site not updated | Med | Build | background.rs | `cargo build --workspace` passes; inspect maintenance_tick call site argument count |
| R-09 — formula sum test message not updated | Med | Unit (modified) | search.rs | `test_fusion_weights_default_sum_unchanged_by_crt030` assertion message references crt-038 |
| R-10 — operator config overrides silently lost | Low | Unit + PR | config.rs | Deserialization test with explicit w_util=0.05 override still applies; PR confirms no production overrides |
| R-11 — stale sequencing comment retained | Low | Grep | background.rs | `grep -n "maybe_run_bootstrap_promotion" crates/unimatrix-server/src/background.rs` = 0 |

---

## Cross-Component Test Dependencies

1. `test_effective_short_circuit_*` tests (search.rs) depend on `FusionWeights` struct being unmodified — the struct fields are unchanged, only `effective()` behavior changes.

2. `test_inference_config_weight_defaults_when_absent` (config.rs) and `test_effective_short_circuit_*` (search.rs) must BOTH pass before AC-12 eval is run. One failing makes the eval invalid.

3. Dead-code removal tests (Group B, structural) are independent of each other and of Group A, but all must complete before the final `cargo test --workspace` run.

4. The deleted tests in `nli_detection.rs` and `background.rs` coexist in the same `#[cfg(test)]` module as retained tests. After deletion, verify no `#[cfg(test)]` or `mod tests {` boundary has been accidentally damaged — confirmed by `cargo test --workspace` passing with expected test count reduced by exactly 17 (13 nli_detection + 4 background).

---

## Integration Harness Plan

### Suite Selection

crt-038 touches search ranking behavior and removes server-side code paths. Per the suite selection table:

| Feature touches | Suites to run |
|-----------------|--------------|
| Search/retrieval behavior (formula defaults) | `tools`, `lifecycle`, `edge_cases` |
| Any change at all | `smoke` (minimum gate) |

The dead-code removals do not add new MCP-visible behavior; they remove internal paths. The formula change IS MCP-visible through `context_search` and `context_briefing` ranking.

**Mandatory**: `pytest -m smoke` — minimum gate, must pass before Stage 3c is complete.

**Required suites**: `tools`, `lifecycle`, `edge_cases` — to confirm ranking behavior is intact and no regression in search/briefing flows.

**Not required**: `security`, `contradiction`, `confidence`, `volume` — these cover infrastructure not touched by this feature.

### Existing Suite Coverage Assessment

The infra-001 integration tests exercise the live binary through MCP JSON-RPC. After crt-038:
- `context_search` and `context_briefing` continue to return results; no new response schema
- `context_store` no longer spawns a post-store NLI task — no observable MCP difference
- `process_auto_quarantine` no longer has NLI guard — no MCP-visible difference
- Bootstrap promotion is gone — no MCP-visible difference

**Gap assessment**: No existing suite validates the specific scoring formula weights applied. The formula change (from w_nli=0.35 to w_nli=0.00 default) is validated by:
1. Unit tests in `search.rs` and `config.rs` (Group A)
2. The AC-12 MRR eval gate (behavioral ground truth at scale)

The infra-001 suites confirm no regression in tool function; they do not independently verify formula correctness. This is acceptable — formula correctness is better tested by the eval harness (1,585 scenarios) than by infra-001 JSON-RPC tests.

**New integration tests needed**: None. The formula change is not testable through MCP responses (ranking differences are not deterministically assertable in integration tests). Dead-code removal has no MCP-visible effect. The infra-001 smoke + tools + lifecycle + edge_cases suites are sufficient to confirm no regression.

---

## AC-12 Eval Gate Procedure

The eval gate is a **blocking pre-merge check**. It must be run AFTER AC-01 and AC-02 are implemented and their unit tests passing.

### Preconditions

Before running the eval:
1. `cargo test --workspace` passes (confirms AC-01 and AC-02 unit tests pass).
2. The binary used for eval is built from the commit that includes the effective() short-circuit and the new config defaults — not from main or any intermediate commit.
3. The Unimatrix MCP server is running with new defaults active (no config file overrides for w_sim, w_nli, w_conf, nli_enabled).

### Baseline Validity (R-03)

ADR-001 documents that ASS-039 was run with `nli_enabled=true` and `w_nli=0.0` (effective(true) path — weights returned unchanged, no re-normalization). Delivery must confirm this before accepting MRR=0.2913 as a valid gate.

Evidence source: `product/research/ass-037/harness/profiles/conf-boost-c.toml` shows `w_nli=0.0` with no nli_enabled field — the ASS-037 harness does not invoke the MCP server; it uses an offline snapshot + profile-based direct scoring. This means the ASS-039 re-run did NOT go through `FusionWeights::effective()` at all — it scored directly from profile weights. The MRR=0.2913 baseline was measured on conf-boost-c weights (`w_sim=0.50, w_conf=0.35, w_nli=0.0`) applied directly, not through the effective() codepath.

This is the critical R-03 finding: the baseline is valid for conf-boost-c direct weights. After AC-02 is implemented, `effective(false)` with w_nli=0.0 returns weights unchanged (the short-circuit), matching the direct-weight scoring used in the eval harness. The gate baseline IS valid provided AC-02 is in place.

Delivery must include this explanation in the PR description as the R-03 evidence.

### Eval Execution

The eval harness runs offline against a snapshot DB, not through the live MCP server. The harness in `product/research/ass-037/` uses a snapshot DB + profile TOML. For AC-12, the eval must confirm the production server's active weights match the conf-boost-c profile.

**Option A (recommended): Offline snapshot eval**
```bash
# From product/research/ass-037/harness/
# Uses the existing ASS-037 ablation infrastructure with the ASS-039 scenarios
python3 <eval_runner.py> \
  --scenarios product/research/ass-039/harness/scenarios.jsonl \
  --snapshot product/research/ass-037/harness/snapshot.db \
  --profile product/research/ass-037/harness/profiles/conf-boost-c.toml \
  --output /tmp/crt-038-eval-output.txt
```

The eval runner is not yet located in a single script (the ASS-037 harness scripts are in the harness directory). Stage 3c tester must locate the correct runner script before executing. Search: `Glob("product/research/ass-037/**/*.py")`.

**Fallback if no runner exists**: Reproduce the MRR calculation from `product/research/ass-039/FINDINGS.md` manually against scenarios.jsonl using the snapshot, or contact the harness author.

### PR Description Requirements (all four mandatory)

1. The exact eval command used (command line with all arguments).
2. Full terminal output of the eval run, including the MRR value produced.
3. Git commit hash at time of eval run (confirms run was post-AC-02 commit).
4. Confirmation that the run used conf-boost-c weights (`w_sim=0.50, w_conf=0.35, w_nli=0.00`) — either via profile TOML reference or the production server config.

MRR ≥ 0.2913 to pass. (Note: ASS-039 FINDINGS.md reports conf-boost-c MRR=0.2911 on 1,585 scenarios — see note below.)

**Important discrepancy**: SPECIFICATION.md and RISK-TEST-STRATEGY.md cite the gate value as MRR ≥ 0.2913, but FINDINGS.md reports conf-boost-c MRR=0.2911. This discrepancy (0.2913 vs 0.2911) must be resolved by Stage 3c before declaring AC-12 pass or fail. The gate value in the spec (0.2913) takes precedence; if the eval reproduces 0.2911, delivery must investigate whether a different run produced 0.2913 or whether the gate value should be 0.2911. **Open question for Stage 3c.**

---

## Known Open Questions

1. **MRR gate value discrepancy**: SPECIFICATION.md cites 0.2913 but FINDINGS.md reports 0.2911 for conf-boost-c. Stage 3c must resolve before claiming AC-12 pass.
2. **Eval runner script location**: No standalone eval runner script is visible in `product/research/ass-037/`. Stage 3c tester must locate or reconstruct the MRR computation script before running the gate. The `analyze_hypotheses.py` in ass-039 is for H1/H2/H3, not eval scoring.
3. **nli_detection.rs post-removal line count**: After removing ~500+ lines, the module may contain very few lines. Note the count in the PR per SPECIFICATION.md open question 2.
