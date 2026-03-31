# crt-037 Retrospective: Architect Agent Report

> Agent: crt-037-retro-architect
> Mode: retrospective (not design)
> Date: 2026-03-31

---

## Summary

Reviewed all five gate reports and both artifact sets (ARCHITECTURE.md, OVERVIEW.md, RISK-COVERAGE-REPORT.md). Feature shipped cleanly — all three gates passed first attempt, 4257/0 unit tests, 22/22 smoke integration tests. The retrospective surfaced one pattern correction, one new procedure, two new lessons, and three ADR validations.

---

## 1. Patterns

### Updated

**#3950** (corrected from #3944) — "Adding a RelationType variant requires four coordinated updates across two files or the new edge type is missing from graph traversal"

The original entry (#3944, stored by graph agent during delivery) described only the three `graph.rs` sites. crt-037 confirmed a fourth required site: both `personalized_pagerank` and `positive_out_degree_weight` in `graph_ppr.rs` need a new `edges_of_type(node_idx, RelationType::NewVariant, Direction::Outgoing)` call. Without it, the variant exists in the graph but PPR traversal ignores it entirely — no mass flows. Correction filed; #3944 deprecated.

### Skipped (with reason)

- **"Ordering invariant comment at cap-break site"** — Phase 5 `remaining_capacity = max_cap.saturating_sub(supports.len())` with a comment is implementation-specific. Not generalizable beyond this pattern; the cap sequencing itself is captured in ADR-002 (#3956).
- **"format_nli_metadata_informs extension pattern"** — Single-edge-type metadata variant. Not a recurring pattern; no store warranted.
- **Dual-site serde default** (#3817) — Already exists and is exact. The config agent confirmed they followed it without deviation. No update needed.
- **NLI neutral-zone pattern** (#3937) — Already stored during design. Not duplicated.

---

## 2. Procedures

### New

**#3951** — "Two-wave delivery for compile-dependent components: Wave 1 independent interfaces, Wave 2 consuming implementations"

crt-037's five-component delivery across three crates used a clean two-wave structure: Wave 1 (graph.rs enum variant, config.rs fields, read.rs query) compiled independently; Wave 2 (graph_ppr.rs, nli_detection_tick.rs) consumed all three Wave 1 interfaces. The procedure captures the wave boundary, compile gate requirement, and when to use it. Not covered by existing entries (#2579, #2957) which address different wave patterns (infrastructure migration waves, per-crate cargo test scoping).

### Existing procedures assessed as unchanged

- InferenceConfig extension procedure: covered by #3817 (dual-site serde default) and #2730 (..Default::default() in struct literals). crt-037 followed both without deviation. No update.
- Adding a RelationType variant: now updated via #3950 correction above.

---

## 3. ADR Status

| ADR | Entry | Status | Notes |
|-----|-------|--------|-------|
| ADR-001: NliCandidatePair tagged union | #3954 (corrected from #3942) | Validated | Implemented exactly. One schema adaptation: feature_cycle is String not Option<String>; .is_empty() guard is semantically equivalent. Noted in validated entry. |
| ADR-002: Combined cap with Informs second-priority | #3955 (corrected from #3939) | Validated | saturating_sub implementation confirmed at line 402-437. R-12 log assertion gap noted (non-blocking). |
| ADR-003: Directional dedup | #3956 (corrected from #3940) | Validated | Line 1465 comment explicitly confirms non-normalization. Direct test verifies (200,100) not found when (100,200) stored. |

No ADRs flagged for supersession. All three decisions held through implementation without requiring modification.

**Architecture correction note:** The initial architecture (pre-Gate 3a) defined NliCandidatePair as a flat struct. The vision guardian's WARN-1 caught the mismatch with FR-10 (which specified a tagged union). Architecture was corrected before pseudocode was written — zero downstream impact. This is captured as a lesson (#3953), not an ADR supersession, since the corrected ADR (#3942) already encodes the right decision.

---

## 4. Lessons

### New

**#3952** — "Long-wait parallel agent resumes with stale ADR context — tick-file agents must re-read architecture after 60+ min gaps"

Cold-restart hotspot: 92-minute gap + 4 re-reads of ADR-001 + ARCHITECTURE.md before nli_detection_tick agent's first write. Distinct from existing #324 (coordinator session gap checkpointing) and #1271 (context-load normalization). This lesson targets Wave 2 agents specifically: when a complex ADR governs a large file, include ADR decision text directly in the spawn prompt rather than referencing file paths. Avoids re-read cost after long parallel waits.

**#3953** — "Spec FR type model overrides architect's initial type choice for Rust safety properties — validate before Gate 3a"

FR-10 specified a tagged union for compile-time routing safety. The architect's initial output was a flat struct. The spec was right; the architecture was wrong. Existing lessons #723, #3899, #3620 address spec/architecture consistency broadly but none specifically addresses the case where the spec's type model encodes a Rust safety property that the architect weakened. New angle: before completing architecture for any multi-path routing component, check every FR prescribing a type model — "tagged union," "compile-time enforcement" are safety requirements, not style preferences.

### Skipped (with reason)

- **compile_cycles (F-01)**: 115 cycles on nli_detection_tick.rs confirms the existing pattern. #3439, #3815, #3887 all cover "batch type changes before compiling." New data point, same lesson. Not stored.
- **context_store failures (16×)**: Attributed to tooling fragility across the session; not investigated to root cause. Insufficient basis for a lesson entry.

---

## 5. Retrospective Findings (Hotspot-Derived)

| Hotspot | Action Taken |
|---------|--------------|
| compile_cycles (115) | No new lesson — covered by #3439/#3815/#3887. Recommendation: batch field additions before first build (already documented). |
| context_load (110 KB pre-first-write) | No new lesson — pre-read of tick + all dependencies before implementation is expected for large files. #1271 covers normalization. |
| cold_restart (92-min gap + 4 re-reads) | New lesson #3952 stored. Fix: embed ADR text in Wave 2 spawn prompts rather than referencing paths. |
| tool_failure_hotspot (context_store×16, Bash×5) | No action — insufficient root cause data. Not a new lesson. |
| sleep_workarounds (12) | No action from architect perspective — tester agent issue. Recommendation already in retro brief (run_in_background + TaskOutput). |
| search_via_bash (20.7%) | No action — search tool discipline is an existing convention, not architecture. |

---

## Knowledge Stewardship

- Queried: context_search across 4 pattern domains, 2 lesson domains; context_get for #3944, #3942, #3939, #3940, #3817, #324 before deciding on any store or correction.
- Stored: #3951 (procedure, two-wave delivery), #3952 (lesson, cold-restart tick agent), #3953 (lesson, spec FR type model authority)
- Corrected: #3944 → #3950 (pattern, RelationType four-site checklist), #3942 → #3954 (ADR-001 validated), #3939 → #3955 (ADR-002 validated), #3940 → #3956 (ADR-003 validated)
