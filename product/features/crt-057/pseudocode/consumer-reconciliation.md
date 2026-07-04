# Component: Consumer reconciliation (docs half of the CON-1 atomic unit)

Files: `.claude/skills/uni-retro/SKILL.md`, `context_cycle_review` tool description (in
`unimatrix-server` tool registration). `uni-agent-routing.md` is EXCLUDED (not a live consumer — do NOT
edit it; a grep guard on it fails spuriously).

These are DOC edit specs (what changes where), not code. They ship in the SAME atomic unit as the
server change (CON-1) — server alone silently starves the harvest (#5219).

## `uni-retro/SKILL.md` — edit spec

### E1. The candidate-bearing call must request the `transcript{}` block (SKILL.md:43)

Under the new contract a bare `context_cycle_review({format:"markdown"})` returns NO candidates (lean
default). The retro's harvest call must OPT IN via the read-only scoped block:

```
# BEFORE (:43) — returns no candidates under crt-057:
mcp__unimatrix__context_cycle_review({"feature_cycle": "{feature-id}", "format": "markdown"})

# AFTER — full retained candidate set, non-destructive, repeatable:
mcp__unimatrix__context_cycle_review({"feature_cycle": "{feature-id}", "format": "markdown", "transcript": {}})
```

`transcript: {}` (present, all-None) = the full candidate set under the per-cycle cap ≡ `match:".*"`
(FR-8). Note in the surrounding prose that the retrieval is REPEATABLE and NON-DESTRUCTIVE — the retro
may re-call in any scope (`{match:"..."}`, `{anchor:"F-03", window:{millis: 120000}}`, `{phase:"design"}`)
as often as needed; there is NO one-shot to sequence around, and NO purge occurs.

### E2. Loss / honesty surfacing (SKILL.md:102-167 "Consuming transcript_candidates")

- Keep the `transcript_candidates` consumption guidance (candidates + `SessionLossInfo` unchanged).
- ADD guidance for the NEW `transcript_search` response item (per-session `matched` / `search_complete`
  / `elided_bytes` / `provenance` + `resolved_bounds`): a `match` no-match with `search_complete:false`
  is INDETERMINATE ("the buffer was lossy — could be past the tail / in a hole / a 0.81 `Reconstructed`
  rebuild"), NOT "didn't happen." Never treat a bare no-match as a negative.
- The memo-hit note (:165-167 — candidates distilled FRESH from call-time buffers, may differ from cached
  metrics) STAYS TRUE and unchanged (memo-hit still reads the live buffer — cycle-review-handler.md).

### E3. Remove any purge/one-shot framing

- No language may imply "any review carries candidates" (it does not — must opt in via `transcript`) or
  "review purges" / "one-shot extraction." Retrieval is opt-in, read-only, repeatable.
- The Ownership Boundary stays (NG-5): the retro AGENT owns synthesis; the tool returns honest planes
  only, no cross-plane join. Existing "human-intervention ledger" / rework-join guidance is the AGENT's
  synthesis, NOT a tool capability — keep it framed as agent-side.

## `context_cycle_review` tool description — edit spec

Rewrite to document exactly THREE orthogonal, non-destructive axes and state plainly there is no purge
verb:

```
context_cycle_review — non-destructive retrospective for a feature cycle. Three orthogonal axes:
  • format ("markdown" | "json", default markdown) — render only; identical content; never retrieves,
    never purges. (No "summary" value — unknown format → ERROR_INVALID_PARAMS.)
  • force (bool, default false) — recompute the report from durable observations; never retrieves
    candidates, never purges.
  • transcript ({ phase?, anchor?, match?, window? }, optional; omit = summary only) — READ-ONLY scoped
    retrieval over the in-memory candidate buffer. Returns candidates + per-session SessionLossInfo +
    per-session search_complete. Purges NOTHING; repeatable; buffer survives.
This tool has NO purge verb. Reclamation is handled entirely by the independent backstops (24h TTL /
64-session cap / session-close).
```

## AC-16 grep guard (corrected four-doc set)

FAILS if any of `uni-retro/SKILL.md`, the tool description, `uni-delivery-protocol.md`,
`uni-bugfix-protocol.md` still contains `include_transcript_candidates` / "any review carries
candidates" / "review purges" language, OR omits the `transcript{}` reference. MUST NOT grep
`uni-agent-routing.md` (excluded).

## Key test scenarios

- End-to-end harvest-fires (R-02 sc.1): drive the reconciled `/uni-retro` path; assert the call issues a
  `transcript{}` block (not a bare default) and the response actually contains candidates + loss.
- Doc/grep guard over the corrected four-doc set (R-02 sc.2); assert `uni-agent-routing.md` absent from
  the atomic-unit ship.
- Tool description states no purge verb + lists the three axes (R-02 sc.3).
- SKILL.md no longer sequences around a one-shot extraction (R-02 sc.4).
