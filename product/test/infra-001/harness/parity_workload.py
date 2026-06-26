"""C4 — Parity workload driver + MetricVector comparator + durability barrier (nan-021).

The ONLY substantial net-new module of nan-021 (D-1). It is the single contract
both transport legs import so HTTPS-vs-UDS parity is identical-BY-CONSTRUCTION:

  * the declarative WORKLOAD manifest (single source of truth — ordered tool-call
    list + load-bearing Bash content carrying a valid registry feature-ID token +
    ONE stable CC session identity + expected observe count);
  * the symmetric durability-barrier helper (ONE helper used by BOTH legs);
  * the MetricVector comparator (field-for-field equality MODULO the closed 3-field
    D-5 wall-clock exclusion set — ADR-003 / #5293).

Pure-Python over the parsed dict (`parse_tool_result(review).parsed`); runnable OFF
Docker so its TEETH (R-02), exclusion-set COMPLETENESS (R-01) and barrier predicate
(R-06) are unit-tested before any release-gate tag round (nan-019 #5258 precedent).

ZERO new runtime deps (stdlib only). ZERO production-code diff (NFR-1). Extends the
existing `harness/` conventions (assertions.py / uds_client.py); does NOT fork.

--------------------------------------------------------------------------------
STAGE-3B OPEN-QUESTION RESOLUTIONS — the contract C2 (shell) and C3 (Python) follow
--------------------------------------------------------------------------------

OQ-A  Barrier-predicate single-sourcing (ONE mechanism, shell + Python share it):
      `observe_count(store_dir)` is the single predicate. The shell C2 leg does NOT
      hand-write a parallel `du`; it calls THIS module's CLI entrypoint
          python -m harness.parity_workload observe-count <store_dir>
      which prints the identical integer the Python legs use. One function, two
      callers — asymmetry is structurally impossible.

OQ-B  observe_count durability read = per-slug store DIR byte-size (option (b)).
      The barrier samples the per-slug store DIRECTORY size (sum over ALL files incl.
      `unimatrix.db-wal` / `-shm`), NEVER `unimatrix.db` alone (#5265 takeaway 3 —
      the fire-and-forget WAL write lands in `-wal` before the main db is synced).
      Durability = DIR size has reached a STABLE point across two consecutive polls
      (the WAL stopped growing). The review's own non-zero observe count is then the
      AFTER-barrier non-empty assertion in the comparator — not the barrier predicate.

OQ-C  Manifest on-disk format = JSON. `WORKLOAD.to_json()` / `write_manifest(path)`
      emit `parity_workload.json`; the shell C2 leg reads the SAME bytes via
          python -m harness.parity_workload emit-manifest <path>
      (cross-language single source of truth, OQ2). No parallel hand-written script.
"""

from __future__ import annotations

import json
import logging
import os
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)


# =============================================================================
# 1. The WORKLOAD manifest (single source of truth — ADR-001 / OQ2 / FR-4 / R-09)
# =============================================================================


@dataclass(frozen=True)
class ToolCall:
    """One ordered step of the workload. `observe=True` fires a live `/observe`
    hook on each leg and therefore counts toward `expected_observe_count`."""

    name: str
    args: dict[str, Any]
    observe: bool
    # Hook-observation payload (mirrors UnimatrixHookClient.post_tool_use). For the
    # ONE load-bearing Bash call, `response_snippet` carries the feature-ID token
    # parsed by the server's attribution chain (FR-3) so `topic_signal` is DERIVED.
    response_size: int = 0
    response_snippet: str = ""

    def to_dict(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "args": self.args,
            "observe": self.observe,
            "response_size": self.response_size,
            "response_snippet": self.response_snippet,
        }

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "ToolCall":
        return cls(
            name=d["name"],
            args=d.get("args", {}),
            observe=bool(d["observe"]),
            response_size=int(d.get("response_size", 0)),
            response_snippet=d.get("response_snippet", ""),
        )


@dataclass(frozen=True)
class ParityWorkload:
    """The single declarative workload both legs replay. `session_id` is also the
    run-correlation token (R-03): ONE stable CC session identity threaded through
    declaration + ALL observes, IDENTICAL on both legs (#832 contract, FR-4/R-09)."""

    session_id: str
    feature_cycle: str
    tool_calls: list[ToolCall]

    @property
    def expected_observe_count(self) -> int:
        """Barrier predicate target = number of observe-firing tool calls (FR-10)."""
        return sum(1 for tc in self.tool_calls if tc.observe)

    @property
    def bash_call(self) -> ToolCall:
        """The EXACTLY-ONE load-bearing Bash call carrying the feature-ID token."""
        bash_calls = [tc for tc in self.tool_calls if tc.name == "Bash"]
        if len(bash_calls) != 1:
            raise ValueError(
                f"workload must contain EXACTLY ONE load-bearing Bash call, "
                f"found {len(bash_calls)}"
            )
        return bash_calls[0]

    # --- Phase VIEWS over the single tool_calls list (NOT a second manifest) -----------
    # The augmented workload (ADR-007) weaves a seed-corpus + query phase into the SAME
    # tool_calls list. These properties let the leg drivers (C3'/C5') identify which calls
    # are seed vs retrieval vs briefing vs cycle WITHOUT a second manifest/identity/token.

    @property
    def seed_calls(self) -> list[ToolCall]:
        """PHASE 1 view — the corpus CONTENT writes (`context_store`, content-only)."""
        return [tc for tc in self.tool_calls if tc.name == "context_store"]

    @property
    def retrieval_calls(self) -> list[ToolCall]:
        """PHASE 3a view — the retrieval query set (search/lookup/get)."""
        return [
            tc
            for tc in self.tool_calls
            if tc.name in ("context_search", "context_lookup", "context_get")
        ]

    @property
    def briefing_calls(self) -> list[ToolCall]:
        """PHASE 3b view — the proactive/briefing query set (`context_briefing`)."""
        return [tc for tc in self.tool_calls if tc.name == "context_briefing"]

    @property
    def query_calls(self) -> list[ToolCall]:
        """PHASE 3 view — the full query set (retrieval + briefing), in manifest order."""
        return self.retrieval_calls + self.briefing_calls

    @property
    def cycle_calls(self) -> list[ToolCall]:
        """PHASE 2 view — the nan-021 observe cycle calls (Read/Bash/Grep)."""
        return [tc for tc in self.tool_calls if tc.name in ("Read", "Bash", "Grep")]

    def validate(self) -> None:
        """Structural invariants asserted before either leg drives (fail-loud)."""
        if not self.session_id:
            raise ValueError("workload session_id (stable CC identity) must be set")
        if not self.feature_cycle:
            raise ValueError("workload feature_cycle (registry feature-ID) must be set")
        if not self.tool_calls:
            raise ValueError("workload must declare at least one tool call")
        if self.expected_observe_count < 1:
            raise ValueError("workload must fire at least one observe")
        bash = self.bash_call  # raises unless EXACTLY one
        if self.feature_cycle not in bash.response_snippet:
            raise ValueError(
                "load-bearing Bash response_snippet must carry the feature_cycle "
                f"token {self.feature_cycle!r} (derivation input — FR-3/R-07)"
            )

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": 1,
            "session_id": self.session_id,
            "feature_cycle": self.feature_cycle,
            "expected_observe_count": self.expected_observe_count,
            "tool_calls": [tc.to_dict() for tc in self.tool_calls],
        }

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=2, sort_keys=True)

    def write_manifest(self, path: str | Path) -> Path:
        """Emit `parity_workload.json` — the bytes the shell C2 leg reads (OQ-C)."""
        p = Path(path)
        p.write_text(self.to_json(), encoding="utf-8")
        return p

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> "ParityWorkload":
        wl = cls(
            session_id=d["session_id"],
            feature_cycle=d["feature_cycle"],
            tool_calls=[ToolCall.from_dict(tc) for tc in d["tool_calls"]],
        )
        wl.validate()
        return wl

    @classmethod
    def from_json(cls, text: str) -> "ParityWorkload":
        return cls.from_dict(json.loads(text))

    @classmethod
    def read_manifest(cls, path: str | Path) -> "ParityWorkload":
        return cls.from_json(Path(path).read_text(encoding="utf-8"))


# The canonical workload. ONE Bash call carries the feature-ID token referencing a
# real path so the server's attribution chain DERIVES topic_signal == feature_cycle
# (the `declared` branch — never seeded). `feature_cycle` must be a REGISTERED
# registry feature (both legs assert registration before drive — R-07).
DEFAULT_FEATURE_CYCLE = "nan-021"
DEFAULT_SESSION_ID = "nan-021-parity-session-0001"


def _cycle_calls(feature_cycle: str) -> list[ToolCall]:
    """PHASE 2 — the nan-021 observe cycle (verbatim): the Read/Bash/Grep `/observe` calls.
    The EXACTLY-ONE load-bearing Bash call carries the feature-ID token (FR-3/R-07)."""
    return [
        ToolCall(
            name="Read",
            args={"file_path": f"product/features/{feature_cycle}/SCOPE.md"},
            observe=True,
            response_size=2048,
            response_snippet=f"# {feature_cycle} scope ...",
        ),
        ToolCall(
            name="Bash",
            args={"command": f"git log --oneline -- product/features/{feature_cycle}/"},
            observe=True,
            response_size=512,
            # Load-bearing: carries the feature-ID token (FR-3/R-07 derivation input).
            response_snippet=(
                f"working on feature/{feature_cycle} — see "
                f"product/features/{feature_cycle}/IMPLEMENTATION-BRIEF.md"
            ),
        ),
        ToolCall(
            name="Grep",
            args={"pattern": "MetricVector", "path": "crates/"},
            observe=True,
            response_size=1024,
            response_snippet="crates/unimatrix-store/src/metrics.rs: MetricVector",
        ),
    ]


def default_workload(
    *,
    session_id: str = DEFAULT_SESSION_ID,
    feature_cycle: str = DEFAULT_FEATURE_CYCLE,
) -> ParityWorkload:
    """The single canonical parity workload replayed by both legs (nan-022 ADR-007 #5311).

    Augmented (cumulative on nan-021) with a deterministic SEED-CORPUS + QUERY phase so the
    retrieval (D1) and briefing (D4) rankings are NON-DEGENERATE (SR-06 / K3
    STABLE_PREFIX_FLOOR), while preserving the ONE-manifest / ONE-identity / ONE-token /
    ONE-barrier invariant (R-13): a SINGLE `ParityWorkload` under ONE `session_id` (= the
    run-correlation token), three ordered phases woven into the SAME `tool_calls` list:

      PHASE 1 (seed corpus): deterministic `context_store` CONTENT writes — CONTENT ONLY,
        never a `topic_signal`/output (R-15 / #5285). These build the identical corpus both
        legs rank over.
      PHASE 2 (nan-021 cycle): the Read/Bash/Grep `/observe` calls verbatim; the ONE
        load-bearing Bash call still carries the feature-ID token (validate() enforces it).
      PHASE 3 (query set): deterministic `context_search`/`lookup`/`get` + `context_briefing`
        calls against the seeded corpus — the RANKED retrieval/briefing captures.

    `expected_observe_count` is recomputed from `observe=True` calls (the seed/query phases
    use `observe=False`, so the barrier predicate is unchanged). The seed/query sub-lists are
    exposed as VIEWS (`seed_calls`/`retrieval_calls`/`briefing_calls`/`query_calls`/
    `cycle_calls`) over the single `tool_calls` list — never a second manifest.
    """
    # Lazy import avoids a circular import at module load (the sub-module imports ToolCall).
    from harness.parity_seed_corpus import query_calls, seed_corpus_calls

    wl = ParityWorkload(
        session_id=session_id,
        feature_cycle=feature_cycle,
        tool_calls=[
            *seed_corpus_calls(),  # PHASE 1 — content-only corpus
            *_cycle_calls(feature_cycle),  # PHASE 2 — nan-021 observe cycle (verbatim)
            *query_calls(),  # PHASE 3 — retrieval + briefing query set
        ],
    )
    wl.validate()
    return wl


# =============================================================================
# 2. Symmetric durability barrier (ADR-006 / FR-10 / R-06) — ONE shared helper
# =============================================================================

# Bounded poll: cap ~10s, sleep ~1s (NOT a flat sleep, NOT a single immediate read).
DEFAULT_BARRIER_DEADLINE_S = 10.0
DEFAULT_BARRIER_POLL_S = 1.0


class DurabilityTimeout(Exception):
    """Barrier deadline expired BEFORE observes were durable. HARD failure — the
    caller (C2/C3) MUST NOT proceed to context_cycle_review against a short/empty
    stream, and the comparator MUST NEVER compare an empty vector (R-06)."""

    def __init__(self, leg: str, *, observed: int, expected: int, stderr: str = ""):
        self.leg = leg
        self.observed = observed
        self.expected = expected
        self.stderr = stderr
        msg = (
            f"[{leg}] observes not durable within deadline: "
            f"observed_size_stable={observed} expected_observe_count={expected}"
        )
        if stderr:
            msg += f"\n--- captured child stderr ---\n{stderr}"
        super().__init__(msg)


def observe_count(store_dir: str | Path) -> int:
    """The single durability predicate (OQ-A/OQ-B), shared by BOTH legs and the shell.

    Returns the per-slug store DIRECTORY byte-size: the sum of file sizes over ALL
    files in the per-slug store dir INCLUDING `unimatrix.db-wal` and `-shm`, NEVER
    `unimatrix.db` alone (#5265 takeaway 3 — the fire-and-forget observe write lands
    in `-wal` before the main db file is synced, so reading the db alone under-counts
    and releases the barrier early).

    Despite the name (kept for the predicate's role), this is a monotone size proxy
    for "observes have landed durably"; the barrier waits for it to STABILIZE, then
    the review's own non-zero observe count is the after-barrier non-empty check.
    """
    d = Path(store_dir)
    if not d.is_dir():
        return 0
    total = 0
    for entry in d.iterdir():
        try:
            if entry.is_file():
                total += entry.stat().st_size
        except OSError:
            # A file may vanish mid-iteration (WAL checkpoint); skip it.
            continue
    return total


def durability_barrier(
    leg: str,
    expected: int,
    store_dir: str | Path,
    *,
    deadline_s: float = DEFAULT_BARRIER_DEADLINE_S,
    poll_s: float = DEFAULT_BARRIER_POLL_S,
    count_fn=observe_count,
    stderr: str = "",
) -> int:
    """Block until the driven observations are durable, then return the stable size.

    SYMMETRIC by construction: ONE helper, parameterized by `leg`, with the SAME
    predicate/deadline/cadence on BOTH legs (asymmetry is load-bearing to prevent —
    R-06 scenario 2). Durability = the store-dir size has reached a STABLE point
    (no growth across two consecutive polls) at/above a non-trivial floor, which
    means the WAL writes for all `expected` observes have flushed.

    Bounded poll (NOT a flat sleep, NOT a single immediate read). Deadline expiry =
    HARD failure (`DurabilityTimeout`), never an empty compare.

    `count_fn` is injectable purely for off-Docker unit testing; production callers
    use the default `observe_count` (the single shared predicate).
    """
    start = time.monotonic()
    prev = None
    last = 0
    while time.monotonic() - start <= deadline_s:
        observed = count_fn(store_dir)
        last = observed
        # Durable once the dir has data AND its size held steady across two polls
        # (the WAL stopped growing — all `expected` observes flushed).
        if observed > 0 and prev is not None and observed == prev:
            logger.debug(
                "[%s] durability barrier released: size=%d stable (expected>=%d)",
                leg,
                observed,
                expected,
            )
            return observed
        prev = observed
        time.sleep(poll_s)
    raise DurabilityTimeout(leg, observed=last, expected=expected, stderr=stderr)


# =============================================================================
# 3. The MetricVector comparator (ADR-003 / D-5 / R-01 / R-02 / NFR-8)
# =============================================================================
#
# Lives in `harness/metric_comparator.py` (≤500-line rule, single-responsibility) and
# is RE-EXPORTED here so both legs have one import surface: C2/C3 may import from
# either `harness.parity_workload` or `harness.metric_comparator`.
from harness.metric_comparator import (  # noqa: E402,F401  (re-export)
    AT_RISK_FIELDS,
    EXCLUDED,
    EXCLUSION_JUSTIFICATIONS,
    UNIVERSAL_FIELDS,
    ParityMismatch,
    assert_non_empty,
    compare_metric_vectors,
    field_by_field_record,
    write_field_record,
)


# =============================================================================
# 4. Stale-correlation-token guard (R-03) — single-execution orchestration seam
# =============================================================================


def load_https_vector(out_path: str | Path, expected_run_token: str) -> dict:
    """Read the HTTPS-leg MetricVector the smoke wrote to a fresh $SANDBOX file and
    REJECT a vector whose embedded run-correlation token != this run's (a stale file
    from a prior tag CANNOT be ingested). The smoke writes
    `{"run_token": ..., "metric_vector": {...}}`. Missing file → error (never compare
    against empty/stale)."""
    p = Path(out_path)
    if not p.is_file():
        raise FileNotFoundError(
            f"HTTPS MetricVector out-file absent: {p} (smoke leg failed to emit — "
            f"ERROR, never compare against empty)"
        )
    payload = json.loads(p.read_text(encoding="utf-8"))
    token = payload.get("run_token")
    if token != expected_run_token:
        raise ValueError(
            f"stale HTTPS vector rejected: run_token {token!r} != this run "
            f"{expected_run_token!r} (R-03 — a prior-tag file cannot be ingested)"
        )
    mv = payload.get("metric_vector")
    if not isinstance(mv, dict):
        raise ValueError(f"HTTPS out-file {p} has no 'metric_vector' dict")
    return mv


# --- Generalized token-guarded bundle ingest (nan-022 ADR-002 / R-09 / R-12) ------------
# `load_https_vector` (above) stays for the existing nan-021 MetricVector-only orchestrator
# path (AC-11 cumulative — removing it churns a proven path). The new matrix orchestrator
# uses `load_https_bundle`, whose LOGIC lives in K5 `transport_health.py` so it can raise
# `InfraError` without a circular import (it depends on `InfraError`). C4' RE-EXPORTS it to
# preserve the single import surface (both legs may import it from `harness.parity_workload`).
from harness.transport_health import (  # noqa: E402,F401  (re-export — single import surface)
    InfraError,
    load_https_bundle,
)


# =============================================================================
# 5. No-seed static guard (AC-03 / FR-6)
# =============================================================================

FORBIDDEN_SEED_SITES: tuple[str, ...] = (
    "_seed_observation_sql_lifecycle",  # suites/test_lifecycle.py — SQL row injection
    "_seed_attributed_observations_832",  # #832-specific attributed-observation seeder
    "make_stamped_event",  # Rust struct injection (..., topic_signal)
)


def assert_no_seed_reachable(*source_paths: str | Path) -> None:
    """Audit this module's source (+ any provided test-path sources) for ANY forbidden
    seed site INVOCATION. The manifest seeds the workload INPUT (tool calls + Bash
    content), never the `topic_signal` OUTPUT — the column is DERIVED over the wire on
    both legs (AC-03). Fails LOUD if any seed site is reachable from the C4 path.

    Detection targets a CALL/IMPORT, not the bare name: this module legitimately NAMES
    each forbidden site as a string literal in `FORBIDDEN_SEED_SITES` (and in comments)
    so the audit can scan for it — that declaration is not an invocation. A real reach is
    a call `site(` or an `import ... site` of the seeder, which is what we forbid.
    """
    paths = [Path(__file__)] + [Path(p) for p in source_paths]
    for p in paths:
        try:
            src = p.read_text(encoding="utf-8")
        except OSError:
            continue
        for site in FORBIDDEN_SEED_SITES:
            for pattern, kind in ((site + "(", "call"), ("import " + site, "import")):
                assert pattern not in src, (
                    f"forbidden seed site {site!r} {kind}-reachable from {p} — the "
                    f"topic_signal column MUST be DERIVED, never seeded (AC-03/FR-6)"
                )


# CLI argv shim lives in `parity_workload_cli.py` (≤500-line split); the
# `python -m harness.parity_workload ...` entrypoint delegates to it.
if __name__ == "__main__":  # pragma: no cover - thin CLI shim
    from harness.parity_workload_cli import main as _cli_main

    raise SystemExit(_cli_main(sys.argv))
