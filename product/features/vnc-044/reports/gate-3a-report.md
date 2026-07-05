# Gate 3a Report: vnc-044

> Gate: 3a (Component Design Review)
> Date: 2026-07-05
> Result: PASS (re-validation rev2 — was REWORKABLE FAIL rev1)

## Re-Validation (rev2, 2026-07-05)

Prior result was REWORKABLE FAIL on Check 4 only (`parse_detail` case-policy contradiction).
The tester corrected `test-plan/verbosity.md`. Re-checked the fixed item + regression on the
other four:

| Re-check | Status | Evidence |
|----------|--------|----------|
| Check 4 contradiction resolved | **PASS** | test-plan/verbosity.md lines 73–75 now pin case-INSENSITIVE accept: `Some("Summary")`/`Some("SUMMARY")`→`Ok(Summary)`, `Some("Full")`/`Some("FULL")`→`Ok(Full)`, explicitly mirroring `response/mod.rs::parse_format`'s `f.to_lowercase().as_str()`. The "case-sensitive reject is the expected default" clause is gone. Now agrees with pseudocode/verbosity.md lines 44–58 (`d.to_lowercase().as_str()`). |
| Reject cases intact (no regression) | **PASS** | Line 75 restricts `Err`→`ERROR_INVALID_PARAMS` to genuinely-unknown values (`"brief"`/`"bogus"`/`""`); empty string still rejects. New tests `test_parse_detail_case_insensitive` + `test_parse_detail_unknown_rejected` added (line 77) alongside the retained `test_parse_detail_none_defaults_summary`. |
| R-01/R-02 coverage intact (no regression) | **PASS** | R-01 UTF-8 flooring table (lines 15–29, cases 1–10 incl. 2/3/4-byte straddle) and R-02 truncated byte-compare (lines 44–53 incl. the non-negotiable 257B-floors-to-256 false-negative trap) are untouched — edit was scoped to the `parse_detail` subsection only. |
| Checks 1, 2, 3, 5 unaffected | **PASS** | Edit touched only `test-plan/verbosity.md`'s `parse_detail` table; no architecture/spec/risk/stewardship surface changed. graph_read.md resolver table (lowercase inputs only) unaffected. |

**Re-validation result: PASS.** The single blocking contradiction is fully resolved with no
regression. Gate 3a clears.

---

## Original rev1 findings (retained for provenance)

> Gate: 3a (Component Design Review)
> Date: 2026-07-05
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | 4 components match ARCHITECTURE decomposition; signatures match Integration Surface; resolver-not-parse_format, distinct projection type, shared verbosity module all per ADR-001/ADR-002. |
| 2. Specification coverage | PASS | FR-1..FR-12 and NFR-1..NFR-6 each have corresponding pseudocode; no scope additions. |
| 3. Risk coverage | PASS | R-01..R-14 all mapped to scenarios in test-plan OVERVIEW + component plans. |
| 4. Interface consistency | **FAIL** | `parse_detail` case policy: pseudocode is case-INSENSITIVE; test-plan/verbosity.md pins case-SENSITIVE reject as "expected default." Direct pseudocode↔test-plan contradiction. |
| 5. Knowledge stewardship | PASS | pseudocode + testplan (Queried), architect (#5509/#5510) + risk (#5511) Stored; all blocks present with reasons. |

**Adjudication of the 5 surfaced items:** Item 1 = REWORKABLE FAIL (below). Items 2–5 = resolved/confirmed PASS.

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**: Component set (`response/verbosity.rs`, `graph_read_projection.rs`, `graph_read.rs`, `tools.rs`) matches ARCHITECTURE Component Breakdown 1:1. Every new signature in the pseudocode (`parse_detail`, `content_preview`, `node_summary`, `edge_summary`, `resolve_graph_output`, `GraphSummaryProjection::to_summary_json`, additive `GraphParams.detail`) matches the ARCHITECTURE Integration Surface table and the brief's Function Signatures. ADR contracts honored: graph uses its own `resolve_graph_output`, not shared `parse_format` (ADR-002 §2); `NodeSummary` is a distinct type, no `skip_serializing_if` on `EntryRecord`/`EdgeRecord` (ADR-002 §3, C-2/C-3); `CONTENT_PREVIEW_BYTES=256` single-sourced in the new shared module (ADR-001 SR-03, C-9); `content_preview` uses the mandated char-boundary idiom, not `&s[..256]`/`floor_char_boundary`/`.chars().take()` (ADR-001 §6).

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: FR-1 two axes (E-1 + resolver); FR-2 end-to-end threading (E-5 seam, fixes `:251`); FR-3 default summary (`parse_detail(None)=Summary`); FR-4 node field set (`NodeSummary`); FR-5 edge field set (`edge_summary` json! of 4 keys); FR-6/FR-7 preview + truncated byte-compare; FR-8 markdown reject + neighbors/path accept-and-ignore; FR-9 legacy alias + conflict; FR-10 full byte-identical (`Detail::Full` arm serializes raw result); FR-11 all five node-bearing modes get trait impls; FR-12 tool description (tools.md). NFR-1 golden byte-equality; NFR-2 shared types UNTOUCHED; NFR-3 additive field; NFR-4 `fetch_nodes_batch` untouched (payload-only win); NFR-5 new module; NFR-6 traversal unchanged. No unrequested features introduced.

### Check 3 — Risk coverage
**Status**: PASS
**Evidence**: test-plan OVERVIEW Risk→Test Mapping covers R-01..R-14. R-01 boundary table (10 cases incl. 2/3/4-byte straddle + no-panic property); R-02 truncated byte-compare with the 257B-floors-to-256 non-negotiable trap; R-03 per-envelope metadata preservation (unit) + default/explicit-summary per mode ×5 (integration, "no mode covered by subgraph alone"); R-04 golden byte-equality subgraph + ≥1 other; R-05 markdown reject ×7 modes pre-dispatch; R-06 shared-type compile+grep+regression gate; R-07 present-AND-absent key sets for node and edge; R-08/R-09 alias/conflict + accept-and-ignore; R-11 documentation/expectation gate correctly framed (not tested-as-defect). Integration/edge/security risk sections all mapped.

### Check 4 — Interface consistency
**Status**: FAIL (REWORKABLE)
**Evidence**: `pseudocode/verbosity.md` (lines 44–58, and test-hint line 126 `"SUMMARY"→Summary`) implements `parse_detail` **case-insensitively** via `d.to_lowercase().as_str()`, explicitly "mirrors the established `parse_format` idiom." `test-plan/verbosity.md` line 73 states: *"`Some("Summary")` / `Some("FULL")` | pin the case policy — assert whichever the impl chooses (**case-sensitive reject is the expected default**)."* These contradict: the test plan names case-sensitive REJECT as expected, while the pseudocode ACCEPTS `"Summary"`/`"FULL"`.
**Adjudication**: The pseudocode is correct. Verified against the codebase: `crates/unimatrix-server/src/mcp/response/mod.rs::parse_format` matches with `f.to_lowercase().as_str()` — the established idiom **is** case-insensitive. ADR-002 §2 specifies `parse_detail` resolves via the shared parser semantics (`summary`→Summary, `full`→Full, else→ERROR) and the resolver's own legacy-alias check also lowercases `format`. Case-insensitive accept is what ADR-001/ADR-002 + the codebase support; the test plan's "case-sensitive reject" note is wrong and would drive the tester to write a test asserting `"Full"` is rejected — a guaranteed red bar, or worse an impl bent away from the codebase idiom to satisfy a wrong test.
**Issue**: `test-plan/verbosity.md` line 73 must be corrected to pin **case-INSENSITIVE accept**: `Some("Summary")` → `Ok(Detail::Summary)`, `Some("FULL")` → `Ok(Detail::Full)`; drop the "case-sensitive reject is the expected default" clause. This is the paired-test-plan sweep the pseudocode's case decision requires. Pseudocode needs no change.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: pseudocode report — `## Knowledge Stewardship` with Queried (context_briefing, #5457/#5449/#4518/#4490/#4491) + "Storage: none expected (read-only tier)" (valid read-only reason). testplan report — Queried (context_briefing + testing search, #5449/#4469/#2928) + "Stored: nothing novel at plan-design stage — patterns reused, not discovered" (reason present). architect report — Queried + Stored #5509/#5510. risk report — Queried + Stored #5511 (cross-feature pattern). All blocks present; no "nothing novel" without a reason.

## Adjudication of Surfaced Items

**Item 1 — `parse_detail` case-sensitivity CONFLICT** → **REWORKABLE FAIL.** ADR-001/ADR-002 + the codebase `parse_format` idiom support **case-INSENSITIVE**. Pseudocode is right; test-plan/verbosity.md line 73 is wrong and must be swept (see Check 4).

**Item 2 — resolve_graph_output vs validate_no_unsupported_params ordering** → **CONFIRMED / PASS.** Pseudocode graph_read.md E-5 places `resolve_graph_output` (Step 0) before `validate_no_unsupported_params` (Step 1), matching ADR-002 §2 ("runs at the top of `handle_graph`, before mode dispatch, so markdown/invalid values are rejected uniformly for all seven modes") and the ARCHITECTURE data-flow diagram. `detail` is universal, so validation logic is unchanged (no per-mode arm). The only behavioral effect is error-precedence for a call that is *both* `format=markdown` *and* otherwise param-invalid — now returns the markdown error first; no source document requires the old precedence, and R-05 mandates uniform pre-dispatch markdown rejection. No existing per-mode validation behavior is broken.

**Item 3 — graph_read.rs line budget** → **CONFIRMED / non-blocker.** Pseudocode estimates 389→~460 (< 500). Escape hatch documented in graph_read.md (E-4 lines 116–117), OVERVIEW, and ADR-002 §Consequences: relocate `resolve_graph_output` + `GraphSerialization` to `graph_read_validation.rs` if it crosses 500. Watch at Gate 3b.

**Item 4 — summary envelope key ORDER non-contractual** → **CONFIRMED / PASS.** graph_read_projection.md (lines 150–154) states summary top-level key order is not contractual (built via `serde_json::json!`); AC-03 asserts the key SET (present AND absent), AC-04 pins byte-for-byte only for `detail=full`, which never routes through the projection module. Test plans assert key sets via `BTreeSet` (order-independent) — no test over-asserts summary key order. `NodeSummary`'s internal field order is fixed by the struct but not byte-asserted in any summary test.

**Item 5 — R-01/R-02/R-03 coverage** → **CONFIRMED / PASS.** R-01 UTF-8 flooring: verbosity.md table cases 1–10 (empty/<256/255/256/257-ASCII/2-3-4-byte straddle/boundary-exact/all-multibyte) + no-panic property test. R-02 `content_truncated == content.len() > 256`: table cases 1–5 incl. the non-negotiable 257B-floors-to-256 false-negative (`test_content_truncated_257_ascii_true`) + direct invariant over {0,1,255,256,257,300,1000}. R-03: all five node-bearing modes covered (unit per-impl metadata table + integration default/explicit-summary per mode), explicitly "no mode covered by subgraph alone." Fully covered.

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `parse_detail` case policy contradicts the pseudocode (case-insensitive) and the codebase `parse_format` idiom | uni-tester (owns test-plan/verbosity.md) | Correct `test-plan/verbosity.md` line 73: pin **case-INSENSITIVE accept** — `Some("Summary")`→`Ok(Detail::Summary)`, `Some("FULL")`→`Ok(Detail::Full)`; remove the "case-sensitive reject is the expected default" clause. No pseudocode change; no other test-plan file affected (graph_read.md resolver table uses lowercase inputs only). |
