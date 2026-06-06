# ASS-071: Subagent Sidechain Transcript Value — Implementer-Level Lessons + Capture Architecture

## Question

Subagent sidechain transcripts hold the implementer-level narrative the main thread never sees — dead ends, gotchas, workarounds, rationale never surfaced to the SM. Is that content worth capturing for distillation, and if so, what is the most effective capture architecture given its volume profile (3–11 MB per-session aggregate vs the 4 MiB main-thread buffer)?

## Why It Matters

ass-070 measured zero recall gain from SM-level spawn channels — but never measured the sidechains themselves. They are a different extraction target needing their own ground truth. crt-052 (#689) designs the distillation pass next: if sidechain value is real, it shapes crt-052's inputs; if not, this spike closes the question before crt-052 designs around an assumption. **Sequenced before crt-052.**

## Prior Evidence (verify, don't re-derive)

- Sidechains are plain files at `<transcript-dir>/<session-id>/subagents/agent-<id>.jsonl`, same JSONL schema (`isSidechain: true`, `agentId`, `sessionId`). The hook receives `transcript_path` for the main thread (`hook.rs:204`) — subagents dir derivable, zero new configuration. **326 sidechain files exist locally now.**
- The hook already processes SubagentStop (`hook.rs:67,100`). Sidechain files are complete and immutable once the agent stops — capture is a **one-shot read at SubagentStop**, not incremental tailing. No out-of-order merge, no contiguity tracking (much simpler than vnc-025's delta stream).
- Per-session subagent aggregate runs 3–11 MB — up to 5× the largest observed main thread (2.12 MiB). Raw streaming breaks the 4 MiB buffer math.
- ass-070 validated marker-selection rules with ~95% volume reduction when run as candidate pre-filtering.

## Bounded Questions

### Q1: Corpus and ground truth — stratified by agent type

- Sample the 326 local sidechain files, stratified by agent type (rust-dev, tester, validator, security-reviewer, architect, researcher, pseudocode, …) and by session type (delivery, bugfix, design).
- Label for the Q2 value categories. Reuse ass-070's two-layer method: what was actually curated from those sessions (precision reference) + hand-labeled pass for what was missed (recall ceiling — the value-add).

### Q2: Value categories — what is actually in there

Measure density and quality per category. **The listed categories are seeds, not a fence — the researcher must actively hunt for value categories not listed here.** The spike's job is to determine whether anything worthwhile exists, not to confirm a list.

- **Implementer lessons** — dead ends hit, gotchas worked around, rationale never surfaced to the SM
- **Action/problem provenance** — can the sidechain answer "what did agent X actually do / encounter" questions about a delivery after the fact?
- **cycle_review enrichment** — could sidechain content explain rework hotspots or per-component findings that structured `ObservationRecord`s only count? (Extends ass-070 Q8's opportunity list to the subagent layer.)
- **Attribution provenance** — does the sidechain strengthen the chain of which agent produced which knowledge/decision/code (complementing, not duplicating, the existing agent-report files)?
- **Other** — anything the corpus reveals that the above misses. Negative findings are findings.

### Q3: Per-agent-type value profile — selective capture

- Value density by agent type: the working hypothesis is developers/testers high, reviewers/validators low (their output is already a structured report/comment). Confirm or refute with Q1 data.
- If value is concentrated: recommend a selective-capture policy (which agent types to capture, which to skip) and quantify the volume saved vs value lost.

### Q4: Capture architecture — against the volume math

Evaluate at minimum, against real corpus volumes:
- **(a) Raw one-shot ship at SubagentStop** — simplest; breaks the 4 MiB buffer math at 3–11 MB aggregate. Quantify exactly how badly.
- **(b) Client-side marker pre-filter at SubagentStop** — run the ass-070-validated marker-selection rules in the hook, ship only candidate blocks (~95% reduction claimed — verify on sidechain content, which may have different marker density than main threads).
- **(c) Per-agent distill at stop** — distill in/near the hook, ship distilled output only; raw bytes never leave the client.
- Interaction with Q3: selective capture composes with all three.
- Recommend one, with the quality/cost/volume evidence.

### Q5: Wire and registry shape for the winning option

- `TranscriptDeltaPayload { offset, bytes }` is session-keyed; sidechains need an `agent_id` discriminator or a separate one-shot event type. Recommend the wire shape (respecting the frozen F1 contract — additive event type is fine, mutation is not).
- Registry shape: per-agent buffers vs pre-filtered append to the session buffer vs no buffering at all (option c). Purge lifecycle inheritance from vnc-025 (key removal, cycle-review clear, content-free audit).
- Enterprise seams: capability/bearer inheritance, retention policy applicability — name, don't design.

### Q6: Go/no-go and routing

- Is there enough value to justify capture at all? If yes: what lands in crt-052 vs a separate feature, and what is the minimal first slice?
- If no or marginal: say so plainly — closing this question is a valid spike outcome.

## Output

`product/research/ass-071/FINDINGS.md`:
- Value-category × agent-type density matrix (Q1–Q3), including researcher-discovered categories
- Selective-capture recommendation (Q3)
- Capture architecture recommendation with volume/quality/cost evidence (Q4)
- Wire + registry shape recommendation (Q5)
- **Go/no-go with routing into crt-052 or follow-on** (Q6) — the spike's primary deliverable

## Constraints & Prior Art

- **Privacy**: sidechain transcripts may contain secrets. Corpus stays local — no transcript content committed, stored in Unimatrix, or quoted in FINDINGS beyond short sanitized excerpts.
- **Claude Code JSONL format only** (consistent with ass-070 / crt-052 boundaries).
- **Throwaway instruments only** — any extractor/filter built here is measurement gear, not a deliverable.
- **Frozen F1 wire contract** — recommendations may add event types, never mutate `TranscriptDeltaPayload` or existing bindings.
- Prior art: ass-070 FINDINGS (extractor quality, marker-selection rules, ground-truth method), ass-069 FINDINGS (buffer/purge/attribution mechanics), vnc-025 SCOPE (buffer semantics), crt-052 #689 (the consumer).

## Tracking

GitHub Issue: #690
Consumer: crt-052 (#689) — sequenced before it; findings route into its distillation-input design.
