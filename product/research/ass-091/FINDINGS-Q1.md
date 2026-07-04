# FINDINGS-Q1: The two data planes, articulated precisely (ass-091 FOUNDATION)

**Spike**: ass-091 (GH #898) · **Date**: 2026-07-04 · **Approach**: read-only, file:line-grounded · **Confidence**: validated

> The ONE authoritative data-plane map. Q2/Q3/headline reference this rather than re-deriving. The one divergence trap — content-opaque transcript *fold* (durable integers) vs. raw transcript *content* (memory-only) — is called out explicitly.

## TL;DR
- **Plane A — durable observations.** Hook tool-events persisted to SQL; the source of record. The cycle-review summary (`RetrospectiveReport`) is **100% Plane-A-derived and buffer-independent** — `build_report()` takes no transcript argument. `force`-reproducible.
- **Plane B — in-memory `transcript_candidates`.** Per-session byte ring buffer, **never persisted to disk**, held past session close. Bounded (4 MiB/session tail-elision, 64-session hold cap, 24 h TTL, 1 MiB per-frame clip). Consumed at exactly one seam (`take_transcripts_for_feature` → `distill_before_purge`), attached out-of-band, semantically distilled OUTSIDE the server.
- **Sharp line.** Review prose consumes **nothing** of Plane B content; the retro flow is Plane B's only consumer. The one transcript-derived survivor is a **content-opaque integer fold** on a *separate* aggregate record — not the prose, not `force`-reproducible after purge.
- **Third de-facto source.** Persisted GH-issue `## Knowledge Stewardship` comment blocks — full-fidelity durable prose, neither plane.

---

## Plane A — durable observations (hook tool-events)

### A.1 Observation event + `RetrospectiveReport` schema
- Hook tool-events stream into the session registry and are persisted as `ObservationRecord` rows; `build_report` consumes `&[ObservationRecord]`. It derives `session_count` by de-duplicating `r.session_id` (`crates/unimatrix-observe/src/report.rs:23-27`) and `total_records = records.len()` (`report.rs:32`) — a row carries at minimum `session_id` + the tool-event payload the reckoners read.
- `RetrospectiveReport` construction: `report.rs:29-52`; type def in `unimatrix-observe/src/types.rs`. Fields: `feature_cycle`, `session_count`, `total_records`, `metrics`(MetricVector), `hotspots`, `is_cached`, `baseline_comparison`, `entries_analysis`, `narratives`, `recommendations`, `session_summaries`, `feature_knowledge_reuse`, `rework_session_count`, `context_reload_pct`, `attribution`, `phase_narrative`, `goal`, `cycle_type`, `attribution_path`, `is_in_progress`, `phase_stats`, `curation_health`.

### A.2 What survives the cycle (durable SQL substrate — Plane A is a union, not one table)
- **observation records** — tool-event source of record (`report.rs:15-17`).
- **`cycle_events`** — phase-lifecycle audit timeline; rank-1 phase reckoning (`crates/unimatrix-server/src/mcp/review_aggregates.rs:105-118` → `reckon_phase_aggregates`).
- **`SessionRecord`s** — rework ratio, rank-2 (`review_aggregates.rs:81-97`).
- **`query_log ∪ injection_log`** — served-knowledge union, rank-3/#320 (`review_aggregates.rs:88-102`).
- **historical `MetricVector`s** — baseline source.
- **cached `CycleReviewRecord`** — memoized full-report JSON (`crates/unimatrix-store/src/cycle_review_index.rs:57` `SUMMARY_JSON_MAX_BYTES = 4*1024*1024`; `:96` "Full RetrospectiveReport JSON. No evidence_limit truncation").

### A.3 Which summary fields derive from Plane A
`build_report(feature_cycle, records, metrics, hotspots, baseline, entries_analysis) -> RetrospectiveReport` — **`report.rs:15-22`. No `transcript_candidates` argument.**

| Field | From | Where |
|---|---|---|
| counts (`session_count`,`total_records`) | observations | `report.rs:23-32` |
| hotspots + recommendations | hotspot findings over observations | `report.rs:34,39`; `recommendations_for_hotspots` `report.rs:60-104` |
| phase timeline (`phase_narrative`) | `cycle_events` | `review_aggregates.rs:105-118`; **ADR-004 crt-025** — PhaseNarrative sourced from CYCLE_EVENTS, explicitly NOT observation telemetry (Option A rejected) |
| knowledge-reuse (`feature_knowledge_reuse`) | `query_log ∪ injection_log` | `review_aggregates.rs:88-102` |
| rework (`rework_session_count`) | `SessionRecord` outcome ratio | `review_aggregates.rs:81-97` |
| `context_reload_pct` | reload reckoner | `review_aggregates.rs:120+` |
| `baseline_comparison` | historical MetricVectors | `report.rs:20,36` |

### A.4 crt-057's enabling fact — buffer-independent, `force`-reproducible
- No transcript input to `build_report` (`report.rs:15-22`) ⇒ `RetrospectiveReport` is a pure function of durable SQL; the buffer's presence cannot change a byte.
- **ADR-004 crt-052 (#4850)** attaches `transcript_candidates` at **response-assembly level, OUTSIDE the memoized `RetrospectiveReport`**; the report struct has NO candidate field, so its stored bytes are identical with or without a transcript. On memo-HIT candidates are distilled *fresh from the live buffer at call time* — never baked into the cached summary.
- **`force`** (`RetrospectiveParams.force`, `crates/unimatrix-server/src/mcp/tools.rs:446-448`; gate `tools.rs:2221,2227`; description `tools.rs:2123` "Use force=true to recompute fresh telemetry") recomputes from durable substrate ⇒ byte-identical summary. This is the property crt-057's non-destructive default rests on.
- **`RetrospectiveParams` today has NO `include_transcript_candidates`** (full struct `tools.rs:432-460`) — that boolean is crt-057's *provisional* addition, the exact axis ass-091 redesigns. Confirmed absent.

### A.5 Precision note (do NOT conflate — carry to Q2/Q3)
The one transcript-derived datum that survives is **not** summary prose: a **content-opaque integer fold** — `transcript_bytes_total`, `transcript_delta_count`, `transcript_error_count`, `transcript_refusal_count`, `signal_class_counts_json` — landed **read-before-purge** onto the *separate* `CycleReviewRecord` aggregate, NOT onto `RetrospectiveReport` (`review_aggregates.rs:59-79` `land_fold`; module doc `:15-16,25-26` "Every value is i64/String and content-free … structural leak gate holds"). Model = **ADR-005 crt-054 (#5030)**: running-fold-only, content-opaque, integers only; raw content read stays on the ingest path. Consequences:
1. "100% observation-derived / buffer-independent" is about `RetrospectiveReport`. The fold counts are a distinct durable signal on a distinct record.
2. The fold is **not** `force`-reproducible after purge — read live strictly before purge; once purged a re-review lands zeros/unavailable. Byte-identity is a property of the observation-derived summary, not the fold.

---

## Plane B — in-memory `transcript_candidates`

### B.1 Structure + never-persisted
- `TranscriptBuffer` (per-session byte ring): `crates/unimatrix-server/src/infra/session_transcript.rs:45-59` — fields `base_offset`, `data`, `holes`, `high_water`, `elided_bytes`. Held behind `Arc<Mutex<TranscriptBuffer>>` in the registry (`crates/unimatrix-server/src/infra/session.rs:266` `sessions: Mutex<HashMap<String, SessionState>>`; handles `session.rs:447,507-509`).
- Streamed in via `apply_delta(offset, bytes)` (`session_transcript.rs:168-225`); frame ceiling ~1 MiB (`session_transcript.rs:14-16` "any bytes up to the 1 MiB frame ceiling … clip").
- **Never persisted to disk**: **ADR-005 vnc-024 (#4721)** — in-memory only, `PurgeOnCycleClose` default, `RetainDays` rejected in OSS, no content secret-scanner. This is NG-1. Reinforced by **ADR-005 crt-054 (#5030)** — content read never leaves the ingest path.

### B.2 Bounds — the fidelity ceiling
- **4 MiB per-session ring-tail elision.** `DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES = 4_194_304` (`session_transcript.rs:24-26`); applied via `with_transcript_cap` (`session.rs:291`). `apply_delta` advances `base_offset = end - max_bytes` and accumulates `elided_bytes` when the span exceeds the cap (`session_transcript.rs:194-211`). Tail-retention: newest bytes kept, oldest elided.
- **64-session hold cap.** `transcript_hold_max_sessions` (config `crates/unimatrix-server/src/infra/config.rs:1859-1860`), **default 64** (`config.rs:1906-1909`), validated non-zero (`config.rs:2048-2052`). Oldest-first eviction ordered by `last_activity_at` (`transcript_hold.rs:113`). (Note: `MAX_HOLE_RANGES = 64` at `session_transcript.rs:31` is a *different* 64 — hole-range count, not the session cap.)
- **24 h TTL.** `transcript_hold_ttl_secs` (`config.rs:1872-1873`), **default 86_400** (`config.rs:1910-1913`), validated non-zero (`config.rs:2059-2063`). Enforced by independent `sweep_expired(ttl)` reclaiming buffers whose `last_activity_at` exceeds TTL regardless of review cadence (`transcript_hold.rs:18-19`).
- **Per-event clip** at the 1 MiB frame ceiling (`session_transcript.rs:14-16`); the practical retained slice per event is a truncated head — the "few-hundred-bytes" retained-head is what Q3's regex/`match` runs over.
- **Primary vs Reconstructed provenance (~0.81 fidelity, ADR-007).** `distill/reconstruct.rs:6-8` — "fidelity FLOOR (0.81 ceiling, DEC-weakest — ass-070 Q6), NOT parity"; degraded path made discriminable by `provenance: Reconstructed`, assigned per-session by the C6 handler via `SessionLossInfo` (ADR-006/ADR-007); `:67-69` emits `SessionLossInfo` so loss stays visible; `:153` no fabricated narrative. Provenance discriminability asserted in `distill/reconstruct_tests.rs:226-234`.

### B.3 Lifecycle & every reclamation path
| Path | Trigger | Removes | File:line |
|---|---|---|---|
| Ring-tail elision | `apply_delta` exceeds 4 MiB/session | oldest bytes; bumps `elided_bytes` | `session_transcript.rs:194-211` |
| Cycle-close purge (the seam) | `context_cycle_review` SUCCESS return | all attributed-feature session buffers (extract-all-then-purge) | `server.rs:640-680` (`emit_purge_audits … "cycle_review"` `:669,676-680`); read-before-purge `review_aggregates.rs:15-16` |
| Session-close purge | per-turn session close | that session's buffer | audit trigger `session_close` (**#4742** detail `bytes=<n> trigger=<session_close\|stale_sweep\|cycle_review>`) |
| Independent stale-sweep TTL | `last_activity_at` > 24 h | expired buffers, review-independent | `transcript_hold.rs:18-19`; **#4857** |
| Hold-count cap eviction | held sessions > 64 | oldest-first by `last_activity_at` | `config.rs:1859-1860`; `transcript_hold.rs:113`; **#4857** |
| `clear()` | called by purge paths | returns purged bytes; `high_water`/`elided_bytes` unchanged | `session_transcript.rs:349-373` |

Held-buffer store + dual reclamation (cap + TTL) is **ADR-008 crt-052 (#4857)**; content-free purge audit is **ADR-004 vnc-025 (#4742)**. `STALE_SESSION_THRESHOLD_SECS = 4*3600` (`session.rs:22`) is a separate registered-session staleness threshold, not the hold TTL. Poison-recovery on the hold lock: `transcript_hold.rs:201,388` (#4764).

### B.4 The single content seam — distilled OUTSIDE the server
- `take_transcripts_for_feature(feature_cycle) -> Vec<(String, TranscriptSnapshot)>` (`session.rs:502-545`, return `:505`) scans **registered ∪ held**, dedup by Arc (`session.rs:3678`); under-lock work is scan + `Arc::clone` only, no parse/I/O (`session.rs:215`).
- `distill_before_purge(...)` (`crates/unimatrix-server/src/mcp/distill_handler.rs:48`) calls `registry.take_transcripts_for_feature` (`:65`), returns `None` for `RetainDays` (`:60`), and does **mechanical** work only: `select_candidates` + `reconstruct_from_observations` (`distill_handler.rs:27`), reconstruct fallback on hole-fraction (`:150`), cycle-cap chronological keep-earliest (`:222`), loss merge (`:254`). It **attaches** the distilled section at assembly level as a JSON content item — `attach_to_response_assembly` (`:269-306`; test `:495`), consistent with **#4750** (four success-return points; distill-before-purge at the same seam).
- **Semantic distillation happens OUTSIDE the server.** The server hands out selected/reconstructed candidates + loss accounting attached out-of-band (ADR-004 assembly-level, #4850); the retro architect agent turns them into narrative. "Distill" is two-sense: server-side = mechanical select/reconstruct/attach; agent-side = prose synthesis.
- **One-shot destructive today**: extract-all (fold + distill, read-before-purge `review_aggregates.rs:15-16`) then purge-all (`server.rs:640-680`). This is the model Q3's scoped-retrieval purge-reconciliation must break.

### B.5 Loss/provenance accounting a consumer can see
- `elided_bytes` surfaced via snapshot (`session_transcript.rs:59,309,379-380`) — flags data past the 4 MiB tail (feeds Q3's "no-match = didn't happen vs. past truncation").
- `high_water` (`session_transcript.rs:373`).
- `SessionLossInfo` rows (`reconstruct.rs:69`; merged `distill_handler.rs:254`).
- `provenance: Primary | Reconstructed` label per session (`reconstruct.rs:7,67`).
- Dropped-by-session-cap count surfaced (`distill_handler.rs:183`; test `:418` `test_aggregate_cap_drop_surfaces_count`).

---

## The sharp line
- The review **prose** reads nothing from Plane B: `build_report` has no transcript input (`report.rs:15-22`).
- Plane B is consumed at **exactly one seam** — `take_transcripts_for_feature` → `distill_before_purge` (`distill_handler.rs:48-65`) — attached out-of-band (`attach_to_response_assembly` `:269`), for the **downstream retro flow only** (#4850 attaches outside the memoized report).
- The **only** transcript-derived durable survivor is the content-opaque integer fold on the *separate* `CycleReviewRecord` (`review_aggregates.rs:59-79`; #5030) — counts, never prose. So: summary prose ⟂ Plane B content; fold counts are integers, not a Plane-B content leak.

## The third de-facto source — GH-issue `## Knowledge Stewardship` blocks
Agents post a `## Knowledge Stewardship` comment block (Queried / Stored / Declined) to the GH issue at end of work — convention defined across agent specs, e.g. `.claude/agents/uni/uni-architect.md:179-194`. These are **durable** (GitHub), **full-fidelity prose**, readable by a fresh-context retro subagent — but **neither plane**: not in the observations SQL (Plane A), not in the buffer (Plane B). This is Appendix A tier-2. In bugfix-891 the standout IDs #5417/#3827 came from these blocks — not the summary's top-entries table (which ranked #92/#93/#648/#684/#922) and not primarily Plane B (Reconstructed corroboration only). The retro's real provenance runs across all three sources.

---

## Unanswered / carry-forward
- **Out-of-scope (Q3):** the extract-all-then-purge one-shot model (`server.rs:640-680` + read-before-purge `review_aggregates.rs:15-16`) is the load-bearing constraint scoped retrieval must reconcile — flagged here, resolved in Q3.
- **Precision flag for ass-090:** the content-opaque fold (#5030) is the existing "distill signal INTO the summary" beachhead; ass-090 should extend the fold at this seam, not touch Plane B raw content.

**Citations: ~70 file:line + 6 grounding entries (#4721, #4850, #4742, #4857, #4750, #5030).**
