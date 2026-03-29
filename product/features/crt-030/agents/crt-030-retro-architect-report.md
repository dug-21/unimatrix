# crt-030 Retrospective — Architect Report

> Agent: crt-030-retro-architect
> Date: 2026-03-29

---

## 1. Patterns

### New entries

| ID | Title | Disposition |
|----|-------|-------------|
| #3752 | Node-ID-sorted accumulation for deterministic power iteration | NEW — generalizes to any iterative graph scoring algorithm requiring deterministic f64 output; not specific to PPR |
| #3753 | Use pre-cloned lock snapshot for new pipeline steps — never re-acquire the lock | NEW — generalizes the col-031 snapshot pattern to any new Step inserted into search.rs that needs signal data from a previously-locked handle |
| #3756 | Wave structure for multi-component features: parallel independent components first, sequential dependents second | NEW — extracted from 0 coordinator-respawns / 0 post_completion_work outcome |

### Existing entries — already covered

| ID | Title | Reason skipped |
|----|-------|----------------|
| #3740 | Graph traversal submodule pattern: #[path] submodule of graph.rs | Already fully covers graph_ppr.rs as pure-function submodule. No update needed. |
| #3730 | Search pipeline step numbering, graph traversal module pattern, use_fallback guard | Already covers the pipeline step insertion pattern. No update needed. |
| #3744 | PPR power iteration uses Direction::Outgoing (reverse walk) | Already stored by implementation agent during deliver phase. No update needed. |
| #3694 | Phase snapshot extraction pattern: acquire read lock once pre-loop | Already covers the snapshot extraction pattern; #3753 is the complementary "use it in new steps" extension. |
| #3746 | Moving a pre-loop extraction block when inserting a new pipeline step | Already stored by implementation agent. No update needed. |
| #3743 | InferenceConfig TOML tests must use flat top-level fields — no [section] header | Already stored. Incorporated into procedure #3759. |

---

## 2. Procedures

### New entries

| ID | Title | Disposition |
|----|-------|-------------|
| #3759 | How to add new fields to InferenceConfig | NEW — no prior procedure existed for InferenceConfig field addition. Covers 5 locations in config.rs, TOML round-trip Deserialize-only constraint (#3743), validation error reuse pattern, and test checklist. |

### Procedure review finding
No existing `procedure` entry for InferenceConfig field addition was found. The pattern was distributed across ADRs (#3743, #3730) and agent reports. Consolidated into a single reusable procedure entry with explicit checklist.

**InferenceConfig gotcha (crt-030 specific)**: The `#[serde(default = "fn")]` pattern is established. The new constraint from this cycle is the Deserialize-only restriction: `toml::to_string` fails at compile time. Any future agent adding config fields that writes round-trip tests using `toml::to_string` will get a confusing compile error. Procedure #3759 documents this explicitly.

---

## 3. ADR Status

### Validated by delivery

| Entry | Title | Status |
|-------|-------|--------|
| #3731 | ADR-001: graph_ppr.rs as Submodule of graph.rs | Validated — implementation matched exactly |
| #3732 | ADR-002: personalized_pagerank() function signature | Validated — signature at graph_ppr.rs:39 matches exactly |
| #3734 | ADR-004: Deterministic accumulation via node-ID-sorted iteration | Validated — sort at line 59, before iteration loop |
| #3735 | ADR-005: PPR Pipeline Position Step 6d between 6b and 6c | Validated — step order 6b(713) → 6d(839) → 6c(962) confirmed |
| #3736 | ADR-006: Personalization vector via pre-cloned snapshot | Validated — no phase_affinity_score() call in Step 6d; snapshot read only |
| #3737 | ADR-007: ppr_blend_weight dual role intentional | Validated — doc-comment at config.rs:453-466 documents both roles |
| #3739 | ADR-009: PPR score map memory profile, no traversal depth cap | Validated — implementation uses unbounded HashMap; no depth cap |
| #3741 | ADR-008 (corrected): Inline synchronous path only, RayonPool deferred | Validated — no Rayon offload in implementation |
| #3750 | ADR-003 (corrected): Direction::Outgoing for reverse/transpose PPR | Validated — implementation uses Direction::Outgoing throughout |

### Deprecated during cycle (corrected by this cycle)

| Original | Corrected | Reason |
|----------|-----------|--------|
| #3733 | #3750 | ADR-003 specified Direction::Incoming (standard forward PPR). Implementation used Direction::Outgoing (reverse/transpose PPR) — correct behavior. Spec was wrong, code was right. Security reviewer caught post-merge; 4 artifacts corrected. |
| #3738 | #3741 | ADR-008 original included RayonPool offload threshold constant in scope. Revised to defer entirely — no threshold constant, no conditional branch in crt-030. |

### Flag for human review: are other ADRs potentially wrong?

After reviewing all ADRs #3731–#3739, #3741, #3750:

**No additional ADRs require re-examination.** Rationale:
- ADR-001 (#3731): module structure decision — fully structural, no semantic ambiguity possible. Validated.
- ADR-002 (#3732): function signature — mechanical match, verified at gate 3b by code inspection. Validated.
- ADR-004 (#3734): sort placement — a placement rule, not a semantic claim. Verified by static check (AC-05) and line number. Validated.
- ADR-005 (#3735): step ordering — verified by three independent checks (step comment line numbers, pipeline diagram, integration test). Validated.
- ADR-006 (#3736): snapshot read pattern — verified by grep (no `phase_affinity_score(` in Step 6d block). Validated.
- ADR-007 (#3737): dual-role config field — a design intent declaration, not a behavioral claim. No way to be "wrong." Validated.
- ADR-008 (#3741, corrected): deferred scope — the simpler of the two ADR-008 formulations. No behavioral claims to verify. Validated.
- ADR-009 (#3739): memory profile analysis — mathematical claim (HashMap bounded by node count). Validated by timing tests and memory analysis. Validated.

The direction semantics error (ADR-003) was the only case where a spec ADR made a wrong behavioral claim. The other ADRs are either structural/mechanical decisions or deferred-scope declarations that cannot be "wrong" in the same way.

---

## 4. Lessons

### New entries

| ID | Title | Source signal |
|----|-------|--------------|
| #3754 | Graph algorithm direction semantics: spec describing conceptual traversal vs. iteration variable direction are not the same | Post-merge spec correction: ADR-003 survived Gate 3a and 3b, caught only by security reviewer |
| #3755 | Large file compile-cycle amplification: search.rs at 4000+ lines produces 94 compile cycles | compile_cycles hotspot = 94 |
| #3760 | context_store failures during heavy compilation phases — contention or timing issue | tool_failure_hotspot: context_store 8×, Read 19× |

### Updated entries

| Old ID | New ID | Title | Change |
|--------|--------|-------|--------|
| #3577 | #3757 | Agent reports omit Knowledge Stewardship section — recurring | Added crt-030 as 4th confirmed instance; strengthened enforcement recommendation |
| #3551 | #3758 | Agents Default to Bash grep Instead of Grep Tool | Added crt-030 data point (20.2%); updated trend analysis showing oscillation, not improvement |

### Lesson: search_via_bash (20.2%)
Existing lessons #3757 (updated) and #3758 (updated) already cover this. crt-030 is a data point confirming the pattern persists in medium-sized features (62 files, develop phase). No new lesson needed; existing entries updated.

---

## 5. Retrospective Findings

### Hotspot-driven actions

| Hotspot | Finding | Action taken |
|---------|---------|--------------|
| compile_cycles = 94 | search.rs at 4845 lines amplifies every compile cycle; Wave 2 paid the full cost of a large-file edit | Lesson #3755 stored. Recommendation: batch type defs before first compile. Structural fix (search.rs split) is a separate follow-up. |
| context_load = 119 KB before first write | Heavy briefing phase; normal for architecture-first features with large existing codebases | No action — within expected range for a feature reading 9 ADRs + related entries before writing. |
| file_breadth = 62 files | Develop phase touched 62 distinct files; correlates with search_via_bash rate | No separate action; root cause is the bash-search behavior addressed by #3758. |
| tool_failure_hotspot: 8× context_store, 19× Read | Clustered during Wave 2 high-compile period | Lesson #3760 stored with mitigation guidance. |
| search_via_bash = 20.2% | Existing lessons cover this; crt-030 is the 4th above-threshold feature | Updated existing lessons #3757 and #3758 with crt-030 data. |

### Recommendation actions

| Recommendation | Action |
|----------------|--------|
| Batch field additions before compiling — complete type defs in-memory before each build | Incorporated into lesson #3755. Also relevant to procedure #3759 (InferenceConfig fields: all 5 locations before first compile). |
| Use run_in_background + TaskOutput instead of sleep polling | No Unimatrix entry warranted — this is a general tooling best practice already in CLAUDE.md. No novel insight from crt-030 specifically. |

### Positive outcomes worth preserving

The wave structure (Wave 1 parallel + Wave 2 sequential) produced 0 coordinator respawns and 0 post_completion_work. This is the first feature explicitly tracked with this pattern producing clean completion metrics. Pattern #3756 extracts this for future use.

---

## Knowledge Stewardship

**Queried:**
- `context_briefing` — surfaced #703, #3732, #3739, #724, #3730
- `context_search("graph submodule pure function edges_of_type pattern", category: pattern)` — found #3740, #3730, #3650, #3636
- `context_search("power iteration algorithm implementation pattern", category: pattern)` — found #3744 (existing)
- `context_search("search pipeline step insertion pattern")` — found #882, #3746, #3730
- `context_search("InferenceConfig field addition procedure", category: procedure)` — no match found → new procedure #3759
- `context_search("grep bash search tool anti-pattern", category: lesson-learned)` — found #3545, #3551
- `context_search("architect agent missing knowledge stewardship gate 3a", category: lesson-learned)` — found #3577
- `context_search("direction semantics graph algorithm spec wrong")` — found #3750, #3733

**Stored:**
- #3752: Node-ID-sorted accumulation for deterministic power iteration (pattern, new)
- #3753: Use pre-cloned lock snapshot for new pipeline steps (pattern, new)
- #3754: Graph algorithm direction semantics: conceptual vs. iteration variable direction (lesson-learned, new)
- #3755: Large file compile-cycle amplification (lesson-learned, new)
- #3756: Wave structure for multi-component features (pattern, new)
- #3757: Agent reports missing stewardship section (lesson-learned, corrected #3577 — added crt-030 as 4th instance)
- #3758: Agents default to Bash grep (lesson-learned, corrected #3551 — added crt-030 data point)
- #3759: How to add new fields to InferenceConfig (procedure, new)
- #3760: context_store failures during heavy compilation (lesson-learned, new)
