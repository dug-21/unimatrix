# RETRO ARCHITECT REPORT — crt-036

> Agent: crt-036-retro-architect
> Date: 2026-03-31
> Feature: crt-036 — Intelligence-Driven Retention Framework

---

## Patterns

**Already stored during delivery (do not re-store):**
- #3914 — Two-hop join through sessions (valid, no drift detected in implementation)
- #3928 — Nested config test via UnimatrixConfig (valid, implementation confirmed the pattern)
- #3929 — observation_phase_metrics FK constraint (valid, no drift)
- #3930 — list_purgeable_cycles includes already-purged cycles (valid, tester applied it correctly)

**New pattern stored:**
- None. The RetentionConfig struct itself follows the same conventions as InferenceConfig. However, adding a NEW top-level config section (as opposed to fields to an existing section) is a distinct procedure — stored as #3934 below.

**Skipped patterns:**
- CycleGcStats / UnattributedGcStats stats structs: feature-specific types; the general pattern (stats struct per GC pass returned from store method) is straightforward and not novel relative to existing patterns.
- gc_unattributed_activity single-connection optimization (acquiring one connection across multiple DELETEs in a max_connections=1 pool): this was a valid optimization but is a narrow sqlx-specific technique already implied by entry #3799 (acquire before execute). Not separately stored.

---

## Procedures

**New procedure stored:**
- **#3934** — "How to add a new top-level config section to UnimatrixConfig"
  Covers: struct definition with `#[serde(default)]`, Default impl, validate() method with new ConfigError variant, wiring into UnimatrixConfig, validate_config() call site, Arc threading into background tick, config.toml block, test checklist. Distinguishes from #3769 (adding fields to an existing section).

**Existing procedure reviewed:**
- #3769 (How to add new fields to InferenceConfig): not updated. RetentionConfig is a distinct case — a new top-level section, not InferenceConfig extension. The two procedures are complementary.

---

## ADR Status

All three ADRs were validated by successful delivery:

| ADR | Entry | Validated? | Notes |
|-----|-------|------------|-------|
| ADR-001: Per-cycle transaction granularity | #3915 | Yes | pool.begin()/txn.commit() confirmed in gc_cycle_activity(); connection released per cycle; R-13 (crash-between-steps) accepted as low-priority per risk strategy |
| ADR-002: max_cycles_per_tick in RetentionConfig | #3916 | Yes | Field confirmed in RetentionConfig, not InferenceConfig; threading pattern identical to InferenceConfig precedent |
| ADR-003: PhaseFreqTable guard as warn-only | #3917 | Yes | Implemented as private function run_phase_freq_table_alignment_check(); emits tracing::warn! only; does not block GC; AC-17 tests pass with correct "retention window" string |

**Flagged for supersession:** None. All ADRs held through implementation and testing without contradictions.

**Architecture deviation noted (non-blocking):** The ARCHITECTURE.md Integration Surface table specified `list_purgeable_cycles(k: u32) -> Result<Vec<String>>` but the pseudocode (and implementation) used `(k: u32, max_per_tick: u32) -> Result<(Vec<String>, Option<i64>)>`. This extension was documented and justified (ADR-003 by-product, SQL LIMIT cap). No supersession required — it is an architecture table under-specification, not a decision reversal.

---

## Lessons

**New lesson stored:**
- **#3935** — "Tracing-test AC deferred from Gate 3b: structural coverage without exercising production code path"
  AC-15 (test_gc_tracing_output) was named in the test plan but not implemented in Stage 3b. The gate-skip sub-test was written using direct-emit format verification because the Ok(None) branch is structurally unreachable through normal data setup. Documents both the deferral anti-pattern and the acceptable direct-emit approach for truly unreachable defense-in-depth branches.

**Existing lessons updated (corrections):**
- **#2478 → #3932** — High Compile Cycles lesson: added crt-036 as recurrence (76 cycles) with the specific cross-crate type threading root cause (RetentionConfig threaded through 4 files across 2 crates, each consumer discovering type errors in sequence).
- **#3807 → #3933** — Knowledge Stewardship missing from architect reports: added crt-036 as 6th confirmed instance (architect report missing section on first Gate 3a submission).
- **#3924 → #3936** — run_in_background/sleep lesson: added crt-036 as 8th recurrence; noted spec-phase sleep instances as first confirmed occurrence in a design/spec-phase agent role (previously tester and fix agents only).

**Skipped lessons with reasons:**
- Gate 3a warn-message-text mismatch (pseudocode used different log string than spec required): the gate 3a report itself noted this is feature-specific. No recurrence pattern across 2+ features identified. The gate 3a validator caught and required the fix in iteration 0; no new lesson needed.
- Cold restart / context load (480-min gap, 28 re-reads, 72 files): within normalized bounds per #1271. crt-036 touched ~6 production components across 2 crates; 28 re-reads and 155KB context load are within the ~75KB/component and 15-20 re-reads/gap thresholds. Not actionable.
- knowledge_entries_stored outlier (17 vs mean 8.7): positive signal attributable to new GC domain establishment (4 patterns + 3 ADRs in a domain with no prior entries). No new lesson needed — this is the expected output when a feature opens a new domain.

---

## Retrospective Findings

### Hotspot-Derived Observations

**compile_cycles (76 cycles, 36 clusters):** Root cause was cross-crate type threading — RetentionConfig defined in unimatrix-server but consumed by unimatrix-store (retention.rs methods) and background.rs. Each consumer wave discovered type errors sequentially. Mitigation added to compile cycle lesson (#3932): complete the full type definition (struct, Default impl, validate(), serde attributes) in Wave 1 before any downstream file references it.

**cold_restart (480-min gap, 28 re-reads):** Within normalized bounds for a 6-component, 2-crate feature. Entry #1271 covers this. No new action needed.

**sleep_workarounds (4 instances in spec phase):** Corrected into existing lesson chain (#3936). First confirmed spec-phase occurrence — expands the known scope of the pattern from tester/fix agents to design-phase agents.

**file_breadth (72 distinct files) and mutation_spread (52 files):** Expected for a feature removing two legacy DELETE sites across tools.rs and status.rs, adding a new module (retention.rs), and threading a config type through 4 files. These are structural breadth from the scope, not process inefficiency.

**edit_bloat (87KB average, 6697KB total):** Driven by the large inline test module in retention.rs (1145 lines of tests in a 1435-line file). This is consistent with project convention (tests-in-file). Gate 3b noted this as WARN but non-blocking. Not actionable.

**tool_failure_hotspot (17 Read failures, 10 context_store failures, 5 Bash failures):** Not analyzed in detail — pre-existing tool reliability characteristics. The 10 context_store failures are notable for a session that stored 17 entries; suggests retry overhead. No lesson stored (insufficient detail to generalize).

### Recommendation Actions

- [sleep_workarounds] Covered by existing procedure #3561 and corrected lesson #3936. No new action.
- [compile_cycles] Mitigation note added to #3932. Implementation brief wave plans for cross-crate type threading features should explicitly sequence the type-definition wave before consumer waves.

### Positive Outlier

**knowledge_entries_stored: 17 vs mean 8.7:** Strong positive outlier, 2x typical. Driven by crt-036 establishing the cycle-based GC domain from scratch (no prior ADRs or patterns in this area). The 4 patterns + 3 ADRs + this retrospective's additions represent appropriate knowledge capture for a new domain. No corrective action needed — this is the intended behavior when a feature opens genuinely new territory.

---

## Knowledge Stewardship

**Queried:**
- context_briefing for crt-036 retrospective task
- context_search: "retention GC background tick cycle-based pruning activity data"
- context_search: "config validation InferenceConfig validate startup error"
- context_search: "tracing_test traced_test structured log assertion background tick tests"
- context_search: "pseudocode log message text deviation spec required string gate failure rework"
- context_search: "knowledge stewardship section missing architect report gate failure"
- context_search: "cross-crate type propagation struct threading delivery sequencing wave order compile cycles"
- context_search: "cold restart agent context resume re-read files long gap session timeout"
- context_search: "run_in_background TaskOutput sleep polling background command"
- context_search: "test AC gap deferred later stage tracing output test coverage Gate 3b WARN"
- context_search: "defense-in-depth unreachable branch format test direct emit warn log"
- context_get: #2478, #3547, #324, #3807, #3769, #3898, #3386, #1271, #3924

**Stored:**
- #3934 — New procedure: How to add a new top-level config section to UnimatrixConfig
- #3935 — New lesson: Tracing-test AC deferred from Gate 3b (structural coverage without exercising production code path)
- #3932 — Correction of #2478: compile cycle lesson updated with crt-036 recurrence + cross-crate type threading pattern
- #3933 — Correction of #3807: knowledge stewardship missing lesson updated with crt-036 as 6th instance
- #3936 — Correction of #3924: run_in_background lesson updated with crt-036 as 8th recurrence (spec-phase context)
