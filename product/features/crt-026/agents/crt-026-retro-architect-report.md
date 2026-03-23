# crt-026 Retrospective — Architect Agent Report

Agent ID: crt-026-retro-architect
Role: Retrospective knowledge extraction (not design)
Feature: crt-026 (WA-2 Session Context Enrichment)

---

## ADR Validation

All four ADRs confirmed as implemented exactly as designed. No deviations found.

| ADR | Decision | Delivery Status |
|-----|----------|----------------|
| ADR-001 (#3161) | Boost inside `compute_fused_score` as first-class dimension | CONFIRMED — search.rs lines 219–221; status_penalty applied after fused (correct SR-09 ordering) |
| ADR-002 (#3162) | Pre-resolve histogram in handler, pass via ServiceSearchParams | CONFIRMED — MCP path tools.rs 324–329, UDS path listener.rs 973–977, both before first await |
| ADR-003 (#3163) | w_phase_explicit=0.0 placeholder, W3-1 reserved | CONFIRMED — phase_explicit_norm hardcoded 0.0 at search.rs 871 with ADR-003 comment; no dead-code removal |
| ADR-004 (#3175) | No rebalancing; w_phase_histogram=0.02 carries full session signal budget | CONFIRMED — six-weight sum check unchanged, sum 0.95 → 0.97 passes validate() cleanly; test T-FS-01 asserts delta == 0.02 exactly |

Consolidated delivery validation stored as #3209.

---

## Pattern Extraction

### Skipped (already covered)

| Entry | Reason |
|-------|--------|
| #3027 | SR-07 pre-snapshot pattern already covers the pre-resolution-before-await discipline |
| #3182 | `FusionWeights::effective()` NLI-absent denominator exclusion already stored (delivery-era) |
| #3180 | `make_state_with_rework` test helper and `Default` derive pattern already stored |
| #3177 | Synthetic histogram concentration test pattern already stored |
| #1269/#2478 | High compile cycles → targeted test invocations already covered |
| #1272 | edit_bloat/mutation_spread inflated by design artifacts already covered |

### Updated

**#3157 → #3210** (corrected): Enriched with WA-4a forward-compatibility caveat confirmed by delivery — pre-resolution in handler is impossible for WA-4a (no handler on call stack for proactive injection); WA-4a must expect to add `Arc<SessionRegistry>` to `SearchService` and supersede ADR-002. Also added the empty→None mapping detail.

### New entries stored

| ID | Title | Category | Why new |
|----|-------|----------|---------|
| #3205 | Weight budget assignment when one of two planned scoring terms is deferred | procedure | No prior entry covers the arithmetic of collapsing a two-term split when one term ships at 0.0 |
| #3206 | FusionWeights additive field: dual exemption from sum-check and NLI-absent denominator | pattern | #3182 covers the denominator mechanic; #3206 adds the InferenceConfig::validate() sum-check exemption and the doc-comment update obligation — needed together for W3-1 |
| #3207 | compute_fused_score extension pattern: full 7-step recipe for adding a scoring dimension | pattern | Synthesizes the full extension workflow into a reusable checklist. W3-1 implementers can follow this verbatim |
| #3208 | Validate new scoring weight defaults against research spike, not product vision prose | lesson-learned | The 0.005 vs 0.02 discrepancy (caught at gate-3a) reveals a reusable lesson: ASS-NNN spike value is authoritative; vision document value is aspirational |
| #3209 | crt-026 ADR delivery validation: all four decisions confirmed as implemented | decision | Confirms design-era ADRs against shipped code; records WA-4a forward-risk for ADR-002 |

---

## Procedure Review

### Weight calibration workflow

The sequence "read ASS-028 → compare to product vision two-term split → recognise that the deferred term collapses the budget onto the active term → raise default from implied 0.005 to 0.02" is a reusable procedure. Stored as #3205. Key insight: the research spike (ASS-028) is the authoritative source for the numerical value; the vision document describes intent and relative priority but not empirically calibrated magnitudes.

### FusionWeights::effective() NLI-absent denominator protection

The technique is: pass new additive fields through unchanged in all three return paths of `effective()` and add an explicit comment naming the fields excluded from the denominator. This is part of the #3206 pattern (dual exemption). The protection worked cleanly in delivery — no test failures related to NLI-absent re-normalization.

---

## Hotspot-Derived Actions

### compile_cycles (134 cycles, threshold 6)

Already covered by #1269 and #2478. No new lesson to store. The observation is: 134 cycles in a multi-component feature across 5 files is consistent with the pattern — agents compile after each wave to catch struct literal update failures early. The wave-based build discipline (#2957) applies here: agents should scope `cargo test` to `--package unimatrix-server` rather than `--workspace` during mid-wave development, and only run workspace-wide at gate boundaries.

No new entry needed — existing entries cover this adequately.

### edit_bloat_ratio 0.61 vs mean 0.15

Driven by design artifact files (ARCHITECTURE.md, SPECIFICATION.md, pseudocode files). Code edits in the five modified source files were targeted and small (Component 1–8 additions, not rewrites). This is the "design-heavy feature baseline" pattern documented in #1272. Not a problem — no action needed.

### permission_retries (Read 26, Bash 8)

See settings.json finding below.

### search_via_bash (21% of Bash calls are search commands)

Agents used `grep`/`find` via Bash instead of the Grep/Glob tools. This is a CLAUDE.md violation. The lesson is already documented in project instructions. Not a new Unimatrix entry — enforcement belongs in protocols.

---

## settings.json Finding

The current `settings.json` contains only hook definitions (8 lifecycle hooks). It has **no `allowedTools` or `permissions` section** defining a cargo/build/test command allowlist.

The retrospective flags 26 Read retries and 8 Bash retries. With no allowlist in settings.json, every `cargo test`, `cargo build`, `cargo clippy`, and `cargo check` invocation requires a permission prompt or retry — this is the source of the 34 combined retries.

**What is missing:** A `permissions.allow` block covering at minimum:
- `Bash(cargo test*)`
- `Bash(cargo build*)`
- `Bash(cargo clippy*)`
- `Bash(cargo check*)`

This is a persistent friction point that has appeared in multiple retrospectives. The fix is a settings.json change, not a Unimatrix entry — it should be tracked as a follow-up action (GH issue or direct edit to settings.json).

---

## Knowledge Stewardship

Queried before work:
- `context_search(query: "session state accumulator histogram scoring", category: "pattern")` → #3157, #3177, #3180
- `context_search(query: "FusedScoreInputs FusionWeights extension pattern compute_fused_score")` → #3161, #3156, #2972, #3181, #3164
- `context_search(query: "weight calibration research spike validation before shipping defaults")` → #751, #2985
- `context_search(query: "additive weight field normalization denominator excluded")` → #3182, #2971
- `context_search(query: "pre-resolve session state before await snapshot SR-07")` → #3027
- `context_search(query: "deferred placeholder weight budget split")` → no matching prior entry
- `context_search(query: "compile cycles cargo test scope narrowing")` → #1370, #2957, #2478, #1269
- `context_search(query: "edit bloat design artifact large files")` → #1272

Stored:
- #3205 (procedure): weight budget assignment for deferred terms
- #3206 (pattern): FusionWeights additive field dual exemption
- #3207 (pattern): compute_fused_score extension full recipe
- #3208 (lesson-learned): validate weight defaults against research spike
- #3209 (decision): ADR delivery validation with WA-4a forward risk
- #3210 (corrected #3157, pattern): pre-resolution pattern enriched with WA-4a caveat
