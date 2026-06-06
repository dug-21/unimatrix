# FINDINGS: Subagent Sidechain Transcript Value — Implementer-Level Lessons + Capture Architecture

**Spike**: ass-071 (#690)
**Date**: 2026-06-06
**Approach**: measurement (throwaway instruments in `/tmp/ass071/`, reusing ass-070 gear in `/tmp/ass070/`; never committed)
**Confidence**: empirical (Q1–Q4 measured against the full local sidechain corpus + an 18-file hand-labeled sample; Q5 code-verified; Q6 synthesis)
**Consumer**: crt-052 (#689)

---

## Findings

### Q1: Corpus and ground truth — stratified by agent type

**Answer**: Full corpus enumerated: **328 sidechain files** (SCOPE said 326; 2 appeared between scoping and measurement) across **50 sessions**, **83.7 MiB raw**, at `~/.claude/projects/-workspaces-unimatrix/<session-id>/subagents/agent-<id>.jsonl`. Agent type was recovered for **328/328 files** by joining each sidechain's first user block to the parent session's `Agent` tool_use `subagent_type` (note: the spawn tool is named `Agent`, not `Task`, in these transcripts). Session types: 13 bugfix, 11 research, 10 design, 4 delivery, 2 review, 10 other (uni-zero etc.).

Corpus profile (raw bytes / usr+ast prose bytes):

| Agent type | files | raw MiB | usr+ast MiB |
|---|---|---|---|
| uni-rust-dev | 60 | 18.91 | 0.52 |
| uni-tester | 29 | 11.01 | 0.23 |
| uni-validator | 40 | 9.64 | 0.25 |
| uni-architect | 43 | 8.50 | 0.42 |
| uni-spike-researcher | 21 | 7.86 | 0.45 |
| uni-security-reviewer | 19 | 3.92 | 0.10 |
| uni-researcher | 13 | 3.59 | 0.08 |
| uni-risk-strategist | 21 | 3.30 | 0.08 |
| uni-bug-investigator | 12 | 2.92 | 0.17 |
| uni-pseudocode | 8 | 2.55 | 0.05 |
| 11 other types | 62 | 11.5 | 0.39 |
| **Total** | **328** | **83.7** | **2.74** |

Ground truth: an **18-file stratified sample** (12 agent types × 5 session types; 396 KiB usr+ast read and labeled in full or near-full) yielded **≈54 labeled value items** in two layers adapted from ass-070: **layer-a ≈28** — items restated in a surfaced channel (agent final message to SM, agent-report file, gate report, GH issue/PR comment, or a Unimatrix store); **layer-b ≈26** — items that exist **only** in the sidechain interior. The surfaced-channel set is broader than ass-070's (sidechain agents have four official outlets), which is why layer-b is the decisive number.

**Evidence**: `/tmp/ass071/{survey,join2,sesstype,volumes,sample,dump}.py`, `corpus_typed.tsv`, `volumes.tsv`, 18 `view_*.txt` labeling dumps. All run 2026-06-06.

**Recommendation**: Reuse this join (sidechain first-user block ↔ parent `Agent` tool_use prompt) wherever agent-type attribution of a sidechain is needed; it is exact on this corpus.

---

### Q2: Value categories — what is actually in there

**Answer**: Real, but **sharply bimodal by agent role**, and the highest-value category was not on the seed list.

**Seeded categories, measured:**

- **Implementer lessons** — CONFIRMED, the core layer-b mass. Examples from the sample (sanitized, one line each): a mid-task "all my changes were reverted by the linter" incident that turned out to be stash/lint interaction confusion (never surfaced; the report says only "COMPLETE"); `NOT NULL DEFAULT ''` schema columns rejecting explicit-NULL test fixtures; `block_in_place` requiring `#[tokio::test(flavor = "multi_thread")]`; `#[serde(flatten)]` putting event fields at top level (tester's test-data bug); binary-target vs library-crate `pub(crate)` visibility (independently re-derived by the pseudocode agent and a rust-dev in different features). Density in implementer files: ~3 sidechain-only items per rust-dev file sampled.
- **Action/problem provenance** — CONFIRMED with a structural caveat. Sidechains can answer "what did agent X actually do/hit" (e.g., a rust-dev diagnosed a parallel agent's test as having a NaN-into-NOT-NULL SQL bug — actionable defect knowledge that appears nowhere else). Caveat: agent-report files already carry the conclusions **when they exist** — and they reliably don't: vnc-024's Gate 3b returned REWORKABLE FAIL precisely because **zero implementation-agent reports were written**. The sidechain is the only implementer record whenever the report channel is skipped; capture is the backstop for a compliance mechanism that demonstrably fails.
- **cycle_review enrichment** — CONFIRMED, modest density, uniquely sidechain-sourced. Examples: a vnc-022 agent spent most of its turn re-implementing code a parallel agent had already committed (wave-overlap duplicate work — explains an edit/compile hotspot the ObservationRecords only count); a tester's 9-minute "slow suite" wall-clock anomaly explained by a concurrent cargo build in another worktree; the 500-line-rule violation consciously waived with reasoning by three different agents (a governance signal that never reaches the SM).
- **Attribution provenance** — WEAK incremental value. Commit hashes, files touched, and parallel-agent boundaries do appear in sidechain interiors, but agent reports and commit messages carry the same chain when present. Verdict: complementary backstop only (same caveat as provenance above), not a driver.

**Researcher-discovered categories (not on the seed list):**

1. **Negative-result catalogs (rule-outs)** — the single highest-value unlisted category. The #638 bug-investigator eliminated ~10 hypotheses (setsid fd behavior, Rust `Stderr` buffering, double subscriber init, `RUST_LOG` target filtering `unimatrix` vs `unimatrix_server`, posix_spawn, log truncation, …) and could not reproduce on Linux; its GH comment carries only the conclusion + fix. If #638 resurfaces, the rule-out catalog is the difference between hours and minutes — and it exists only in the sidechain.
2. **Recurring cross-agent micro-frictions** — visible only by reading *multiple* sidechains: the `cargo test 'a\|b'` filter failure hit by 2 agents; workspace test-binary linker OOM hit by 2 agents; the rust-1.95 clippy lint-bump noise independently re-triaged ("pre-existing, not my crate") by **4+ agents**, each re-paying the same investigation. A "known pre-existing failures/frictions" registry distilled from sidechains would eliminate repeated cost. No single surfaced channel can see this; it is an aggregation product.
3. **In-sidechain compaction summaries** — sidechains compact too; the auto-generated continuation summary (observed in the nxs-012 rust-dev file) is a dense, structured self-distillate of the agent's first context window — a free distillation artifact worth preferring when present.

**Negative findings** (equally important): reviewer-class sidechains are **near-empty of layer-b value**. The vnc-024 security-reviewer, both validators, the retro-architect, vision-guardian, and synthesizer surfaced essentially everything into their reports/PR comments/Unimatrix stores — their *job* is the report, and they do it. Researcher sidechains are zero-value-add by construction (the deliverable is a committed FINDINGS file); Explore agents return their full cited report to the main thread. Their interiors are verification narration with audit value only.

**Evidence**: 18 labeled `view_*.txt` files; agent-report cross-checks (`product/features/nxs-012/agents/nxs-012-agent-7-skip-quarantined-report.md` confirmed the surfaced/unsurfaced split item-by-item); vnc-024 Gate 3b report (missing-reports REWORKABLE FAIL).

**Recommendation**: crt-052's sidechain distillation targets should be: implementer lessons, rule-out catalogs, rework/wall-clock explanations, and cross-agent friction aggregation — not attribution, not reviewer narratives.

---

### Q3: Per-agent-type value profile — selective capture

**Answer**: The working hypothesis is **confirmed and sharpened**. Layer-b (sidechain-only) value concentration in the sample:

| Tier | Agent types | layer-b items (sample) | share | corpus raw |
|---|---|---|---|---|
| High | rust-dev, tester, bug-investigator | ≈23 | ~88% | 32.8 MiB (39%) |
| Marginal | pseudocode, risk-strategist | ≈2 | ~8% | 5.9 MiB (7%) |
| Near-zero | validator, security-reviewer, architect(retro), vision-guardian, synthesizer, spike-researcher, researcher, Explore, docs, zero-reviewer | ≈1 | ~4% | 45.0 MiB (54%) |

The split tracks **role mechanics**, not agent quality: review/report-producing roles are contractually self-surfacing; doing/investigating roles surface conclusions but not process, and the process is where the lessons live.

**However** — selective capture is the right *policy lever* but the wrong *volume lever*. After the Q4 channel filter, the worst whole-session capture is 249 KiB; skipping the near-zero tier saves bytes that no longer matter at capture time. Where selectivity does pay is **distillation input cost** (the cycle-review turn): implementer-tier-only roughly halves the distillation input for a heavy session at a measured ~12% layer-b loss (mostly low-value friction items).

**Recommendation**: capture-side, make agent-type selection a config knob defaulting to **all types** (volume is solved by the channel filter; type names like `uni-*` are project-specific and a hook-side allowlist would not generalize). Distillation-side (crt-052), prioritize candidates from implementer/investigator-class agents first under the per-session token cap; reviewer-class sidechain candidates are the first to drop under budget pressure.

---

### Q4: Capture architecture — against the volume math

**Answer**: Measured against the full corpus:

**(a) Raw one-shot ship at SubagentStop — REJECT.** 6/50 sessions exceed the 4 MiB buffer on sidechain aggregate alone (worst 10.60 MiB = 2.65× the buffer, before any main-thread bytes); ring-tail overflow would silently drop >60% of the worst session. 2 individual files exceed the 1 MiB wire frame (max 1.27 MiB), so chunking is mandatory even per-file. Quantified: raw breaks the buffer math exactly as feared.

**(b) Client-side marker pre-filter — REJECT AS SPECIFIED, replace with channel filter.** The ass-070 marker rules were verified on sidechain content: volume is excellent (1.96 MiB total, 2.3% of raw; worst session 149 KiB) **but recall collapses**: a spot test against 14 labeled sidechain-only items selected only **6/14 (43%)** — versus 0.93 on main threads. Root cause: the rule vocabulary is SM-protocol language (gates, waves, REWORKABLE, decisions); implementer gotchas speak compiler/schema/tooling language (`NOT NULL`, `multi_thread`, `pub(crate)`, `serde(flatten)`) the rules never targeted. Missed items include exactly the category the spike exists for.

**The winning variant — (b′) channel filter: ship whole `user`+`assistant` text blocks, drop tool_use/tool_result/thinking.** Measured: **2.74 MiB corpus-wide (3.3% of raw — a 96.7% reduction)**; worst session 249 KiB (6% of the 4 MiB buffer), median session 38 KiB; max single block 48.2 KiB with **0 of 5,596 blocks** over the 64 KiB delta cap; **100% recall by construction** (every labeled item lives in usr+ast prose). It is also simpler than markers: a JSONL parse + type check in the hook, no regex rule set to maintain or port.

**(c) Per-agent distill at stop — REJECT.** The hook is a fast-exit Rust binary with no LLM (re-confirmed: `unimatrix-embed` is ONNX-only, no generation — ass-070 Q2); distilling at stop would require an extra paid agent invocation per SubagentStop (~328 in this corpus) and adds latency to a hook path designed for exit-0. The economics are upside-down relative to one batched distillation at cycle review.

**Residual gap (handed to crt-052)**: 249 KiB worst-session capture ≈ 65 K tokens — well above ass-070's ~24 KiB/session candidate envelope for the cycle-review turn. A second-stage reduction at distillation time is required: extend the marker families with an implementer-lesson vocabulary (error/constraint/tooling phrases; the 6/14 spot test provides the initial miss list), prefer in-sidechain compaction summaries when present, and apply the Q3 type-priority ordering under the cap. This is server-side selection at review time — cheap to iterate, unlike a hook-side filter.

**Evidence**: `volumes.py` output tables (per-type, per-session), block-size scan (5,596 blocks), marker spot test (`6/14`), all in `/tmp/ass071/`.

**Recommendation**: **One-shot read at SubagentStop in the hook → parse JSONL → keep whole user/assistant text blocks → ship as a new fire-and-forget event.** Composes with Q3's knob. Marker logic stays server-side, at distillation time, where it can be tuned without client redeploys.

---

### Q5: Wire and registry shape for the winning option

**Answer** (code-verified against the frozen F1 surface):

**Wire — new additive event type, do not overload `transcript_delta`.** `ImplantEvent.payload` is open `serde_json::Value` (`wire.rs:225-251`), so a new `event_type` rides the existing `RecordEvent`/`RecordEvents` carrier with zero changes to existing bindings — the same additive move vnc-024 used for `transcript_delta`. Recommend:

- `SUBAGENT_TRANSCRIPT_EVENT = "subagent_transcript"` + typed `SubagentTranscriptPayload { agent_id: String, agent_type: Option<String>, seq: u32, bytes: String }` (new ts-rs export; `TranscriptDeltaPayload` and all 6 existing bindings untouched).
- `seq` is a chunk index, not a byte offset: the source is complete and immutable at stop, so chunking (at the 64 KiB soft cap, under the 1 MiB frame) replaces vnc-025's offset-merge machinery entirely — no contiguity tracking, no out-of-order merge; duplicate delivery is idempotent on `(agent_id, seq)`.
- Do **not** encode the agent into the session key (`{session}/agent-{id}`): registered-session lookup would no-op it, and it would corrupt the session-keyed attribution contract ass-069 validated.

**Registry — sibling per-agent map, not appends into the main buffer.** The session `TranscriptBuffer` is offset-merge/ring-tail semantics (vnc-025); interleaving sidechain bytes would corrupt both its `high_water` math and PreCompact's tail block. Recommend a sibling structure in `SessionState` behind the same shared-handle discipline (AC-10 clone-cost rule): `subagent_transcripts: HashMap<agent_id, Vec<u8>>` plus a `subagent_transcript_max_bytes` knob (own budget, suggested default 1 MiB per session — 4× the observed worst filtered session). Purge lifecycle inherited verbatim from vnc-025: freed by key removal at `drain_and_signal_session`/`sweep_stale_sessions`, cleared by the same feature→sessions cycle-review method (crt-052 snapshots both buffers before the clear), `transcript_session_purged` audit event extended with a sidechain byte count — never content. Never disk, no spill, same principle-8 posture.

**Client-side discovery — one verification item.** The hook receives `transcript_path` on every event (`HookInput`, `wire.rs:71-72`) and unknown stdin fields land in the `extra` flatten — whether Claude Code's SubagentStop stdin carries the *agent's* id/transcript path needs a runtime check at design time. Fallback if absent: derive `<transcript-dir>/<session-id>/subagents/`, ship files not yet shipped (mtime/size ledger next to the client's existing per-session state). Either way zero new configuration.

**Enterprise seams (named, not designed)**: deltas ride `RecordEvent` → `SessionWrite` capability + bearer gating inherited; `transcript_retention` governs the sidechain buffer as part of the same raw-ephemeral-transcript unit (#4721 scope definition extends naturally); `(tenant, project, session)` key seam unchanged; the fire-and-forget audit gap (ass-069 Q7) applies identically.

**Evidence**: `wire.rs:46,225-251,284-289`; `hook.rs:195-215` (SubagentStart precedent reading `extra.agent_type` + `transcript_path`); vnc-025 SCOPE buffer/purge semantics; ass-069 Q1/Q4.

---

### Q6: Go/no-go and routing

**Answer**: **GO — qualified.** The value is real but concentrated: ~88% of sidechain-only value sits in implementer/investigator agents, and its strongest forms (rule-out catalogs, cross-agent friction aggregation, rework explanations, the backstop for missing agent reports) are things no existing channel can provide. The volume objection dissolves under the channel filter (96.7% reduction, 100% recall, worst session 6% of buffer). The qualified part: this is a **second-order** knowledge source relative to crt-052's main-thread distillation — it should not delay crt-052.

**Routing**:

- **Separate small capture feature** (vnc-track, after F3): hook SubagentStop one-shot read + usr/ast channel filter + `subagent_transcript` event + chunking; server sibling buffer + knob + purge + audit. It is vnc-025-shaped wiring (simpler — no merge machinery) and does not belong inside crt-052's distillation design.
- **crt-052 takes two inputs by contract, ships main-thread first**: design the distillation pass with a `sidechain candidates` input slot (per-agent provenance, type-priority ordering, extended implementer marker family, in-sidechain summary preference, per-session token cap) but make it optional — crt-052 v1 distills the main thread; sidechain input activates when the capture feature lands ("ships dark" symmetry with how vnc-025 preceded F3).
- **Minimal first slice** if appetite is small: capture + buffer only, nothing reads it except a byte-count in the cycle-review response — proves the plumbing and the volume envelope on live sessions before any distillation spend.

---

## Unanswered Questions

- **Does Claude Code's SubagentStop stdin payload carry the subagent's id/transcript path?** Not verifiable from the corpus (sidechain files don't contain their own hook payloads); needs a one-line hook-side dump at design time. Fallback (directory enumeration + shipped-ledger) is specified in Q5 either way.
- **Extended implementer-marker recall** — the corrected marker family for distillation-time selection was *characterized* (6/14 miss list, vocabulary classes) but not built and re-measured; that is crt-052 design work, sized by this spike's labeled sample.
- **Generalization beyond this project's agent roster** — value concentration was measured against `uni-*` swarm roles; a project with different role mechanics (e.g., no self-surfacing reviewer contract) would shift the Q3 tiers. Same single-project caveat as ass-070.
- **Non-Claude-Code sidechain formats** — out of scope by constraint (consistent with ass-070 / crt-052 boundaries).

## Out-of-Scope Discoveries

- **Agent-report compliance is the weak link in the surfaced-channel story** — vnc-024 shipped with zero implementation-agent reports (caught only by gate REWORKABLE FAIL). Independent of sidechain capture, the delivery protocol could enforce report existence pre-gate. Carry-forward to protocol maintenance.
- **A "known pre-existing failures/frictions" registry** would have saved 4+ independent re-triages of the same clippy toolchain noise and 2 re-discoveries each of the linker-OOM and cargo-test-filter traps in this corpus alone. This is distillation-output shaping for crt-052 (a procedure/lesson category), but the observation stands on its own.
- **In-sidechain compaction summaries are free distillates** — when a subagent compacts, Claude Code writes a dense structured summary into the sidechain. Any future transcript consumer (including main-thread work in crt-052) should detect and prefer these blocks.
- **The `Agent` tool_use join is a general sidechain-attribution technique** — exact agent-type recovery for 328/328 files with no configuration; useful for any future tooling over local transcript trees.

## Recommendations Summary

- **Q1 (corpus)**: 328 files / 50 sessions / 83.7 MiB typed 100% via the parent `Agent` tool_use join; 18-file stratified sample labeled into ~54 items; **layer-b ≈26 items exist only in sidechains** — the value-add is real.
- **Q2 (categories)**: implementer lessons + provenance confirmed in implementer/investigator sidechains; **rule-out catalogs and cross-agent friction aggregation are the discovered top categories**; attribution is a weak backstop; reviewer/researcher sidechains are near-zero (negative finding).
- **Q3 (selectivity)**: high tier = rust-dev/tester/bug-investigator (~88% of layer-b in 39% of bytes); capture all types (volume solved by filtering, type names don't generalize), apply type-priority at distillation under the token cap.
- **Q4 (architecture)**: raw one-shot breaks the buffer 2.65× worst-case — reject; ass-070 markers recall only 43% on implementer content — reject as the client filter; **ship whole usr+ast text blocks at SubagentStop: 96.7% reduction, 100% recall, worst session 249 KiB, all blocks < 64 KiB**; per-agent distill at stop infeasible (no LLM in hook) — reject; second-stage marker/type selection happens server-side at cycle review.
- **Q5 (wire/registry)**: additive `subagent_transcript` event + `SubagentTranscriptPayload { agent_id, agent_type, seq, bytes }` (chunked, idempotent, frozen F1 untouched); sibling per-agent map in `SessionState` with its own cap knob — not the offset-merge buffer; vnc-025 purge/audit lifecycle inherited; capability/bearer/retention seams inherit by construction.
- **Q6 (go/no-go)**: **GO, qualified** — capture as a separate small vnc-track feature; crt-052 designs a sidechain-candidates input contract but ships main-thread distillation first; minimal slice = capture + buffer + byte-count visibility, dark until distillation consumes it.
