# Agent Report: 881-investigator (uni-bug-investigator)

Diagnosis posted: https://github.com/dug-21/unimatrix/issues/881#issuecomment-4870060638

## Outcome
Root cause confirmed with high confidence, plus two findings beyond the issue text:

1. Reported defaults verified: `unwrap_or(false)` at `graph_read_subgraph.rs:158`, `graph_read_neighbors.rs:164`, **and a third site the issue omits — `graph_read_path.rs:135`**.
2. **G1 (new)**: the depth-1 neighbors arm (`graph_read_neighbors.rs:161-162`, depth default 1) never consults `resolve_supersessions` — even explicit `true` silently no-ops. Flipping the :164 default alone leaves the most common neighbors call broken; `neighbors_sql` needs resolve support.
3. NG-1 confirmed: `build_edges_view` (`get_edges.rs:44`) renders stored target ids/titles with no resolution — a deliberate vnc-042 deferral (ADR-003 #5387) now promoted to fix.
4. **ADR conflict**: vnc-042 ADR-001 (entry #5388, stored 2026-07-01) explicitly forbids this flip ("Do NOT unify… either consistency fix is a regression"). Issue #881 (human, 2026-07-02) overrides it. The fix MUST `context_correct` #5388 or a future agent will revert the flip per the ADR's own instruction.

Fix approach (5 steps), blast radius (no internal `handle_graph` callers; no JS-client `context_graph` callers; agent-side consumers all want terminals), regression risk (medium-low; ~31 graph-read + ~18 get-edges test references need review), and missing tests (cross-surface consistency test; depth-1 resolve test; get-edges deprecated-target test) — all in the GH comment.

## Follow-up: consumer map (human question)
Posted: https://github.com/dug-21/unimatrix/issues/881#issuecomment-4870158848

Per-consumer verdict — is the OFF default reachable by briefing/injection/gating?
- **context_briefing**: NOT a context_graph consumer. `tools.rs:1547` → `IndexBriefingService::index` (`index_briefing.rs:135`) → SearchService `RetrievalMode::Strict` (hard Active-only + non-superseded, `search.rs:759-763`) + Active post-filter (`index_briefing.rs:207-212`). Output shape has zero edge data (`response/briefing.rs:25-37`).
- **Injection (UserPromptSubmit/SubagentStart/UDS Briefing)**: NOT a context_graph consumer. `listener.rs:1336/1498/1661` — same Strict search path; renders entries only, no edges.
- **Gating**: no server-side gating code reads the graph — `handle_graph`'s only caller is the MCP tool dispatch (`tools.rs:~3911`). Real gating exposure is agent-side: `uni-capability` SKILL.md:171/:207 recomputes the next-unblocked capability set via default-parameter `context_graph` (depth-1 neighbors → hit by G1: flag bypassed entirely).

**Bottom line**: defect reachable ONLY via external agent MCP calls of context_graph; internal pipelines are provably immune via RetrievalMode::Strict. Fix still justified (agents are the tool's whole consumer base and universally omit the undocumented flag), severity framing softens, G1 rises in importance. Issue/changelog wording should say "external agent callers", not "briefing/injection/gating consumers".

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — direct hits: #5388 (ADR forbidding the flip), #5387 (NG-1 deferral), #4507 (original audit-mode default rationale), #4494/#4468 (resolution mechanics patterns).
- Stored: nothing novel to store — findings are issue-specific code defects (bugs are GH issues, not lessons); ADR-conflict handling is encoded in the fix plan (context_correct #5388), not a lesson.
