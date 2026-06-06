# FINDINGS: Transcript Distillation Extractor Quality — Decisions / Rework / Phase Narrative

**Spike**: ass-070 (#683)
**Date**: 2026-06-06
**Approach**: measurement (throwaway extractors in `/tmp/ass070/`, never committed)
**Confidence**: empirical (Q2–Q6 measured against a hand-labeled corpus; Q7 code-verified; Q8 exploratory)
**Consumer**: crt-052 (#689)

---

## Findings

### Q1: Corpus and ground truth

**Answer**: Corpus assembled: **12 real local Claude Code JSONL sessions** (4 delivery, 4 bugfix, 4 design), 0.34–1.45 MiB each, 44 minutes to 21+ hours wall, drawn from `~/.claude/projects/-workspaces-unimatrix/` (81 sessions available, 200 MB total). Ground truth hand-labeled on a **6-session subset** (2 delivery, 2 bugfix, 2 design; 4.78 MB): **43 items** — 17 decisions (DEC), 12 rework narratives (RWK), 8 pattern-lessons (LSN), 6 phase narratives (PHN). Two layers: **layer-a = 15 items** that correspond to human-curated knowledge (Unimatrix ADRs/lessons/patterns or GH issue comments those sessions actually produced — e.g., lesson #4684 string-error-discrimination, lesson #4728 disk-full-masquerading-as-socket-errors, ADR correction #4718→#4726); **layer-b = 28 items** hand-labeled and uncurated (the recall ceiling beyond current practice — e.g., wave-plan decisions, doc-trigger SKIPs, gate-retry sequences, false-alarm rework). One held-out session (bugfix-650) used for unbiased extraction inspection.

**Evidence**: `corpus.txt`, `groundtruth.py` (each item: session, family, byte anchor, content regexes, layer, one-line description), `channels.py` (per-session channel volumes). Each anchor verified against the transcript during labeling.

**Recommendation**: Use this two-layer framing in crt-052 acceptance tests: layer-a agreement is the "matches human curation" bar; layer-b is the uplift the feature exists to deliver — **65% of labeled value (28/43) is currently uncurated** and lost at session end.

---

### Q2: Rule/marker extractor — quality floor

**Answer**: A ~50-pattern regex/marker extractor (4 families: decision phrases, rework signals, lesson markers, phase/gate markers) over parsed JSONL user+assistant text blocks achieves **0.93 recall (40/43) but low precision (0.65 overall block-level, and much lower per-family)**. The server-alone floor is a good *selector* and a poor *extractor*.

Per-family, rules alone (6 labeled sessions):

| Family | Precision | Recall | layer-a recall | layer-b recall |
|--------|-----------|--------|----------------|----------------|
| DEC | 0.52 (46/88) | 0.88 (15/17) | 7/8 | 8/9 |
| RWK | 0.55 (40/73) | 0.92 (11/12)¹ | 2/2 | 9/10 |
| LSN | 0.39 (20/51) | 0.88 (7/8) | 5/5 | 2/3 |
| PHN | 0.11 (13/119) | 1.00 (6/6) | — | 6/6 |

¹ 12/12 lenient (anchor-proximity); 11/12 strict content-match. PHN precision is structurally low: gate/phase markers fire on every progress-narration block; recall is perfect because retrospective summaries always restate the timeline.

The three persistent rule misses are all **content-in-unmarked-human-reply** cases: a human's open-question resolutions and an "untested freebie" decision phrased with no decision vocabulary.

**GGUF check (in passing)**: the server has **no generation capability and no GGUF runtime at all**. `unimatrix-embed` is ONNX-only (`ort 2.0.0-rc.9`, `Cargo.toml:12`); its surface is `EmbeddingProvider::embed/embed_batch` (`provider.rs:12-15`) plus a cross-encoder *scorer* (`cross_encoder.rs`) — embeddings and relevance scores, no text generation. "GGUF" appears only in W2-4 TODO comments (`main.rs:674`, `services/mod.rs:260`). The zero-LLM-server constraint stands; the calculus is unchanged.

**Evidence**: `extractor.py` (rules + scorer), `perfam.py` (table above), `measure2.py` (precision 216/331 = 0.65 overall; selected volume 219 KB of 4,783 KB = 4.6%). All re-run 2026-06-06.

**Recommendation**: Do **not** ship rule output directly as extractions — at 0.11–0.55 precision it would pollute the knowledge base. Use rules exclusively for candidate *selection* (Q4).

---

### Q3: Agent-side semantic extraction — quality ceiling

**Answer**: The calling agent (this researcher, performing the extraction over the rule-selected candidate excerpts exactly as crt-052's cycle-review caller would) achieves **0.95 recall (41/43) at ~0.96 precision** — recovering two items the strict rule scorer missed (a socket-error→user-retry rework visible only across adjacent blocks; a multi-block scope decision requiring synthesis) while missing only the two items whose *content* never entered the candidate set. Per-family:

| Family | Agent recall | layer-a | layer-b |
|--------|-------------|---------|---------|
| DEC | 15/17 | 7/8 | 8/9 |
| RWK | 12/12 | 2/2 | 10/10 |
| LSN | 8/8 | 5/5 | 3/3 |
| PHN | 6/6 | — | 6/6 |

Precision ~0.96: every emitted item is grounded in a cited excerpt; the one mis-framing observed was a "false alarm" rework (bugfix-638: agent changes appeared missing but were actually self-committed) that candidates narrate as a real failure because the resolution block carried no marker — the extraction is real but the framing degrades without adjacent context.

**Cost**: candidate excerpt volume per session 4.1–19.9 KB (mean 9.9 KB ≈ **2.6 K tokens**, max ≈ 5.2 K tokens at 3.8 chars/token). A multi-session feature cycle (design + delivery) sums to ~25 KB ≈ **6.6 K input tokens** added to the cycle-review turn. Output cost: ~1–2 K tokens of `context_store` calls. **Latency**: zero extra round-trips — excerpts ride the existing `context_cycle_review` response; extraction happens in the turn the agent already spends reading the retrospective.

**Evidence**: `cand_{4469,1bf8,4284,c724,1661,90b7}.txt` (the exact candidate sets, `/tmp/ass070/`), extraction performed and scored item-by-item against `groundtruth.py`; `heldout_650.txt` (held-out session inspection — root cause, fix decision, pre-existing-dirty-state note, and phase timeline all recoverable from 13 candidates / 5.5 KB).

**Recommendation**: Agent-side semantic extraction over server-selected candidates is the quality ceiling and it is cheap. Adopt it (Q4).

---

### Q4: Hybrid division of labor

**Answer**: The right cut: **server-side rules select whole user+assistant text blocks matching any marker family; the calling agent does all semantics.** Measured selection trade-off (6 sessions, 4,783 KB raw):

| Candidate input | Strict GT ceiling | Volume | Tokens |
|----------------|------------------:|-------:|-------:|
| Full transcript, all channels | 42/43 | 1,156 KB | ~312 K |
| usr+ast channels, unselected | 42/43 | 83 KB | ~22 K |
| **usr+ast, marker-selected (recommended)** | **40/43 (agent achieves 41/43)** | **58 KB** | **~16 K** |
| + spawn-prompt channel | 40/43 | 209 KB | ~56 K |
| ±400-char windowing of blocks | 38/43 | 373 KB | ~101 K |

Marker selection shrinks input **95%** (4.6% of raw bytes) at a 1–2 item recall cost. Adding the sub-agent-spawn channel costs 3.6× volume for zero recall gain. Windowing excerpts *hurts* (loses multi-paragraph decision context) — keep matched blocks whole. Tool-result and bash channels add nothing the assistant's own narration doesn't restate.

**Concrete architecture for crt-052**:
1. **Server-side** (at `context_cycle_review`, after snapshot, before purge): parse buffered JSONL → keep `user`/`assistant` text blocks only (drop tool_use/tool_result/thinking; drop command-noise blocks) → match against the 4 marker families → dedup → cap (~24 KB per session covers the observed max; make it a config knob) → attach as a `transcript_candidates` section of the cycle-review response, ordered, with session-id + byte-offset provenance.
2. **Surfaced**: whole matched blocks, tagged with candidate family hints (advisory only — the agent re-classifies).
3. **Agent-side**: the calling agent extracts decisions / rework narrative / lessons / phase narrative from the candidates and stores via `context_store` — attribution-preserving, agent curates, provenance real. The protocol prompt (uni-retro / cycle-review skill) instructs the four target families.

**Evidence**: `ablate.py`, `coverage.py`, `windowed.py` outputs above; Q3 agent pass.

**Recommendation**: Implement exactly this cut in crt-052. Do not attempt server-side family classification beyond advisory hints; do not window; do not include spawn/result channels in v1.

---

### Q5: Input-window adequacy (feedback to vnc-025/F3)

**Answer**: **The 4 MiB accumulated-buffer cap is adequate — zero ground-truth loss today, with ~2.8× headroom.** No corpus session exceeds 1.45 MiB; the largest main-thread transcript in the entire 81-session project history is **2.12 MiB** (a 21-hour session). No GT item falls outside a 4 MiB tail (0/43 elided). However, GT anchors are **uniformly distributed** through sessions (quartiles at 16% / 38% / 61% / 78% of file length) — meaningful bits do *not* concentrate late, so if elision ever did occur it would silently lose early decisions roughly in proportion to bytes dropped. There is no natural protection.

**Evidence**: anchor-position computation over `groundtruth.py` anchors vs file sizes; `survey.py` over all 81 local transcripts.

**Recommendation**: **Keep the 4 MiB knob unchanged; no F3 escalation.** Two cheap guards instead of a bigger knob: (1) vnc-025 already records an elision marker + dropped-byte count — crt-052's distillation should surface "N bytes elided before distillation" in the cycle-review response so loss is visible, never silent; (2) track `transcript_high_water` at purge (the audit event already carries byte count) and revisit only if real sessions approach ~3 MiB. Periodic distill-and-truncate (ass-069 Open Thread 3) is the right *eventual* mechanism but is not justified by data today — defer.

---

### Q6: Integration surface check

**Answer**: The winning Q4 architecture fits the planned insertion with no redesign. Confirmed against code:

- **Insertion point**: `context_cycle_review` handler (`mcp/tools.rs:1918`) has `self.session_registry` in scope; vnc-025 SCOPE already plans the feature→sessions registry method that clears transcripts of sessions where `state.feature == feature_cycle`, and names crt-052's extension: *snapshot bytes out before clearing* (distill-before-purge).
- **Snapshot-and-release**: `SessionRegistry` is `Mutex<HashMap<String, SessionState>>` (`session.rs:159-161`). The distill step must clone the buffer `Vec<u8>` out under the lock and parse **after** release. Parsing cost makes this comfortable: the full Python rule pass over 4.7 MiB ran in 0.24 s; a Rust implementation is single-digit milliseconds — but it still never belongs under the lock.
- **`synthesis.rs` / `phase_narrative.rs` fit**: both are pure/deterministic (`synthesize_narratives` over `HotspotFinding`s; `build_phase_narrative` over `CycleEventRecord`s — "No I/O, no async", `phase_narrative.rs:1-9`). Candidate selection is a third pure function alongside them (input: bytes; output: candidate blocks) feeding the response assembly in `mcp/response/retrospective.rs` — additive, no change to either existing module. The 23 detection rules (`detection/mod.rs:3` — agent 7, friction 5, session 5, scope 6) are untouched.
- **Reconstruction-fallback delta**: feeding the extractor `ObservationRecord`-shaped input instead (tool name + full input + 500-char `response_snippet`; no user text, no assistant text — `observation.rs:21-41`) drops the GT content ceiling from **42/43 to 35/43 (0.81)**. Misses concentrate in **DEC (5 of 8 lost)** — human decisions live in user/assistant prose that observations never carry — plus 2 RWK and 1 PHN. Volume 461 KB (worse than candidates *and* worse recall).

**Evidence**: `reconstruct.py` (re-run: 35/43, missed-by-family Counter DEC:5 RWK:2 PHN:1); code reads cited above; vnc-025 SCOPE.md "Key code surfaces" section.

**Recommendation**: Proceed with the planned insertion. Ship the reconstruction fallback as scoped (it degrades acceptably for RWK/LSN/PHN) but have crt-052 label fallback-derived extractions as degraded provenance and expect weak decision capture from that path — it is a fidelity floor, not parity.

---

### Q7: ObservationRecord population boundary — verified, plain language

**Answer — the two pipes** (readable without the code):

Unimatrix watches a coding session through **two completely separate pipes**.

**Pipe 1 — the event ledger (`ObservationRecord`).** Every time the coding agent uses a tool, hook events fire and arrive at the server, which writes one structured row per event: timestamp, event type, session, tool name, the tool's input, the response's size, and at most the first 500 characters of the response. That's the whole record (`observation.rs:21-41`). The 23 detection rules — friction, rework loops, scope drift, session health — read **only these rows**. So does the phase timeline. This pipe is quantitative: it knows *that* you compiled 29 times; it cannot know *why*.

**Pipe 2 — the transcript buffer (vnc-025).** The client also streams raw conversation bytes (`transcript_delta` events) into a per-session in-memory buffer. These bytes are **never written to the database** — at cycle review they are distilled (crt-052) and then purged.

**Where they never touch — verified in code.** At the single point where both kinds of event arrive (`uds/listener.rs:996-1025`), transcript deltas are filtered out *before* the durable batch is built: `events.iter().filter(|event| event.event_type != TRANSCRIPT_DELTA_EVENT)` guards `insert_observations_batch`, and the comment block at `listener.rs:999-1006` (vnc-024 ADR-004, gate-critical AC-12) states the invariant: deltas never enter the batch, so no observation row is ever sourced from buffer content. The same guard holds on all three arms (UDS single-event, UDS batch, HTTP `/observe` — verified in vnc-024 Stage 3c, 15/15 ACs). The two enrichment heuristics that run pre-persist — `enrich_topic_signal` (`listener.rs:144`) and `check_eager_attribution` (`listener.rs:850, 972`) — read only the event's `feature_cycle`/`topic_signal` *fields* and the session registry, never delta bytes. **Therefore the 23 detection rules' inputs are bit-identical with the buffer active or absent.**

**The one planned exception — F4 (vnc-027).** F4 will *demote* those two attribution heuristics in favor of transcript-derived attribution. That changes how the `topic_signal`/feature fields of observation rows get *filled in* — better-filled attribution fields, same records, same pipe, still no transcript content stored. It is an upgrade to a label on Pipe 1, informed by Pipe 2, not a leak between them.

**Evidence**: code citations above; `extraction_pipeline.rs` / `detection_isolation.rs` tests exercise rules on `ObservationRecord` only.

**Recommendation**: crt-052 must preserve this boundary verbatim: distillation output flows to `context_store` entries (curated knowledge), never to observation rows. State it as an explicit AC.

---

### Q8: Synthesis uplift opportunities — beyond the four targets

**Answer**: Today's cycle-review output (hotspot narratives from `synthesize_narratives` — timestamp clusters, top files, sequence patterns; phase narrative from `build_phase_narrative` — sequence + durations; retrospective report) is **accurate but mute on causation**. Observed against the corpus: the report says `compile_cycles: 29 — elevated` but the *because the human requested an error-handling restructure mid-PR* lives only in the transcript; it timestamps a 62-minute cold-restart gap but not the disk-exhaustion cascade that caused it; bugfix-638's cycle was left `IN PROGRESS` with missed events (MCP disconnect) and the report shows the anomaly without the explanation.

Ranked opportunities (impact × effort):

| # | Opportunity | Impact | Effort | Route |
|---|------------|--------|--------|-------|
| 1 | **Rework-hotspot explanation**: join detection-finding evidence timestamps to nearest transcript candidates; surface the *why* beside each warning-level finding | High | Low (timestamps exist on both sides) | **crt-052** |
| 2 | **Gate-failure narratives**: REWORKABLE FAIL reason + resolution + retry outcome as a unit (rules already hit these at 0.92+ recall) | High | Low | **crt-052** (subset of RWK target) |
| 3 | **Human-intervention ledger**: scope corrections, approvals, "good catch" reversals from user blocks — the highest-value, most-lost content (both Q3 misses were here) | High | Low | **crt-052** (subset of DEC target; add user-block adjacency to selection) |
| 4 | **Phase-transition narration**: what was concluded at each `cycle_phase_end`, not just when (PHN markers: 6/6 recall) | Medium | Low | **crt-052** |
| 5 | **Decision→outcome linking**: connect a decision excerpt to later rework/revert touching the same artifact (e.g., ci.yml addition → revert → ADR correction chain) | High | Medium (cross-excerpt entity linking) | **follow-on** |
| 6 | **Grounding "declared-but-never-closed phase" (#556)**: transcript shows whether work continued, the session died, or MCP disconnected (bugfix-638 case) | Medium | Low-Medium (needs #556 finding shape) | **follow-on** |
| 7 | **Cross-session cycle stitching**: narrative continuity across a cycle's design+delivery sessions | Medium | Medium | **follow-on** |
| 8 | **Wall-clock anomaly explanation** (idle gaps, wakeups behind σ-outliers) | Low | Low | **drop** (humans read these fine) |
| 9 | **Auto-drafted lesson/ADR text from excerpts** | — | — | **drop** (duplicates the core agent-extraction function itself) |

**Evidence**: `synthesis.rs`/`phase_narrative.rs`/`report.rs` reads; retrospective excerpts present in corpus sessions (4469, c724, 1661, 1bf8 candidate sets).

**Recommendation**: Fold 1–4 into crt-052 — all four are candidate-surfacing/prompting refinements of the same mechanism, not new machinery. File 5–7 as one follow-on candidate after crt-052 ships real extractions to link against.

---

## Per-target precision/recall table (consolidated)

Against the 43-item ground truth (lenient scoring; strict deltas noted in Q2/Q4):

| Target | Rules alone P / R | Agent-over-candidates P / R | Hybrid (recommended) P / R | layer-a R (rules → hybrid) | layer-b R (rules → hybrid) |
|--------|------------------|----------------------------|---------------------------|---------------------------|---------------------------|
| Decisions | 0.52 / 0.88 | ~0.96 / 0.88 | ~0.96 / 0.88 | 7/8 → 7/8 | 8/9 → 8/9 |
| Rework narrative | 0.55 / 0.92 | ~0.96 / 1.00 | ~0.96 / 1.00 | 2/2 → 2/2 | 9/10 → 10/10 |
| Pattern-lesson | 0.39 / 0.88 | ~1.0 / 1.00 | ~1.0 / 1.00 | 5/5 → 5/5 | 2/3 → 3/3 |
| Phase narrative | 0.11 / 1.00 | ~1.0 / 1.00 | ~1.0 / 1.00 | — | 6/6 → 6/6 |
| **Overall** | **0.65 / 0.93** | **~0.96 / 0.95** | **~0.96 / 0.95** | 14/15 → 14/15 | 26/28 → 27/28 |

(Hybrid = agent-over-candidates; they coincide because the recommended hybrid *is* rules-select → agent-extract. The hybrid's distinguishing win is the 95% input-volume reduction, not a P/R difference vs the unconstrained agent ceiling — full-transcript agent input would buy at most +1 item for ~19× the tokens.)

**Cost/latency envelope at cycle-review time**: server rule pass < 10 ms (Rust, est. from 0.24 s Python over 4.7 MiB); response payload +10–25 KB; agent turn +2.6–6.6 K input tokens (single-session → multi-session cycle), +1–2 K output tokens of stores; **zero added round-trips**.

**Go/no-go on crt-052 as scoped**: **GO.** Layer-b recall of 27/28 means the mechanism recovers nearly everything humans currently fail to curate, at ~0.96 precision and negligible cost; the scoped architecture (rules-select server-side, agent extracts, reconstruction fallback, distill-before-purge) is confirmed against both the data and the code surfaces.

---

## Unanswered Questions

- **Wall-clock latency of the agent extraction turn** — not measurable in this spike (depends on the production cycle-review prompt and model); bounded analytically by the token envelope above. crt-052 can measure post-ship.
- **Extractor quality on non-Claude-Code transcript formats** — out of scope by constraint (ass-069 Open Thread 1, crt-052 non-goal).
- **Whether marker families need per-project tuning** — the rule set was developed against this project's protocol vocabulary (gates, waves, stages). A single-project corpus cannot answer generalization; flag for whenever Unimatrix is exercised on a second project.

## Out-of-Scope Discoveries

- **Sub-agent (sidechain) transcripts are invisible to this pipeline** — the corpus main-thread files contain the SM's narration of agent work, which proved sufficient (spawn-channel ablation: zero recall gain), but the agents' own reasoning is in separate sidechain files that vnc-025 does not stream. If a future feature wants *implementer*-level lessons rather than SM-level, the data-source question reopens.
- **The main-thread assistant narration is a near-lossless index of the session** — usr+ast channels alone hold 42/43 of labeled value in 1.7% of file bytes. Useful beyond distillation (e.g., cheap session summaries, search indexing).
- **`.mcp.json` RUST_LOG shape** (`unimatrix_server=debug` filtering out binary-crate `unimatrix` targets) recurred as a theme in corpus sessions; already curated as lesson #4599 — no action.

## Recommendations Summary

- **Q1 (corpus)**: 12-session corpus, 43-item two-layer ground truth on 6 sessions; 65% of labeled value is currently uncurated — reuse the layer framing in crt-052 ACs.
- **Q2 (rules floor)**: rules reach 0.93 recall but 0.11–0.55 per-family precision — selection-only, never direct extraction; server has no LLM and no GGUF runtime (ONNX embeddings + cross-encoder scoring only).
- **Q3 (agent ceiling)**: agent over candidates: 0.95 recall / ~0.96 precision at 2.6–6.6 K input tokens per cycle review, zero extra round-trips — adopt.
- **Q4 (hybrid cut)**: server selects whole marker-matched user+assistant blocks (95% volume cut, ≤2-item recall cost), agent does all semantics — this is crt-052's extraction architecture.
- **Q5 (window)**: 4 MiB cap adequate (largest-ever session 2.12 MiB; 0/43 items elided); keep knob, surface elision counts in the cycle-review response, defer distill-and-truncate.
- **Q6 (integration)**: insertion at `context_cycle_review` fits; clone-then-parse (never under the registry lock); reconstruction fallback degrades to 0.81 ceiling, decisions weakest — ship it as a labeled degraded mode.
- **Q7 (two pipes)**: verified — transcript deltas are filtered before `insert_observations_batch` (`listener.rs:1007-1009`); no observation field sources buffer content; 23 rules' inputs identical; F4's attribution demotion is the sole, field-level exception.
- **Q8 (uplift)**: fold rework-explanation, gate-failure narratives, human-intervention ledger, and phase-transition narration into crt-052; defer decision→outcome linking, #556 grounding, and cycle stitching to one follow-on; drop wall-clock explanation and auto-drafting.
- **Go/no-go**: **GO** — distillation quality justifies crt-052 exactly as scoped in #689.
