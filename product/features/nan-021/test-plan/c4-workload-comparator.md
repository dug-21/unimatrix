# C4 — Workload Driver + Comparator (Python) — Test Plan

> **Component:** the single parameterized workload manifest fed to BOTH legs; the `MetricVector` comparator
> (field-for-field modulo the named D-5 exclusion set); the symmetric durability-barrier helper.
> **NEW module** `harness/parity_workload.py` — the ONLY substantial net-new code. This is the SPINE — test
> it first and hardest (pure-Python over parsed dicts, runnable OFF Docker).
> **ACs:** AC-04 (primary, incl. the first-live-run gate), AC-03 (manifest carries the derivation input),
> AC-07 (sole net-new module). **Risks:** R-01, R-02, R-03, R-06, R-09 (Critical-heavy).

---

## Why this component is tested off-Docker (R-12 mitigation)

The comparator + barrier predicate are pure functions over parsed dicts (`parse_tool_result` →
`.parsed`). The live legs run only on the release-gate tag (R-12 first-green tax). So the comparator's
TEETH (R-02), its exclusion-set COMPLETENESS classification (R-01), and the barrier's predicate (R-06)
MUST be unit-tested with synthetic `MetricVector` dicts BEFORE any tag round — this is the nan-019
stub-drive precedent (#5258) applied to the Python comparator.

---

## Test Expectations

### R-01 — exclusion-set completeness (no field unclassified)

- **`test_c4_every_field_classified`**: assert every one of the 21 `UniversalMetrics` fields +
  `PhaseMetrics.{duration_secs, tool_call_count}` is EXPLICITLY classified `deterministic` or `excluded` in
  the comparator — NO field is unclassified. The excluded set is named as a LITERAL of exactly 3 fields:
  `MetricVector.computed_at`, `UniversalMetrics.total_duration_secs`, `PhaseMetrics.duration_secs`.
- **`test_c4_ratio_fields_compared_exactly`**: assert the f64 ratio fields (`search_miss_rate`,
  `edit_bloat_ratio`, `parallel_call_rate`, `post_completion_work_pct`, `edit_bloat_total_kb`) are compared
  EXACTLY (no float tolerance) — they are num/den over identical workloads; no denominator is wall-clock.
  A divergence on any ratio is a real failure, not a tolerance to widen.
- **`test_c4_excludes_wallclock_not_luck`** (negative / live-burst design): inject a 1-second artificial
  delay into one leg's workload; assert the comparator still PASSES — proves it truly excludes wall-clock
  (`computed_at`, `total_duration_secs`, `phases[*].duration_secs`), not that the runs happened to match.
- **`test_c4_repeat_burst_zero_out_of_set_divergence`** (live, ≥20×): run HTTPS-vs-UDS parity ≥20×
  back-to-back in one session; assert ZERO field outside the enumerated D-5 set EVER differs. A field that
  differs even once is dispositioned per §First-Live-Run (defect vs. signed amendment) — NEVER silently
  tolerated.

### R-02 — comparator has teeth / set is minimal (no vacuous green)

- **`test_c4_mutation_drop_observe_fails`** (MUTATION HARNESS): force one STRUCTURAL (non-excluded) field to
  differ — drop one observe from the HTTPS leg so `total_tool_calls`/a `phases` count diverges; assert the
  comparator FAILS LOUDLY with the field name + both values. Proves the comparator has teeth on counts.
- **`test_c4_count_fields_never_excludable`**: assert `total_tool_calls`, `session_count`,
  `knowledge_entries_stored`, the hotspot counts (`agent_/friction_/session_/scope_hotspot_count`), and the
  `phases` key set are NEVER in the exclusion set (count semantics are transport-invariant).
- **`test_c4_each_excluded_field_justified`**: assert each of the 3 excluded fields carries an inline
  wall-clock/jitter justification in the comparator source.
- **`test_c4_non_empty_on_structural_fields`**: non-empty is checked on STRUCTURAL fields
  (`total_tool_calls > 0`, `session_count > 0`, `phases` populated), not on excluded ones — a believable `0`
  from a race cannot satisfy parity (#5265 gaze-width).

### R-03 — single-execution orchestration seam (correlation token)

- **`test_c4_https_vector_fresh_under_sandbox`**: assert the HTTPS-vector file is created FRESH under
  `$SANDBOX` (not a fixed path) and is absent/deleted at test start.
- **`test_c4_rejects_stale_correlation_token`**: the HTTPS vector carries a run-correlation token (the
  workload's stable session identity / run id); assert the comparator REJECTS a vector whose token ≠ this
  run's — a stale file from a prior tag CANNOT be ingested.
- **`test_c4_missing_or_failed_https_leg_errors`**: if the smoke shell-out exits non-zero OR the HTTPS-vector
  file is missing, the pytest test ERRORS (not skips, not compares against empty). Assert explicitly.

### R-06 — durability barrier predicate (the shared symmetric helper)

- **`test_c4_barrier_predicate_expected_observe_count`**: the barrier polls until the EXPECTED observe count
  (from the manifest's count of observe-firing tool calls) is present — bounded deadline (cap ~10 s), sleep
  1 between polls; NOT a flat sleep, NOT an immediate single read.
- **`test_c4_barrier_samples_dir_granularity`**: assert durability is sampled at the per-slug store DIR
  granularity (includes `-wal`), never `unimatrix.db` alone (#5265 takeaway 3) — or the review's own observe
  count once non-zero AND stable.
- **`test_c4_barrier_symmetric_single_helper`**: assert it is ONE shared helper parameterized by leg (not
  two hand-written waits) — asymmetry is forbidden by construction (mirrors ADR-001's one-workload rule).
- **`test_c4_barrier_timeout_hard_fails`**: deadline expiry → HARD fail ("observes not durable" +
  observed-vs-expected count + captured child stderr); NEVER proceed to review against a short/empty stream,
  NEVER compare an empty vector.

### R-09 — single workload driver owns the identity

- **`test_c4_one_driver_both_legs`**: assert the manifest is ONE driver consumed by both legs (the shell C2
  gate reads/replays it for HTTPS; the Python driver executes it for UDS) — not two parallel scripts.
- **`test_c4_manifest_stable_session_identity`**: assert the manifest declares ONE stable session identity
  used on both legs (FR-4) — the value is also the run-correlation token (R-03).

### AC-03 (manifest input) — derivation has real, valid input

- **`test_c4_manifest_bash_carries_valid_feature_id`**: assert the manifest's Bash content carries a
  parseable feature-ID token that IS a valid registry feature, so the `declared` branch resolves (FR-3 / R-07
  input). The literal `feature_cycle` is pinned in the manifest (single source of truth).
- **`test_c4_no_seed_site_reachable`** (AC-03 static, no-seed — the comparator/driver path): assert by
  construction (import/grep audit) that the C4 workload-driver + manifest + comparator path invokes NONE of
  the **three** forbidden seed sites: `_seed_observation_sql_lifecycle` (`suites/test_lifecycle.py:1253`,
  SQL rows), `_seed_attributed_observations_832` (`suites/test_lifecycle.py:4428`, the #832-specific
  attributed-observation seeder), and Rust `make_stamped_event(..., topic_signal)`
  (`uds/listener/tests/stamp_read.rs:28`, struct injection). The manifest seeds the workload INPUT (tool
  calls + Bash content), never the `topic_signal` OUTPUT — the column is DERIVED over the wire on both legs.
  No seed site is reachable from the C4 path (AC-03); `_seed_attributed_observations_832` is the #832-class
  injection this fixture (the #832 regression guard, SR-05/R-09) must NOT touch.

### AC-07 — sole net-new substantial module

- **`test_c4_is_only_substantial_net_new`**: assert C4 (`parity_workload.py` + comparator + barrier) is the
  ONLY substantial net-new module; C1/C2/C3/C5 are extensions of named parents.

---

## ⚠ First-Live-Run Field-by-Field Validation (AC-04 / NFR-8 / ADR-003 #5293)

C4 owns the machinery that makes the first-run gate executable. See OVERVIEW.md §4 for the full procedure.

- **`test_c4_emits_full_field_table_on_first_run`**: assert the comparator can emit BOTH parsed
  `MetricVector` dicts (HTTPS + UDS) to a `$SANDBOX` artifact keyed by the correlation token — the
  field-by-field evidence record across all 18 non-excluded `UniversalMetrics` fields + `phases` key set /
  per-phase `tool_call_count` + `domain_metrics`.
- **`test_c4_divergence_surfaced_loudly_with_field_name`**: assert ANY non-wall-clock field divergence is
  surfaced LOUDLY with the field name + both values + which leg — never auto-added to the exclusion set.
- **`test_c4_at_risk_fields_examined_first`**: assert the session-lifecycle fields
  (`cold_restart_events`, `coordinator_respawn_count`, `context_load_before_first_write_kb`,
  `total_context_loaded_kb`, `permission_friction_events`) are surfaced as the PRIME suspects for the
  first-run examination — a divergence on any is a PRODUCT/HUMAN disposition (defect → GH bug, OR
  transport-inherent → product-signed exclusion amendment recorded in ADR-003 via `context_correct`),
  **NEVER a silent widen by the implementer/tester** (R-01/R-02 failure mode).

> **Disposition authority is NOT a code path** — it is a process gate. The comparator's job is to surface
> the divergence with full evidence; product/human decides defect-vs-amendment. The tester records the
> disposition in RISK-COVERAGE-REPORT.md (Stage 3c), never edits the exclusion set to green a red.

---

## Edge cases

- A field carrying hidden sub-second wall-clock jitter NOT in the set → REAL failure (R-01), surfaced with
  the field name; dispositioned per the first-run gate, never auto-widened.
- A `domain_metrics` (schema-v14) key-set difference → participates in the compare; not silently dropped.
- Empty/short vector from a race → blocked by the barrier (R-06), never reaches the compare.

## Integration boundary

C4 is the spine: consumes `MetricVector(UDS)` (C3, in-process) + `MetricVector(HTTPS)` (C2, `$SANDBOX`
file, token-correlated), and owns the barrier helper both legs invoke. Its comparator is the operational
definition of C0 parity — hence the first-live-run human gate.
