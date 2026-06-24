# C3 — UDS-Leg Baseline (Python) — Test Plan

> **Component:** drive the IDENTICAL workload over `UnimatrixUdsClient` (MCP/UDS) + `UnimatrixHookClient`
> (hook IPC) against a local-UDS `serve` daemon, in the SAME test execution as the HTTPS leg, to produce
> the live-vs-live parity baseline. **Extends** `harness/uds_client.py`, `harness/hook_client.py`,
> `conftest.py` fixtures. **ACs:** AC-04 (the parity baseline), AC-03 (derived attribution on this leg),
> AC-07 (extends, no fork). **Risks:** R-06 (symmetric barrier), R-09 (same identity), R-03 (same execution).

---

## Test Expectations

### FR-5 / NFR-4 / R-09 — identical workload, single driver, same stable identity

- **`test_c3_drives_same_manifest_object`**: assert the UDS leg drives the SAME C4 workload manifest object
  (same tool calls, same order, same Bash content, same declared session identity) — NOT a hand-written
  parallel script. Divergent identity is structurally impossible if there is a single source of truth.
- **`test_c3_same_session_identity_as_https`**: assert the declared CC session identity on the UDS leg is
  the SAME value the HTTPS leg (C2) uses (FR-4/SR-05). One identity threaded through declaration + all
  observes on this leg too.
- **`test_c3_uses_existing_clients_not_fork`** (AC-07): assert the leg uses the EXISTING
  `UnimatrixUdsClient` (`connect()`/`disconnect()`, `context_cycle()`, `context_cycle_review()`) and
  `UnimatrixHookClient` (`post_tool_use`, `pre_tool_use`, `session_start`, `session_stop`) — no net-new
  UDS spawn / hook-IPC path. Extends `conftest.py` fixtures (`server` family).

### AC-03 — derived `topic_signal == feature` on the UDS leg too (R-07)

- **`test_c3_topic_signal_equals_feature_exact`**: assert `topic_signal == feature` EXACTLY for every UDS
  observation, derived by `extract_topic_signal → enrich_topic_signal_with_source` from the observed Bash
  content (same load-bearing feature-ID token as HTTPS). `unattributed` is a HARD fail.
- **`test_c3_no_seed_site_reachable`** (AC-03 static, no-seed): assert by construction (import/grep audit)
  that the UDS-leg setup + assertion path invokes NONE of the **three** forbidden seed sites:
  `_seed_observation_sql_lifecycle` (`suites/test_lifecycle.py:1253`, SQL rows), `_seed_attributed_observations_832`
  (`suites/test_lifecycle.py:4428`, the #832-specific attributed-observation seeder), and Rust
  `make_stamped_event(..., topic_signal)` (`uds/listener/tests/stamp_read.rs:28`, struct injection).
  **No seed site is reachable from this test** — the forbidden seed sites must be unreachable (AC-03). The
  UDS leg is the leg where the SQL seed helpers live in a SIBLING suite, so this audit is especially
  load-bearing here; `_seed_attributed_observations_832` is precisely the #832-class injection the derived
  path (this fixture IS the #832 regression guard, SR-05/R-09) must NOT touch.

### R-06 — symmetric durability barrier on the UDS leg

- **`test_c3_uds_barrier_before_review`**: the SAME C4 durability-barrier helper runs on the UDS leg AFTER
  `context_cycle(stop)` and BEFORE `context_cycle_review` — SAME predicate, deadline, cadence as the HTTPS
  leg. The UDS observe path is ALSO async fire-and-forget; an asymmetric barrier (one leg waits, the other
  reads immediately) SELF-INDUCES parity divergence. Symmetry is load-bearing — assert the helper is the
  shared one, parameterized only by leg, not a hand-written UDS-only wait.
- **`test_c3_uds_review_non_empty_after_barrier`**: assert the UDS `MetricVector` is non-empty
  (`total_tool_calls > 0`, `session_count > 0`, `phases` populated) only AFTER the barrier passes.

### R-03 — same-execution baseline (the live-vs-live invariant)

- **`test_c3_runs_in_same_pytest_invocation`**: the UDS leg runs in the SAME pytest invocation that shells
  out to the HTTPS C2 gate (pytest-as-orchestrator, ADR-001). The UDS `MetricVector` and the ingested HTTPS
  `MetricVector` are both from THIS run — live-vs-live (D-6), NOT a captured golden, NOT a prior run.

---

## Edge cases

- Empty/short UDS `MetricVector` from an observe race → the durability barrier (R-06) gates the review;
  timeout HARD-fails, never an empty compare.
- A UDS-leg `unattributed` while HTTPS resolves `feature` (or vice-versa) → surfaces as a parity divergence
  on the attribution-dependent counts AND a per-leg AC-03 hard fail.

## Integration boundary

C3 produces `MetricVector(UDS)` directly in-process (Python) and hands it to the C4 comparator alongside the
ingested `MetricVector(HTTPS)` from C2's `$SANDBOX` file. The barrier helper and the workload driver are
C4-owned; C3 is the consumer that exercises the existing UDS clients.
