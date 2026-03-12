## ADR-004: Terminal-Only Output for Agent Orientation Recommendations

### Context

`/unimatrix-init` Phase 2 scans `.claude/agents/**/*.md` and produces a recommendation report identifying agents missing context_briefing calls, outcome reporting, and unimatrix-* skill references.

Two output options:
1. **Terminal-only**: print the report, no file written
2. **File output**: write a recommendation report file (e.g., `.unimatrix-init-recommendations.md`)

The SCOPE.md resolved design decisions specify terminal-only, with the rationale: "files become stale immediately; the scan can be re-run."

### Decision

Agent orientation recommendations from `/unimatrix-init` Phase 2 are printed to the terminal only. No file is written.

**Rationale**:
- Agent definition files change frequently during feature development. A recommendation file written today is wrong tomorrow when agents are updated.
- The scan is cheap to re-run — it only reads agent files that are already present. Re-running `/unimatrix-init` with `--dry-run` re-produces the recommendations without modifying CLAUDE.md (idempotency guard).
- A recommendation file in the repo root creates ambiguity: is it authoritative? Is it current? Should it be committed? Terminal output sidesteps all these questions.
- The recommendation report serves its purpose at the moment of reading. It is not reference material that needs to persist.

**Recommendation format** (terminal):
```
Agent Orientation Report
========================
Agent file                     | Missing                          | Suggested Addition
-------------------------------|----------------------------------|------------------------------------------
.claude/agents/custom-dev.md   | context_briefing, unimatrix-*   | Add context_briefing call at session start
                                |                                  | Reference /unimatrix-seed for knowledge queries
.claude/agents/my-reviewer.md  | outcome reporting                | Add /record-outcome reference at session end
```

If no `.claude/agents/` directory exists: print "No agent files found — no orientation recommendations." (Not an error.)

### Consequences

- No stale recommendation files accumulate in the repo
- Users must re-run `/unimatrix-init --dry-run` to get fresh recommendations — this is a feature, not a bug (always current)
- The recommendation report cannot be shared asynchronously (e.g., sent to a teammate) — acceptable given the scope of this feature
- Future: if there is demand for persistent recommendations, `/unimatrix-init --report` could write a file, but this is out of scope for nan-003
