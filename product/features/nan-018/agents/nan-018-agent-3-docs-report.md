# Agent Report — nan-018 Band-1/2 docs (agent-3-docs, Wave 2)

## Files created / modified (all docs-only)
- M `docs/testing/eval-harness.md` (Band-1, AC-10)
- A `docs/testing/eval-config-knobs.md` (Band-2, AC-11)
- A `docs/testing/eval-fixture-authoring.md` (Band-2, AC-11)
- A `docs/testing/eval-corpus-migration.md` (Band-2, AC-11)
- A `docs/testing/eval-two-corpus-model.md` (Band-2, AC-11)

Commit: `78498d1a impl(docs): Band-1/2 eval-harness docs ... (#716)` on `feature/nan-018`.

## Coverage vs AC-10 / AC-11
- **AC-10**: eval-harness.md now covers all six new capabilities (tunability,
  trust class, token-weighted cost, fixture corpus, two-corpus model, drift guard),
  the correlated `## 5C` report section, the extended regression reasons
  (`mrr`/`p@k`/`trust`/`cost`), and the advisory ε=0.0 cost-growth boundary. ADRs
  are already stored in Unimatrix (ADR-001..006; #4894/#4895 etc.) by
  synthesis/Wave-1 — no ADR files authored (Unimatrix-only).
- **AC-11**: all four Band-2 guides exist and are self-sufficient (author / migrate
  / sweep from docs alone). Required flags present: clean_replacement AMPLIFIED knob
  (ADR-001); multiplier scales severities only + per-field override wins + the
  deliberate-set-to-default ambiguity caveat; cost-proxy fidelity (faithful subword
  default, word×1.3 fallback ~±20%, tier logged — NFR-08/R-07); eval-only penalty
  boundary (ADR-006); asymmetric rank-below (A absent⇒pass, B absent⇒FAIL);
  ADR-004 §5 depth obligation (deprecated-connected crossover bracket); corpus not
  frozen (one revision pass); R-04 named-human manifest-completeness gate.

Accuracy: all field names, file paths, error names, defaults, and the manifest
stamp values were taken from committed Wave-1 code
(`infra/config.rs`, `eval/shape/{manifest,guard}.rs`, `eval/runner/{cost,trust,sweep}.rs`,
`eval/corpus/{loader,assertions}.rs`, fixtures/*.toml).

## Wave independence (NFR-04 / R-14)
Docs reference Wave-1 behavior conceptually; no code imports them. Wave-1
acceptance passes with these files absent. No Rust / `.claude/protocols/` /
`product/` files modified.

## Issue noted (non-blocking, accuracy point)
There is **no CLI subcommand** for the fixture-corpus sweep — `run_fixture_sweep`
is a library/test entry point (AC-14 proof harness, exercised by `sweep_tests.rs`).
The `[graph_penalty]` levers ARE CLI-accessible via `eval run` on a snapshot. Docs
state this distinction accurately rather than implying a CLI fixture-sweep flag
that does not exist.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced the canonical nan-018
  procedure #4911 (migrate-corpus-+-re-stamp / author-fixture-scenario), convention
  #4910 (shape-change ⇒ corpus migration), ADR-002 #4895, ADR-006 #4894, and the
  eval-harness dual-type pattern #3574. Aligned the migration runbook and authoring
  guide to #4911/#4910 verbatim.
- Stored: nothing novel to store — this is documentation of already-decided
  architecture (ADRs #4894–#4898) and an already-stored procedure (#4911) /
  convention (#4910). No new gotcha or runtime trap was discovered; authoring prose
  that restates committed code/ADRs surfaces no reusable implementation pattern.
