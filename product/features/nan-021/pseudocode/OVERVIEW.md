# nan-021 Pseudocode — OVERVIEW

> Per-component pseudocode for the HTTPS-Bridge Integration Fixture. Pure test-infrastructure,
> CUMULATIVE extension to `infra-001`. ZERO production-code diff (NFR-1). Every helper names the
> infra-001 asset it extends (SR-04). The ONLY substantial net-new module is C4 (`harness/parity_workload.py`).
> Authoritative source: ARCHITECTURE.md §Integration Surface + IMPLEMENTATION-BRIEF §Key Function Signatures.
> Signatures below are EXACT — do not invent APIs.

## Components

| File | Component | Owner substrate | Net-new? |
|------|-----------|-----------------|----------|
| c1-https-standup.md | C1 — HTTPS-leg standup (shell) | Docker smoke (extends Gates 1–7) | extends |
| c2-bridge-cycle.md | C2 — bridge-driven cycle (shell→node) | Docker smoke (NEW `cloud_cycle_gates` fn, reuses bridge JS as-is) | new fn, no new transport code |
| c3-uds-baseline.md | C3 — UDS-leg baseline (Python) | Python harness (extends uds/hook clients + conftest) | extends |
| c4-workload-comparator.md | C4 — workload driver + comparator (Python) | NEW `harness/parity_workload.py` | new (sole substantial module) |
| c5-gate-wiring.md | C5 — gate wiring (shell/YAML) | `release-gate-lib.sh` + release workflow | extends |

## Pytest-as-orchestrator seam (ADR-001, OQ3, resolves R-03)

ONE pytest invocation owns both legs in a single execution (D-6 live-vs-live):

```
pytest test (C3, new suite file)
  ├─ load WORKLOAD manifest from C4 (single source of truth)
  ├─ UDS leg (C3): drive manifest over UnimatrixUdsClient + UnimatrixHookClient
  │     → durability_barrier(UDS)  → context_cycle_review → MetricVector(UDS)  [parsed dict]
  ├─ HTTPS leg: subprocess.run the smoke's C2 cloud_cycle_gates gate, passing
  │     manifest path + a fresh per-run RUN_TOKEN + a fresh $SANDBOX HTTPS-vector out-path
  │     → smoke does C1 standup + C2 bridge cycle + durability_barrier(HTTPS) + review
  │     → smoke WRITES MetricVector(HTTPS)+RUN_TOKEN to the out-file
  │     ← pytest reads it back, asserts RUN_TOKEN == this run's (stale-file guard)
  └─ comparator (C4): MetricVector(HTTPS) ?= MetricVector(UDS) modulo D-5 exclusion set
```

Failure-mode contract (R-03): smoke exits non-zero OR out-file missing OR token mismatch
→ pytest **ERRORS** (never skips, never compares against empty/stale). The out-file path is
fresh under `$SANDBOX` per invocation and asserted absent at test start.

## Shared type — the WORKLOAD MANIFEST (single source of truth, C4-owned, ADR-001/OQ2)

Defined ONCE in `harness/parity_workload.py`; both legs replay it (no parallel scripts → closes
R-09 by construction). Serializable to JSON so the shell C2 leg reads the same bytes.

```
WORKLOAD = {
  "session_id":     <one stable CC session identity, str>   # threaded through declaration + ALL observes,
                                                            #   SAME value on both legs (#832 contract, FR-4/R-09)
  "feature_cycle":  <literal VALID-registry feature-ID, str># declared so enrich resolves `declared` branch (R-07)
  "cycle_start":    { tool:"context_cycle", args:{cycle_type:"start", topic:<feature_cycle>, ... } }
  "tool_calls":     [ ordered list, each: { name, args, observe:bool, response_size, response_snippet } ]
                    #   includes EXACTLY ONE load-bearing Bash call whose response_snippet carries the
                    #   feature-ID token parseable by extract_topic_signal (FR-3)
  "cycle_stop":     { tool:"context_cycle", args:{cycle_type:"stop", topic:<feature_cycle>, ... } }
  "expected_observe_count": <int>   # = count of tool_calls with observe==true; drives the durability barrier (FR-10)
}
```

`expected_observe_count` is the durability-barrier predicate target on BOTH legs. `session_id`
and `feature_cycle` are identical across legs. The Bash call's `response_snippet` is the derivation
input for `topic_signal` (must yield `declared`, not `unattributed` — R-07).

## MetricVector comparator contract (ADR-003, D-5, resolves R-01/R-02)

Read from the JSON text of `context_cycle_review(feature)` via `parse_tool_result(...).parsed`.
The comparator operates on the parsed **dict**, never the Rust struct.

```
MetricVector = { computed_at:u64, universal:UniversalMetrics(21 fields), phases:{name->{duration_secs,tool_call_count}}, domain_metrics:{str->f64} }
```

**D-5 EXCLUSION SET — exactly 3 wall-clock fields, named as a closed literal in the comparator:**
```
EXCLUDED = {
  "computed_at",                          # MetricVector.computed_at (wall-clock stamp)
  "universal.total_duration_secs",        # sum of phase durations (sub-second jitter)
  "phases.*.duration_secs",               # per-phase duration (sub-second jitter)
}
```
**Compared exactly (everything else):** all 20 remaining `UniversalMetrics` fields (incl. ratios —
identical workload ⇒ exact equality, NO float tolerance unless a future ratio's denominator is a
wall-clock duration; none currently is), the `phases` KEY SET + per-phase `tool_call_count`, and
`domain_metrics` key set + values.

**Unexpected-field policy:** any field OUTSIDE `EXCLUDED` that differs is a REAL failure surfaced
loudly with field name + both values — NEVER silently added to `EXCLUDED` (R-01/R-02 / NFR-8).
Disposition of a divergence is a **PRODUCT/HUMAN** call (file a GH bug OR product-signed ADR-003
amendment via `context_correct`) — never an implementer/tester edit. First-live-run gate: the
3-field set is a LOAD-BEARING ASSUMPTION until the first dual-transport run is examined
field-by-field across all 18 non-excluded `UniversalMetrics` fields + phases. Prime suspects flagged
by name: `cold_restart_events`, `coordinator_respawn_count`, `context_load_before_first_write_kb`,
`total_context_loaded_kb`, `permission_friction_events`.

## Symmetric durability barrier (ADR-006 / FR-10, resolves R-06)

ONE shared helper in C4, parameterized by leg; SAME predicate/deadline/cadence on BOTH legs.
Gates BOTH `context_cycle_review` calls. Bound ~10s, sleep 1, DIR-granularity read incl. `-wal`,
never `unimatrix.db` alone. Timeout = HARD fail ("observes not durable", observed-vs-expected) —
never an empty compare. Non-emptiness asserted only AFTER the barrier.

## Sequencing constraints (build order for Stage 3b)

1. **C4 first** — the manifest type + comparator + barrier helper are the contract both legs import.
2. **C3** (UDS leg) and **C2** (shell HTTPS gate) consume C4's manifest in parallel after C4.
3. **C1** is reused-verbatim Gates 1–7; C2's `cloud_cycle_gates` runs after them.
4. **C5** wraps the whole thing; it depends only on the smoke emitting the marker + exit codes —
   stub-drivable pre-merge via `SMOKE_*_CMD` seams independently of C1–C4 (R-12).

## Cross-cutting (all components)

- **Capture-first stderr (ADR-005, #5266):** every child (`mcp-bridge.js`, `init`, container)
  → stderr to a `$SANDBOX` file, tail-dumped on FAILURE only. ONE exception: `emit_bundle` stays
  suppressed (its blob carries the bearer — R-13/security).
- **Readiness is event-driven, never `sleep`** (SR-01): log line / file present / session-id captured.
- **Hermeticity:** `init --bundle` writes credstore ONLY under `$HOME=$SANDBOX/home`, fresh per run (R-14).
- **No seed sites reachable** (AC-03): the test path imports/invokes NONE of the three forbidden seed
  sites — `_seed_observation_sql_lifecycle`, `_seed_attributed_observations_832`, and the Rust
  `make_stamped_event(..., topic_signal)`. Static-asserted in C3/C4.

## Open questions / gaps flagged

- **OQ1/R-11 (projectHash read-back):** resolved in C2 — `projectHash` is READ BACK by listing the
  single dir under `$SANDBOX/home/.unimatrix/` after `consume_bundle`; NO hashing primitive in the
  fixture. If a future `init.js` change stops writing exactly one dir there, C2's read-back assert fails loud.
- **First-live-run gate (ADR-003):** the comparator is wired but the 3-field exclusion set is NOT
  TRUSTED until the tester's first-live-run field-by-field confirmation passes once. This is a
  delivery-gate obligation on the tester/leader, surfaced in C4's test scenarios — not codeable here.
