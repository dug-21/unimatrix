# Agent Report: 437-investigator

**Feature**: bugfix-437
**Task**: Diagnose recurring_friction and dead_knowledge extractor wrong write path

## Corrected Bug Scope

The bug report overstates the issue. `dead_knowledge.rs` — `DeadKnowledgeRule` was already removed from the extraction pipeline in GH #351. The file now contains only detection helpers. Half the bug report is stale.

`recurring_friction.rs` — `RecurringFrictionRule` is still active in `default_extraction_rules()` and still writes to ENTRIES. This is the live bug.

## Root Cause

`RecurringFrictionRule` implements `ExtractionRule::evaluate()`, producing `ProposedEntry` objects with `category:"lesson-learned"`, `topic:"process-improvement"`, `confidence: 0.5 + 0.1*N`. These pass all quality gate checks and are persisted to ENTRIES via `store.insert()` with `created_by:"background-tick"`.

### Code Path Trace

```
background_tick_loop (background.rs:291)
  → run_single_tick (background.rs:398)
    → extraction_tick (background.rs:1407)
      → spawn_blocking: run_extraction_rules(obs, store, default_extraction_rules())
            → RecurringFrictionRule::evaluate (recurring_friction.rs:24)
                  groups obs by session_id → detect_hotspots per session
                  for rules with sessions.len() >= 3:
                    ProposedEntry { title:"Recurring friction:{rule}",
                                    category:"lesson-learned",
                                    topic:"process-improvement" }
      → quality_gate (extraction/mod.rs:136) ← ALL CHECKS PASS
      → neural gate (shadow mode) ← passes through unchanged
      → embedding gate ← PASSES on first occurrence
      → store.insert(NewEntry { created_by:"background-tick" }) (background.rs:1603)
```

### Why quality gate passes

| Check | Condition | Friction value |
|-------|-----------|---------------|
| Rate limit | ≤10/hr | 1-3 per tick — PASS |
| Title length | ≥10 chars | 36 chars — PASS |
| Content length | ≥20 chars | Hundreds of chars — PASS |
| Category | lesson-learned in allowlist | PASS |
| Cross-feature | ≥3 source_features | sessions.len()≥3 enforced by rule — PASS |
| Confidence floor | ≥0.2 | 0.5+0.1×N — PASS |

The dedup guard `existing_entry_with_title()` prevents repeat writes only — the first write always succeeds.

## Affected Files and Functions

| File | Function | Role in Bug |
|------|----------|-------------|
| `crates/unimatrix-observe/src/extraction/recurring_friction.rs` | `RecurringFrictionRule::evaluate()` | Produces ProposedEntry — replace with Vec<String> producer |
| `crates/unimatrix-observe/src/extraction/recurring_friction.rs` | `existing_entry_with_title()` | Dedup guard — dead code after fix |
| `crates/unimatrix-observe/src/extraction/mod.rs` | `default_extraction_rules()` | Contains RecurringFrictionRule — remove it |
| `crates/unimatrix-observe/src/extraction/mod.rs` | `min_features_for_rule()` | `"recurring-friction"` branch — dead code after fix |
| `crates/unimatrix-server/src/background.rs` | `TickMetadata` struct | Add `pub friction_signals: Vec<String>` field |
| `crates/unimatrix-server/src/background.rs` | `run_single_tick()` | Add post-extraction step: compute friction signals, write to tick_metadata |
| `crates/unimatrix-server/src/background.rs` | (new) `run_recurring_friction_migration_v1()` | One-shot quarantine of existing "Recurring friction:" entries |
| `crates/unimatrix-server/src/mcp/tools.rs` | context_status handler (~line 916) | Append tick_meta.friction_signals to report.maintenance_recommendations |

## Proposed Fix (6 steps)

**Step 1 — recurring_friction.rs:** Remove `ExtractionRule` impl and `existing_entry_with_title()`. Add:
```rust
pub fn compute_friction_recommendations(observations: &[ObservationRecord]) -> Vec<String>
```
Same logic, returns strings: `"Recurring workflow friction: '{rule}' fired in {n} sessions -- {remediation}"`. No store access. Ephemeral.

**Step 2 — extraction/mod.rs:** Remove `RecurringFrictionRule` from `default_extraction_rules()` (4→3 rules). Remove `"recurring-friction"` from `min_features_for_rule()`.

**Step 3 — background.rs TickMetadata:** Add `pub friction_signals: Vec<String>`. Default derive covers initialization.

**Step 4 — background.rs run_single_tick():** After `extraction_tick` timeout block, call `compute_friction_recommendations` via `spawn_blocking`, write result to `tick_metadata.lock().friction_signals`.

**Step 5 — mcp/tools.rs context_status handler:** After setting extraction_stats from tick_meta (~line 934):
```rust
report.maintenance_recommendations
    .extend(tick_meta.friction_signals.iter().cloned());
```
Unconditional — not gated by lambda.

**Step 6 — one-shot migration:** Add `run_recurring_friction_migration_v1(store).await` after `run_dead_knowledge_migration_v1`. Counter key: `"recurring_friction_migration_v1"`. Query `topic="process-improvement"`, filter `tag="recurring-friction"` + `status==Active`, set `Status::Quarantined`. Cap 200. Non-fatal.

## Risk Assessment

- **Blast radius:** Tests asserting `rules.len() == 4` → update to 3. Tests checking `"recurring-friction"` in names → remove. Tests in `recurring_friction.rs` testing `ProposedEntry` → rewrite to test `compute_friction_recommendations()`. `TickMetadata` new field is non-breaking.
- **Regression risk:** LOW — after restart, `friction_signals` empty until first tick (~15 min); expected. Quarantine migration one-shot, COUNTERS-gated. Other extraction rules unaffected.
- **Confidence:** HIGH. Full call chain traced. All 6 quality gate checks confirmed to pass.

## Missing Test

`test_extraction_tick_does_not_write_recurring_friction_to_entries` (background.rs tests):
1. Create observations with orphaned-call friction across 3+ sessions
2. Call `extraction_tick()`
3. Query ENTRIES with `topic="process-improvement"` AND tag `"recurring-friction"`
4. Assert: zero entries in ENTRIES
5. Assert `tick_metadata.friction_signals` is non-empty after fix

Root gap: `recurring_friction.rs` tests verify `ProposedEntry` format but never assert routing. No test exercised `extraction_tick → store.insert` to verify operational signals stay out of ENTRIES.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- loaded prior lessons #3252 (ExtractionRule must not perform side effects) and #3254 (additive rules must not signal deprecation), confirmed consistent with diagnosis.
- Stored: nothing novel -- root cause pattern already in entry #3252. Fix is direct application.
