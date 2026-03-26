# Retrospective Architect Report: nan-008

Agent: nan-008-retro-architect
Role: uni-architect
Date: 2026-03-26
Feature: nan-008 (Distribution-Aware Metrics: CC@k and ICD)

---

## 1. Duplicate / Cleanup Resolution

| Entry | Action | Reason |
|-------|--------|--------|
| #3528 — Shannon entropy ICD pattern: skip zero-count categories | Deprecated | Duplicate of #3525. Entry #3525 covers the same pattern at a higher abstraction level, distinguishes it from the denominator-zero risk, and is scoped to any future entropy metric. #3528 is implementation-scoped to a specific HashMap in metrics.rs. |
| #3537 — Retrospective findings: nan-008 | Deprecated | Auto-generated meta-entry containing only feature-specific statistics (hotspot_count, feature_cycle). No generalizable knowledge. |
| #3525 — Shannon entropy implementations must skip zero-count categories | Retained | Stronger entry: explains what/why/scope, distinguishes ln(0) risk from denominator-zero risk, explicitly marks applicability to future metrics. |

---

## 2. Patterns

### Updated

**#3512 -> #3550** — Eval harness dual-type constraint (corrected)

Reason: nan-008 confirmed a third synchronization site beyond the two type-copy files. When a new per-profile metric requires per-scenario distribution data, `render_report` also gains a new parameter (e.g., `cc_at_k_rows: &[CcAtKScenarioRow]`). The original entry described two sites; the corrected entry documents all three and explicitly names `AggregateStats` and `render_report` as mandatory update targets.

### Existing — No Update Needed

**#3526** — Eval Harness Feature Test Strategy: Round-Trip Over infra-001 for JSON Schema Boundary Risk

Accurate and complete. nan-008 followed this pattern exactly: round-trip test in `report/tests.rs` as primary integration vehicle; smoke-only infra-001 as compile/binary health gate. No drift from pattern.

**#3525** — Shannon entropy zero-count NaN guard

Accurate and complete after deprecating duplicate #3528.

### Skipped

- runner/metrics.rs, runner/output.rs, runner/replay.rs, report/mod.rs, report/aggregate.rs, report/render.rs: no new patterns beyond those already captured in #3550, #3526, #3525, and the ADRs.

---

## 3. Procedures

### New

**#3549** — Eval Harness Baseline Recording Procedure (New Metrics)

The 6-step procedure (snapshot check → create if absent → run eval → extract metrics → append log.jsonl → update README.md) was not previously stored as a procedure entry. ADR-005 (#3524) captures the architectural decision; #3549 captures the concrete executable steps for future delivery agents. Key note included: as of nan-008, the snapshot command is `unimatrix snapshot` (top-level), not `eval snapshot` as the ADR originally described — agents should verify with `--help`.

### Existing — No Update Needed

No prior baseline recording procedure existed in Unimatrix. The ADR (#3524) was the only prior record.

---

## 4. ADR Validation

All five ADRs (#3520–#3524) were borne out by implementation. Evidence from Gate 3b and Gate 3c reports:

| ADR | Entry | Status | Evidence |
|-----|-------|--------|----------|
| ADR-001: `category: String` on ScoredEntry in both copies | #3520 | VALIDATED | `runner/output.rs` line 21 (no `#[serde(default)]`); `report/mod.rs` lines 53–54 (with `#[serde(default)]`). Wave 1 agent confirmed `se.entry.category` populated from the search result. Gate 3b PASS. |
| ADR-002: ICD raw entropy with `ICD (max=ln(n))` label | #3521 | VALIDATED | `render.rs` lines 54, 219, 261 all emit `ln(n)` annotation. Section 6 interpretation note present. Gate 3b confirmed "ICD (max=ln(n))" in header. |
| ADR-003: Round-trip integration test mandatory | #3522 | VALIDATED | `test_report_round_trip_cc_at_k_icd_fields_and_section_6` present at `report/tests.rs:835`, passing. Non-trivial values 0.857, 1.234, 0.143 asserted in rendered output. Gate 3b PASS. |
| ADR-004: `tracing::warn!` on empty configured_categories | #3523 | VALIDATED | `metrics.rs` lines 236–240. Gate 3b confirmed no hardcoded category strings anywhere. Gate 3c PASS. |
| ADR-005: Baseline recording as named delivery step | #3524 | VALIDATED WITH NOTE | `log.jsonl` line 7 present: `cc_at_k: 0.2636`, `icd: 0.5244`, `feature_cycle: "nan-008"`. Note: the snapshot command used was `unimatrix snapshot` (top-level), not `eval snapshot` as ADR-005 describes. The ADR text should note the command may be top-level; this is captured in the new procedure entry #3549 rather than superseding the ADR itself, since the decision rationale is unchanged. |

No ADRs require supersession. The command-location note for ADR-005 is a clarification, not a decision change.

---

## 5. Lessons

### New

**#3547** — Wave-Parallel Swarm: Type-Definition Waves Must Complete Before Downstream Waves Are Spawned

Source: F-03 hotspot (64 compile cycles). Root cause: Wave 1 owned type definitions in `runner/output.rs` and `report/mod.rs`; Waves 2-3 were spawned concurrently before Wave 1 merged. Downstream agents added 0.0/empty-string stubs to compile, then reconciled after Wave 1 merged — producing cascading type error loops. Distinct from #3439 (single-agent struct extension iteration); this is a swarm-coordination problem. Prescription: type-definition waves must be sequential gating steps, not concurrent with consumer waves.

**#3548** — Test Exists but Omits Test-Plan Assertion — Coverage Is Weaker Than Specified

Source: Gate 3c WARNs on AC-14 and R-07. Implementation was correct; the gap was that the delivery agent wrote the test but omitted the specific assertion the test plan required (`assert!(content.contains("ln("))`). Distinct from #1204 (pseudocode-vs-test-plan behavioral contradiction). The lesson: Gate 3b check for test name presence is necessary but not sufficient — reviewers must also verify the test body contains the key assertions the test plan specified.

### Existing — Updated

**#3389 -> #3551** — Agents Default to Bash grep Instead of Grep Tool

Added nan-008 data point (7.8% bash-for-search rate). This is below prior outliers (col-024: 18.4%, col-023: 22.4%) and within normal range — not an outlier for this feature. Data point retained to show trend: behavior persists at lower rates in smaller-scope features but has not been eliminated.

### Skipped

- GH #407 byte-slice truncation: #3103 covers `str::len()` bytes-vs-chars for validation. The security finding is `str[..N]` byte-index slicing for truncation — same root cause, different code site. Security reviewer explicitly assessed this as non-recurring cross-feature and did not store it. The security reviewer's judgment is deferred to; no lesson added.
- Gate 3c WARN on R-07 (dedicated backward-compat test absent): the structural `#[serde(default)]` coverage was judged equivalent by the gate. The test-plan-assertion-gap lesson (#3548) is the more generalizable extraction from this observation.

---

## 6. Summary Table

| Type | Entry | Action |
|------|-------|--------|
| Duplicate cleanup | #3528 | Deprecated |
| Meta-entry cleanup | #3537 | Deprecated |
| Pattern update | #3512 → #3550 | Corrected (third sync site added) |
| Lesson update | #3389 → #3551 | Corrected (nan-008 data point added) |
| Lesson new | #3547 | Stored — wave-parallel type-definition ordering |
| Lesson new | #3548 | Stored — test-plan assertion gap |
| Procedure new | #3549 | Stored — eval baseline recording 6-step procedure |
| ADR-001 | #3520 | Validated |
| ADR-002 | #3521 | Validated |
| ADR-003 | #3522 | Validated |
| ADR-004 | #3523 | Validated |
| ADR-005 | #3524 | Validated (command-location note in #3549) |

---

## Knowledge Stewardship

- Queried: `context_get` on #3525, #3528, #3537 (duplicate assessment); `context_search` for wave-parallel compile patterns, dual-type-copy, baseline recording procedure, search-via-bash, test-plan assertion gaps, byte-slice truncation; `context_get` on #3439, #3512, #3524, #3389, #1204, #3103, #3526 (content review before deciding to update or skip)
- Deprecated: #3528 (duplicate), #3537 (meta-entry)
- Corrected: #3512 → #3550 (three-site sync); #3389 → #3551 (nan-008 data point)
- Stored new: #3547 (wave-parallel type ordering lesson), #3548 (test-plan assertion gap lesson), #3549 (baseline recording procedure)
