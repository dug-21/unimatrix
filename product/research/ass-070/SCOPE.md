# ASS-070: Transcript Distillation Extractor Quality — Decisions / Rework / Phase Narrative

## Question

Given an authoritative session transcript (the vnc-025 buffer, F2/#670), how well can an automated pass extract the *meaningful bits* — *decisions*, *rework narrative*, *patterns/lessons*, *phase narrative* — and which extractor architecture gives crt-052 (#689) the best quality/cost: server-side rule/marker extraction, agent-side semantic extraction over server-surfaced candidate excerpts, or a hybrid?

## Why It Matters

This is the qualitative payoff of the whole client-streamed-transcript investment (`goal:self-learning`). ass-069 Q5 settled *where* distillation runs (server-side insertion at `context_cycle_review`, additive to the 23 detection rules, reusing `synthesis.rs` / `phase_narrative.rs`) and *what* it targets — but explicitly did not measure extractor quality (Open Thread 2: "requires a separate measurement spike"). crt-052's central design decision — its extraction approach — is scoped to be made "on evidence, not guesses." This spike is that evidence.

**Deferral lifted**: #683 deferred this until "F2 ships and transcripts accumulate." That is unnecessary — the bytes F2 buffers are the local Claude Code JSONL transcripts that already exist for this project's real delivery/bugfix sessions. The corpus exists today; F2 only changes the transport. Running this spike parallel to F2 delivery means crt-052 can start the moment vnc-025 ships.

## Bounded Questions

### Q1: Corpus and ground truth

- Assemble a corpus of 8–12 real historical Unimatrix sessions (delivery, bugfix, design — local Claude Code JSONL transcripts), spanning short bugfixes and multi-hour feature sessions.
- Ground truth, two layers: (a) **curated-knowledge reference** — the ADRs, lessons, patterns, and GH issue comments those sessions actually produced (human-validated by construction; an extractor that finds these agrees with human curation); (b) **hand-labeled pass** over a subset for items humans *didn't* curate (the recall ceiling — what extraction could add beyond current practice).
- Label per target: decision / rework-narrative / pattern-lesson / phase-narrative.

### Q2: Rule/marker extractor — quality floor

- Build a throwaway rule-based extractor (regex/markers: ADR headings, decision phrases, cycle/phase markers, rework signals — reverts, "instead", repeated tool failures, gate rejections).
- Measure precision/recall per target against the Q1 ground truth.
- This is the zero-LLM floor: what the server can do alone (crt-052 constraint: the server has no LLM — verify in passing whether the existing GGUF surface is embeddings-only or generation-capable, and note if that changes the calculus).

### Q3: Agent-side semantic extraction — quality ceiling

- Simulate crt-052's candidate-excerpt path: server surfaces transcript excerpts in the `context_cycle_review` response; the *calling agent* (an LLM by definition) extracts and stores via `context_store` — attribution-preserving, the agent curates.
- Measure precision/recall on the same corpus; measure cost (tokens added to a cycle-review turn) and latency.

### Q4: Hybrid division of labor

- The realistic architecture is rules-select-candidates → agent-does-semantics. Where is the right cut? Measure: how much can rule-based candidate *selection* shrink the excerpt volume (Q3 cost) before agent-side recall degrades?
- Output a concrete recommendation: what runs server-side, what is surfaced, what the agent does.

### Q5: Input-window adequacy (feedback to vnc-025/F3)

- vnc-025 retains a 4 MiB ring-tail with elision. Against the Q1 corpus: what fraction of ground-truth items fall in transcript regions that would be elided in real multi-hour sessions?
- If early-session decisions are systematically lost: is the answer a bigger knob, or periodic distill-and-truncate (ass-069 Open Thread 3)? Recommend, with data.

### Q6: Integration surface check

- Confirm the `synthesis.rs` / `phase_narrative.rs` insertion fit and the snapshot-and-release discipline (never parse under the registry lock) against the winning Q4 architecture.
- Reconstruction-fallback input (crt-052): does the winning extractor degrade acceptably when fed `ObservationRecord`-reconstructed input instead of real transcript? A rough delta is enough — crt-052 designs the fallback.

### Q7: ObservationRecord population boundary — verify and document (plain-language deliverable)

- Document, with code evidence, exactly how `ObservationRecord` is populated today (hook events → dispatch → rows) and **verify the transcript path changes none of it**: deltas filtered before `insert_observations_batch`, no field of any observation row sourced from buffer content, 23 detection rules' inputs identical with the buffer active.
- Name the one planned exception explicitly: F4 (vnc-027) demotes the attribution heuristics (`enrich_topic_signal`, `check_eager_attribution`) in favor of transcript-derived attribution — better-filled attribution *fields*, same records, same pipe.
- Deliverable: a short plain-language "two pipes" section in FINDINGS readable by a non-implementer — what fills ObservationRecord, what fills the buffer, where they never touch, and what F4 changes.

### Q8: Synthesis uplift opportunities — beyond the four targets

- Survey what `context_cycle_review` synthesis output looks like today (`synthesis.rs`, `phase_narrative.rs`, the retrospective report) against the same Q1 corpus: where is the report thin, wrong, or silent because it only sees structured observations?
- With transcript access, what could synthesis do *better* beyond extracting the four targets — e.g., explaining rework hotspots (the *why* behind a detection-rule finding), linking decisions to their downstream outcomes, narrating phase transitions the records only timestamp, grounding "declared-but-never-closed phase" findings (#556) in what actually happened?
- Deliverable: a ranked opportunity list (impact × effort), each tagged: belongs in crt-052 / separate follow-on feature / not worth it. This is exploratory — opportunities, not designs.

## Output

`product/research/ass-070/FINDINGS.md`:
- Per-target precision/recall table: rules vs agent-side vs hybrid, against both ground-truth layers
- **Recommended extraction architecture for crt-052** (the spike's primary deliverable — crt-052's scope consumes it directly)
- Cost/latency envelope at cycle-review time
- Buffer-window adequacy verdict (feedback to vnc-025 knob / F3)
- Plain-language "two pipes" section: ObservationRecord population verified unchanged, F4 exception named (Q7)
- Ranked synthesis-uplift opportunity list with routing (crt-052 / follow-on / drop) (Q8)
- Go/no-go on whether distillation quality justifies crt-052 as scoped

## Constraints & Prior Art

- **Privacy**: raw transcripts may contain secrets. The corpus stays local — no transcript content committed to the repo, stored in Unimatrix, or pasted into FINDINGS beyond short sanitized excerpts.
- **Claude Code JSONL format only** (multi-provider parsing is explicitly out — ass-069 Open Thread 1, crt-052 non-goal).
- **No changes to the 23 detection rules** — they stay on `ObservationRecord` (ass-069 Q5).
- **Throwaway code only** — extractors built here are measurement instruments, not deliverables; crt-052 implements for real.
- Prior art: ass-069 FINDINGS (Q5, Q6, Open Threads 1–3), ass-040 retrospective pipeline, vnc-025 SCOPE (buffer semantics: ring-tail, contiguity, purge lifecycle), crt-052 #689 (the consumer).

## Tracking

GitHub Issue: #683
Consumer: crt-052 (#689) — extraction approach decided by these findings
