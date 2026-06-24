"""C4 — MetricVector comparator (ADR-003 / D-5 / R-01 / R-02 / NFR-8) — nan-021.

Split out of `parity_workload.py` (≤500-line rule, single-responsibility): the
operational DEFINITION of C0 parity. Field-for-field equality MODULO the closed
3-field D-5 wall-clock exclusion set. Pure-Python over the parsed dict
(`parse_tool_result(review).parsed`) — never the Rust struct. Re-exported by
`parity_workload` for a single import surface. ZERO new runtime deps.

DISPOSITION AUTHORITY: any field OUTSIDE the closed exclusion set that differs is a
REAL failure surfaced LOUD; the implementer/tester NEVER silently widens the set —
disposition is a PRODUCT/HUMAN call (GH bug OR product-signed ADR-003 #5293 amendment
via context_correct).
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

# Closed, enumerated D-5 exclusion set — EXACTLY 3 wall-clock fields, named as a
# literal here. Each carries an inline wall-clock/jitter justification. NO count or
# ratio field is excludable: count semantics are transport-invariant and every ratio
# is num/den over an identical workload (no denominator is wall-clock).
EXCLUDED: frozenset[str] = frozenset(
    {
        # MetricVector.computed_at — absolute wall-clock stamp of the review call.
        "computed_at",
        # UniversalMetrics.total_duration_secs — sum of phase durations; sub-second
        # wall-clock jitter differs between two live transports.
        "universal.total_duration_secs",
        # PhaseMetrics.duration_secs (per phase) — per-phase wall-clock duration;
        # sub-second jitter, transport-inherent.
        "phases.*.duration_secs",
    }
)

# Inline justification for each excluded field — asserted present by the test plan
# (`test_c4_each_excluded_field_justified`). Keyed by the literal in EXCLUDED.
EXCLUSION_JUSTIFICATIONS: dict[str, str] = {
    "computed_at": "wall-clock stamp of the review call",
    "universal.total_duration_secs": "sum of phase durations — sub-second wall-clock jitter",
    "phases.*.duration_secs": "per-phase wall-clock duration — sub-second jitter",
}

# The 21 UniversalMetrics fields (metrics.rs:48-88), explicit literal so every field
# is CLASSIFIED — exactly one (`total_duration_secs`) is excluded; the other 20 are
# compared exactly. A key present in a vector but absent here (or vice versa) is
# schema drift and FAILS (not silently ignored).
UNIVERSAL_FIELDS: tuple[str, ...] = (
    "total_tool_calls",
    "total_duration_secs",  # EXCLUDED (wall-clock)
    "session_count",
    "search_miss_rate",
    "edit_bloat_total_kb",
    "edit_bloat_ratio",
    "permission_friction_events",
    "bash_for_search_count",
    "cold_restart_events",
    "coordinator_respawn_count",
    "parallel_call_rate",
    "context_load_before_first_write_kb",
    "total_context_loaded_kb",
    "post_completion_work_pct",
    "follow_up_issues_created",
    "knowledge_entries_stored",
    "sleep_workaround_count",
    "agent_hotspot_count",
    "friction_hotspot_count",
    "session_hotspot_count",
    "scope_hotspot_count",
)

# Session-lifecycle-derived fields — the PRIME suspects for a transport-inherent
# (not workload) difference. Surfaced first on the first-live-run examination; a
# divergence on any is a PRODUCT/HUMAN disposition (GH bug OR product-signed ADR-003
# amendment via context_correct), NEVER a silent widen by implementer/tester.
AT_RISK_FIELDS: tuple[str, ...] = (
    "cold_restart_events",
    "coordinator_respawn_count",
    "context_load_before_first_write_kb",
    "total_context_loaded_kb",
    "permission_friction_events",
)


class ParityMismatch(AssertionError):
    """≥1 field OUTSIDE the closed D-5 exclusion set differs between the two legs.

    A REAL failure surfaced LOUD with field name + both values + which leg. NEVER
    resolved by widening EXCLUDED (R-01/R-02/NFR-8) — disposition is a PRODUCT/HUMAN
    call: (a) a parity DEFECT → GH bug (gate stays RED), or (b) a transport-inherent
    field → exclusion-set amendment ONLY with product sign-off + recorded ADR-003
    rationale via context_correct. The implementer/tester does NOT decide."""

    def __init__(self, diffs: list[tuple[str, Any, Any]]):
        self.diffs = diffs
        lines = ["MetricVector parity mismatch (non-wall-clock fields differ):"]
        for fieldname, https_val, uds_val in diffs:
            flag = (
                " [AT-RISK session-lifecycle field]"
                if fieldname.replace("universal.", "") in AT_RISK_FIELDS
                else ""
            )
            lines.append(
                f"  {fieldname}: HTTPS={https_val!r} != UDS={uds_val!r}{flag}"
            )
        lines.append(
            "DISPOSITION (product/human): file a GH bug (defect) OR product-signed "
            "ADR-003 #5293 amendment via context_correct. NEVER silently widen EXCLUDED."
        )
        super().__init__("\n".join(lines))


def assert_non_empty(mv: dict, label: str) -> None:
    """Non-empty asserted on STRUCTURAL fields AFTER the barrier (AC-04). A believable
    `0` from a race (a never-excluded count/phase) cannot satisfy parity (#5265)."""
    universal = mv.get("universal", {})
    assert universal.get("total_tool_calls", 0) > 0, (
        f"{label} empty: universal.total_tool_calls must be > 0 (barrier failed?)"
    )
    assert universal.get("session_count", 0) > 0, (
        f"{label} empty: universal.session_count must be > 0"
    )
    assert len(mv.get("phases", {})) > 0, f"{label} empty: phases must be populated"


def compare_metric_vectors(
    mv_https: dict, mv_uds: dict
) -> list[tuple[str, Any, Any]]:
    """Field-for-field equality MODULO the closed 3-field D-5 exclusion set.

    Compares: the 20 non-excluded UniversalMetrics fields (incl. ratios — EXACT, no
    float tolerance), the `phases` KEY SET + per-phase `tool_call_count`, and
    `domain_metrics` (key set + values). Excludes ONLY `computed_at`,
    `universal.total_duration_secs`, and every `phases.*.duration_secs`.

    Asserts BOTH vectors non-empty AFTER the barrier first. Any field outside EXCLUDED
    that differs raises `ParityMismatch` (loud). Returns the (empty) diff list on parity
    so callers can also emit the field-by-field evidence record (first-live-run gate).
    """
    assert_non_empty(mv_https, "HTTPS")
    assert_non_empty(mv_uds, "UDS")

    diffs: list[tuple[str, Any, Any]] = []

    # ---- universal: all 21 fields EXCEPT total_duration_secs (every field classified) ----
    uni_https = mv_https.get("universal", {})
    uni_uds = mv_uds.get("universal", {})
    # Schema-drift guard: the parsed key sets must match our explicit literal list.
    for label, uni in (("HTTPS", uni_https), ("UDS", uni_uds)):
        unknown = set(uni) - set(UNIVERSAL_FIELDS)
        assert not unknown, f"{label} universal has unclassified field(s): {sorted(unknown)}"
    for f in UNIVERSAL_FIELDS:
        if f == "total_duration_secs":  # EXCLUDED (wall-clock)
            continue
        a, b = uni_https.get(f), uni_uds.get(f)
        if a != b:  # integers + ratios compare EXACTLY (identical workload)
            diffs.append(("universal." + f, a, b))

    # ---- phases: KEY SET equal, per-phase tool_call_count equal, duration_secs EXCLUDED ----
    ph_https = mv_https.get("phases", {})
    ph_uds = mv_uds.get("phases", {})
    if set(ph_https) != set(ph_uds):
        diffs.append(("phases.keys", sorted(ph_https), sorted(ph_uds)))
    for k in sorted(set(ph_https) & set(ph_uds)):
        a = ph_https[k].get("tool_call_count")
        b = ph_uds[k].get("tool_call_count")
        if a != b:
            diffs.append((f"phases.{k}.tool_call_count", a, b))
        # phases[k].duration_secs is EXCLUDED — not compared.

    # ---- domain_metrics: key set + values equal (schema-v14 extension participates) ----
    dm_https = mv_https.get("domain_metrics", {})
    dm_uds = mv_uds.get("domain_metrics", {})
    if dm_https != dm_uds:
        diffs.append(("domain_metrics", dm_https, dm_uds))

    if diffs:
        raise ParityMismatch(diffs)
    return diffs


def field_by_field_record(
    mv_https: dict, mv_uds: dict, *, run_token: str
) -> dict[str, Any]:
    """Emit BOTH parsed vectors + a per-field comparison table (first-live-run gate,
    ADR-003 #5293). The comparator EMITS the raw vectors, not just pass/fail, so the
    first-live-run field-by-field evidence record exists for product/human disposition.
    Keyed by the run-correlation token so a stale prior-tag artifact cannot masquerade."""
    rows: list[dict[str, Any]] = []
    uni_https = mv_https.get("universal", {})
    uni_uds = mv_uds.get("universal", {})
    for f in UNIVERSAL_FIELDS:
        excluded = f == "total_duration_secs"
        a, b = uni_https.get(f), uni_uds.get(f)
        rows.append(
            {
                "field": "universal." + f,
                "https": a,
                "uds": b,
                "equal": a == b,
                "excluded": excluded,
                "at_risk": f in AT_RISK_FIELDS,
            }
        )
    return {
        "run_token": run_token,
        "excluded_set": sorted(EXCLUDED),
        "at_risk_fields": list(AT_RISK_FIELDS),
        "universal_table": rows,
        "phases_https": mv_https.get("phases", {}),
        "phases_uds": mv_uds.get("phases", {}),
        "domain_metrics_https": mv_https.get("domain_metrics", {}),
        "domain_metrics_uds": mv_uds.get("domain_metrics", {}),
        "raw_https": mv_https,
        "raw_uds": mv_uds,
    }


def write_field_record(record: dict[str, Any], path: str | Path) -> Path:
    """Persist the first-live-run field-by-field evidence record under $SANDBOX."""
    p = Path(path)
    p.write_text(json.dumps(record, indent=2, sort_keys=True), encoding="utf-8")
    return p
