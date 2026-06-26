# C4' — Workload manifest, augmented (`harness/parity_workload.py`)

**Extended in place**, cumulative (NFR-2). ADR-007 (#5311). Consumes the nan-021 module
verbatim; augments three surfaces ONLY. No fork, no second manifest/identity/barrier.

## Purpose

Augment the single canonical workload with a deterministic SEED-CORPUS + QUERY phase so
retrieval (D1) and briefing (D4) rankings are NON-DEGENERATE (SR-06/NFR-7), while preserving the
ONE-manifest / ONE-identity / ONE-token / ONE-barrier invariant (R-13). Generalize the
token-guarded ingest from a single vector to the dimension bundle. Extend the no-seed audit
coverage to all net-new modules + the seed loader.

## Consumed verbatim (do NOT re-author)

`ToolCall`, `ParityWorkload` (incl. `validate`, `to_json`/`from_json`, `write_manifest`/
`read_manifest`, `expected_observe_count`, `bash_call`), `durability_barrier`, `observe_count`,
`DurabilityTimeout`, the re-exports of the MetricVector comparator, `FORBIDDEN_SEED_SITES`,
`assert_no_seed_reachable`, the CLI entrypoints (`observe-count`/`emit-manifest`/
`expected-observe-count`).

## Change 1 — augment `default_workload` (seed corpus + query phase)

The manifest gains TWO ordered phases woven into the SAME `tool_calls` list under the SAME
`session_id`/`feature_cycle` (ONE identity preserved):

```
def default_workload(*, session_id=DEFAULT_SESSION_ID, feature_cycle=DEFAULT_FEATURE_CYCLE) -> ParityWorkload:
    # PHASE 1 (seed corpus): a deterministic set of context_store CONTENT writes — CONTENT ONLY.
    #   Each seed is a ToolCall(name="context_store", args={content, topic, category, ...}, observe=...).
    #   The corpus size + content is fixed so retrieval/briefing produce a ranking of depth
    #   >= STABLE_PREFIX_FLOOR (N > 1) on both legs (NFR-7). Concrete size/content is OQ-3/OQ-C
    #   (Stage-3a test design); pseudocode fixes: SEED CONTENT ONLY, never a topic_signal output.
    # PHASE 2 (existing workload): the nan-021 Read/Bash/Grep observe calls (verbatim) — the ONE
    #   load-bearing Bash call still carries the feature-ID token (validate() still enforces this).
    # PHASE 3 (query set): a deterministic set of context_search/lookup/get + context_briefing
    #   calls against the seeded store — the retrieval (D1) and briefing (D4) query set. These are
    #   the RANKED captures; they do NOT seed any compared output.
    ...
    wl.validate()
    return wl
```

CRITICAL no-seed rule (R-15 / #5285): the seed corpus writes CONTENT via the real
`context_store` path ONLY. It NEVER seeds a compared OUTPUT — no `topic_signal`, no MetricVector
field, no edge ID, no briefing id. The compared outputs are DERIVED over the wire on both legs.

Invariant preservation (R-13): ONE `ParityWorkload` object; `run_token == workload.session_id`
unchanged; `expected_observe_count` recomputed from `observe=True` calls (seed/query calls that
fire `/observe` count; the manifest stays the single source). `validate()` still asserts EXACTLY
ONE load-bearing Bash call carrying the feature token. The augmented manifest round-trips
`to_json`/`from_json` stably (asserted off-Docker, R-13 sc.2).

A new factory parameter or helper MAY expose the seed/query sub-lists for the leg drivers to
identify which calls are retrieval vs briefing vs seed (e.g. tag each `ToolCall` via its `name`
+ a documented convention, OR expose `seed_calls`/`query_calls`/`briefing_calls` properties on
`ParityWorkload`). Keep it ONE manifest — these are VIEWS over the single `tool_calls` list, not
a second manifest.

## Change 2 — generalize `load_https_vector` -> `load_https_bundle`

Per K5 (transport_health.md), the LOGIC of `load_https_bundle` lives in K5 (so it can raise
`InfraError`). C4' RE-EXPORTS it to preserve the single import surface and keeps the legacy
`load_https_vector` for the existing nan-021 `test_https_uds_parity` MetricVector path
(unchanged, still green — do not break the existing test).

```
from harness.transport_health import load_https_bundle   # re-export (single import surface)
# load_https_vector stays as-is (consumed by the existing MetricVector-only orchestrator test).
```

Rationale for keeping both: the existing `test_https_uds_parity` (MetricVector) test and its
contract tests reference `load_https_vector` by name (verified in the suite). Removing it breaks
AC-11's "cumulative, no fork" by churning a proven path. The new matrix test uses
`load_https_bundle`.

## Change 3 — extend `assert_no_seed_reachable` coverage + keep the single forbidden set

`FORBIDDEN_SEED_SITES` REMAINS defined ONCE here (K2 re-exports this exact tuple object —
SR-05). The audit is extended to cover EVERY net-new module AND the seed-corpus loader:

```
# the orchestrator/off-Docker test calls:
assert_no_seed_reachable(
    parity_dimensions.__file__, parity_comparator.__file__, ranking_tolerance.__file__,
    parity_outcome.__file__, transport_health.__file__, parity_legs.__file__,
    <seed-corpus loader source>, __file__,
)
```

If the seed corpus needs new content fixtures, the loader source is added to the audit list so a
forbidden seed site (e.g. `make_stamped_event(..., topic_signal)`, `_seed_attributed_
observations_832`) stays unreachable from the compared-output path (R-15 sc.1/2).

## Data flow

- INPUT: `session_id`, `feature_cycle` (defaults preserved).
- OUTPUT: a single augmented `ParityWorkload` driven byte-identically by both legs; the seed +
  query sub-views consumed by C3'/C5' to perform retrieval/briefing captures.

## Error handling

- `validate()` (verbatim) raises `ValueError` on any structural invariant violation (one Bash
  call, token present, ≥1 observe).
- `load_https_bundle` raises `InfraError` (K5) on missing/stale/malformed/missing-key bundles.
- The no-seed audit raises `AssertionError` off-Docker if any forbidden site is reachable.

## Key test scenarios (hints)

- After augmentation: ONE `ParityWorkload` object, `run_token == workload.session_id`, one
  barrier helper, `validate()` still passes (R-13 sc.1).
- Augmented manifest round-trips `to_json`/`from_json` byte-stably; both legs replay the SAME
  manifest (R-13 sc.2).
- Seed corpus writes CONTENT only via `context_store`; no compared output in the seed path
  (R-15 sc.2 / R-06 sc.3).
- `assert_no_seed_reachable` covers all net-new modules + the seed loader; forbidden sites stay
  unreachable (R-15 sc.1).
- The seed corpus size + query set yield a ranking of depth >= STABLE_PREFIX_FLOOR (N > 1) —
  non-degenerate (R-06 sc.1).
- `FORBIDDEN_SEED_SITES` is the single object K2 re-exports (SR-05 — assert identity).
- The existing `load_https_vector` MetricVector path remains unbroken (AC-11 cumulative).
