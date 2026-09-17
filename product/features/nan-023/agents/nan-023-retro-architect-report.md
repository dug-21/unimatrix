# nan-023 Retrospective — Architect Stewardship Report

> Mode: retrospective (SHIPPED — PR #996 merged, issue #990 closed) · Reviewer: uni-architect (swarm id nan-023-retro-architect)
> Scope: stewardship review of this cycle's 10 specialist entries + 6 ADRs; pattern/procedure/lesson extraction; ADR validation; edges.

## 1. Patterns

- **New:** none. The three genuinely-new infrastructure targets are already well-captured:
  - Surgical format-preserving TOML table writer → **#5772** (line-scan header boundaries, exact parent-vs-child match, triple-quote state, malformed→skip). Complete what/why/scope.
  - Per-harness WireLeg wiring layer → **ADR-001 #5763** (decision) + **#5774** (WireLeg note-vs-reason invariant + codex traps) + **#5771** (structural foreign-untouched install by iterating the source tree). Complete.
  - C14 execute-the-manifest verifier (non-tautology) → **#5769** (decouple execute-wired-command from external-harness-fires under Gate-0 trust-feasibility). Complete.
- No genuinely-generic net-new structure is missing; the "structured manifest the verifier executes" abstraction is already carried by ADR-001 #5763 + #5769. Storing another entry would duplicate.
- **Confirmed delivery-validated (high quality):** #5769, #5770, #5771, #5772, #5773, #5774, #5775, #5777, #5778. All match the pattern template (what/why/scope + substantive why).

## 2. Procedures

- **New/updated:** none.
- Gate-0 trust-feasibility precondition (confirm-before-delivery) is already captured as the reusable reasoning in **#5769** (split a trust-gated behavioral AC into tool-independent [hard] vs tool-dependent [documented-conditional]). A standalone procedure would duplicate it.
- Pre-tag real-server exercise is captured as a decision in **ADR-006 #5768 §4**, rooted in lesson **#5267** (release-only gates fail one-tag-per-round). No HOW-TO changed beyond what the ADR states; not extracted separately.

## 3. ADR status

All six validated by Gate 3b (PASS) and Gate 3c (PASS, load-bearing claims re-executed):

| ADR | Entry | Status |
|-----|-------|--------|
| ADR-001 per-harness wiring layer | #5763 | Validated — manifest is single source of truth for verifier/dry-run/summary; WireLeg contract matched exactly (Gate 3b check 3). |
| ADR-002 in-house surgical TOML | #5764 | Validated — byte-for-byte foreign-table preservation suites PASS (R-04); zero new dependency. |
| ADR-003 codex hooks → JS client, `--provider` hint | #5765 | Validated — commands target hook-client/index.js not the binary (AC-08); fail-loud provider (AC-07). |
| ADR-004 skills-only install-if-absent + `--force` | #5766 | Validated — default keeps edited skill byte-for-byte, force overwrites owned only, foreign untouched (AC-01/02). |
| ADR-005 `wire` verb, `--harness`, #960 intent | #5767 | Validated — argv→wire() dispatch, intent gate both arms, wire skips skills+DB (AC-03/11/16). |
| ADR-006 C14 verification spine | #5768 | Validated **as-authored**. It was written conditionally ("where feasible", "conditional on a trusted `.codex/`"). Delivery resolved that conditional: Gate-0 confirmed cloud-trust seeding is **NOT feasible in CI**, yielding **proven(local) / partial(cloud)** — exactly the branch the ADR anticipated. The proven(local)/partial(cloud) resolution is recorded in #5769 and now linked to #5768 via a Supports edge (task 3 satisfied). |

- **Flagged for supersession:** none. No ADR was revealed wrong or incomplete; ADR-006's conditional resolved to a documented boundary, not a defect. No supersession attempted (would require human approval).

## 4. Lessons

- **New: #5780** — "New test suites added outside the CI runner's glob path silently escape CI even while green locally." Generalizable, agent-actionable, repo-recurring (CI globs only `test/hook-client/*`; nan-023's core + C14 suites at `test/` top level are unguarded by CI). Not covered by #5778 (Node-24 bare-runner) or #4780 (size gate). GH#995 tracks the fix; the lesson is the preventive rule for future authors.
- **Not stored (report only):** the 290 KB footprint gate pre-existing RED on `main`, surfaced at Stage 3b. The correct handling (GH Issue + skip/xfail, route gate-raise-vs-trim to human) is already captured by **#4781** and was served + correctly applied this cycle (GH#994 filed, deferred to human). The remaining action is human-owned. No new lesson.
- Did **not** duplicate #5778 or #4780.

## 5. Retrospective findings (hotspot-derived)

- **F-01 cold_restart / F-05 session_timeout / F-04 coordinator_respawns=23:** session-shape artifacts of a 3-session delivery resumed via SendMessage across the human merge gate + long background test waits. Not process defects; no action.
- **F-02 file_breadth / F-03 mutation_spread:** design-artifact-heavy scope phase + pre-tag real-server exercise touching many fixtures. Expected; no action.
- **F-09 sleep_workarounds (7 sleeps):** from the tools/lifecycle integration baseline run. Already covered by lessons #2658/#5102 (use run_in_background + TaskOutput instead of sleep polling), served this cycle. Redundant to re-store — no new entry.
- **F-08/F-10 design_artifact_count 51 / adr_count 6:** informational; a large well-decomposed design. All gates PASS first-pass, 0% rework — decomposition paid off.
- **Baseline outliers** (coordinator_respawn 23 vs 0.4; session_hotspot 6 vs 1.7) are the session-resume shape. What-went-well confirmed: permission_friction 0.0, bash_for_search 136 (Grep/Glob used correctly), post_completion_work 0.0.
- **Transcript candidates:** all 3 sessions reconstructed/low-fidelity (per ADR-007 weighting); event-log echoes only, no gate-failure or human-reasoning narratives. Not over-extracted.

## 6. Relationship edges

Three asserted (all traversal-necessary; source→type→target):

- **#5763 (ADR-001) —Prerequisite→ #5768 (ADR-006):** the C14 verifier is defined entirely on the WireLeg manifest contract; an agent modifying the verifier must read ADR-001 first or misread what it executes.
- **#5769 (pattern) —Supports→ #5768 (ADR-006):** the trust-decoupling technique is what makes ADR-006's proven(local)/partial(cloud) claim achievable; a reader of the ADR's conditional must follow it to see how FIRE/RETURN stay HARD without the external tool.
- **#5772 (pattern) —Supports→ #5764 (ADR-002):** the boundary-scan gotchas (multi-line-string state, exact parent-vs-child header match) are what make ADR-002's "in-house surgical writer, not round-trip lib" decision actually preserve foreign bytes; an implementer must follow it or reintroduce false-boundary corruption.

Deliberately **not** asserted: ADR-002/003→ADR-001 and ADR-005→ADR-001 (prose cross-refs suffice; not traversal-necessary to avoid a wrong decision). Supersession handled as context outcome, not an edge (none needed here).

## 7. Stewardship

- **Reviewed (12):** ADRs #5763, #5764, #5765, #5766, #5767, #5768; patterns #5769, #5770, #5771, #5772, #5773, #5774, #5775, #5777; lesson #5778.
- **Confirmed delivery-validated:** all of the above — no low-quality, miscategorized, or duplicate entries found. ADRs carry full decision/context/consequences; patterns carry substantive what/why/scope; #5778 (lesson) has what-happened/root-cause/takeaway.
- **Corrected:** none.
- **Deprecated:** none.
- **Added:** lesson #5780; edges 5763→5768 (Prerequisite), 5769→5768 (Supports), 5772→5764 (Supports).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search / context_get — reviewed this cycle's ADRs #5763-#5768 and patterns/lessons #5769-#5778; cross-checked prior #4781, #5192, #5267, #4780, #5778 for redundancy.
- Stored: lesson #5780 "New test suites added outside the CI runner's glob path silently escape CI even while green locally" via /uni-store-lesson; 3 typed edges (5763→5768 Prerequisite, 5769→5768 Supports, 5772→5764 Supports). No new patterns/procedures/ADRs — the cycle's specialist entries already cover the new infrastructure; no corrections or deprecations needed.
