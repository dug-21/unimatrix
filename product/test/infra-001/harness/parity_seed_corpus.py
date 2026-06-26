"""C4' — Deterministic seed-corpus + query phase for the augmented parity workload.

nan-022 ADR-007 (#5311). Pure-Python, stdlib-only, OFF-Docker unit-testable. Factored
out of `parity_workload.py` (the ≤500-line / single-responsibility split — nan-021's
`metric_comparator.py` lib-split precedent) so the workload manifest module stays the
manifest contract and this module owns the seed/query CONTENT.

WHY this exists (ADR-007 / SR-06):
  nan-021's manifest is a 3-call cycle that produces ONE MetricVector. Retrieval (D1) and
  briefing (D4) parity need a PRE-SEEDED multi-entry store + a non-trivial query set — a
  degenerate single-hit ranking is a VACUOUS parity pass (#5177), and the K3 stable-prefix
  policy is meaningless unless the corpus is deep enough that the stable prefix is a real
  ranking signal (STABLE_PREFIX_FLOOR = 3, single-sourced in `ranking_tolerance.py`).

CRITICAL no-seed rule (R-15 / #5285 / nan-021 ADR-004 #5289):
  The seed phase writes corpus CONTENT via the normal `context_store` tool-call path ONLY.
  It NEVER seeds a compared OUTPUT — no `topic_signal`, no MetricVector field, no `Informs`
  edge id, no briefing id. The compared outputs are DERIVED over the wire on BOTH legs from
  the identical seeded corpus, so any cross-leg ranking diff is a transport effect, not a
  corpus difference. The forbidden seed sites (`FORBIDDEN_SEED_SITES`, single-sourced in
  `parity_workload.py`) stay unreachable from this module — it issues `context_store`,
  `context_search`, `context_lookup`, `context_get`, `context_briefing` tool calls, never a
  SQL/struct seeder.

ONE-identity invariant (R-13): these phases are woven into the SAME `tool_calls` list under
the SAME `session_id`/`feature_cycle` as the nan-021 cycle phase. They are VIEWS over the
single manifest, never a second `ParityWorkload`.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:  # pragma: no cover - typing only
    from harness.parity_workload import ToolCall


# =============================================================================
# Corpus sizing — the load-bearing tunable (ADR-007 (3) / OQ-C, pairs with K3)
# =============================================================================
#
# STABLE_PREFIX_FLOOR (K3) = 3 is the floor at which the stable ranked prefix is a real
# parity signal rather than a single trivial hit. The seed corpus MUST be deep enough that
# both legs can return a ranking of depth >= that floor over a SHARED topic, otherwise the
# tolerance policy has nothing to bite on (vacuous pass, #5177). We seed SEED_CORPUS_SIZE
# entries on the shared retrieval topic — comfortably above the floor (head-room so HNSW tail
# churn below the prefix does not starve the prefix). The exact size is the OQ-C/Stage-3a
# test-design call fixed HERE; the SHAPE (size > STABLE_PREFIX_FLOOR > 1) is ADR-fixed.
SEED_CORPUS_SIZE: int = 5

# The single shared topic the corpus is seeded under and the query set retrieves over, so the
# ranking is non-degenerate (>= STABLE_PREFIX_FLOOR hits). Content-only — never an output.
SEED_TOPIC: str = "nan-022-parity-corpus"
SEED_CATEGORY: str = "pattern"


# Per-entry distinct subjects (Stage-3c first-live-run fix — Stage-3c fix; see product/features/nan-022/testing/RISK-COVERAGE-REPORT.md / R-06 / OQ-3).
# The corpus MUST survive the server's near-duplicate collapse: the previous boilerplate
# entries differed only by a 2-digit index and the server deduped ALL of them into a single
# entry (`similarity: 1.00 | duplicate: true`), so retrieval returned a single hit (< the
# STABLE_PREFIX_FLOOR of 3) — a degenerate, vacuous ranking (R-06). Each entry now carries a
# semantically DISTINCT subject so the embeddings differ enough to land as separate entries
# while all sharing the SEED_TOPIC keyword (so a topic query still returns >= the floor).
# CONTENT ONLY — never a compared output (no topic_signal / edge / briefing id; R-15).
_SEED_SUBJECTS: tuple[str, ...] = (
    "transport-layer socket framing and unix-domain stream handshakes",
    "embedding-vector similarity scoring and approximate nearest-neighbour recall",
    "write-ahead-log checkpoint durability and per-slug store isolation",
    "proactive briefing index ranking and injection-set selection heuristics",
    "behavioral observation attribution from declared cycle topic signals",
    "JSON-RPC tool-call envelope routing over the pinned HTTPS bridge surface",
    "contradiction detection across incompatible stored directive entries",
    "confidence Wilson-score re-ranking under sparse vote evidence",
)


def _seed_entry_content(i: int) -> str:
    """Deterministic, DISTINCT content for corpus entry i. Each entry carries a unique
    subject (`_SEED_SUBJECTS`) so it survives the server's near-duplicate collapse and the
    corpus ranks to depth >= STABLE_PREFIX_FLOOR; the shared SEED_TOPIC keyword keeps a topic
    query returning the whole corpus (related enough to rank together, distinct enough not to
    dedup). Index `i` is taken modulo the subject pool so any SEED_CORPUS_SIZE stays distinct."""
    subject = _SEED_SUBJECTS[i % len(_SEED_SUBJECTS)]
    return (
        f"{SEED_TOPIC} corpus entry {i:02d}: {subject}. This entry documents {subject} "
        f"as cross-transport parity reference material number {i:02d} for retrieval and "
        f"briefing ranking over the HTTPS-vs-UDS canonical workload."
    )


def seed_corpus_calls() -> list["ToolCall"]:
    """PHASE 1 — the deterministic seed corpus: SEED_CORPUS_SIZE `context_store` CONTENT
    writes under ONE shared topic. CONTENT ONLY — no `topic_signal`/output is set; the args
    carry `content`/`topic`/`category` exactly as a real `context_store` MCP call would.

    `observe=False`: the seed writes are MCP-bridge store calls, not `/observe`-hooked tool
    uses, so they do NOT inflate `expected_observe_count` (the barrier predicate). They are
    the corpus the QUERY phase ranks over, identical on both legs.
    """
    from harness.parity_workload import ToolCall  # local import — avoid circular import

    return [
        ToolCall(
            name="context_store",
            args={
                "content": _seed_entry_content(i),
                "topic": SEED_TOPIC,
                "category": SEED_CATEGORY,
            },
            observe=False,
            response_size=len(_seed_entry_content(i)),
            # CONTENT echo only — NEVER a derived output (no topic_signal/edge/briefing id).
            response_snippet=f"stored {SEED_TOPIC} entry {i:02d}",
        )
        for i in range(SEED_CORPUS_SIZE)
    ]


def retrieval_query_calls() -> list["ToolCall"]:
    """PHASE 3a — the retrieval (D1) query set: > 1 distinct `context_search`/`lookup`/`get`
    calls against the seeded corpus. These produce the RANKED retrieval captures; they seed
    NO compared output. `observe=False` — reads, not observed tool uses."""
    from harness.parity_workload import ToolCall  # local import — avoid circular import

    return [
        ToolCall(
            name="context_search",
            args={"query": f"{SEED_TOPIC} cross-transport parity", "k": SEED_CORPUS_SIZE},
            observe=False,
            response_snippet=f"search {SEED_TOPIC}",
        ),
        ToolCall(
            name="context_lookup",
            args={"topic": SEED_TOPIC, "category": SEED_CATEGORY},
            observe=False,
            response_snippet=f"lookup {SEED_TOPIC}",
        ),
        ToolCall(
            name="context_search",
            args={"query": f"{SEED_TOPIC} retrieval ranking seed", "k": SEED_CORPUS_SIZE},
            observe=False,
            response_snippet=f"search {SEED_TOPIC} ranking",
        ),
    ]


def briefing_query_calls() -> list["ToolCall"]:
    """PHASE 3b — the proactive/briefing (D4) query set: > 1 distinct `context_briefing`
    calls against the seeded corpus. These produce the RANKED briefing captures; they seed
    NO compared output. `observe=False` — reads, not observed tool uses."""
    from harness.parity_workload import ToolCall  # local import — avoid circular import

    return [
        ToolCall(
            name="context_briefing",
            args={"task": f"work on {SEED_TOPIC} cross-transport parity"},
            observe=False,
            response_snippet=f"briefing {SEED_TOPIC}",
        ),
        ToolCall(
            name="context_briefing",
            args={"task": f"rank {SEED_TOPIC} entries for retrieval and proactive delivery"},
            observe=False,
            response_snippet=f"briefing {SEED_TOPIC} rank",
        ),
    ]


def query_calls() -> list["ToolCall"]:
    """PHASE 3 — the full query set (retrieval + briefing), in deterministic order."""
    return retrieval_query_calls() + briefing_query_calls()
