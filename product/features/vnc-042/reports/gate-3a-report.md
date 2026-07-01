# Gate 3a Report: vnc-042

> Gate: 3a (Component Design Review)
> Date: 2026-07-01
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | 3 components match ARCHITECTURE Component Breakdown; interfaces match Integration Surface; ADR-001/002/003 all faithfully reflected |
| 2. Specification coverage | PASS | FR-01..FR-14 all have corresponding pseudocode; AC-01..AC-08 traced; no scope additions |
| 3. Risk coverage (test plans) | PASS | R-01..R-12 all mapped; R-02 behavioral default-on + TS-01/02 canaries treated as regression guards |
| 4. Interface consistency | PASS | `ResolutionNote`, `follow_to_current`, `effective_id` threading consistent across all files; no contradictions |
| 5. Knowledge stewardship | PASS | pseudocode + testplan agent reports both carry compliant `## Knowledge Stewardship` blocks with reasons |

Open questions (3): all acceptable as-specified or correctly deferred — none require rework (see below).

## Detailed Findings

### 1. Architecture alignment
**Status**: PASS
**Evidence**:
- Component decomposition matches ARCHITECTURE §Component Breakdown exactly: (1) handler + `GetParams` + tool-desc `tools.rs`, (2) formatter `response/entries.rs`, (3) `follow_to_current` visibility. `pseudocode/OVERVIEW.md` component table is 1:1.
- Interfaces match ARCHITECTURE §Integration Surface: `follow_to_current(&Store, u64) -> Option<u64>`; new `format_single_entry_with_note(&EntryRecord, ResponseFormat, Option<&EdgesView>, &ResolutionNote) -> CallToolResult` mirrors `format_store_success_with_note`; `format_single_entry` unchanged.
- ADR fidelity:
  - **ADR-001** — pseudocode 1a mandates `Option<bool>` + `#[serde(default)]`, explicitly FORBIDS bare `#[serde(default)] bool` (default-OFF footgun). Handler owns the default via branch logic (`None | Some(true) => follow`). Matches ADR ruling and the load-bearing invariant.
  - **ADR-002** — resolution branch uses `follow_to_current` (canonical copy), `None => effective_id = id; DeadEnd{requested:id}`. Returns originally-requested id, no new walk, `handle_current` explicitly excluded (`follow-to-current-reexport.md` "Do NOT" guards). Matches.
  - **ADR-003** — note injected only in `format_single_entry_with_note`; edges rebuilt on `effective_id`; JSON `resolution` object present only on non-clean paths; R-08 pointerless footer built from `Option<u64>`, never unwrapped. Matches.

### 2. Specification coverage
**Status**: PASS
**Evidence**: Every functional requirement maps to pseudocode:
- FR-01 (Option<bool>, handler-owned default) → handler 1a + branch. FR-02/FR-03 (terminal content via `follow_to_current`) → step 3/4. FR-04/FR-05 (hop notice / clean passthrough) → `Followed` / no-note arms. FR-06/FR-07 (as-stored + deprecated footer, orphaned well-formed) → `Some(false)` path + `AsStoredDeprecated{Some|None}`. FR-08 (dead-end loud flag, requested id) → `DeadEnd`. FR-09 (note in wrapper) → step 6 route. FR-10 (orthogonality) → matrix/OVERVIEW §Orthogonality. FR-11 (edges on effective_id) → step 5. FR-12 (json shape) → formatter JSON render. FR-13 (tool-desc) → handler §1b. FR-14 (fail-loud post-primary-read) → Error Handling.
- No unrequested features. NG-1..NG-5 respected (single-tool, no schema/SQL, no other-tool changes); NG-1 neighbor-target non-resolution explicitly out of scope in formatter plan.

### 3. Risk coverage (test plans)
**Status**: PASS
**Evidence**: `test-plan/OVERVIEW.md §2` maps all R-01..R-12 to concrete tests with owning component plans.
- **R-02 (Critical)** — `test_get_handler_field_absent_resolves_to_terminal` is behavioral (field ABSENT ⇒ resolves to terminal), not a serde field-value round-trip. This is the load-bearing invariant the spawn flagged; correctly reflected in both handler test plan and pseudocode 1a.
- **R-01/TS-01/TS-02** — `response-formatter.md` §Regression guards treats byte-identity canary + ~15 shape tests as MUST-stay-green with "edits are FLAG events (#5099)" — correctly framed as regression guards, not silent fixes.
- Dead-end (R-04): orphaned/quarantined/>50-hop/cycle/store-error all enumerated in handler plan; >50-hop exercised through the handler, not only `graph_queries_tests.rs`.
- Integration + edge scenarios present (OVERVIEW §4). Risk priorities reflected in emphasis (Criticals get behavioral + canary coverage).

### 4. Interface consistency
**Status**: PASS
**Evidence**: `ResolutionNote` enum defined identically in OVERVIEW §Shared Types and `response-formatter.md` §2a; handler produces it, formatter renders it (clean read of the boundary contract). `ResolutionStatus` json discriminant table identical across OVERVIEW and formatter. `effective_id` threads to BOTH `entry_store.get` AND `build_edges_view` consistently in handler pseudocode + OVERVIEW data flow + test-plan cross-component dependencies (R-03 single-fetch invariant). No contradictions between files.

### 5. Knowledge stewardship compliance
**Status**: PASS
**Evidence**:
- `vnc-042-agent-1-pseudocode-report.md` — `## Knowledge Stewardship` block present. Read-only tier (pseudocode) with `Queried:` entries (#298 parameterized-formatter, #4474 exact-tool-desc, #5388/5387/5385 ADRs). Deviations: none stated. Compliant for a read-only agent.
- `vnc-042-agent-2-testplan-report.md` — block present with `Queried:` (#5388, #4781, #5383, #3789) and `Stored: nothing novel -- harness kwarg is additive mirror of include_edges; #5383 governs exclusion` (reason supplied). Compliant.

## Open Questions Assessment (pseudocode flagged 3; none require rework)

1. **Quarantined-status footer scope vs Deprecated-only FR-07** — *Acceptable as-specified.* FR-07 explicitly scopes the footer to deprecated entries; AC-08's "orphaned/quarantined deprecated" refers to deprecated-status entries with `superseded_by IS NULL`. The domain-model "Quarantined dead-end" is a resolution-path (follow=true) concept, not an as-stored footer case. Pseudocode's default (Quarantined-*status* requested entry via escape hatch gets no footer) is consistent with the locked spec. Not a design gap.
2. **Audit/usage id = effective vs requested** — *Acceptable deferred detail (WARN-level).* Not bound by any AC; a low-risk behavior choice correctly FLAGGED for reviewer confirmation. Recommend resolving during Gate 3b code review (record `effective_id` for access accounting, or both). Does not block design approval.
3. **`graph_read.rs` re-export idiom** — *Acceptable deferred.* Correctly deferred to the implementer to match the existing sibling idiom (`handle_graph`) rather than pin a synthetic line. Implementation detail, not a design gap.

The OQ-3 JSON-shape sub-toggle (structured `resolution` object vs flat `note`) is pinned by ADR-003 with the spec bound to the ruling; the recommendation stands and no rework is needed unless the human overrides.

## Blast-radius tracking confirmation

All test/CI surfaces called out in the spawn are tracked as work, not delivery-time surprises:
- TS-01/TS-02 regression guards; TS-03 classification table (pre-delivery); TS-04..TS-09 new tests; AC-08 → TS-06 ext; additive harness-client `follow_supersessions` kwarg (`client.py:496` + `uds_client.py:379`) tracked in OVERVIEW §4.3; R-11 JS schema parity budgeted as a post-PR CI round-trip.

## Rework Required

None.
