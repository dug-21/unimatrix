# Agent Report — vnc-041-agent-2-testplan (Stage 3a Test Plan Design)

## Deliverables (all under product/features/vnc-041/test-plan/)
- OVERVIEW.md — strategy, risk→test map, AC anchors, integration harness plan
- seed-write-primitive.md (C1) — R-03/R-09/R-10, AC-05
- per-slug-seed-renderer.md (C2) — R-04/R-06/R-12/R-14, AC-03
- per-slug-seed-writer.md (C3) — R-05/R-13/R-10/R-03, AC-02/AC-05
- global-serve-seed.md (C4) — R-01/R-02/R-11/R-10, AC-01/AC-06
- seam-warn.md (C5) — R-04/R-06/R-07/R-08, AC-04

Component files map 1:1 to the pseudocode filenames in the Component Map. ~41 planned tests.

## Six load-bearing scenarios all covered
- AC-01/R-01: seed fires with http.enabled=true AND base_dir=None (gate is http.enabled).
- AC-06/R-02: empirical zero-files sentinel (local delta==0) + mandatory negative control (container delta>0).
- AC-05/R-03: no-clobber on BOTH (a) and (b); create_new primitive; no path.exists() precheck (3c structural).
- AC-03/AC-04/R-04/R-06: A→B flip test in both C2 (annotation) and C5 (WARN) — one flip moves both.
- AC-02/R-05: register→resolve_slug_config round-trip, no hand-placement; (a)/(c) byte-unchanged.
- AC-04/R-07: WARN-only equivalence; raw parse never adds an error; once-per-boot-per-key.

## Integration suite plan
infra-001 is a REGRESSION GATE only. Both new behaviors (file seeding, locked-key WARN) are filesystem/
log effects with NO MCP-visible surface → covered by in-crate unit/integration tests; NO new infra-001
tests planned. Stage 3c runs: smoke (mandatory gate) + protocol + lifecycle as a no-regression net
(global seed now fires on the http-enabled boot the harness uses). Per-crate command: cargo test -p unimatrix-server.

## Flagged for delivery (3b)
- C1 extraction: the existing force=true arm uses fs::write (overwrite, config.rs:4858); force=false uses
  create_new (4873). write_if_absent is the force=false no-clobber body ONLY; force=true keeps overwrite
  semantics. The existing test_write_default_config_overwrites_with_force MUST stay green.
- C4 harness depth: prefer function-level (call write_default_config_if_absent against a temp data dir
  under each branch's conditions) + a separate structural placement check, over a full daemon boot. The
  file-count delta assertion is mandatory at any depth.
- C2/C5 flip test mechanism: cleanest if the legend/WARN render accepts a test-injectable ConfigKeyClass
  slice so a single OverlayDisposition flip moves both surfaces without mutating the static production
  registry. Otherwise prove each surface binds to is_per_slug_overlayable independently.

## Open questions
1. C4 harness reachability: is the seed call site reachable function/seam-level, or does the empirical
   AC-06 sentinel require a full `serve` daemon boot under a temp HOME? (Affects test cost; sentinel is
   mandatory either way.) — defer to 3b dev to confirm against main.rs:1011.
2. C5 dedup: does ADR-005's "once per boot per (slug, key)" require an explicit dedup map, or is it
   satisfied by construction (resolver called once per slug per boot, main.rs:1089)? The plan covers both
   — the dedup-map tests are conditional on a map existing.
3. State B reachability in projects/tests.rs Fixture: confirm the harness can construct a State B
   (re-attach) register without bespoke scaffolding (R-13 requires the State B seed proof).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (category=decision, topic=vnc-041) —
  surfaced ADR-001..005 (#5235/#5236/#5237/#5238/#5239), consumed as ground truth for the gate
  corrections; plus #3386 (edge-case-skip → flip test + sentinel mandatory), #4876 (empirical
  gate-integrity → AC-06 negative control), #4749 (content-free logging → WARN names key+slug not value).
- Stored: nothing novel to store — the plans reuse established conventions (traced_test/logs_contain,
  TempBase, Fixture, create_new no-clobber). A reusable "flip-an-OverlayDisposition harness" pattern may
  be worth storing at Stage 3c if it generalizes beyond this feature.
