# Agent Report — nan-022-agent-3-parity-workload (C4′)

## Scope
Extend C4′ `harness/parity_workload.py` (cumulative, in place) per ADR-007 (#5311):
augment `default_workload()` with seed-corpus + query phase, generalize
`load_https_vector` → `load_https_bundle` (re-export K5), extend `assert_no_seed_reachable`
coverage. Off-Docker Tier-A only.

## Files created / modified
- MODIFIED: `product/test/infra-001/harness/parity_workload.py`
  - `default_workload()` augmented: 3 ordered phases woven into ONE `tool_calls` list —
    PHASE 1 seed corpus (`context_store` content-only, observe=False), PHASE 2 nan-021
    observe cycle (verbatim, extracted to `_cycle_calls`), PHASE 3 retrieval + briefing
    query set (observe=False).
  - `ParityWorkload` gained VIEW `@property` accessors: `seed_calls`, `retrieval_calls`,
    `briefing_calls`, `query_calls`, `cycle_calls` (filters over the single manifest — not a
    second workload).
  - Re-exported `load_https_bundle` and `InfraError` from K5 `transport_health`.
    `load_https_vector` kept verbatim (nan-021 MetricVector path, AC-11).
  - `FORBIDDEN_SEED_SITES` + `assert_no_seed_reachable` unchanged (single source K2 re-exports
    by identity; the `*source_paths` signature already supports net-new-module coverage).
  - CLI argv shim moved to new `parity_workload_cli.py` to stay ≤500 lines (now 497).
- CREATED: `product/test/infra-001/harness/parity_seed_corpus.py` (146 lines) — deterministic
  seed corpus (SEED_CORPUS_SIZE=5 > STABLE_PREFIX_FLOOR=3) + query set helpers.
- CREATED: `product/test/infra-001/harness/parity_workload_cli.py` (44 lines) — CLI shim;
  `python -m harness.parity_workload {observe-count|emit-manifest|expected-observe-count}`
  entrypoint preserved via thin delegating `__main__`.
- MODIFIED: `product/test/infra-001/suites/test_parity_workload.py` — added 19 new tests
  (R-13 one-identity/token/barrier, R-06 corpus depth/non-degenerate, R-15 seed-content-only +
  extended no-seed audit + single-source identity, R-09/R-12 `load_https_bundle`).

## Tests
- `test_parity_workload.py`: **48 passed** (29 nan-021 backward-compat unchanged + 19 net-new).
- Full off-Docker harness module regression
  (parity_workload, parity_comparator, parity_dimensions, parity_outcome, ranking_tolerance,
  transport_health): **164 passed, RC=0** — zero regression in already-committed K1–K5/MC.
- CLI entrypoint verified (`expected-observe-count` → 3; `observe-count` works).
- `expected_observe_count` unchanged at 3 (seed/query phases observe=False) — barrier predicate
  intact. Manifest round-trips byte-stably. No circular import (verified across the
  parity_comparator → parity_workload → transport_health chain).

## Issues / adjacent breakage flagged
- None. The daemon/Docker-bound suites (test_security, test_lifecycle, test_tools, etc.) hang
  off-Docker by design (not Tier-A; Stage 3c owns them) — NOT a regression from this change.
- Decisions fixed for OQ-C: SEED_CORPUS_SIZE=5, query set = 3 retrieval (search/lookup/search) +
  2 briefing calls. All > 1 and ≥ STABLE_PREFIX_FLOOR (3), single-sourced from K3.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (category=decision, topic=nan-022) — surfaced ADR-007
  (#5311), ADR-001 (#5305), ADR-003 (#5307); read ADR detail from architecture/brief.
- Stored: entry #5319 "Augment single-manifest parity workload via in-list phases + VIEW
  properties, never a second manifest" via /uni-store-pattern (in-list phasing + observe=False
  barrier preservation; the K5 lazy-import re-export safety condition; the ≤500-line CLI split
  without relocating the single-source forbidden-seed set / self-audit).
