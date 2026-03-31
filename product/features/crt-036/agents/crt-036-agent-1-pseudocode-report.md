# Agent Report: crt-036-agent-1-pseudocode

## Task
Produce per-component pseudocode for all 5 crt-036 components: RetentionConfig,
CycleGcPass store methods, run_maintenance GC block, legacy DELETE removal, and
PhaseFreqTable alignment guard.

## Outputs Produced

| File | Status |
|------|--------|
| `product/features/crt-036/pseudocode/OVERVIEW.md` | Complete |
| `product/features/crt-036/pseudocode/retention-config.md` | Complete |
| `product/features/crt-036/pseudocode/cycle-gc-pass.md` | Complete |
| `product/features/crt-036/pseudocode/run-maintenance-gc-block.md` | Complete |
| `product/features/crt-036/pseudocode/legacy-delete-removal.md` | Complete |
| `product/features/crt-036/pseudocode/phase-freq-table-guard.md` | Complete |

## Components Covered

1. **RetentionConfig** — struct, 3 default fns, Default impl, validate(), new
   ConfigError::RetentionFieldOutOfRange variant, UnimatrixConfig wiring, config.toml block.
2. **CycleGcPass** — `retention.rs` (new file), CycleGcStats, UnattributedGcStats,
   list_purgeable_cycles (now returns (Vec<String>, Option<i64>) to serve alignment guard),
   gc_cycle_activity, gc_unattributed_activity, gc_audit_log. lib.rs `pub mod retention`.
3. **run_maintenance GC block** — step 4 replacement, RetentionConfig param addition,
   per-cycle loop with labeled block pattern, background.rs threading changes.
4. **Legacy DELETE removal** — both sites identified with grep patterns, no-replacement
   policy for tools.rs, verification requirements.
5. **PhaseFreqTable alignment guard** — tick-time advisory warn, comparison direction
   documented explicitly (oldest <= lookback_cutoff), skipped when None.

## Design Decisions Made in Pseudocode

**list_purgeable_cycles return type change:** The architecture spec defines the
signature as `async fn list_purgeable_cycles(&self, k: u32, max_per_tick: u32) -> Result<Vec<String>>`.
The PhaseFreqTable guard (ADR-003) requires the K-th oldest retained `computed_at`
value. Rather than a separate query at the call site in `status.rs` (which would
require a second DB read), this pseudocode combines both into one return value:
`Result<(Vec<String>, Option<i64>)>`. This keeps the two reads cohesive (both query
cycle_review_index) and avoids a second round-trip. The architecture's Integration
Surface table says `list_purgeable_cycles` signature, but ADR-003 explicitly says
the alignment check is "a by-product of resolving the purgeable set." The combined
return is the cleanest implementation of that intent. Implementation agents must
use the (Vec<String>, Option<i64>) return type.

**Labeled block for list_purgeable_cycles failure:** The `goto_step_4f` pattern
described in the architecture cannot be expressed with goto in Rust. Pseudocode
documents using `'gc_cycle_block: { ... break 'gc_cycle_block; }` to allow audit_log
GC to continue even when the cycle query fails.

## Open Questions

None blocking. One implementation note flagged:

**query_log_lookback_days type:** The pseudocode uses `u32` for
`inference_config.query_log_lookback_days` in the alignment guard computation.
Implementation agents must verify the actual type in `InferenceConfig` and cast
accordingly. Arithmetic overflow is not a risk at any validated value.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for "maintenance tick GC store methods background"
  (pattern) — found entries #3254 (ExtractionRule vs maintenance action), #1560
  (Arc<RwLock<T>> background-tick state cache), #1542 (consecutive counter error semantics),
  #3213 (Arc startup resource threading), #3822 (promotion tick idempotency). Entry #1560
  confirmed pass-by-value is simpler than Arc<RwLock<T>> for RetentionConfig (NFR-06). No
  new Arc<RwLock<T>> introduced.
- Queried: `mcp__unimatrix__context_search` for "crt-036 architectural decisions" (decision,
  topic: crt-036) — found ADR entries #3915, #3916, #3917. All three ADRs applied directly
  in pseudocode.
- Also read: ADR-001, ADR-002, ADR-003 files in full; gc_sessions() reference pattern in
  sessions.rs; ConfigError enum in config.rs; run_maintenance() signature in status.rs;
  both 60-day DELETE sites in status.rs and tools.rs.

- Deviations from established patterns:
  - `list_purgeable_cycles` return type extended to `(Vec<String>, Option<i64>)` to serve
    ADR-003 alignment guard as a by-product (see Design Decisions above).
  - All other patterns (pool.begin()/txn.commit(), write_pool_server() usage, ConfigError
    variant structure, serde(default) field fns, validate() error style) match established
    codebase patterns exactly.
