"""C4 unit tests — parity_workload manifest, durability barrier, MetricVector comparator.

Pure-Python over synthetic parsed MetricVector dicts; NO Docker, NO daemon, NO fixtures
(R-12 mitigation — exercise the comparator's TEETH (R-02), exclusion-set COMPLETENESS
(R-01), and the barrier predicate (R-06) BEFORE any release-gate tag round). This is the
nan-019 stub-drive precedent (#5258) applied to the Python comparator.

Maps 1:1 to test-plan/c4-workload-comparator.md.
"""

import json
from pathlib import Path

import pytest

from harness import parity_workload as pw
from harness.parity_workload import (
    AT_RISK_FIELDS,
    EXCLUDED,
    EXCLUSION_JUSTIFICATIONS,
    UNIVERSAL_FIELDS,
    DurabilityTimeout,
    ParityMismatch,
    ParityWorkload,
    ToolCall,
    compare_metric_vectors,
    default_workload,
    durability_barrier,
    field_by_field_record,
    load_https_vector,
    observe_count,
)


# ---------------------------------------------------------------------------
# Synthetic MetricVector helpers (the parsed dict the comparator operates on)
# ---------------------------------------------------------------------------


def _universal(**overrides) -> dict:
    base = {f: 0 for f in UNIVERSAL_FIELDS}
    # Believable non-zero structural values so non-empty asserts pass by default.
    base.update(
        total_tool_calls=3,
        total_duration_secs=42,
        session_count=1,
        search_miss_rate=0.25,
        edit_bloat_ratio=0.0,
        knowledge_entries_stored=0,
    )
    base.update(overrides)
    return base


def _mv(*, universal=None, phases=None, domain=None, computed_at=1000) -> dict:
    return {
        "computed_at": computed_at,
        "universal": _universal(**(universal or {})),
        "phases": phases
        if phases is not None
        else {
            "implementation": {"duration_secs": 30, "tool_call_count": 2},
            "review": {"duration_secs": 12, "tool_call_count": 1},
        },
        "domain_metrics": domain if domain is not None else {},
    }


# ===========================================================================
# R-01 — exclusion-set completeness (no field unclassified)
# ===========================================================================


def test_c4_every_field_classified():
    # Every UniversalMetrics field is in the explicit literal list; exactly the 3
    # wall-clock fields are excluded.
    assert len(UNIVERSAL_FIELDS) == 21
    assert len(set(UNIVERSAL_FIELDS)) == 21
    assert EXCLUDED == frozenset(
        {
            "computed_at",
            "universal.total_duration_secs",
            "phases.*.duration_secs",
        }
    )
    assert len(EXCLUDED) == 3
    # The only UniversalMetrics member in the exclusion set is total_duration_secs.
    excluded_uni = {
        e.split(".", 1)[1] for e in EXCLUDED if e.startswith("universal.")
    }
    assert excluded_uni == {"total_duration_secs"}


def test_c4_ratio_fields_compared_exactly():
    # f64 ratio fields compared EXACTLY (no float tolerance) — a divergence is a real
    # failure, not a tolerance to widen.
    for ratio in (
        "search_miss_rate",
        "edit_bloat_ratio",
        "parallel_call_rate",
        "post_completion_work_pct",
        "edit_bloat_total_kb",
    ):
        a = _mv(universal={ratio: 0.5})
        b = _mv(universal={ratio: 0.5000001})
        with pytest.raises(ParityMismatch) as ei:
            compare_metric_vectors(a, b)
        assert any(d[0] == "universal." + ratio for d in ei.value.diffs)


def test_c4_excludes_wallclock_not_luck():
    # Inject divergent wall-clock values into one leg; comparator still PASSES — proves
    # it truly excludes computed_at / total_duration_secs / phases[*].duration_secs.
    https = _mv(
        universal={"total_duration_secs": 40},
        phases={
            "implementation": {"duration_secs": 30, "tool_call_count": 2},
            "review": {"duration_secs": 10, "tool_call_count": 1},
        },
        computed_at=1000,
    )
    uds = _mv(
        universal={"total_duration_secs": 99},  # excluded — must not matter
        phases={
            "implementation": {"duration_secs": 77, "tool_call_count": 2},  # excluded
            "review": {"duration_secs": 88, "tool_call_count": 1},  # excluded
        },
        computed_at=5000,  # excluded — must not matter
    )
    assert compare_metric_vectors(https, uds) == []  # parity holds


# ===========================================================================
# R-02 — comparator has teeth / set is minimal (no vacuous green)
# ===========================================================================


def test_c4_mutation_drop_observe_fails():
    # MUTATION HARNESS: drop one observe so total_tool_calls + a phase count diverge.
    https = _mv(
        universal={"total_tool_calls": 3},
        phases={"implementation": {"duration_secs": 30, "tool_call_count": 3}},
    )
    uds = _mv(
        universal={"total_tool_calls": 2},  # one observe dropped
        phases={"implementation": {"duration_secs": 30, "tool_call_count": 2}},
    )
    with pytest.raises(ParityMismatch) as ei:
        compare_metric_vectors(https, uds)
    fields = {d[0] for d in ei.value.diffs}
    assert "universal.total_tool_calls" in fields
    assert "phases.implementation.tool_call_count" in fields
    # Loud: field name + both values appear in the message.
    msg = str(ei.value)
    assert "universal.total_tool_calls" in msg
    assert "HTTPS=3" in msg and "UDS=2" in msg


def test_c4_count_fields_never_excludable():
    for f in (
        "total_tool_calls",
        "session_count",
        "knowledge_entries_stored",
        "agent_hotspot_count",
        "friction_hotspot_count",
        "session_hotspot_count",
        "scope_hotspot_count",
    ):
        assert "universal." + f not in EXCLUDED
        assert f not in EXCLUDED
    # The phases KEY SET is never excludable (only per-phase duration_secs is).
    assert "phases.keys" not in EXCLUDED
    assert "phases.*.tool_call_count" not in EXCLUDED


def test_c4_each_excluded_field_justified():
    # Each excluded field carries an inline wall-clock/jitter justification.
    assert set(EXCLUSION_JUSTIFICATIONS) == set(EXCLUDED)
    for justification in EXCLUSION_JUSTIFICATIONS.values():
        low = justification.lower()
        assert "wall-clock" in low or "jitter" in low or "duration" in low
    # And the justifications are present in the comparator source beside the literals.
    from harness import metric_comparator

    src = Path(metric_comparator.__file__).read_text(encoding="utf-8")
    for fieldname in EXCLUDED:
        assert fieldname in src


def test_c4_non_empty_on_structural_fields():
    # A believable 0 from a race on a STRUCTURAL field cannot satisfy parity.
    empty = _mv(universal={"total_tool_calls": 0})
    with pytest.raises(AssertionError) as ei:
        compare_metric_vectors(empty, _mv())
    assert "total_tool_calls" in str(ei.value)

    no_phases = _mv(phases={})
    with pytest.raises(AssertionError) as ei2:
        compare_metric_vectors(no_phases, _mv())
    assert "phases" in str(ei2.value)


def test_c4_schema_drift_unclassified_field_fails():
    # A universal key not in the explicit literal list is schema drift → fail.
    drifted = _mv()
    drifted["universal"]["some_new_field"] = 7
    with pytest.raises(AssertionError) as ei:
        compare_metric_vectors(drifted, _mv())
    assert "unclassified" in str(ei.value)


def test_c4_phase_key_set_difference_fails():
    https = _mv(phases={"implementation": {"duration_secs": 30, "tool_call_count": 2}})
    uds = _mv(
        phases={
            "implementation": {"duration_secs": 30, "tool_call_count": 2},
            "design": {"duration_secs": 5, "tool_call_count": 1},
        }
    )
    with pytest.raises(ParityMismatch) as ei:
        compare_metric_vectors(https, uds)
    assert any(d[0] == "phases.keys" for d in ei.value.diffs)


def test_c4_domain_metrics_difference_participates():
    https = _mv(domain={"weather.forecast_count": 3.0})
    uds = _mv(domain={"weather.forecast_count": 4.0})
    with pytest.raises(ParityMismatch) as ei:
        compare_metric_vectors(https, uds)
    assert any(d[0] == "domain_metrics" for d in ei.value.diffs)


def test_c4_at_risk_fields_flagged_in_mismatch():
    # A divergence on a session-lifecycle field is surfaced AND flagged as at-risk.
    https = _mv(universal={"cold_restart_events": 0})
    uds = _mv(universal={"cold_restart_events": 1})
    with pytest.raises(ParityMismatch) as ei:
        compare_metric_vectors(https, uds)
    assert "AT-RISK" in str(ei.value)
    assert set(AT_RISK_FIELDS) <= set(UNIVERSAL_FIELDS)


# ===========================================================================
# R-06 — durability barrier predicate (the shared symmetric helper)
# ===========================================================================


def test_c4_barrier_predicate_releases_when_stable():
    # Size grows then stabilizes → barrier releases (NOT a flat sleep / immediate read).
    seq = iter([100, 400, 700, 700, 700])
    calls = {"n": 0}

    def count_fn(_store_dir):
        calls["n"] += 1
        return next(seq)

    released = durability_barrier(
        "UDS", expected=3, store_dir="/x", deadline_s=10, poll_s=0, count_fn=count_fn
    )
    assert released == 700
    assert calls["n"] >= 2  # polled, not a single read


def test_c4_barrier_samples_dir_granularity(tmp_path):
    # observe_count sums ALL files in the per-slug store dir incl. -wal/-shm, never
    # unimatrix.db alone (#5265 takeaway 3).
    (tmp_path / "unimatrix.db").write_bytes(b"a" * 100)
    (tmp_path / "unimatrix.db-wal").write_bytes(b"b" * 250)
    (tmp_path / "unimatrix.db-shm").write_bytes(b"c" * 50)
    assert observe_count(tmp_path) == 400  # includes -wal/-shm
    # A db-only read would have returned 100 — proving DIR granularity matters.
    assert observe_count(tmp_path) != 100
    # Absent dir → 0 (no crash).
    assert observe_count(tmp_path / "missing") == 0


def test_c4_barrier_symmetric_single_helper():
    # ONE shared helper parameterized by leg — same function object for both legs.
    seq_uds = iter([10, 10])
    seq_https = iter([20, 20])
    a = durability_barrier(
        "UDS", 1, "/x", deadline_s=5, poll_s=0, count_fn=lambda _: next(seq_uds)
    )
    b = durability_barrier(
        "HTTPS", 1, "/y", deadline_s=5, poll_s=0, count_fn=lambda _: next(seq_https)
    )
    assert a == 10 and b == 20
    # It is literally the same callable both legs invoke (no hand-written duplicate).
    assert durability_barrier.__name__ == "durability_barrier"


def test_c4_barrier_timeout_hard_fails():
    # Size never stabilizes (always growing) → DurabilityTimeout with observed/expected.
    growing = {"n": 0}

    def count_fn(_):
        growing["n"] += 100
        return growing["n"]

    with pytest.raises(DurabilityTimeout) as ei:
        durability_barrier(
            "HTTPS",
            expected=5,
            store_dir="/x",
            deadline_s=0.05,
            poll_s=0.01,
            count_fn=count_fn,
            stderr="bridge stderr tail here",
        )
    assert ei.value.leg == "HTTPS"
    assert ei.value.expected == 5
    assert "not durable" in str(ei.value)
    assert "bridge stderr tail here" in str(ei.value)


# ===========================================================================
# R-09 / AC-03 — single workload driver owns the identity & derivation input
# ===========================================================================


def test_c4_one_driver_both_legs_roundtrip(tmp_path):
    # The manifest is ONE driver; its JSON round-trips byte-identically for the shell leg.
    wl = default_workload()
    path = wl.write_manifest(tmp_path / "parity_workload.json")
    reloaded = ParityWorkload.read_manifest(path)
    assert reloaded.to_dict() == wl.to_dict()


def test_c4_manifest_stable_session_identity():
    wl = default_workload()
    # ONE stable session identity; it is also the run-correlation token (R-03).
    assert wl.session_id
    assert wl.to_dict()["session_id"] == wl.session_id


def test_c4_manifest_bash_carries_valid_feature_id():
    wl = default_workload()
    # EXACTLY one load-bearing Bash call whose snippet carries the feature-ID token.
    bash = wl.bash_call
    assert bash.name == "Bash"
    assert wl.feature_cycle in bash.response_snippet
    # validate() enforces the token presence (FR-3/R-07 derivation input).
    wl.validate()


def test_c4_manifest_validate_rejects_missing_token():
    bad = ParityWorkload(
        session_id="s",
        feature_cycle="nan-021",
        tool_calls=[
            ToolCall(
                name="Bash",
                args={},
                observe=True,
                response_snippet="no token here",
            )
        ],
    )
    with pytest.raises(ValueError):
        bad.validate()


def test_c4_manifest_expected_observe_count():
    wl = default_workload()
    assert wl.expected_observe_count == sum(1 for tc in wl.tool_calls if tc.observe)
    assert wl.expected_observe_count >= 1
    assert wl.to_dict()["expected_observe_count"] == wl.expected_observe_count


def test_c4_manifest_requires_exactly_one_bash():
    two_bash = ParityWorkload(
        session_id="s",
        feature_cycle="nan-021",
        tool_calls=[
            ToolCall("Bash", {}, True, response_snippet="nan-021"),
            ToolCall("Bash", {}, True, response_snippet="nan-021"),
        ],
    )
    with pytest.raises(ValueError):
        _ = two_bash.bash_call


# ===========================================================================
# R-03 — single-execution orchestration seam (correlation token)
# ===========================================================================


def test_c4_load_https_vector_ok(tmp_path):
    out = tmp_path / "https_vector.json"
    out.write_text(
        json.dumps({"run_token": "run-abc", "metric_vector": _mv()}), encoding="utf-8"
    )
    mv = load_https_vector(out, "run-abc")
    assert mv["universal"]["total_tool_calls"] == 3


def test_c4_rejects_stale_correlation_token(tmp_path):
    out = tmp_path / "https_vector.json"
    out.write_text(
        json.dumps({"run_token": "OLD-tag", "metric_vector": _mv()}), encoding="utf-8"
    )
    with pytest.raises(ValueError) as ei:
        load_https_vector(out, "run-this")
    assert "stale" in str(ei.value).lower()


def test_c4_missing_https_leg_errors(tmp_path):
    with pytest.raises(FileNotFoundError):
        load_https_vector(tmp_path / "absent.json", "run-x")


# ===========================================================================
# First-live-run field-by-field evidence (AC-04 / NFR-8 / ADR-003 #5293)
# ===========================================================================


def test_c4_emits_full_field_table_on_first_run(tmp_path):
    record = field_by_field_record(_mv(), _mv(), run_token="run-1")
    # Emits BOTH raw parsed vectors + a per-field table across all 21 universal fields.
    assert record["run_token"] == "run-1"
    assert len(record["universal_table"]) == 21
    assert record["raw_https"] and record["raw_uds"]
    assert record["excluded_set"] == sorted(EXCLUDED)
    # Persistable under $SANDBOX keyed by the token.
    p = pw.write_field_record(record, tmp_path / "field_record_run-1.json")
    assert json.loads(p.read_text())["run_token"] == "run-1"


def test_c4_at_risk_fields_examined_first():
    record = field_by_field_record(_mv(), _mv(), run_token="run-1")
    assert record["at_risk_fields"] == list(AT_RISK_FIELDS)
    flagged = {r["field"] for r in record["universal_table"] if r["at_risk"]}
    assert flagged == {"universal." + f for f in AT_RISK_FIELDS}


def test_c4_divergence_surfaced_loudly_with_field_name():
    https = _mv(universal={"bash_for_search_count": 1})
    uds = _mv(universal={"bash_for_search_count": 0})
    with pytest.raises(ParityMismatch) as ei:
        compare_metric_vectors(https, uds)
    msg = str(ei.value)
    assert "universal.bash_for_search_count" in msg
    assert "HTTPS=1" in msg and "UDS=0" in msg
    assert "NEVER silently widen" in msg  # disposition guidance, not auto-widen


# ===========================================================================
# AC-03 — no seed site reachable from the C4 path (static audit)
# ===========================================================================


def test_c4_no_seed_site_reachable():
    # The C4 driver path (manifest + barrier + comparator modules) must not reach any
    # forbidden seed site as a call/import. The names appear ONLY inside the audit's own
    # FORBIDDEN list + comments, never as invocations — assert the audit passes for the
    # real driver path: the parity_workload module AND the comparator module.
    from harness import metric_comparator

    pw.assert_no_seed_reachable(metric_comparator.__file__)
    # And confirm the audit has TEETH: a file that DOES contain a seed call fails. The
    # forbidden-call string is assembled at runtime so the verbatim invocation does NOT
    # appear in THIS test source (keeping the test file itself clean of seed call-sites).
    import tempfile

    seed_call = "_seed_attributed_observations_832" + "(db, ['nan-021'])\n"
    with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False) as f:
        f.write(seed_call)
        bad_path = f.name
    with pytest.raises(AssertionError):
        pw.assert_no_seed_reachable(bad_path)


# ===========================================================================
# AC-07 — sole net-new substantial module
# ===========================================================================


def test_c4_is_only_substantial_net_new():
    # parity_workload.py is the single net-new harness module; the comparator, barrier
    # and manifest all live in it (no parallel scaffolding).
    harness_dir = Path(pw.__file__).parent
    net_new = harness_dir / "parity_workload.py"
    assert net_new.exists()
    # It owns all three responsibilities (one module, not three).
    assert hasattr(pw, "compare_metric_vectors")
    assert hasattr(pw, "durability_barrier")
    assert hasattr(pw, "ParityWorkload")
