# vnc-046 Retrospective — Architect

MODE: retrospective (SHIPPED, merged PR #936). Selective knowledge extraction from a bugfix→design→delivery feature. Reviewed ARCHITECTURE.md, pseudocode/OVERVIEW.md, gate-3a/3b/3c reports, RISK-COVERAGE-REPORT.md, and all entries stored this cycle.

## 1. Patterns

| Entry | Verdict | Note |
|-------|---------|------|
| #5637 multi_thread test flavor | KEEP as-is | what/why/scope substantive; runtime-panic tell is precise |
| #5638 fallible config compiles at build_project_server call site | KEEP as-is | distinct from #5643 (see below) |
| #5639 codemod positional-arg removal scoping | KEEP as-is | in-call-span gating + `_`-prefix-at-pub-boundary is reusable |
| #5640 field census must live in lib crate | KEEP as-is | includes the runtime IsolationProbe companion; complete |
| #5643 boot guard reads resolved per-slug config | KEEP as-is | PR #936 Finding 1 (post-gate security fix) captured with overlayable-field list + test shape |
| #5629 → **#5645** construction-parity + funnel-completeness | **UPDATED (context_correct)** | added Rule 3 (full `.with_*` builder-chain parity) — the Gate 3a rework insight |

**#5638 vs #5643 — NOT consolidated (deliberate).** They share the "per-slug resolved value" theme but give different actionable guidance: #5638 is about WHERE to compile/thread a fallible config artifact into *construction* (call site, where `r` lives, params-at-end); #5643 is about what value a *boot/validation guard READS* (the resolved per-slug value, never global). Merging would dilute two distinct traps into one blurry entry. Kept separate.

**New pattern — NONE.** The one candidate ("construction-parity audit must walk the full `.with_*` builder chain", from the Gate 3a REWORKABLE FAIL) was folded into the governing pattern **#5629→#5645** as Rule 3 rather than spawned as a sibling. #5629 is where a future auditor actually looks; a near-duplicate sibling would fragment the signal. Rule 3 records the concrete miss (registry field + cap+hold pair enumerated, `.with_signature_scanner` missed → class names with zero counts → AC-07 parity break) and the heuristic (diff per-slug vs daemon builder expressions call-by-call).

## 2. Procedures

**bugfix→design scope transition** (a bug that reveals a state-CLASS split-brain becomes a design feature) — **SKIPPED, not stored.** This is session-type/workflow choreography, which lives in `.claude/protocols/uni/` (CLAUDE.md: "Do not store workflow choreography in Unimatrix"). The bugfix protocol already owns session-type transitions; the judgment trigger ("a single defect that is one instance of a whole mis-wired class") is adequately carried by the pattern layer (#5645 frames the class). No procedure entry.

## 3. ADR Status

ADR-001..005 validated by successful implementation (Gates 3b/3c PASS) + real-HTTPS #800 suite (4/0) + in-process behavioral suite (30/0); the AC-07/OQ-2 live-count leg is a declared, named deferral to the infra-003 Docker gate (structurally wired + boot-asserted in-process), not a defect.

| ADR | Entry | Status | Note |
|-----|-------|--------|------|
| ADR-001 funnel on StoreResolver, no side-map | #5630 | ACTIVE, accurate | — |
| ADR-002 full construction parity | #5631 → **#5635** | corrected in-cycle | scanner triple added (Gate 3a); original #5631 correctly deprecated, chain intact |
| ADR-003 real boot assertion + census | #5632 → **#5636** | corrected in-cycle | IsolationProbe param refinement + categories PER-SLUG census; original deprecated |
| ADR-004 bidirectional N≥2 behavioral gate | #5633 | ACTIVE, accurate | — |
| ADR-005 #925 NOT subsumed | #5634 | ACTIVE, accurate | independent metrics-plane track preserved |

**Flag for supersession: NONE.** The two in-cycle corrections (#5635, #5636) already reconciled the ADRs with the shipped code; both were the right lightweight `context_correct` refinements, not wrong decisions.

## 4. Lessons

| Lesson | Action | ID |
|--------|--------|-----|
| Construction-parity full-builder-chain (Gate 3a rework) | folded into pattern, not a separate lesson | #5645 (Rule 3) |
| Boot/validation guard on a per-slug-overlayable field must read the RESOLVED value (post-gate security fix) | already covered by pattern #5643 — no duplicate | #5643 |
| Tester reap / competing-run-on-resume friction (Stage 3c) | **UPDATED** existing lesson | #5446 → **#5646** |
| Worktree path-confusion (F-02, Read failed 69×) | already covered — no new entry | #5393 (+#3622, #5618) |

**#5446 → #5646 update** added two new facets to the same tester anti-pattern, beyond the existing foreground-execution rule: (a) RESUME-SIDE — a resumed tester must reconcile an in-flight run, not spawn a competing one against the same ports/fixtures (leader had to TaskStop it); (b) REAP-PRONE-SANDBOX ROUTING — SIGTERM-reap (exit 143) kills long non-smoke pytest, so the long/MCP-bound integration proof routes to the Docker gate (infra-003) as a named vehicle rather than local foreground, while everything that CAN complete locally still runs foreground-in-turn.

**Worktree path-confusion — no new lesson.** The F-02 symptom (agents/leader referencing the main-checkout path `/workspaces/unimatrix/product/...` when artifacts live under `/workspaces/unimatrix/.claude/worktrees/bugfix-930/product/...`) is already covered by **#5393** ("Verification agents must run tests from the worktree path, not the sibling main checkout") plus #3622 (design agents write to main-repo paths) and #5618 (implementation agents must not leave their worktree). Storing another would duplicate.

**Not stored (per instruction):** the dogfooding note (this session's own cycle review shows Transcript bytes 0 — a *validation* the #930 bug is real, not an action item); no bug-specific defects as lessons (bugs are GH issues).

## 5. Retrospective Findings

- **Stewardship health: good.** Every design/delivery agent stored substantive, non-duplicative knowledge with what/why/scope. The ADR corrections used `context_correct` (provenance preserved), not deprecate+store. No low-quality or redundant entry required deprecation.
- **The construction-parity pattern is now the load-bearing artifact of this feature.** #5645 carries all three rules (parity, funnel completeness, full-builder-chain) — the single most reusable output for any future per-slug field on `UnimatrixServer`.
- **One systemic gap the pattern now closes:** field-presence audits are insufficient when the field is builder-constructed. Gate 3a caught it once; Rule 3 makes it a standing check.
- **Transcript candidates weighted LOW** (provenance=RECONSTRUCTED, search_complete=false — the #930-empty symptom per ADR-007). No decision-family knowledge extracted from them.

## 6. Edges

**None asserted — bar not met for anything new.** The one edge the briefing flagged to consider — construction-parity pattern —Supports→ ADR-002 — **already exists**: #5645 (successor of #5629) carries the `Supports → #5635` edge forward (confirmed "Carried 2 outgoing edges forward" on the correction). It meets the traversal-necessity bar (a future agent auditing per-slug construction must reach the concrete P1/P2/P3 decision), and it is already present, so no new assertion. No other pair clears the HIGH bar.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (vnc-046 construction parity; tester foreground/background; worktree path confusion), mcp__unimatrix__context_get (#5629/#5630/#5631/#5635/#5636/#5637/#5638/#5639/#5640/#5643/#5446) — reviewed all 11 cycle entries + related prior art (#5393/#3622/#5618 worktree, #5446 tester); found the full-builder-chain insight missing from #5629 and the vnc-046 tester facets missing from #5446.
- Stored: nothing net-new (conservative retro). UPDATED entry #5629 → **#5645** "Per-slug construction parity …" via context_correct (added Rule 3, full `.with_*` builder-chain parity); UPDATED entry #5446 → **#5646** "A swarm sub-agent must not offload terminal deliverable work …" via context_correct (added vnc-046 resume-side + reap-prone-routing facets). No new patterns/procedures/lessons/edges — candidates were either already covered (#5393, #5643) or better folded into the governing entries (#5645), and the bugfix→design transition is protocol-owned choreography.
