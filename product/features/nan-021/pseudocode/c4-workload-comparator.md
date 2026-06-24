# C4 — Workload driver + comparator (Python)

**NEW module:** `harness/parity_workload.py` — the ONLY substantial net-new code (D-1). Owns: the single
declarative WORKLOAD manifest (single source of truth), the symmetric durability-barrier helper, the
`MetricVector` comparator (field-for-field modulo the named D-5 exclusion set), and the no-seed static
guard. Consumed by C3 (UDS leg) and C2 (HTTPS leg, via the JSON-serialized manifest). ≤500 lines; no
stubs/`todo!()`; logs via standard logging.

## Purpose

Be the single contract both legs import so parity is identical-by-construction (closes SR-05/R-09). Define
"what gets driven" once, "when observes are durable" once, and "what equality means" once.

## 1. The WORKLOAD manifest (shared type — see OVERVIEW for full shape)

```
WORKLOAD = ParityWorkload(
    session_id = "<one stable CC session identity>",        # threaded through declaration + ALL observes, both legs (FR-4)
    feature_cycle = "<literal VALID-registry feature-ID>",  # resolves `declared` branch (R-07)
    tool_calls = [ ToolCall(name, args, observe, response_size, response_snippet), ... ],
        #   EXACTLY ONE entry is the load-bearing Bash call; its response_snippet carries the
        #   feature-ID token parseable by extract_topic_signal (FR-3). e.g. content referencing
        #   "product/features/<feature_cycle>/..." or a `feature/<feature_cycle>` token.
    expected_observe_count = count(tc for tc in tool_calls if tc.observe),   # barrier predicate target
)
```
- Provide `to_json()` / a `parity_workload.json` artifact so the shell C2 leg reads the SAME bytes
  (cross-language single source of truth — OQ2). The shell leg must NOT hand-write a parallel script.
- `feature_cycle` MUST be a registered registry feature (assert registration before drive on both legs)
  so `enrich_topic_signal_with_source` resolves `declared`, not `vote`/`registry-fill`/`unattributed` (R-07).

## 2. Symmetric durability barrier (ADR-006 / FR-10) — ONE helper, parameterized by leg

```
durability_barrier(leg, expected, store_dir, *, deadline_s=10, poll_s=1):
    # SAME predicate/deadline/cadence on BOTH legs (symmetry is load-bearing — R-06 scenario 2)
    start = now()
    last_observed = None
    while now() - start <= deadline_s:
        observed = observe_count(store_dir)     # DIR-granularity read incl. -wal, NEVER unimatrix.db alone (#5265)
        last_observed = observed
        if observed >= expected:
            return                              # durable; caller proceeds to review
        sleep(poll_s)                           # bounded poll, NOT a flat sleep, NOT a single immediate read
    raise DurabilityTimeout(leg, observed=last_observed, expected=expected)   # HARD fail — never empty compare

observe_count(store_dir):
    # the EXPECTED-count predicate. Pick ONE single-sourced read (Stage 3b decision, flagged in C2):
    #   (a) review's own observe/tool-call count once non-zero AND stable across 2 polls, OR
    #   (b) per-slug store DIR size delta (du -s over the dir incl -wal/-shm) reaching a stable point.
    # Whichever is chosen, the SAME function is used by BOTH legs (no hand-duplicated predicate).
```
Timeout is a HARD failure surfaced with observed-vs-expected; the caller (C2/C3) NEVER reviews a short/
empty stream. Non-emptiness is asserted by the comparator AFTER the barrier, never before.

## 3. The MetricVector comparator (ADR-003 / D-5) — the parity contract

Operates on the parsed **dict** from `parse_tool_result(review_response).parsed` — NOT the Rust struct.

```
# Closed, enumerated exclusion set — named as a literal in-code (D-5). Exactly 3 wall-clock fields.
EXCLUDED = frozenset({
    "computed_at",                       # MetricVector.computed_at — wall-clock stamp
    "universal.total_duration_secs",     # sum of phase durations — sub-second jitter
    "phases.*.duration_secs",            # per-phase duration — sub-second jitter
})
# Inline justification REQUIRED beside each: all three are wall-clock/duration; NO count/ratio field is excludable.

UNIVERSAL_FIELDS = [ the 21 names from metrics.rs:45–89 ]   # explicit literal list — every field classified

compare_metric_vectors(mv_https, mv_uds):
    # ---- non-emptiness, asserted on BOTH vectors AFTER the barrier (AC-04) ----
    for mv,label in [(mv_https,"HTTPS"),(mv_uds,"UDS")]:
        assert mv["universal"]["total_tool_calls"] > 0,  f"{label} empty: total_tool_calls"
        assert mv["universal"]["session_count"]   > 0,  f"{label} empty: session_count"
        assert len(mv["phases"]) > 0,                   f"{label} empty: phases"

    diffs = []
    # ---- universal: compare all 21 fields EXCEPT total_duration_secs ----
    for f in UNIVERSAL_FIELDS:
        if f == "total_duration_secs": continue          # EXCLUDED
        a, b = mv_https["universal"][f], mv_uds["universal"][f]
        if a != b: diffs.append(("universal."+f, a, b))  # integers + ratios compare EXACTLY (identical workload)

    # ---- phases: KEY SET equal, per-phase tool_call_count equal, duration_secs EXCLUDED ----
    if set(mv_https["phases"]) != set(mv_uds["phases"]):
        diffs.append(("phases.keys", sorted(mv_https["phases"]), sorted(mv_uds["phases"])))
    for k in mv_https["phases"]:
        if k in mv_uds["phases"]:
            if mv_https["phases"][k]["tool_call_count"] != mv_uds["phases"][k]["tool_call_count"]:
                diffs.append((f"phases.{k}.tool_call_count", ...))
            # phases[k].duration_secs is EXCLUDED — not compared

    # ---- domain_metrics: key set + values equal ----
    if mv_https["domain_metrics"] != mv_uds["domain_metrics"]:
        diffs.append(("domain_metrics", mv_https["domain_metrics"], mv_uds["domain_metrics"]))

    # ---- verdict ----
    if diffs:
        # ANY field outside EXCLUDED that differs is a REAL failure — surfaced LOUD with name + both values.
        # NEVER silently add a diverging field to EXCLUDED (R-01/R-02/NFR-8). Disposition is a PRODUCT/HUMAN
        # call (GH bug OR product-signed ADR-003 amendment via context_correct) — not an implementer edit.
        raise ParityMismatch(diffs)
```

**First-live-run gate (ADR-003, not codeable — a delivery obligation):** the 3-field `EXCLUDED` set is a
LOAD-BEARING ASSUMPTION until the tester's first dual-transport run is examined field-by-field across all
18 non-excluded `UniversalMetrics` fields + the `phases` key set / `tool_call_count` and confirmed equal
ONCE. Prime transport-inherent suspects: `cold_restart_events`, `coordinator_respawn_count`,
`context_load_before_first_write_kb`, `total_context_loaded_kb`, `permission_friction_events`.

## 4. No-seed static guard (AC-03 / FR-6)

```
assert_no_seed_reachable():
    # by construction: the test path imports/invokes NONE of the three forbidden seed sites.
    FORBIDDEN_SEED_SITES = [
        "_seed_observation_sql_lifecycle",      # suites/test_lifecycle.py:1253 — SQL row injection
        "_seed_attributed_observations_832",    # #832-specific attributed-observation seeder
        "make_stamped_event",                   # Rust struct injection (..., topic_signal)
    ]
    audit the test module's imports/source for ANY of FORBIDDEN_SEED_SITES;
    fail if ANY is reachable from this test.
```

## Data flow

- IN: `parse_tool_result(review_response).parsed` dicts from C2 (HTTPS) and C3 (UDS).
- OUT: pass (parity proven, both non-empty) OR `ParityMismatch`/`DurabilityTimeout`/`AssertionError`.

## Error handling

- `DurabilityTimeout` → propagates as `pytest.fail` (R-06, never empty compare).
- `ParityMismatch` → loud, with field name(s) + both values; never auto-widens `EXCLUDED` (R-01/R-02).
- Missing/extra `UniversalMetrics` key vs the explicit literal list → fail (schema drift surfaced).

## Key test scenarios (hints for tester)

- Comparator-mutation (R-02): force a non-excluded field (e.g. drop one observe → `total_tool_calls` off)
  → comparator FAILS. Proves teeth on counts.
- Wall-clock exclusion (R-01): inject a 1s delay into one leg → comparator still PASSES (proves it truly
  excludes wall-clock, not coincidental match).
- Every one of the 21 `UniversalMetrics` fields + `PhaseMetrics.{duration_secs,tool_call_count}` is
  explicitly classified `deterministic` or `excluded`; zero out-of-set divergence across a ≥20x burst.
- `EXCLUDED` is minimal: contains ONLY the 3 wall-clock fields, each with an inline justification;
  `total_tool_calls`/`session_count`/`knowledge_entries_stored`/hotspot counts/`phases` keys NEVER excludable.
- Barrier symmetry: same helper drives both legs; asymmetric barrier is structurally impossible.
- No seed site reachable from the test path (static audit).
