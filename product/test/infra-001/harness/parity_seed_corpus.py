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
#
# #844 (ass-085 #852): raised 5 -> 25. At N=k=5 hnsw_rs short-returns (<5 results) in
# ~14% of builds, starving the stable prefix below the floor -> D1/D4 PARITY_FAIL. ass-085
# measured N~=25 with a >=0.20 head/tail "moat" cuts the flip ~30x and eliminates the
# short-return mode. N is the dominant lever; exact-zero is unreachable (~0.2-0.6% intrinsic
# hnsw_rs build-RNG residual remains, owned by ADR-004 stable-prefix policy + the C0 #5304
# documented exception). The corpus complements that policy; it does not replace it.
SEED_CORPUS_SIZE: int = 25

# Requested retrieval breadth (top-k) for the D1 context_search query, DECOUPLED from
# SEED_CORPUS_SIZE (#844). Must be >= STABLE_PREFIX_FLOOR (3, the asserted stable prefix
# depth, single-sourced in ranking_tolerance.py) and <= the head size (5). We seed 25 entries
# for graph stability but query a small stable HEAD: querying k=25 would return the whole
# corpus and re-introduce the boundary noise the deep corpus exists to suppress (ass-085 Q2).
RETRIEVAL_QUERY_K: int = 5

# The single shared topic the corpus is seeded under and the query set retrieves over, so the
# ranking is non-degenerate (>= STABLE_PREFIX_FLOOR hits). Content-only — never an output.
SEED_TOPIC: str = "nan-022-parity-corpus"
SEED_CATEGORY: str = "pattern"


# Per-entry distinct subjects (Stage-3c R-06 fix; expanded for #844 / ass-085 #852).
#
# Two invariants this pool must hold (verified once at authoring against the REAL MiniLM
# model — sentence-transformers/all-MiniLM-L6-v2 — via scripts/docker-embed-readiness-smoke.sh
# DESIGN-VERIFY mode; ass-085's model was synthetic, so the geometry the fix rests on was
# unverified until #844 measured it):
#
#  1. DISTINCTNESS / no near-duplicate collapse (R-06). The previous boilerplate entries
#     differed only by a 2-digit index and the server deduped ALL of them
#     (`similarity: 1.00 | duplicate: true`), collapsing the ranking to one hit (< the
#     STABLE_PREFIX_FLOOR of 3) — a vacuous parity pass (#5177). Each entry carries a
#     semantically DISTINCT subject; measured MAX pairwise cosine ~0.74, comfortably below
#     the server DUPLICATE_THRESHOLD (0.92, store_ops.rs). The pool size is also >=
#     SEED_CORPUS_SIZE so the `i % len` modulo in `_seed_entry_content` never wraps and
#     re-collapses distinctness (locked by a unit test).
#
#  2. GEOMETRY: a coherent HEAD (the first RETRIEVAL_QUERY_K subjects — all "cross-transport
#     parity" aspects) ranks above a TAIL (distinct subsystems / off-domain) by a >=0.20
#     cosine "moat" against the primary retrieval query, with the head internally graded so
#     the asserted top-3 prefix has no boundary tie. Measured against the primary D1 query:
#     moat ~0.225, top-3 intra-head gaps ~0.10 / ~0.04, residual projection ~0.3% (within the
#     ~0.4% envelope the C0 exception is signed for). The retrieval-flavored SECONDARY queries
#     reorder the (intrinsically clustered) head more tightly — that residual is the intrinsic
#     hnsw_rs flip the stable-prefix policy + C0 exception own; it cannot be designed to zero
#     (ass-085 Q2 geometric cap, confirmed on real embeddings in #844).
#
# Content geometry note: the stored embed text is f"{title}: {content}" (embed/text.rs,
# separator ": "). seed_corpus_calls sets title == content == subject so the embed text is
# SUBJECT-DOMINANT. The MCP default title is f"{topic}: {category}" — a UNIFORM string across
# all 25 entries — which (measured) compresses the head/tail moat to ~0.03 and destroys the
# geometry; hence the explicit per-entry title.
#
# CONTENT ONLY — never a compared output (no topic_signal / edge / briefing id; R-15).
_SEED_SUBJECTS: tuple[str, ...] = (
    # HEAD (first RETRIEVAL_QUERY_K) — graded cross-transport-parity ladder, mutually distinct
    # aspects. Order here is the intended descending relevance to the primary retrieval query.
    "cross-transport parity of identical ranked results on each transport",
    "cross-transport parity of byte-identical stored-entry rankings on both legs",
    "cross-transport parity for ranked search output on both request legs",
    "cross-transport parity of HTTPS and unix-domain search responses",
    "cross-transport parity between the HTTPS bridge and the unix-domain socket",
    # TAIL — distinct subsystems / off-domain subjects, each >=0.20 cosine below the head
    # against the retrieval query (the "moat"), and pairwise-distinct from each other.
    "write-ahead-log checkpoint durability and crash recovery",
    "contradiction detection across incompatible stored directives",
    "quarterly payroll tax withholding schedules",
    "per-slug database isolation and multi-tenant project configuration",
    "dashboard panel layout and time-series visualization widgets",
    "kitchen pantry inventory and grocery restocking lists",
    "open-source license compliance and dependency vulnerability auditing",
    "markdown documentation generation and changelog formatting",
    "knowledge graph edge traversal and provenance lineage",
    "agent role orchestration and swarm coordination",
    "secret management and environment-variable credential handling",
    "garbage collection of expired ephemeral session records",
    "JPEG chroma subsampling and color-space conversion",
    "cron scheduling of recurring background maintenance jobs",
    "geospatial tile rendering and map projection mathematics",
    "houseplant watering frequency by season",
    "thermal sensor calibration drift over temperature cycles",
    "audio waveform resampling and loudness normalization",
    "spreadsheet pivot-table aggregation and cell formatting",
    "container image layer caching and registry digest pinning",
)


def _seed_entry_content(i: int) -> str:
    """Deterministic, DISTINCT content for corpus entry i: the bare subject string.

    The content is the SUBJECT alone (no shared boilerplate). #844/ass-085 measured that the
    previous template's shared "{SEED_TOPIC} ... cross-transport parity ... HTTPS-vs-UDS"
    filler appeared in EVERY entry and pulled all 25 embeddings toward the query, compressing
    the head/tail moat to ~0.02 and destroying the geometry the fix depends on. A subject-only
    content lets the distinct subject dominate, realizing the >=0.20 moat (see _SEED_SUBJECTS).

    Index `i` is taken modulo the subject pool. With len(_SEED_SUBJECTS) >= SEED_CORPUS_SIZE
    the modulo never wraps, so every seeded entry stays distinct (no R-06 dedup collapse).
    Topic-scoped recall does NOT rely on a content keyword: context_lookup matches the
    structured `topic` field, and seed_corpus_calls sets topic=SEED_TOPIC on every write."""
    return _SEED_SUBJECTS[i % len(_SEED_SUBJECTS)]


def seed_corpus_calls() -> list["ToolCall"]:
    """PHASE 1 — the deterministic seed corpus: SEED_CORPUS_SIZE `context_store` CONTENT
    writes under ONE shared topic. CONTENT ONLY — no `topic_signal`/output is set; the args
    carry `content`/`topic`/`category` exactly as a real `context_store` MCP call would.

    `observe=False`: the seed writes are MCP-bridge store calls, not `/observe`-hooked tool
    uses, so they do NOT inflate `expected_observe_count` (the barrier predicate). They are
    the corpus the QUERY phase ranks over, identical on both legs.

    `title` is set EXPLICITLY per entry (to the subject) — NOT left to the MCP default. The
    default title is f"{topic}: {category}", a UNIFORM string across all 25 entries; since the
    stored embed text is f"{title}: {content}" (embed/text.rs), that uniform prefix (measured,
    #844) compresses the head/tail moat to ~0.03 and destroys the retrieval geometry. With
    title == content == subject the embed text is subject-dominant and the moat is realized.
    """
    from harness.parity_workload import ToolCall  # local import — avoid circular import

    return [
        ToolCall(
            name="context_store",
            args={
                "title": _seed_entry_content(i),
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
            args={"query": f"{SEED_TOPIC} cross-transport parity", "k": RETRIEVAL_QUERY_K},
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
            args={"query": f"{SEED_TOPIC} retrieval ranking seed", "k": RETRIEVAL_QUERY_K},
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
