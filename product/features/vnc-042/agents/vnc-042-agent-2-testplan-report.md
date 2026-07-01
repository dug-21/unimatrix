# Agent Report — vnc-042-agent-2-testplan (Stage 3a Test Plan Design)

## Deliverables
- `product/features/vnc-042/test-plan/OVERVIEW.md` — strategy, risk→test mapping, integration harness plan, CI notes
- `product/features/vnc-042/test-plan/context-get-handler.md` — R-02/R-03/R-04/R-09 + call-site
- `product/features/vnc-042/test-plan/response-formatter.md` — R-01/R-06/R-07/R-08 + byte-identity canaries
- `product/features/vnc-042/test-plan/follow-to-current-reexport.md` — R-05 build/gate/call-site

Component plans map 1:1 to the IMPLEMENTATION-BRIEF component map / pseudocode boundaries.

## Risk coverage
All 12 risks mapped to concrete tests. Criticals: R-01 (TS-01/02 canaries stay green + strip-and-compare),
R-02 (behavioral field-absent⇒resolves, the single highest-value test). Regression guards flagged edit-as-FLAG:
TS-01, TS-02, NFR-06, graph_queries_tests.rs. Accepted/flagged: R-09 (non-code coverage impossible by design),
R-10/R-12 documented-not-gated, R-11 CI-only.

## AC coverage
AC-01→TS-04, AC-02→TS-04/05, AC-03→TS-06, AC-04→TS-07, AC-05→BLD-02 grep, AC-06→TS-09 behavioral,
AC-07→TS-08, AC-08→TS-06 ext. All 8 covered.

## Integration suite plan
Gate: smoke (mandatory). Full: test_tools, test_protocol, test_lifecycle, test_get_edges, test_edge_cases.
6 new MCP tests specified (anchored on existing correction-chain fixtures). Requires additive
`follow_supersessions` kwarg on harness `context_get` client (client.py:496 + uds_client.py:379 mirror) —
tracked, not a surprise.

## Open questions / flags
- OQ-3 json shape: structured `resolution` object (ADR-003 recommended); flat-`note` toggle flips json asserts only.
- R-09 non-code-consumer behavioral coverage impossible by design — flag for human; proxy = tool-desc assert (BLD-04).
- Store-layer read-back tests EXCLUDED as false positives (#5383).

## Knowledge Stewardship
- Queried: context_briefing + context_search — #5388 (vnc-042 ADR-001 divergence), #4781 (Stage-3c xfail procedure),
  #5383 (blast-radius partitioning, store-layer false positives), #3789 (mandatory MCP-dispatch integration test).
- Stored: nothing novel — harness `context_get` kwarg is an additive mirror of the existing `include_edges` pattern;
  #5383 governs the exclusion rule. Revisit storage at Stage 3c if a new fixture/harness technique emerges.
