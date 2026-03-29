# Retrospective Report: bugfix-439-retro-architect

Feature: bugfix-439
Agent: bugfix-439-retro-architect (uni-architect)

---

## 1. Patterns

### New

**#3727** — `NLI score distribution as private pure helper: max/mean/p75 over NliScores slice`
- Tags: nli, observability, distribution, percentile, pure-function, testability, nli-score-stats, unimatrix-server, tick
- Rationale: The `nli_score_stats(&[NliScores]) -> (f32, f32, f32)` helper introduced in this bugfix is a distinct, reusable pattern — not covered by #2809 (NliProvider gotchas: softmax/label-order/ONNX wiring) or #2743 (apply_nli_sort extraction for re-ranking testability). This pattern is about observability over scored results using a pure nearest-rank percentile helper, following the same shape as `compute_observed_spread` in status.rs.

### Skipped

- **#2809** (NliProvider gotchas): Not superseded. Covers inference-correctness traps; this bugfix adds no new inference gotchas.
- **#2743** (extract NLI sort as pure function): Not superseded. Covers re-ranking logic isolation; nli_score_stats is a different concern.

---

## 2. Procedures

### New

**#3729** — `Read clippy output directly — avoid multi-pipe shell chains that produce output_parsing_struggle`
- Tags: clippy, output-parsing, pipe, shell-filter, tester, cargo, procedure
- Rationale: No existing procedure covered the multi-pipe anti-pattern. Entry #3257 covers crate scoping; entry #3561 covers run_in_background. This fills the gap: use `--message-format=short`, one grep max, read raw output via file when full context needed.

### Skipped

- **#3257** (scope clippy to affected crates): Not updated — still accurate, complementary rather than superseded by #3729.
- **#3561** (run_in_background + TaskOutput): Fully covers the sleep_workaround hotspot. No update needed — referenced in #3729.

---

## 3. ADR Status

N/A — no architectural decisions made during this bugfix.

---

## 4. Lessons

### New

**#3728** — `context_get fails 13x due to integer serialization bug — use context_search as fallback`
- Tags: context_get, integer-serialization, mcp, tool-failure, fallback, context_search
- Rationale: The 13 context_get failures in this session are a distinct failure mode from #3500 (ToolSearch not loaded) and #3703 (forward-reference IDs). The integer serialization bug causes failures spread across different existing entry IDs. No prior lesson covered this specific cause or its context_search fallback strategy.

### Skipped (already covered)

- **Compile cycles (26)**: Entry #3439 (active, comprehensive, covers both multi-file bugfix and struct extension scenarios). No update needed.
- **Sleep workarounds (2 instances)**: Entry #3561 (procedure) and #3498 (lesson) both fully cover run_in_background + TaskOutput. No update needed.
- **Entry #3723** (tick completion logs without score distribution): Already high-quality, no action per brief.

---

## 5. Retrospective Findings — Hotspot Actions

| Hotspot | Action |
|---------|--------|
| tool_failure_hotspot: context_get 13x | New lesson #3728 stored — integer serialization fallback to context_search |
| output_parsing_struggle: clippy 7 pipes | New procedure #3729 stored — use --message-format=short, max 1 grep, run_in_background |
| compile_cycles: 26 cycles | Covered by #3439 — no new entry |
| sleep_workarounds: 2 instances | Covered by #3561/#3498 — no new entry |
| cold_restart / session_timeout / reread_rate | Session gap artifacts — no actionable pattern beyond existing knowledge |
| bash_for_search_count: 46 (positive) | No action needed — confirms good Grep/Glob discipline |
