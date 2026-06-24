# C3 — UDS-leg baseline (Python)

**Extends:** `harness/uds_client.py` (`UnimatrixUdsClient`), `harness/hook_client.py`
(`UnimatrixHookClient`), `harness/conftest.py` fixtures (`server` and friends), `harness/assertions.py`
(`parse_tool_result`). The new pytest test file is the **orchestrator** (ADR-001): it drives the UDS leg
AND shells out to the smoke's C2 gate, then runs the C4 comparator — all in ONE execution (D-6).
**Net-new code:** a new suite/test module under `harness/` (or `suites/`); NO new client/transport code.

## Purpose

Drive the IDENTICAL WORKLOAD manifest (C4) over the local-UDS daemon to produce the parity baseline
`MetricVector(UDS)`, in the SAME pytest run that drives the HTTPS leg. Satisfies FR-5, and as orchestrator
satisfies the single-execution half of AC-04. No seed site reachable (AC-03).

## Fixtures / initialization

- Reuse an existing conftest fixture (`server` / `shared_server`) that yields a `UnimatrixClient` over a
  `serve` UDS daemon; obtain the UDS socket path + the per-slug store DIR from it (extend the fixture if
  it does not already surface the store DIR — name the parent fixture, do not fork).
- `uds = UnimatrixUdsClient(socket_path, timeout=...)`; `uds.connect()`.
- `hooks = UnimatrixHookClient(socket_path, timeout=...)`.
- Register/declare the SAME `feature_cycle` (valid registry feature) the manifest pins, so attribution
  resolves the `declared` branch on this leg too (R-07).

## Function: drive_uds_leg(uds, hooks, WORKLOAD, store_dir) -> dict (parsed MetricVector)

```
drive_uds_leg(uds, hooks, WORKLOAD, store_dir):
    sid = WORKLOAD.session_id                                 # SAME stable identity as HTTPS leg (FR-4/R-09)
    uds.context_cycle(cycle_type="start", topic=WORKLOAD.feature_cycle, agent_id=..., ...)  # client.py:679 surface
    for call in WORKLOAD.tool_calls (in order):
        # execute the tool call's effect via the MCP/UDS client as the manifest specifies
        uds.<call.name>(**call.args)            # incl. the load-bearing Bash-equivalent call (FR-3 content)
        if call.observe:
            hooks.post_tool_use(session_id=sid, tool=call.name,
                                response_size=call.response_size,
                                response_snippet=call.response_snippet, ...)   # hook_client.py:108 surface
    uds.context_cycle(cycle_type="stop", topic=WORKLOAD.feature_cycle, ...)

    # SYMMETRIC durability barrier — SAME C4 helper, SAME predicate/deadline as HTTPS leg (FR-10/ADR-006)
    durability_barrier(leg="UDS", expected=WORKLOAD.expected_observe_count, store_dir=store_dir)
    # timeout -> pytest.fail("observes not durable: observed=.. expected=..") — never empty compare

    resp = uds.context_cycle_review(WORKLOAD.feature_cycle, ...)     # client.py:658 surface
    result = parse_tool_result(resp)                                 # assertions.py
    assert not result.is_error
    return result.parsed["metric_vector_or_RetrospectiveReport.metrics path"]   # the MetricVector dict
```

## Orchestrator function: test_https_uds_parity (the single pytest entrypoint)

```
test_https_uds_parity(server fixture):
    WORKLOAD   = parity_workload.WORKLOAD                 # C4 single source of truth
    store_dir  = server.slug_store_dir
    RUN_TOKEN  = WORKLOAD.session_id                      # correlation token (R-03)
    https_out  = fresh path under $SANDBOX                # assert ABSENT at start (stale-file guard)
    assert not exists(https_out)

    # ---- UDS leg (this process) ----
    mv_uds = drive_uds_leg(uds, hooks, WORKLOAD, store_dir)

    # ---- HTTPS leg (shell-out to the smoke's C1+C2) ----
    rc = subprocess.run(docker-http-posture-smoke.sh,
                        env={MANIFEST_PATH, RUN_TOKEN, HTTPS_VECTOR_OUT=https_out, IMAGE, ...},
                        capture stderr)
    if rc != 0:           pytest.fail("HTTPS smoke leg failed rc=%d\n%s" % (rc, captured_stderr))   # ERROR not skip
    if not exists(https_out): pytest.fail("HTTPS vector file missing — not live-vs-live")           # R-03
    payload = json.load(https_out)
    if payload["run_token"] != RUN_TOKEN: pytest.fail("stale HTTPS vector — token mismatch")         # R-03 stale guard
    mv_https = parse the payload["metric_vector"] JSON

    # ---- derived-attribution assertion (AC-03) ----
    assert_derived_attribution(WORKLOAD.feature_cycle, store_dir)   # topic_signal == feature, NOT unattributed

    # ---- comparator (C4) ----
    compare_metric_vectors(mv_https, mv_uds)        # field-for-field modulo D-5 EXCLUDED; non-empty on both
```

## Derived-attribution assertion (AC-03 / FR-6 / R-07, derived not seeded)

```
assert_derived_attribution(feature, store_dir):
    rows = read topic_signal for the driven observations (via review output or store read)
    assert every driven observation's topic_signal == feature   (string-exact; `unattributed` is HARD FAIL — near-miss guard)
```

## No-seed static guard (AC-03 / FR-6 — by construction)

The test module MUST NOT import or invoke ANY of the three forbidden seed sites:
`_seed_observation_sql_lifecycle` (`suites/test_lifecycle.py:1253`),
`_seed_attributed_observations_832` (the #832-specific attributed-observation seeder), nor the Rust
`make_stamped_event(..., topic_signal)`. C4 owns a static-audit assertion (grep/import check) that no
seed site is reachable from this test path (see C4).

## Error handling

- UDS connect/cycle/review failure → `pytest.fail` with the captured tool error text (`result.is_error`).
- HTTPS shell-out non-zero / missing file / token mismatch → `pytest.fail` (ERROR, never skip/empty — R-03).
- Barrier timeout (either leg) → `pytest.fail("observes not durable")` — never compares an empty vector (R-06).
- `topic_signal == unattributed` → `pytest.fail` (near-miss guard, R-07).

## Key test scenarios (hints for tester)

- FR-5/R-09: SAME `session_id` threaded through UDS declaration + every observe; SAME value as HTTPS leg.
- AC-03: derived `topic_signal == feature` exactly; `unattributed` hard-fails; no seed site reachable.
- R-03: one pytest process owns both legs; HTTPS vector is from THIS invocation (token checked); missing/
  stale HTTPS vector ERRORS, never compares empty.
- FR-10: UDS barrier uses the SAME C4 helper/predicate/deadline as the HTTPS leg (symmetry).
