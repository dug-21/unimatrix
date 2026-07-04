---
name: "uni-retro"
description: "Post-merge retrospective — extracts patterns, procedures, and lessons from shipped features into Unimatrix. Use after a feature PR is merged."
---

# Retro — Post-Merge Knowledge Extraction

## What This Skill Does

Analyzes a shipped feature and shares valuable feedback with the human, reports Unimatrix usage, and extracts reusable knowledge — patterns, procedures, and lessons — into Unimatrix. This is how the project learns.

---

## Inputs

From the invoker:
- Feature ID (e.g., `col-011`)
- PR number (merged)
- GH Issue number

---

## When to Go Lightweight

Not every feature needs a full retro:

| Situation | Action |
|---|---|
| Zero gate failures, no rework, zero hotspots | Skip lesson extraction. Focus on patterns/procedures only. |
| Minor enhancement (1-2 components) | Check for pattern drift only, skip procedure review. |
| New infrastructure introduced | Full retro — high likelihood of new patterns and procedures. |
| Multiple SCOPE FAILs or heavy rework | Full retro — prioritize lesson extraction. |
| Many Warning hotspots or baseline outliers | Full retro — prioritize hotspot-driven lessons and procedure updates. |

---

## Phase 1: Data Gathering & Retrospective Analysis

Gather all evidence about the shipped feature:

1. **Run retrospective analysis** (if observation data exists). The candidate-bearing harvest call MUST
   opt in to verbatim candidates via the read-only scoped `transcript: {}` block — a bare call returns
   the summary report with NO candidates:
   ```
   mcp__unimatrix__context_cycle_review({"feature_cycle": "{feature-id}", "format": "markdown", "transcript": {}})
   ```
   This returns structured data: metrics, hotspots, baseline comparisons, narratives, and recommendations,
   PLUS the scoped `transcript_candidates` section (candidates + per-session `SessionLossInfo`).

   `transcript: {}` (present, all fields omitted) is the degenerate full-candidate case ≡ `match: ".*"` —
   the full retained candidate set under the per-cycle cap. Retrieval is **read-only, non-destructive, and
   repeatable**: the call purges NOTHING, and you may re-call it as often as needed in any scope
   (`{"match": "..."}`, `{"anchor": "F-03", "window": {"millis": 120000}}`, `{"phase": "design"}`). There
   is no one-shot extraction to sequence around and no purge to avoid re-triggering.

   **Ownership boundary (NG-5): retro synthesis is AGENT-owned.** The tool returns honest planes only —
   the Plane-A summary report plus the scoped Plane-B candidates + `SessionLossInfo`. It performs no
   attribution, no cross-source join, and no causal claim. All synthesis below (rework-why joins, the
   human-intervention ledger, phase narration) is YOUR work over those returned planes, not a tool
   capability. Read [Consuming `transcript_candidates`](#consuming-transcript_candidates) before analyzing.
   When the section is ABSENT (no transcripts / nothing to harvest), proceed normally — the cycle review
   still produces its standard report.

2. **Analyze the retrospective data** — extract actionable findings:

   a. **Hotspots by severity** — Classify each hotspot:
      - `Warning` hotspots → potential lessons or procedure gaps
      - `Info` hotspots → note trends, may not need action
      - Key hotspot types to watch:
        - `orphaned_calls` → tool invocations with no terminal event — check context overflow or parallel call management
        - `sleep_workarounds` → agents using sleep instead of run_in_background
        - `cold_restart` → context loss after gaps, agents re-reading files
        - `coordinator_respawns` → SM lifetime/handoff issues
        - `post_completion_work` → significant work after task marked done (scope issue?)
        - `lifespan` → agent running too long (context overflow risk)
        - `mutation_spread` → touching too many files (coupling/scope creep?)
        - `file_breadth` / `reread_rate` → agents inefficiently navigating codebase

   b. **Baseline outliers** — Any metric with `status: "Outlier"` deserves attention:
      - Is it a positive shift (e.g., higher `parallel_call_rate`)? Note as trend.
      - Is it a problem (e.g., high `post_completion_work`)? Extract lesson.
      - Is it a `NewSignal`? First time this metric has a non-zero value — note for future tracking.

   c. **Recommendations** — The retrospective returns specific actionable recommendations.
      Each one maps to either a procedure update or a lesson learned.

   d. **Narratives** — Temporal clustering of events. Look for:
      - Burst patterns (many events in short window → agent struggling)
      - Sequence patterns (repeated cycles → inefficient workflow)
      - Top files (which files caused the most friction)

3. **Read feature artifacts**:
   - `product/features/{id}/architecture/ARCHITECTURE.md`
   - `product/features/{id}/pseudocode/OVERVIEW.md`
   - `product/features/{id}/testing/RISK-COVERAGE-REPORT.md`
   - `product/features/{id}/reports/gate-3a-report.md`
   - `product/features/{id}/reports/gate-3b-report.md`
   - `product/features/{id}/reports/gate-3c-report.md`

4. **Check for rework signals**: Did any gate fail before passing? Read the gate report for what went wrong.

5. **Review the full PR commit history — including the post-gate rework window**:
   ```bash
   gh pr view {pr} --json commits --jq '.commits[] | "\(.oid[0:8]) \(.messageHeadline)"'
   ```
   a. Look for rework commits (`fix(gate):`) — these indicate where the process struggled.
   b. **Post-gate rework window**: identify the last gate-report commit (`review: gate 3b/3c`, `test: risk coverage`). Every commit AFTER it up to merge is rework the gate reports do not cover — typically human-directed fixes done outside the delivery session. Read each one (diff summary + any agent reports it adds under `product/features/{id}/agents/`). These commonly carry the highest-value lessons of the whole cycle.
   c. **Post-gate decisions**: scan PR comments and GH issue comments dated after the last gate report — human/uni-zero decisions (constraint changes, gate redefinitions, deferrals) recorded there are retro input. Flag any knowledge entry stored mid-delivery that a later decision obsoleted.

6. **Share the interim results**: Provide cleanesed view of the context_cycle_review output with key observations before moving forward.  At a MINIMUM show the ##Phase Narrative and ##Phase Timeline.  Also Highlight a specific section of content retrieved during the cycle... (This reinforces the Unimatrix Value Prop)

---

## Consuming `transcript_candidates`

The cycle-review response may carry a transient `transcript_candidates` section (crt-052). The server
SELECTS whole marker-matched user/assistant blocks from the reviewed feature's session buffers and
attaches them to the response; **the agent EXTRACTS all semantics.** Rules select, the agent extracts —
there is no server-side semantic extraction or family classification.

The section is response-transient only: it is NOT persisted, NOT part of the memoized
`RetrospectiveReport`, and reaches the knowledge base ONLY through your `context_store` writes below.

### Shape

```
transcript_candidates:
  candidates: [ TranscriptCandidate ... ]   # session_id, byte_offset, ts, family_hints, text (whole block)
  loss:       [ SessionLossInfo ... ]        # session_id, elided_bytes, has_holes, provenance, dropped_candidates
```

### The four marker families (hints are ADVISORY)

Each candidate carries one or more `family_hints` drawn from four families. The hints are a STARTING
POINT only — **you decide the family and re-classify as needed.** Map each candidate to a knowledge store
with feature attribution (`feature` / tags scoped to `{feature-id}`):

| Family hint | Where it usually goes |
|-------------|-----------------------|
| `Decision`  | ADR — `/uni-store-adr` |
| `Rework`    | pattern or lesson, as appropriate |
| `Lesson`    | `/uni-store-lesson` |
| `PhaseGate` | procedure or gate-narrative, as appropriate |

### How candidates fold into extraction (the ass-070 Q8 folds)

These candidates feed the same Q8-class narratives this skill already builds in Phase 2. Fold them in:

- **Rework-why narratives** — join `Warning`-level hotspots to TIMESTAMP-ADJACENT candidates (`ts` /
  `byte_offset`) to recover the reasoning behind the rework.
- **Gate-failure units** — treat a gate-failure narrative as a single UNIT. Do not fragment one gate
  failure across multiple stores.
- **Human-intervention ledger** — build a ledger from USER-block candidate content (human redirections,
  constraint changes, deferrals).
- **Phase narration** — narrate phase transitions from `PhaseGate`-family candidates.

### Provenance weighting (ADR-007) — read `loss` before you trust a candidate

`SessionLossInfo.provenance` is `Primary` or `Reconstructed`:

- **`Reconstructed`** candidates came from the fidelity-floor fallback (0.81 ceiling, DEC-weakest). Weight
  them LOWER and temper decision-family extraction from them.
- **High `elided_bytes`** means buffer head was clipped. Elision clips the HEAD of the stream — the
  highest-value early `Decision` (DEC) content (ass-070 Q5). When `elided_bytes > 0`, assume early
  decisions may be lost and temper decision-family confidence for that session.
- **`has_holes: true`** means the readable window is non-contiguous; narratives spanning a hole may be
  incomplete.

### Cap-drop awareness (AC-08)

`SessionLossInfo.dropped_candidates > 0` means the per-session or per-cycle volume caps truncated
candidates for that session. Note in the resulting narrative that it may be incomplete for that session —
the drop is reported, never silent.

### Scoped-match honesty — a no-match is INDETERMINATE, never a negative (ADR-003)

When you pass a scoped `{"match": "..."}` (or `anchor` / `window` / `phase`) block, each session carries a
per-session search result — `matched`, `search_complete`, `elided_bytes`, `provenance`, and
`resolved_bounds`. Read `search_complete` before you interpret any absence:

- A `match` that returns **no hit with `search_complete: false`** is **INDETERMINATE**, NOT "it didn't
  happen." The buffer was lossy for that session — the target may be past the elided tail, inside a hole,
  or in a `0.81` `Reconstructed` rebuild. Never treat a bare no-match as a negative signal.
- Only a no-hit with `search_complete: true` over a `Primary`, hole-free window is a trustworthy absence.
- `resolved_bounds` reports the window actually searched — use it to see how much of the buffer the scope
  could even reach before you draw any conclusion.

### Call-time vs cached (OQ-4 / AC-05) — IMPORTANT

Candidates reflect the buffer content present **AT CALL TIME**, not the memoized report. On a memoization
HIT, the standard `RetrospectiveReport` is served from cache, but the candidates are distilled FRESH and
**may differ from the cached metrics.** Treat candidates as call-time content; do not assume they line up
with the cached report.

### Extraction is your job, not the server's

Every store you make from these candidates goes through `context_store` (`/uni-store-adr`,
`/uni-store-lesson`, `/uni-store-pattern`, `/uni-store-procedure`) with feature attribution to
`{feature-id}`. This is the ONLY path distillation output reaches the knowledge base — the server never
writes it (two-pipe boundary, AC-09).

### Dependency-posture review gate (AC-13 / NFR-6)

crt-052 adds a regex-class crate only — no LLM/NLP runtime. When reviewing the crt-052 cycle, confirm
`cargo audit` passes and the `Cargo.toml` / `Cargo.lock` dependency diff shows only a regex-class
addition. This is a review-gate check, not agent extraction work.

---

## Phase 2: Pattern & Procedure Extraction (MUST be a subagent)

**Before spawning the architect**, prepare a structured retrospective briefing from Phase 1. This replaces the vague "paste summary" — give the architect concrete data to work with.

Build the briefing:

```
RETROSPECTIVE BRIEFING for {feature-id}
========================================

Session stats: {session_count} sessions, {total_records} records, {total_tool_calls} tool calls, {total_duration_secs}s

HOTSPOTS ({count} detected):
{For each hotspot: "- [{severity}] {rule_name}: {claim} (measured: {measured}, threshold: {threshold})"}

BASELINE OUTLIERS:
{For each baseline entry with status "Outlier" or "NewSignal":
  "- {metric_name}: {current_value} vs mean {mean} (stddev {stddev}) — {status}"}

RECOMMENDATIONS FROM RETROSPECTIVE:
{For each recommendation: "- [{hotspot_type}] {action} — {rationale}"}

REWORK SIGNALS:
{gate failures, rework commits from Phase 1 step 4-5}

POST-GATE REWORK (commits after the last gate report → merge, Phase 1 step 5b-c):
{For each commit: "- {sha} {headline} — {what it fixed; lesson candidate?}"}
{For each post-gate decision from PR/issue comments: "- {decision} — {source}"}
{Mid-delivery knowledge entries obsoleted by post-gate decisions: "- #{id} {title} — {what changed}"}

TRANSCRIPT CANDIDATES (only if the response carried a transcript_candidates section — crt-052):
{Pass the candidates + loss verbatim. Note for the architect: family_hints are ADVISORY (re-classify);
 weight Reconstructed/elided sessions lower (ADR-007); candidates are call-time content that may differ
 from the cached report (OQ-4); fold per "Consuming transcript_candidates" — rework-why hotspot joins,
 gate-failure units, human-intervention ledger, phase narration.}
```

Spawn `uni-architect` to review what was built and extract reusable knowledge:

```
Agent(uni-architect, "
  Your agent ID: {feature-id}-retro-architect
  Your Unimatrix agent_id: uni-architect
  MODE: retrospective (not design)
  Feature: {feature-id}

  You are reviewing a SHIPPED feature to extract reusable knowledge.  More entries not better.  You are looking for aspects future agents will benefit from.  Be selective.
  You are NOT designing anything new.

  Read these artifacts:
  - product/features/{id}/architecture/ARCHITECTURE.md
  - product/features/{id}/pseudocode/OVERVIEW.md (component structure)
  - product/features/{id}/reports/gate-3a-report.md (design review)
  - product/features/{id}/reports/gate-3b-report.md (code review)
  - product/features/{id}/reports/gate-3c-report.md (risk validation)
  - product/features/{id}/testing/RISK-COVERAGE-REPORT.md

  {paste the RETROSPECTIVE BRIEFING from above}

  YOUR TASKS:

  0. STEWARDSHIP REVIEW — Before extracting new knowledge, assess entries already stored during this cycle:
     a. Query: `mcp__unimatrix__context_search({"query": "{feature-id}", "k": 20})`. Also try feature_cycle tag if available.
     b. For each entry, assess against its category template:
        - **Patterns**: Has what/why/scope? Is "why" substantive (not "it works")?
        - **Lessons**: Has what-happened/root-cause/takeaway? Is takeaway actionable?
        - **Procedures**: Has numbered steps? Are steps specific (not generic)?
     c. Low-quality entries (missing structure, no substantive "why", API docs disguised as patterns):
        correct via `context_correct` or remove via `context_deprecate` as appropriate.
     d. Miscategorized entries: correct category via `context_correct`.
     e. High-quality entries confirmed by delivery: carry forward into steps 1-4 as evidence.

  1. PATTERN EXTRACTION — For each component implemented:
     a. Use /uni-query-patterns to find existing patterns for the affected crate(s)
     b. If the component followed an existing pattern: verify it's still accurate.
        If the pattern drifted, use /uni-store-procedure or context_correct to update it.
     c. If the component established a NEW reusable structure (used in 2+ features
        or clearly generic): store it via mcp__unimatrix__context_store({"category": "pattern", ...}).
     d. If the component was one-off: skip — don't store patterns for unique work.

  2. PROCEDURE REVIEW — Check if any HOW-TO changed:
     a. Did the build/test/integration process change?
     b. Did schema migration steps change?
     c. Was there a new technique that future developers need?
     If yes: use /uni-store-procedure (new) or context_correct (update existing).

  3. ADR VALIDATION — For each ADR created during this feature:
     a. Was the decision validated by successful implementation?
     b. Did implementation reveal that an ADR was wrong or incomplete?
        If so: flag for supersession (do NOT supersede without human approval).

  4. LESSON EXTRACTION — Two sources:

     A. From gate failures and rework:
        a. Are there items future agents can learn from? (Don't just report failures)
        b. Is the lesson generalizable beyond this feature?
        c. If yes: use /uni-store-lesson.

     B. From retrospective hotspots and recommendations:
        For each Warning-severity hotspot, ask:
        - Is this a recurring problem (check baseline — is it consistently above threshold)?
        - Can it be prevented by a procedure change or config update?
        - If yes: If HUMAN must take the action - report only (don't store). If future agents need to know: store as lesson (/uni-store-lesson) or procedure (/uni-store-procedure).

     C. From baseline outliers:
        - Positive outliers (improvements): note what changed and why — may be a new pattern.
        - Negative outliers (regressions): root-cause and store as lesson if generalizable.

     D. From the post-gate rework window (briefing section POST-GATE REWORK):
        - Each post-gate commit exists because the gates missed something — root-cause WHY
          the gates missed it, not just what was fixed. That root cause is the lesson.
        - For each mid-delivery entry obsoleted by a post-gate decision: correct it via
          context_correct (or deprecate) NOW — stale guidance is worse than none.

     E. From the transcript_candidates section (briefing section TRANSCRIPT CANDIDATES, if present):
        - family_hints are ADVISORY — you decide the family. Decision→ADR, Lesson→lesson,
          Rework→pattern/lesson, PhaseGate→procedure/gate-narrative. Store with feature attribution.
        - Read the loss list first (ADR-007): weight Reconstructed-provenance candidates lower
          (0.81 floor); when elided_bytes > 0 assume early Decision content may be lost and temper
          decision extraction; note dropped_candidates > 0 as a possible incomplete narrative.
        - Fold per "Consuming transcript_candidates": join Warning hotspots to timestamp-adjacent
          candidates for rework-why; keep gate failures as single units; build a human-intervention
          ledger from USER blocks; narrate phase transitions from PhaseGate candidates.
        - Candidates are call-time content (OQ-4) — they may differ from the cached report; do not
          reconcile them against the cached metrics.

  5. RELATIONSHIP EDGES (retro graph-completion — HIGH bar, default none):
     Retro is the moment to assert the typed edges that COULDN'T be made at authoring — every ID
     and outcome in this cycle now exists. See /uni-store-adr for the full convention. Assert ONLY
     when a future agent must TRAVERSE the link to avoid a wrong decision; most entries get ZERO
     edges and that is correct. Three types only:
        - Supports: a lesson/pattern from this cycle that DIRECTLY caused or validates an ADR
          (context_edge add, source = lesson/pattern, target = decision).
        - Prerequisite: an intra-feature ADR that must be read before another is correct.
        - Contradicts: a real, decision-blocking conflict surfaced this cycle.
     Do NOT assert RelatedTo/Mentions/About/Informs; do NOT use edges for supersession (that is
     context_correct); do NOT aim for coverage. One-clause justification per edge or don't assert it.

  Return:
  1. Patterns: [new entries with IDs, updated entries with IDs, skipped with reason]
  2. Procedures: [new/updated with IDs]
  3. ADR status: [validated ADRs, flagged-for-supersession ADRs with reason]
  4. Lessons: [new entries with IDs]
  5. Retrospective findings: [hotspot-derived lessons, recommendation actions taken, outlier notes]
  6. Relationship edges: [edges asserted: source -> type -> target + one-clause why, or 'none — bar not met']")
```

---


## Phase 3: Worktree Cleanup

Worker agents spawned with `isolation: "worktree"` create directories under `.claude/worktrees/`. Each contains a full `target/` build directory (~1-2GB). Clean up after merge.

```bash
# List worktrees to find stale agent-created ones
git worktree list

# Remove each stale worktree (safe — feature is merged)
git worktree remove .claude/worktrees/{agent-id}/ 2>/dev/null

# Prune stale entries
git worktree prune
```

If a worktree has uncommitted changes, warn the human — do NOT force-remove.

---

## Phase 4: Summary & Outcome

Collect all knowledge base changes from Phases 2-3.

**Commit retro artifacts** before recording outcome:
```bash
git add product/features/{id}/agents/
git commit -m "chore: add retro artifacts ({feature-id})"
git push origin main
```

**Return format:**
```
RETROSPECTIVE COMPLETE — Knowledge base updated.

Cycle: {feature-id}
PR: #{pr-number} (merged)

Retrospective summary:
- Sessions: {session_count}, Tool calls: {total_tool_calls}, Duration: {duration}
- Hotspots: {count} ({warning_count} warnings, {info_count} info)
- Baseline outliers: {list metric names and status}

Knowledge delivered:
- {N} entries served across {N} sessions. Example: #{id} "{title}" retrieved in {phase} — {one sentence on how it shaped the work}.

Knowledge curated:
- Patterns: {count} new, {count} updated
- Procedures: {count} new, {count} updated
- Lessons learned: {count} new ({count} from hotspots, {count} from gate failures)
- ADRs validated: {count}
- ADRs superseded: {count}

Details:
{list each entry with Unimatrix ID, title, and whether new or updated}
```
