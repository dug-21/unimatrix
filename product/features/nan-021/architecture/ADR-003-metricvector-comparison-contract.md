## ADR-003 nan-021: MetricVector Comparison Contract — Field-for-Field Equality Modulo the Enumerated D-5 Exclusion Set

### Context
AC-04 requires `context_cycle_review(feature)` over the HTTPS path to return a NON-EMPTY `MetricVector`
equal field-for-field — modulo the documented exclusion set (D-5) — to the `MetricVector` for the
identical workload driven over local UDS in the same execution (live-vs-live, D-6). D-5 LOCKS exact
field-for-field equality EXCEPT for enumerated non-deterministic fields: `computed_at` (wall-clock stamp)
and any duration-derived field with sub-second wall-clock jitter — at minimum `phases[*].duration_secs`,
plus any `UniversalMetrics` duration/latency field the design enumerates.

The `MetricVector` (`unimatrix-store/src/metrics.rs:102`) is content-opaque and transport-agnostic BY
CONSTRUCTION — its aggregates derive from durable streams (`cycle_events`, `SessionRecord.outcome`,
`query_log ∪ injection_log`), never the transcript. This is exactly why parity is the right assertion:
the same workload must produce the same numbers regardless of transport. The Scope Risk Assessment names
the load-bearing assumption: the exclusion set being COMPLETE. If any field carries hidden wall-clock /
latency jitter NOT in the enumerated set, the gate flakes; the recommendation is to treat an unexpected
non-equal field as a REAL failure, not a tolerance to widen.

The struct has exactly three temporal/wall-clock fields and no others (verified against the live source):
`MetricVector.computed_at:u64`, `UniversalMetrics.total_duration_secs:u64`, and per-phase
`PhaseMetrics.duration_secs:u64`. All other 20 `UniversalMetrics` fields are counts/ratios derived from
content-opaque streams and are transport-invariant.

### Decision
**Field-for-field equality with a CLOSED, ENUMERATED exclusion set of exactly three wall-clock fields.**

The comparator (owned by the Python harness, D-1) parses both `MetricVector`s from the
`context_cycle_review` JSON (via `parse_tool_result`) and asserts equality over:

1. **All 21 `UniversalMetrics` fields EXCEPT `total_duration_secs`.** The 20 compared fields:
   `total_tool_calls`, `session_count`, `search_miss_rate`, `edit_bloat_total_kb`, `edit_bloat_ratio`,
   `permission_friction_events`, `bash_for_search_count`, `cold_restart_events`,
   `coordinator_respawn_count`, `parallel_call_rate`, `context_load_before_first_write_kb`,
   `total_context_loaded_kb`, `post_completion_work_pct`, `follow_up_issues_created`,
   `knowledge_entries_stored`, `sleep_workaround_count`, `agent_hotspot_count`, `friction_hotspot_count`,
   `session_hotspot_count`, `scope_hotspot_count`. (Integer counts compare exactly; ratio `f64` fields
   are derived num/den pairs over identical workloads and compare exactly — no float tolerance is
   introduced unless a ratio's denominator is itself a wall-clock duration, which none of these are.)
2. **The `phases` BTreeMap:** the KEY SET equal, and per phase `tool_call_count` equal. `duration_secs`
   per phase is EXCLUDED.
3. **`domain_metrics` (`HashMap<String,f64>`):** key set + values equal.

**The EXCLUSION SET is exactly these three fields, named in the test (D-5):**
- `MetricVector.computed_at` — wall-clock stamp of when the report was computed.
- `UniversalMetrics.total_duration_secs` — sum of phase durations; carries sub-second wall-clock jitter
  between two live runs.
- `PhaseMetrics.duration_secs` (per phase) — same jitter per phase.

**Non-emptiness (AC-04), asserted on BOTH vectors:** `total_tool_calls > 0`, `session_count > 0`,
`phases` populated (non-empty key set). "Same non-empty metrics" means both transports produce the same
REAL numbers, not a believable `0`.

**Unexpected-field policy (Risk Assessment Rec 4):** the comparator compares the FULL field set minus the
named three. Any field outside the exclusion set that differs is a REAL failure surfaced loudly with the
field name and both values — it is NEVER added to the exclusion set to make the gate green. If a future
schema change adds a wall-clock field, widening the set is a deliberate ADR amendment, not a silent test
edit.

**FIRST-LIVE-RUN VALIDATION GATE (the 3-field exclusion is an ASSUMPTION, not proven).** The premise
"`MetricVector` is transport-agnostic, only the three wall-clock fields differ" is load-bearing but
UNVERIFIED until a real dual-transport run. Several `UniversalMetrics` fields are session-lifecycle-derived
and could legitimately differ by TRANSPORT rather than by workload — at minimum `cold_restart_events`,
`coordinator_respawn_count`, `context_load_before_first_write_kb`, `total_context_loaded_kb`,
`permission_friction_events`. Therefore:
- The gate is NOT TRUSTED until the FIRST live dual-transport run is examined FIELD-BY-FIELD across all 18
  non-excluded `UniversalMetrics` fields (the 20 compared minus the 2 that are themselves counts but
  whose transport-invariance is under scrutiny — i.e. the full non-wall-clock set) PLUS the `phases` key
  set and per-phase `tool_call_count`, and confirmed to actually match. This first-run field-by-field
  confirmation must pass ONCE before the gate is relied upon as a parity proof.
- This is the one place "test infra" touches the DEFINITION of C0 parity — the exclusion set is, in
  effect, the operational definition of "same metrics."

**DISPOSITION AUTHORITY (who decides on a divergence — R-01/R-02 failure mode).** ANY non-wall-clock
divergence observed on that first run (or any later run) is ESCALATED TO A HUMAN / PRODUCT CALL — it is
NOT resolved by the implementer or tester. The disposition is exactly ONE of:
- **(a) Real parity defect** → file a GitHub bug. This is the fixture doing its job (a good catch — the
  whole reason C0 is *measured*, not asserted). The gate stays RED until the defect is addressed.
- **(b) Transport-inherent field** → add it to the exclusion set ONLY with explicit product sign-off AND a
  recorded rationale appended to THIS ADR (a deliberate amendment via `context_correct`, naming the field,
  the transport-inherent reason, and the approver).

The implementer/tester MUST NOT silently widen the exclusion set to make a red go green — that IS the
R-01/R-02 failure mode (reactive widening hides real divergence). **Product / human is the named decider.**

### Consequences
- **Easier:** the comparison is deterministic and self-documenting — the three excluded fields are named
  inline, so the gate is non-flaky and a reviewer sees exactly what is and isn't compared. Because the
  vectors come from one execution of one manifest (ADR-001), every compared field is identical-by-
  construction; a real difference signals a genuine transport-dependent defect (the thing the fixture
  exists to catch).
- **Harder:** the exclusion set's COMPLETENESS is load-bearing — if the `MetricVector` schema gains a
  hidden wall-clock/latency field, the comparator must be amended (a deliberate ADR change, not a
  tolerance widen). The comparator must field-walk the parsed dicts robustly (e.g. `domain_metrics` is a
  schema-v14 extension `HashMap` whose key set itself participates). Ratio `f64` fields require an exact
  equality contract justified by identical workloads; if any ratio ever derives from a wall-clock
  denominator the contract must add it to the exclusion set explicitly. The first-live-run gate means the
  fixture is NOT a trusted parity proof until one field-by-field human confirmation passes — the
  session-lifecycle fields (`cold_restart_events`, `coordinator_respawn_count`,
  `context_load_before_first_write_kb`, `total_context_loaded_kb`, `permission_friction_events`) are the
  prime suspects for a transport-inherent (not workload) difference and may force a product-signed
  exclusion-set amendment before first-green. Disposition authority sits with product/human, not the
  implementer — so a divergence is a deliberate decision point, not a quick test edit.

Related: D-5, D-6, R-01, R-02; AC-04. Field shapes from `unimatrix-store/src/metrics.rs:45–115` and
`unimatrix-observe/src/types.rs:381` (`RetrospectiveReport.metrics`). Fed by ADR-001 (one manifest → both
vectors). Pairs with ADR-004 (the derived-attribution that makes the cycle reviewable in the first place).
