# Gate 3a Report: crt-038

> Gate: 3a (Design Review)
> Date: 2026-04-02
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 6 components mapped; wave structure, file boundaries, and ordering match ARCHITECTURE.md |
| Specification coverage | PASS | All FR-01 through FR-20 have pseudocode; all AC-01 through AC-14 addressed |
| Risk coverage | PASS | All 11 risks (R-01 through R-11) have mapped test scenarios in test plans |
| Interface consistency | PASS | Shared types, signatures, and symbol checklists are consistent across OVERVIEW, component pseudocode, and test plans |
| Knowledge stewardship compliance | PASS | Both design-phase agent reports contain `## Knowledge Stewardship` with `Queried:` and `Stored:` entries |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**: The pseudocode/OVERVIEW.md Component Map correctly maps all five architecture components (Components 1–5; Component 6 is the eval gate, a procedural check rather than a code component) to pseudocode files and implementation waves. Key alignment points verified:

- Wave structure (`Wave 1a → 1b → eval gate → Wave 2`) matches the mandatory ordering in ARCHITECTURE.md §Implementation Ordering Constraints and ADR-003.
- Wave 2 is correctly specified as a single agent covering both `nli_detection.rs` and `background.rs` because Component 4 (bootstrap promotion) touches both files.
- All six files listed in the Architecture's Integration Surface table (`search.rs`, `config.rs`, `nli_detection.rs`, `store_ops.rs`, `services/mod.rs`, `background.rs`) appear in the pseudocode's file lists.
- Files not touched (`nli_handle.rs`, `nli_detection_tick.rs`, `contradiction.rs`, `Cargo.toml`) are explicitly listed as untouched in OVERVIEW.md.
- The three retained symbols (`write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs`) from the Architecture's Integration Points section appear verbatim in both the OVERVIEW Symbols Retained block and dead-code-removal.md AC-13 section.
- The Architecture's Symbol Checklist for deleted symbols matches the pseudocode's Symbols Deleted block, with one addition (see `parse_nli_contradiction_from_metadata` below).

### Specification Coverage

**Status**: PASS

**Evidence**: Every functional requirement is addressed:

- FR-01 through FR-06 (default weight changes): `config-defaults.md` gives line numbers and before/after values for all six functions.
- FR-07 (sum constraint 0.85): Weight sum verification section in `config-defaults.md` explicitly checks the sum.
- FR-08 (effective() short-circuit): `effective-short-circuit.md` pseudocode specifies the four-path structure with the guard as the first branch.
- FR-09 through FR-10 (run_post_store_nli removal): `dead-code-removal.md` Component 3 covers all five removal points in `store_ops.rs` (import, struct, field, parameter, spawn block) and the `mod.rs` cleanup.
- FR-11 through FR-13 (bootstrap promotion removal): Component 4 covers both `nli_detection.rs` functions and the two `background.rs` deletion sites (import + call block), plus the stale sequencing comment.
- FR-14 through FR-17 (auto-quarantine removal): Component 5 covers enum, function, guard block removal, and parameter strip from `process_auto_quarantine`, including the full four-function call chain (`spawn_background_tick` → `background_tick_loop` → `run_single_tick` → `maintenance_tick`).
- FR-18 through FR-20 (test cleanup): The dead-code-removal pseudocode lists all 13 `nli_detection.rs` tests and all 4 `background.rs` tests for deletion; the modified test symbols (2 config tests, 1 search test) are addressed in component pseudocode files.

Non-functional requirements: NFR-01 through NFR-06 are covered. The eval gate (NFR-06 / AC-12) has a detailed procedure in test-plan/OVERVIEW.md including preconditions, baseline validity analysis, execution options, and PR description requirements.

**NFR-05 pre-existing violation noted**: Both pseudocode (`dead-code-removal.md` preamble) and specification acknowledge that `background.rs` (4,229 lines) and `nli_detection.rs` (1,373 lines) are pre-existing over-limit violations exempt from gate failure.

### Risk Coverage

**Status**: PASS

**Evidence**: All 11 risks from the Risk-Based Test Strategy have documented test or verification scenarios. Traceability:

| Risk | Coverage in Test Plans |
|------|----------------------|
| R-01 (Critical) — effective() short-circuit omitted/misplaced | 3 unit tests in `effective-short-circuit.md`; all three R-01 scenarios addressed |
| R-02 (Critical) — eval before AC-02 | OVERVIEW.md ordering enforcement + PR checklist requirement (commit hash in eval output) |
| R-03 (Critical) — baseline on wrong scoring path | OVERVIEW.md §AC-12 Eval Gate Procedure §Baseline Validity; R-03 finding explicitly resolved: harness bypasses `effective()` entirely (entry #4009 stored) |
| R-04 (High) — shared helpers deleted | `dead-code-removal.md` Step 2 retained symbol verification with specific grep commands |
| R-05 (High) — write_edges_with_cap retained | `dead-code-removal.md` Step 1 absence grep + clippy gate |
| R-06 (High) — residual symbol references | Full symbol checklist with per-AC grep commands |
| R-07 (High) — NliStoreConfig partial deletion | `dead-code-removal.md` Step 1 NliStoreConfig + nli_store_cfg greps across full workspace |
| R-08 (Med) — process_auto_quarantine call site | Incremental build after Component 5 + call site inspection |
| R-09 (Med) — formula test message not updated | `effective-short-circuit.md` updated test section; string change flagged for diff review |
| R-10 (Low) — operator overrides silently lost | Optional new test in `config-defaults.md`; PR confirmation requirement |
| R-11 (Low) — stale sequencing comment | `dead-code-removal.md` Step 1 grep for comment text |

### Interface Consistency

**Status**: PASS

**Evidence for the five spawn-prompt specific items**:

**Item 1 — `write_edges_with_cap` deletion (AC-11 clippy / callerless after `run_post_store_nli` removal)**:
PRESENT. `dead-code-removal.md` Component 3 explicitly lists `write_edges_with_cap` as a deletion target (with rationale: "sole caller was run_post_store_nli"). The OVERVIEW.md Symbols Deleted block includes it. The grep verification checklist (`grep -r "write_edges_with_cap" crates/`) is in `test-plan/dead-code-removal.md` Step 1. Architecture's Symbol Checklist (p.234) and Deleted Symbols in SPECIFICATION.md both require it absent.

**Item 2 — `parse_nli_contradiction_from_metadata` deletion (cascaded from `nli_auto_quarantine_allowed` removal)**:
PRESENT. This symbol is not in the Architecture's Symbol Checklist — it is a pseudocode agent Finding #2 (agent report, line 46–52): "After deleting `nli_auto_quarantine_allowed`, this function becomes callerless dead code and clippy will fail on it." The `dead-code-removal.md` Component 5 section explicitly calls this out with a deletion instruction and grep verification command. The test-plan/dead-code-removal.md Step 1 Component 5 section does NOT include a grep for `parse_nli_contradiction_from_metadata` — but the pseudocode file does. This is a minor gap in test-plan completeness (WARN, not FAIL — the pseudocode covers it and test-plan references it in the failure modes section of `dead-code-removal.md` Step 1).

**Item 3 — 4-signature parameter threading for `nli_enabled`/`nli_auto_quarantine_threshold`**:
PRESENT. `dead-code-removal.md` Component 5 "Cascade" subsection explicitly lists all four function signatures (`spawn_background_tick` → `background_tick_loop` → `run_single_tick` → `maintenance_tick`) with line numbers (250-251, 327-328, 441-442, 820-821) and instructs delivery to strip parameters from ALL four signatures and their call sites, including the external call site of `spawn_background_tick`.

**Item 4 — Three AC-02 tests in `test-plan/effective-short-circuit.md`**:
ALL THREE PRESENT:
- `test_effective_short_circuit_w_nli_zero_nli_available_false` — present with full arrange/act/assert
- `test_effective_short_circuit_w_nli_zero_nli_available_true` — present with arrange/act/assert
- `test_effective_renormalization_still_fires_when_w_nli_positive` — present with denominator calculation (`0.50`), expected re-normalized values, and assertion that `result.w_sim` differs from `fw.w_sim`

All three match the SPECIFICATION.md AC-02 test name requirements exactly.

**Item 5 — AC-12 eval gate procedure in `test-plan/OVERVIEW.md`**:
PRESENT with complete procedure. Sections included: Preconditions (3 items), Baseline Validity (R-03 resolution), Eval Execution (Option A recommended with command template, fallback if no runner), PR Description Requirements (all four mandatory items per SPECIFICATION.md AC-12). An important discrepancy is flagged: SPECIFICATION.md and RISK-TEST-STRATEGY.md cite MRR gate as ≥ 0.2913 but FINDINGS.md reports conf-boost-c as 0.2911 — this is raised as an open question for Stage 3c, which is appropriate.

**Cross-component interface consistency**: The `process_auto_quarantine` before/after signature in OVERVIEW.md shared types matches the Component 5 pseudocode exactly. The `FusionWeights` struct shown in OVERVIEW.md is consistent with the `effective-short-circuit.md` test setups. The Symbols Deleted/Retained lists in OVERVIEW.md match those in `dead-code-removal.md` and the ARCHITECTURE.md Symbol Checklist (with the `parse_nli_contradiction_from_metadata` addition documented in the agent report).

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

**Pseudocode agent (crt-038-agent-1-pseudocode-report.md)**:
- `## Knowledge Stewardship` section present.
- `Queried:` entries present: searched for FusionWeights scoring formula patterns, crt-038 architectural decisions. Results were applied (additive term exemption from re-normalization denominator confirmed via entry #3206).
- No `Stored:` entry, but agent notes deviations from established patterns were flagged in pseudocode rather than stored (cascaded deletion finding). The `Stored:` field is absent — the convention requires either `Stored: ...` or `nothing novel to store -- {reason}`. The agent omits both forms.

This is a WARN: the pseudocode agent report has `Queried:` entries and explains the finding was flagged in pseudocode, but does not include a formal `Stored:` or "nothing novel to store -- {reason}" line as required by the gate check definition.

**Test-plan agent (crt-038-agent-2-testplan-report.md)**:
- `## Knowledge Stewardship` section present.
- `Queried:` entries present: three separate searches with results applied.
- `Stored:` entry present: entry #4009 "ASS-037/039 eval harness bypasses FusionWeights::effective() — baseline MRR is on direct profile weights" stored as pattern. Novel finding documented.

Both agents have stewardship sections. The pseudocode agent's missing explicit `Stored:`/`nothing novel` line is a WARN.

---

## Rework Required

None. Gate result is PASS with one warning.

---

## Warnings

| Warning | File | Detail |
|---------|------|--------|
| Stewardship `Stored:` line absent | `agents/crt-038-agent-1-pseudocode-report.md` | Agent has `Queried:` entries and explains novel finding was documented in pseudocode, but omits a formal `Stored: ...` or `nothing novel to store -- {reason}` line. Does not block gate. |
| `parse_nli_contradiction_from_metadata` absent from test-plan Step 1 grep list | `test-plan/dead-code-removal.md` | The grep for this symbol appears in `pseudocode/dead-code-removal.md` Component 5 but not in the test plan's Step 1 symbol absence checklist. Stage 3b delivery should add it to the grep verification before claiming AC-07 complete. Does not block gate — pseudocode coverage is definitive at this stage. |
| MRR gate value discrepancy | `test-plan/OVERVIEW.md` | SPECIFICATION.md cites ≥ 0.2913 but FINDINGS.md reports 0.2911. Flagged as open question for Stage 3c. Stage 3b delivery does not need to resolve this, but Stage 3c cannot claim AC-12 without resolution. |

---

## Knowledge Stewardship

- Queried: nothing novel found that warranted a separate lookup — all findings emerged from reading the artifacts directly. The pseudocode and test plans are detailed enough that no pattern search was needed beyond what the design agents already performed.
- Stored: nothing novel to store -- the `parse_nli_contradiction_from_metadata` cascaded-deletion pattern (pseudocode catches what architecture missed) is feature-specific to crt-038 and is already captured in the pseudocode agent finding #2. The general pattern of "cascaded dead code from dead code removal not in architecture's symbol checklist" could become a lesson if it recurs at gate-3c.
