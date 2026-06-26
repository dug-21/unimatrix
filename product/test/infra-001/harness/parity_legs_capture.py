"""C3' — per-dimension capture helpers for the UDS leg (nan-022 / #837 / ADR-005).

Split out of `parity_legs.py` (≤500-line / single-responsibility rule, the nan-021
`metric_comparator` lib-split precedent): the per-dimension CAPTURE logic the bundle
driver (`drive_uds_leg` in `parity_legs.py`) composes. Each helper captures ONE
dimension's output on the UDS leg over the dimension's correct WIRE SURFACE
(`UnimatrixUdsClient` MCP socket vs `UnimatrixHookClient` hook IPC) and returns the
documented capture shape (architecture §8 / `parity_bundle_contract.md`).

Wire-surface routing (ADR-005 / SR-08):
  * WIRE_MCP_BRIDGE  → `UnimatrixUdsClient` MCP tool calls (retrieval, proactive,
                       analytics review-read, isolation slug-B read).
  * WIRE_HOOK_OBSERVE → `UnimatrixHookClient` RecordEvent frames (behavioral,
                       precompact, analytics cycle frames, isolation slug-A write).
A dimension routed to the WRONG surface records NOTHING → the returned capture is
empty → K4 `_capture_is_empty` classifies it INFRA-ERROR (C-9), NEVER an empty-pass.

Barrier discipline (R-04): every DB-READING capture (behavioral topic_signals,
isolation on-disk landing) MUST run AFTER the shared `durability_barrier`; the driver
gates the barrier before invoking any of these — a pre-barrier read is an INFRA-ERROR,
never a parity verdict. These helpers therefore assume the barrier has already
released (the driver enforces the ordering — see `parity_legs.drive_uds_leg`).

No net-new transport/spawn/framing path: reuses the existing clients verbatim.
"""

from __future__ import annotations

import json
import sqlite3
from pathlib import Path
from typing import Any

from harness.hook_client import UnimatrixHookClient
from harness.parity_dimensions import WIRE_HOOK_OBSERVE, WIRE_MCP_BRIDGE
from harness.parity_workload import ParityWorkload
from harness.transport_health import InfraError
from harness.uds_client import UnimatrixUdsClient


# The wire surface(s) each dimension's driver actually captures over. The off-Docker
# registry-vs-driver consistency test asserts each dimension's PRIMARY surface here ==
# `dim.wire_surface` in the K1 registry, and that the dual-surface dimensions touch
# BOTH. This is the single source that test consults (no hand-list in the test).
DRIVER_WIRE_SURFACES: dict[str, frozenset[str]] = {
    "retrieval": frozenset({WIRE_MCP_BRIDGE}),
    "behavioral": frozenset({WIRE_HOOK_OBSERVE}),
    "analytics": frozenset({WIRE_MCP_BRIDGE, WIRE_HOOK_OBSERVE}),
    "proactive": frozenset({WIRE_MCP_BRIDGE}),
    "precompact": frozenset({WIRE_HOOK_OBSERVE}),
    "isolation": frozenset({WIRE_MCP_BRIDGE, WIRE_HOOK_OBSERVE}),
}


# ===========================================================================
# Result-text extraction (the UDS client returns a raw MCP result dict)
# ===========================================================================


def _result_text(resp: dict) -> str:
    """Extract the text payload of an MCP tool result (``{"content":[{"type":
    "text","text":...}]}``). Returns "" when no text content is present. An MCP
    error result is surfaced as an InfraError so a failed capture folds into the
    INFRA-ERROR class, never a silent empty capture."""
    if isinstance(resp, dict) and resp.get("isError"):
        raise InfraError(
            "uds", "MCP tool result error", detail=str(resp)[:400]
        )
    content = resp.get("content") if isinstance(resp, dict) else None
    if isinstance(content, list) and content:
        first = content[0]
        if isinstance(first, dict) and first.get("type") == "text":
            return first.get("text", "") or ""
    return ""


def _ids_scores_from_entries(entries: Any) -> tuple[list, list | None]:
    """Extract ``(ids, scores|None)`` in RANKED order from a list of entry dicts. Each
    entry's ``id`` is the ranked key; ``score``/``similarity`` (when present on EVERY
    entry) is the aligned score, else None (K3 degrades to membership-only)."""
    if not isinstance(entries, list):
        return [], None
    ids: list = []
    scores: list = []
    have_scores = True
    for e in entries:
        if not isinstance(e, dict):
            have_scores = False
            continue
        ids.append(e.get("id"))
        sc = e.get("score", e.get("similarity"))
        if sc is None:
            have_scores = False
        else:
            scores.append(sc)
    return ids, (scores if have_scores and scores else None)


def _parse_ranked_result(text: str) -> tuple[list, list | None]:
    """Parse a context_search/lookup result into ``(result_ids, scores|None)`` in
    RANKED order. The UDS leg requests ``format="json"``; tolerant of the shapes the
    server emits (top-level ``{"entries":[...]}`` / ``{"results":[...]}`` or a bare
    ``[...]``). Unparseable JSON yields an empty ranking so K4's emptiness guard names
    it INFRA, never a vacuous pass."""
    try:
        doc = json.loads(text)
    except (json.JSONDecodeError, TypeError):
        return [], None
    if isinstance(doc, dict):
        entries = doc.get("entries")
        if entries is None and isinstance(doc.get("results"), list):
            entries = doc["results"]
    else:
        entries = doc
    return _ids_scores_from_entries(entries)


# ===========================================================================
# Retrieval (D1) — MCP bridge surface, double-captured (intra)
# ===========================================================================


def capture_retrieval(uds: UnimatrixUdsClient, workload: ParityWorkload) -> list[dict]:
    """Issue the workload's retrieval query set (context_search/lookup/get) over the
    MCP UDS surface and return a list of ``{"tool","args","result_ids","scores"}`` —
    `result_ids` in RANKED order, `scores` aligned (or None).

    A result set shorter than the non-degenerate floor is returned AS-IS; K4's
    emptiness predicate (`_capture_is_empty`, single-sources STABLE_PREFIX_FLOOR)
    flags it INFRA-ERROR (R-06) — this helper never vacuous-passes on a short result.
    """
    out: list[dict] = []
    for call in workload.retrieval_calls:
        args = dict(call.args)
        args.setdefault("format", "json")
        if call.name == "context_search":
            resp = uds.context_search(args.pop("query", ""), **_clean(args))
        elif call.name == "context_lookup":
            resp = uds.context_lookup(**_clean(args))
        elif call.name == "context_get":
            resp = uds.context_get(int(args.pop("id", args.pop("entry_id", 0))), **_clean(args))
        else:  # pragma: no cover — retrieval_calls only yields the three above
            raise InfraError("uds", f"non-retrieval call in retrieval set: {call.name}")
        result_ids, scores = _parse_ranked_result(_result_text(resp))
        out.append(
            {
                "tool": call.name,
                "args": call.args,
                "result_ids": result_ids,
                "scores": scores,
            }
        )
    return out


# ===========================================================================
# Proactive briefing (D4) — MCP bridge surface, double-captured (intra)
# ===========================================================================


def capture_briefing(uds: UnimatrixUdsClient, workload: ParityWorkload) -> dict:
    """Issue the workload's briefing query set (context_briefing) over the MCP UDS
    surface and return ``{"ids":[...ranked...], "scores":[...], "injection_set":[...]}``.

    The briefing result carries the ranked briefing entry ids (the proactive-delivery
    ranking) and the injection set (the entries the briefing would inject). Both are
    captured from the SAME server response — no second identity/token."""
    ids: list = []
    scores: list = []
    injection: list = []
    have_scores = True
    for call in workload.briefing_calls:
        args = dict(call.args)
        args.setdefault("format", "json")
        role = args.pop("role", "tester")
        task = args.pop("task", "")
        resp = uds.context_briefing(role, task, **_clean(args))
        b_ids, b_scores, b_inj = _parse_briefing_result(_result_text(resp))
        ids.extend(b_ids)
        injection.extend(b_inj)
        if b_scores is None:
            have_scores = False
        else:
            scores.extend(b_scores)
    return {
        "ids": ids,
        "scores": scores if have_scores and scores else None,
        "injection_set": injection,
    }


def _parse_briefing_result(text: str) -> tuple[list, list | None, list]:
    """Parse a context_briefing result into ``(briefing_ids, scores|None,
    injection_set)``. Tolerant of BOTH shapes the server may emit:

      * a JSON object (`{"entries":[...]}` / `{"results":[...]}` + injected set), and
      * the human-readable RANKED TABLE that `context_briefing` actually returns over
        the wire — `context_briefing` does NOT honour `format=json` (Stage-3c first-live-
        run finding, Stage-3c fix; see product/features/nan-022/testing/RISK-COVERAGE-REPORT.md): it always emits the `# id topic cat conf snippet` table.
        The ranked `id` column IS the proactive-delivery ranking, so the table is parsed
        for the ranked ids (the conf column supplies aligned scores when present).

    Unparseable input (neither JSON nor the known table) yields empty lists so K4's
    emptiness guard names it INFRA, never a vacuous pass. Parsed identically by the JS
    bridge leg (`parseBriefingResult`) so the cross-language bundle contract holds (R-09)."""
    stripped = text.strip()
    if stripped.startswith("{") or stripped.startswith("["):
        try:
            doc = json.loads(text)
        except (json.JSONDecodeError, TypeError):
            doc = None
        if isinstance(doc, dict):
            entries = doc.get("entries") or doc.get("results") or []
            ids, scores = _ids_scores_from_entries(entries)
            injected = doc.get("injection_set") or doc.get("injected") or []
            injection_set = [
                e.get("id") if isinstance(e, dict) else e for e in injected
            ]
            return ids, scores, injection_set
    # Text-table fallback — the shape context_briefing actually emits.
    return _parse_briefing_table(text)


def _parse_briefing_table(text: str) -> tuple[list, list | None, list]:
    """Parse the `context_briefing` ranked text table into ``(ids, scores|None, [])``.

    Table shape (server `summary`/`markdown` briefing output)::

         #      id  topic                 cat               conf  snippet
        --  ------  --------------------  --------------  ------  --------...
         1       3  nan-022-parity-corpu  pattern           0.67  ...

    The `id` column (2nd numeric column) is the ranked proactive-delivery key; the
    `conf` column supplies aligned scores when every ranked row carries one (else None,
    K3 membership-only fallback). Rows are taken in printed (ranked) order. The
    injection set is not represented in the table, so it is `[]` — the proactive
    comparator compares the ranked id sequence (the briefing index), which IS present."""
    ids: list = []
    scores: list = []
    have_scores = True
    for line in text.splitlines():
        cols = line.split()
        # A data row begins with the rank counter, then the entry id (both ints), and
        # carries a float conf column. Header (`#`/`id`) and rule (`--`) rows fail this.
        if len(cols) < 2:
            continue
        if not (cols[0].isdigit() and cols[1].isdigit()):
            continue
        ids.append(int(cols[1]))
        conf = next(
            (c for c in cols[2:] if _is_float(c) and "." in c), None
        )
        if conf is None:
            have_scores = False
        else:
            scores.append(float(conf))
    return ids, (scores if have_scores and scores else None), []


def _is_float(tok: str) -> bool:
    try:
        float(tok)
        return True
    except ValueError:
        return False


# ===========================================================================
# Behavioral (D2) — hook /observe surface; DB read AFTER the barrier (R-04)
# ===========================================================================


def read_topic_signals(store_dir: str | Path) -> list[str]:
    """Read DISTINCT topic_signal from the per-slug observations table — the DERIVED
    attribution column (never seeded — AC-03). MUST run AFTER the durability barrier
    (R-04); the driver enforces this ordering. Returns a sorted list (the comparator
    compares the set). An absent db or empty result yields ``[]`` so K4's emptiness
    guard names it INFRA, never an empty-equals-empty pass."""
    db = Path(store_dir) / "unimatrix.db"
    if not db.is_file():
        return []
    conn = sqlite3.connect(str(db))
    try:
        rows = conn.execute(
            "SELECT DISTINCT topic_signal FROM observations "
            "WHERE topic_signal IS NOT NULL"
        ).fetchall()
    finally:
        conn.close()
    return sorted({r[0] for r in rows if r[0] is not None})


# ===========================================================================
# Analytics (D3) — DUAL surface: cycle frames via hook (driver) + review via MCP
# ===========================================================================


def read_informs_edges(uds: UnimatrixUdsClient, workload: ParityWorkload) -> list:
    """Read the Informs edge-ID SET over the MCP surface (barrier-gated — R-11). The
    review report (read by `_extract_metric_vector` in the driver) carries the cycle
    analytics; the Informs edges are read from the same review document here so the
    edge set both legs emit is captured identically. Returns the edge-id list (the
    comparator compares it UNORDERED, ids exact — NFR-6)."""
    resp = uds.context_cycle_review(
        workload.feature_cycle, agent_id="nan-022-uds-leg", format="json"
    )
    text = _result_text(resp)
    try:
        report = json.loads(text)
    except (json.JSONDecodeError, TypeError):
        return []
    if not isinstance(report, dict):
        return []
    edges = report.get("informs_edges")
    if edges is None:
        edges = report.get("edges") or []
    out: list = []
    for e in edges:
        if isinstance(e, dict):
            out.append(e.get("id", e.get("edge_id")))
        else:
            out.append(e)
    return out


def read_phase_signal(metric_vector: dict) -> dict:
    """Extract the phase signal from the MetricVector (the per-phase bucketed signal
    the analytics comparator compares EXACTLY — NFR-6). Returns the `phases` mapping
    (an empty dict if absent — K4's analytics emptiness guard keys off metric_vector,
    not this, so an empty phase signal is reported by the comparator, not swallowed)."""
    if not isinstance(metric_vector, dict):
        return {}
    phases = metric_vector.get("phases")
    return phases if isinstance(phases, dict) else {}


# ===========================================================================
# PreCompact (D5) — hook /observe surface; measurability-aware (ADR-006)
# ===========================================================================

# D5 host-side gap: the restored CompactContext payload has a host-side (Claude-Code)
# component the harness cannot drive test-only (OQ-2 / ADR-006). Until a first live
# drive proves symmetric capturability, the UDS leg reports the gap HONESTLY:
# measurable=False, restored_payload=null, host_side_gap=<named portion>. This is a
# DOCUMENTED EXCEPTION surfaced by K4, NEVER a silent drop or vacuous pass (R-08).
PRECOMPACT_HOST_SIDE_GAP: str = (
    "CompactContext restoration has a Claude-Code host-side component the harness "
    "cannot drive test-only (OQ-2/ADR-006); measured-where-drivable, host-side gap "
    "named for the C0 flip session — never rounded up to fully-measured"
)


def capture_precompact(
    hook_socket_path: str | Path,
    workload: ParityWorkload,
    sid: str,
    *,
    hook_timeout: float = 30.0,
) -> dict:
    """Drive the PreCompact `/observe` frame over the hook IPC (the WIRE_HOOK_OBSERVE
    surface — NOT the MCP bridge) and capture the SERVER-restored CompactContext
    payload, carrying `measurable`/`host_side_gap` HONESTLY (ADR-006 / OQ-2).

    First-drive measurability (OQ-2): the restored payload has a host-side (CC)
    component the harness cannot drive test-only, so this reports
    `measurable=False`, `restored_payload=null`, `host_side_gap=<named>` — a DOCUMENTED
    EXCEPTION K4 surfaces distinctly, never a silent drop and never a vacuous pass
    (a null payload is legal ONLY with measurable=False — the contract). The
    PreCompact `/observe` frame is still EMITTED (so the host-side gap is named from a
    real drive, not an absence); only the host-side restored payload is undrivable.
    """
    # Emit the PreCompact /observe frame so the drive is real (the gap is named from a
    # genuine drive, not a no-op). The restored payload is the host-side undrivable
    # portion — reported null with measurable=False (the legal null-capture case).
    with UnimatrixHookClient(hook_socket_path, timeout=hook_timeout) as h:
        resp = h.record_event(
            sid,
            "PreCompact",
            {"feature_cycle": workload.feature_cycle, "trigger": "parity-precompact"},
        )
        raw = getattr(resp, "raw", {}) or {}
        if raw.get("type") == "Error":
            raise InfraError(
                "uds",
                "PreCompact /observe frame rejected",
                detail=f"code={raw.get('code')} message={raw.get('message')!r}",
            )
    return {
        "restored_payload": None,
        "measurable": False,
        "host_side_gap": PRECOMPACT_HOST_SIDE_GAP,
    }


# ===========================================================================
# Isolation (D6) — DUAL surface: write slug A (hook), read slug B (MCP) + on-disk
# ===========================================================================


def capture_isolation(
    uds: UnimatrixUdsClient,
    hook_socket_path: str | Path,
    store_dir: str | Path,
    sid: str,
    workload: ParityWorkload,
) -> dict:
    """Per-slug isolation probe (D6) — DUAL surface fan-out:
      * WRITE an isolation marker into slug A's store via the analytics cycle on the
        hook `/observe` surface (already driven in PHASE 1) + read its on-disk landing;
      * READ from slug B via the MCP surface to confirm the slug-A write is NOT visible.

    Returns ``{"slug_a_writes_visible_to_b": bool, "landed_only_in_a": bool}`` — both
    booleans compared EXACTLY (NFR-6 — a tolerance here would mask a cross-tenant leak).

    The on-disk landing read MUST run AFTER the barrier (R-04 — the driver enforces
    the ordering). `slug_a_writes_visible_to_b` queries slug B over MCP for the slug-A
    marker content; a non-empty result is a leak (True). `landed_only_in_a` confirms
    the marker IS present in slug A's per-slug db (it landed) — both halves captured
    so a missed fan-out is a half-capture (caught by the dual-surface fan-out test).
    """
    marker = f"isolation-marker-{sid}"

    # --- slug A on-disk landing (hook /observe write already landed in PHASE 1) ---
    landed_in_a = _marker_present_in_db(store_dir, workload.feature_cycle)

    # --- slug B visibility probe over the MCP surface (the cross-slug read) ---
    # A read scoped to a DIFFERENT slug must NOT return slug A's marker. The probe
    # searches slug B's view; any hit on the slug-A marker is a leak.
    try:
        resp = uds.context_search(
            marker, k=5, format="json", feature="nan-022-isolation-slug-b"
        )
        slug_b_text = _result_text(resp)
        b_ids, _ = _parse_ranked_result(slug_b_text)
        visible_to_b = bool(b_ids)
    except InfraError:
        # A failed cross-slug probe is INFRA upstream, not a silent False.
        raise
    return {
        "slug_a_writes_visible_to_b": visible_to_b,
        "landed_only_in_a": bool(landed_in_a),
    }


def _marker_present_in_db(store_dir: str | Path, feature_cycle: str) -> bool:
    """Confirm slug A's per-slug db carries the cycle's landed observations (the write
    landed durably in slug A). Reads the observations table for the cycle's attributed
    rows — barrier-gated upstream (R-04). Absent db / empty → False (K4 names the
    resulting empty capture INFRA, never a vacuous isolation pass)."""
    db = Path(store_dir) / "unimatrix.db"
    if not db.is_file():
        return False
    conn = sqlite3.connect(str(db))
    try:
        row = conn.execute(
            "SELECT COUNT(*) FROM observations WHERE topic_signal = ?",
            (feature_cycle,),
        ).fetchone()
    finally:
        conn.close()
    return bool(row and row[0] > 0)


# ===========================================================================
# Per-dimension routing — route ONE dimension to its correct wire surface (R-03).
# ===========================================================================


def capture_dimension(
    dim,
    uds: UnimatrixUdsClient,
    hook_socket_path: str | Path,
    workload: ParityWorkload,
    store_dir: str | Path,
    sid: str,
    *,
    metric_vector: dict,
    agent_id: str,
    hook_timeout: float,
) -> dict:
    """Route ONE dimension to its correct wire surface and return its capture.

    Routed by `dim.id` (each id's wire surface is asserted == `dim.wire_surface` by the
    off-Docker registry-vs-driver consistency test against DRIVER_WIRE_SURFACES). Dual-
    surface dimensions (analytics, isolation) fan out BOTH surfaces explicitly. Intra-
    check dimensions (retrieval, proactive) double-capture so K4's
    `intra_transport_stable` can detect per-leg instability. The barrier has ALREADY
    been applied by `drive_uds_leg` (R-04), so every DB read here is post-barrier. An
    unrouted dimension → InfraError (never silently skipped — R-03)."""
    if dim.id == "retrieval":
        # WIRE_MCP_BRIDGE. Double-capture (intra) — TWO captures from the same drive.
        capture_1 = capture_retrieval(uds, workload)
        capture_2 = capture_retrieval(uds, workload)
        return {"queries": capture_1, "capture_2": capture_2}

    if dim.id == "behavioral":
        # WIRE_HOOK_OBSERVE. DB read AFTER the barrier (R-04 — the driver gated it).
        return {"topic_signals": read_topic_signals(store_dir)}

    if dim.id == "analytics":
        # DUAL surface: cycle_events written via hook (PHASE 1, by drive_uds_leg);
        # the MetricVector drive_uds_leg already returned IS the review read (not
        # re-read). Informs edges read over MCP, barrier-gated (R-11).
        return {
            "metric_vector": metric_vector,
            "informs_edges": read_informs_edges(uds, workload),
            "phase_signal": read_phase_signal(metric_vector),
        }

    if dim.id == "proactive":
        # WIRE_MCP_BRIDGE. Double-capture (intra) — TWO captures from the same drive.
        capture_1 = capture_briefing(uds, workload)
        capture_2 = capture_briefing(uds, workload)
        return {
            "briefing_ids": capture_1["ids"],
            "briefing_scores": capture_1["scores"],
            "injection_set": capture_1["injection_set"],
            "capture_2": {
                "briefing_ids": capture_2["ids"],
                "briefing_scores": capture_2["scores"],
            },
        }

    if dim.id == "precompact":
        # WIRE_HOOK_OBSERVE. Measurability-aware (ADR-006); honest host-side gap.
        return capture_precompact(
            hook_socket_path, workload, sid, hook_timeout=hook_timeout
        )

    if dim.id == "isolation":
        # DUAL surface: write slug A (hook /observe, PHASE 1) + read slug B (MCP) +
        # on-disk landing (barrier-gated). Booleans compared EXACTLY (NFR-6).
        return capture_isolation(uds, hook_socket_path, store_dir, sid, workload)

    # Never silently skip an unrouted dimension (R-03) — fold into INFRA upstream.
    raise InfraError("uds", f"unrouted dimension {dim.id}")


# ===========================================================================
# Helpers
# ===========================================================================


def _clean(args: dict) -> dict:
    """Forward only the kwargs the typed UDS client's context_* methods accept (it
    raises on unexpected kwargs). The caller pops positional args first; this filters
    the remaining mapping to the recognized kwarg subset."""
    allowed = {
        "topic", "category", "tags", "k", "id", "limit", "status",
        "agent_id", "format", "feature", "helpful", "max_tokens",
    }
    return {k: v for k, v in args.items() if k in allowed}
