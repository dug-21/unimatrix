# Agent Report: nan-022-agent-3-parity-comparator (K2 — parity_comparator)

## Scope
K2 comparator framework + cross-dimension drift guard (ADR-003 / #5307), Wave B.

## Files created / modified
- product/test/infra-001/harness/parity_comparator.py (NEW, 486 lines)
- product/test/infra-001/suites/test_parity_comparator.py (NEW, 424 lines, 31 tests)
- No edits to other components' files. The prescribed bind_comparators({...}) call
  lives in K2's own module at import (K1's published hook), not in K1's file.

## Tests
- K2 unit suite: 31 passed (test_parity_comparator.py).
- Wave A regression (no K2-introduced regressions in their own modules):
  - test_ranking_tolerance.py + test_parity_workload.py + test_transport_health.py: 65 passed.
  - test_parity_dimensions.py ALONE: 20 passed.
- Full combined run (K1+K3+workload+K2): 96 passed, 1 FAILED (see flag below).

## Issues / adjacent breakage FLAGGED (not fixed — out of K2 scope)
- K1 test `suites/test_parity_dimensions.py::test_each_dimension_comparator_is_dimension_comparator_subclass`
  FAILS in a whole-directory `pytest suites/` run, PASSES when test_parity_dimensions.py runs alone.
  Root cause: K2's REQUIRED import-time bind_comparators() reassigns the module-global
  parity_dimensions.DIMENSIONS to real classes. bind_comparators only re-binds str entries,
  so after K2 import the comparators are already `type`; K1's test then binds local _Base
  stand-ins as a no-op and asserts issubclass(_Base) against the real K2 classes -> fail.
  This is order-dependent global-state contamination in K1's test, triggered by the
  load-bearing K2 hook. FIX BELONGS IN K1's test (snapshot/restore DIMENSIONS around its bind,
  drop the string-bound assumption once any K2 importer is in the session). The K2 import-time
  call must NOT be removed — it is mandated by ADR-003 and the K1-published binding contract.
  Detail stored as pattern #5317.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (decision/nan-022) -> ADR-003 #5307 (comparator
  framework + drift guard), ADR-001 #5305, ADR-004 #5315.
- Resolved a brief/ADR vs pseudocode discrepancy on FORBIDDEN_SEED_SITES: ADR text says
  "defined ONCE in parity_comparator.py"; validated pseudocode (Stage 3a spec) + the existing
  nan-021 parity_workload.FORBIDDEN_SEED_SITES say RE-EXPORT the single C4' tuple. Followed the
  pseudocode (re-export, object-identity assertion in the drift guard) — the single-source intent
  is preserved either way; no second tuple object exists.
- Stored: entry #5317 "Module-import-time bind_comparators mutates the shared K1 DIMENSIONS
  global — pytest session order contaminates K1's pre-binding tests" via context_store (pattern).
