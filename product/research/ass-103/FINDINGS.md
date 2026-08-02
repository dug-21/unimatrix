# FINDINGS: ass-103 — Background maintenance-engine (tick) inventory + potential-issue triage

**Spike**: ass-103 | **Date**: 2026-07-19 | **Approach**: investigation (read-only, code-only) | **Confidence**: directional | **Issue**: #961

> Scope discipline: this IDENTIFIES and TRIAGES. No recommendations, no fix backlog, no re-architecture, no capability authoring.

## Orientation: exactly ONE recurring background loop

`tokio::time::interval` appears at exactly two non-test sites in the whole workspace — `background.rs:346` and `background/tick_loop.rs:126` — and they are the same loop wired two ways. **Every periodic operation in Unimatrix rides this tick.** No second scheduler, no cron, no per-feature timer.

| Path | Driver | Wiring | Used by |
|---|---|---|---|
| Per-slug (current) | `spawn_per_slug_tick` → `run_per_slug_tick_pass` (`tick_loop.rs:45-72`) over a 9-job registry (`job.rs:265-278`) | `main.rs:1555` | HTTP daemon, N=1 and N≥2 |
| Legacy (carve-out) | `spawn_background_tick` → `run_single_tick` (`background.rs:428-778`) | `main.rs:1962` | stdio single-store only |

Cadence is a static predicate (`job.rs:194-202`), not a scheduler. Default interval **900s** (`background.rs:105-107`); per-op timeout **120s** (`background.rs:418`); counter is **per-slug** (`job.rs:177-182`).

## (a) Inventory — every tick phase/operation

Registry order IS the ordering invariant (`job.rs:265-278`, test at `:320-339`). All ops are per-slug on the daemon path; every write routes through `ctx.store`.

| # | Op | Cadence | Reads | Writes | Cache rebuilt (Principle 7) | Cost shape |
|---|---|---|---|---|---|---|
| **1** | **maintenance** (`background.rs:1131`, job `jobs.rs:31-100`) | EveryTick | 1a–1n below | below | `EffectivenessState`, `ConfidenceState` | dominant I/O op |
| 1a | `load_maintenance_snapshot` (`status.rs:306-403`) | every tick | `entries WHERE status=0`+`tags`; `injection_log ⋈ sessions`; **2nd** `entries WHERE status=0` scan (`read.rs:1587-1597`) | — | — | O(N)×2 + full `injection_log` scan (grows unbounded) |
| 1b | Effectiveness classify + `build_report` (`status.rs:326-396`, `effectiveness/mod.rs:315-380`) | every tick | in-memory | via 1e | — | O(N) + 5 extra passes for aggregations the tick discards |
| 1c | Quarantined-vector prune (`status.rs:1045-1081`) | every tick | `vector_map ⋈ entries status=3` | `vector_map` DELETE, IdMap | `VectorIndex` | N+1 deletes |
| 1d | Heal pass / re-embed (`status.rs:1095-1217`) | every tick, batch `heal_pass_batch_size` | `entries WHERE embedding_dim=0` | `vector_map`, HNSW, `entries.embedding_dim` | HNSW | **ONNX embed + 4-5 sequential DB round-trips PER ENTRY**, serial |
| 1e | EffectivenessState swap + `consecutive_bad_cycles` (`background.rs:1170-1260`) | every tick | snapshot | `EffectivenessStateHandle` | **full swap** | O(N) under one write lock, no I/O inside |
| 1f | `cleanup_stale_co_access` (`write_ext.rs:330-337` ← `status.rs:1221`) | every tick | `co_access` | DELETE `last_updated < now-365d` | — | one DELETE |
| 1g | Confidence refresh (`status.rs:1232-1292`) | every tick | in-memory | `entries.confidence` per id | — | cap **500** (`coherence.rs:22`), **200ms wall budget**, N+1 |
| 1h | Empirical prior + spread (`status.rs:1294-1359`) | every tick | **2 more full `status=0` scans** | `ConfidenceStateHandle` | **full swap** | 2 scans of data already in memory |
| 1i | Graph compaction (`status.rs:1361-1400`) | `graph_stale_ratio > 0.10` (`coherence.rs:16`) + adapter | `active_entries` | `VectorIndex::compact` | `VectorIndex` | **re-embeds ENTIRE corpus, one ONNX batch, no cap** |
| 1j | Cycle activity GC (`status.rs:1403-1576`) | every tick, cap `max_cycles_per_tick` | `cycle_review_index`, `observations`, `query_log`, `injection_log`, `sessions` | DELETEs ×4 | — | 3 awaits/cycle (N+1), bounded |
| 1k | audit GC (`:1578`), stale-session sweep (`:1598`), session GC (`:1677`) | every tick | `audit_log`, `SessionRegistry`, `sessions`, `injection_log` | DELETEs, `sessions.feature_cycle` | `PendingEntriesAnalysis` | N+1; new `AuditLog` per purge list |
| 1l | Auto-quarantine (`background.rs:1406-1482`) | every tick, `auto_quarantine_cycles>0` (dflt 3) | `EffectivenessState` | `entries.status → Quarantined`, `audit_log` | `EffectivenessState` | N+1 UPDATEs |
| 1m | Lifecycle-guard stub (`background.rs:1293-1305`) | every tick | `list_adaptive()` | — | — | **dead work**, `TODO(#409)` |
| 1n | `run_dead_knowledge_migration_v1` (`background.rs:1330-1397`) | every tick, **one-shot-gated** | `counters` PK lookup | `entries.status` (first run only) | — | O(1) after completion — **correctly gated** |
| **2** | **orphaned_edge_compaction** (`background.rs:805-836`) | EveryTick | `graph_edges ⋈ entries` | UPDATE repoint (`:869`) then **DELETE** (`:810-817`) | — | unbounded repoint + 1 DELETE |
| **3** | **co_access_promotion** (`co_access_promotion_tick.rs:204`) | EveryTick | `co_access ⋈ entries` | `graph_edges` INSERT OR IGNORE + weight UPDATE | — | cap `max_co_access_promotion_per_tick`=**200** |
| **4** | **typed_graph_rebuild** (`typed_graph.rs:91`, `background.rs:968`) | EveryTick, Rayon | `query_all_entries()` + `query_graph_edges()` — **full tables, no cap** | — | **`TypedGraphStateHandle`, full swap** (`:981`) | **O(N+E) every tick, unbounded** |
| **5** | **phase_freq_rebuild** (`phase_freq_table.rs:133`, `background.rs:1013`) | EveryTick | `observations`, `cycle_events ⋈ sessions` | **nothing** | **`PhaseFreqTableHandle`, full swap** | O(R log R), lookback 30d |
| **6** | **contradiction_scan** (`background.rs:1062`) | **EveryN(4)** + adapter | `entries status=Active` | — | **`ContradictionScanCacheHandle`** | **O(N) ONNX + N HNSW searches** |
| **7** | **extraction** (`background.rs:1686-1908`) | EveryTick, Rayon | `observations` (watermark, batch 1000); **3rd** full active load (`:1791`) | `entries` (status **Active**), `tags` | `TickMetadata` | ONNX+HNSW+contradiction **per accepted entry**; N+1 inserts |
| **8** | **graph_inference** (`nli_detection_tick.rs:158`) | EveryTick, Rayon | `entries`, isolated-node/existing-pair queries, per-pair content | `graph_edges` Supports/Informs | — | O(C·ef), ef=32; NLI; caps **100**/**25**/**50** |
| **9a** | **S1** tag co-occurrence (`graph_enrichment_tick.rs:83`) | EveryTick | `entry_tags` self-join | `graph_edges` Informs bidir | — | O(T²) SQL, cap **200** |
| **9b** | **S2** vocabulary (`:165`) | EveryTick | `entries × entries` + per-term `instr()` | `graph_edges` Informs | — | **O(N²·V)** — but `s2_vocabulary` defaults **empty** → early-return no-op |
| **9c** | **S8** search co-retrieval (`:291`) | **`tick % s8_batch_interval_ticks`**, dflt **10** | `counters` watermark, `audit_log`, `entries` | `graph_edges` CoAccess @ 0.25, `counters` | — | O(P²)/row, cap **500** |

**Principle 7 caches rebuilt by the tick** (`PRODUCT-VISION.md:72`): `TypedGraphStateHandle`, `PhaseFreqTableHandle`, `EffectivenessStateHandle`, `ConfidenceStateHandle`, `ContradictionScanCacheHandle`, plus `VectorIndex` HNSW/IdMap. **All five are full-object swaps; none incremental.** The tick is sole writer of every one — it is not merely maintenance, it is **the only supply line to the entire serving hot path**.

## (b) Capability mapping — factual

| Tick op | Capability delivered | Node |
|---|---|---|
| 1g + 1h confidence refresh/prior | **SL2** — useful surfaces more, misleading recedes | #5556 (partial) |
| 1e + 1l effectiveness + auto-quarantine | **SL2** (the "recedes" half) | #5556 |
| 3 co-access promotion; 9c S8 | **SL4** — used together surfaces together | #5560 (partial) |
| 2 compaction (repoint leg) | **SLN3** — graph integrity-consistent under correction | #5538 (partial) |
| 2 compaction (DELETE leg) | **SLN3 — negative contribution** (I-1, I-2) | #5538 |
| 4 TypedGraph rebuild | **SL5** — supplies the PPR substrate | #5564 |
| 5 PhaseFreq rebuild | **PD3** — goal- and phase-conditioned delivery | #5530 (partial) |
| 1j/1k cycle/audit/session GC | **RETAIN** — storage bounded by learning utility | #5581 (**proven**) |
| whole per-slug loop | **C5** — per-slug analytics maintained | #5550 (**proven**) |
| 8 graph inference | SL5 / graph density — **no dedicated node found** | — |
| 6 contradiction scan | serves `context_status` — **no node found** | — |

**Silent dependencies** — capabilities whose delivery secretly rides the tick:
- **SL2** — confidence is refreshed *only* by op 1g; no synchronous path exists. Auto-quarantine (1l) is the only automatic demotion mechanism anywhere.
- **PD3** — the phase-freq table is read-only in the tick and **written nowhere else**. Op 5 is its entire supply.
- **SL5** — PPR runs against `TypedGraphStateHandle`, which only op 4 writes.
- **RETAIN (`delivery:proven`)** — proven status rests entirely on 1j/1k firing every tick.
- **C5 (`delivery:proven`)** — the claim *is* that `run_per_slug_tick_pass` completes for every registered slug.

**Orphans** — ops serving no stated capability: 1m lifecycle stub (`TODO(#409)`, debug-log only); op 6 contradiction scan (O(N) ONNX every 4th tick for a `context_status` cache, no node); 9b S2 (no-op on default config); 1b's `by_category`/`by_source`/`calibration`/`unmatched` (computed every tick, never read by the tick); op 8's cosine-Supports and Informs legs.

**Headline mapping fact:** the tick is sole delivery mechanism for ≥6 capability nodes, two marked `delivery:proven` — and the tick appears nowhere in the goal/capability map as an entity. Nothing records that those proofs are conditional on a background loop continuing to complete.

## (c) Potential-issues register — RANKED URGENT-FIRST

### INTEGRITY-RISK

**I-1. Compaction deletes edges to `Proposed` entries every tick (#889).** `background.rs:810-817` binds `Status::Active` only, so every non-Active status is swept. `Proposed` is an agent-supplied live pending state (`infra/validation.rs:128` parses `"proposed"` from `context_store`). An agent storing a proposed entry with declared edges loses them within ≤15 min, before promotion. The repoint leg (`:869`) scopes itself to Deprecated-with-successor, so `Proposed` gets neither repoint nor reprieve.

**I-2. Quarantine destroys edges irreversibly, and the tick quarantines then destroys in the SAME pass (#890, compounded).** Same DELETE sweeps `Quarantined`. Quarantine is reversible; edge deletion is not. **New evidence:** registry order is `maintenance` (job 1) → `orphaned_edge_compaction` (job 2) (`job.rs:267-268`), and auto-quarantine runs inside job 1 (`background.rs:1406-1482`, called `:1264`). The tick auto-quarantines on an effectiveness heuristic then permanently destroys that entry's relationships microseconds later, same pass, before any human review. Restore returns the entry but not its graph.

**I-3. Degenerate confidence state written into the live serving cache on query failure.** `status.rs:1303-1359`: both prior queries use `.unwrap_or_else(|e| { warn!(); vec![] })` (`:1313-1317`, `:1331-1335`), then `compute_empirical_prior(&[])` / `compute_observed_spread(&[])` results are **written into `ConfidenceStateHandle` anyway** (`:1347-1350`). Silent corruption, not a skip — a transient DB error replaces the live α₀/β₀/spread/weight that SL2 ranking reads. Unlike every other cache in the tick, there is no retain-on-error.

**I-4. No demotion owner exists anywhere for threshold-promoted edges — the graph grows monotonically.** Promotion: `co_access_promotion_tick.rs:204` (`count >= 3`, `read.rs:1815`), `graph_enrichment_tick.rs:96` (`HAVING COUNT(*) >= 3`, hardcoded), `:215` (`>= 2`), `nli_detection_tick.rs:709` (score threshold). **No module in the crate deletes an edge when its count or score falls back below threshold.** Deleters: `background.rs:811` (endpoint status, not counts), `write_ext.rs:331` (deletes the *source* `co_access` row, never the derived `graph_edges` row), `mcp/edge_write.rs` (filters `source='agent'`, structurally excluding all tick-authored edges). This is #3822 realized: co-access GC removes source rows after 365 days while promoted edges persist forever with no owner, and dedup pre-filters (`nli_detection_tick.rs:194-201`) actively exclude already-written pairs from rescoring. SL4 and SLN3 both ride this.

**I-5. A transient coverage dip WIPES the learned phase table.** `phase_freq_table.rs:152-166`: when `coverage_count < min_phase_session_pairs` (dflt 5), rebuild returns a cold-start table and `background.rs:1030-1033` swaps that empty table into the live handle. `:142-147` does the same for empty `rows_a` **with no log at any level**. Every other cache retains-on-error; this replaces good state with empty on a *successful* rebuild that saw thin data. Cycle GC (`status.rs:1403-1576`), in the same tick, deletes `sessions` rows — the tick can push coverage below threshold and erase PD3's entire signal.

### CORRECTNESS

**I-6. A poisoned `tick_metadata` mutex silently pins `current_tick` to 0 forever.** `jobs.rs:147-151`, `:227-231`, `:376-380` use `.lock().map(...).unwrap_or(0)`; contrast `job.rs:177-182` where `next_tick()` is deliberately poison-tolerant (`unwrap_or_else(|e| e.into_inner())`). After any poisoning panic the counter still advances but every job's *internal* gate reads 0 permanently — S8 fires every tick instead of every 10th, and the early-run warn window re-triggers forever. Silent, permanent, and it inverts a cost control.

**I-7. Two divergent copies of the same tick logic in one file.** crt-056 helpers `run_typed_graph_rebuild` (`background.rs:968`), `run_phase_freq_rebuild` (`:1013`), `run_contradiction_scan` (`:1062`) are verbatim extractions — but `run_single_tick` still holds the **original inline copies** at `:537-574`, `:596-639`, `:657-713`, and stdio still runs that path (`main.rs:1962`). A fix to a helper does not reach stdio. `run_orphaned_edge_compaction` carries an explicit comment warning against exactly this (`:517-518`) — honored for compaction, violated for the other three.

**I-8. S8-authored edge weights are overwritten by the co-access promotion tick.** `co_access_promotion_tick.rs:123-171` selects/UPDATEs on `relation_type = 'CoAccess'` **without filtering `source`**. S8 writes `CoAccess` at weight 0.25 with `source = EDGE_SOURCE_S8` (`graph_enrichment_tick.rs:448-477`). Two features write the same relation type; one silently mutates the other's data.

**I-9. Persistent rebuild failure serves an indefinitely stale graph with no distinguishing state.** `background.rs:994-1005` — store error, panic, and timeout all log and retain. No staleness flag, no age, no counter. Principle 7 makes this the *only* graph the query path sees, so a permanently failing rebuild is indistinguishable from a healthy one to every reader. `use_fallback = true` (`:987-993`) is cleared only by a later full successful rebuild overwriting the struct.

**I-10. Effectiveness snapshot failure silently disables auto-quarantine with NO audit event.** `status.rs:387-395` returns `effectiveness = None` inside an `Ok(...)`; `background.rs:1170` then skips steps 2–9. The ADR-002 hold path that emits `tick_skipped` (`:1149-1154`) is never taken because the snapshot returned `Ok`. SL2's demotion half stops, and the audit log — which Principle 2 requires to be complete — records nothing.

**I-11. Auto-quarantine retries a failing UPDATE forever with no backoff.** `background.rs:1468-1477` — on failure the counter is deliberately not reset, so the entry re-qualifies every tick. One failed write per tick per stuck entry, indefinitely, at `warn`, no attempt counter.

**I-12. Near-threshold oscillation in auto-quarantine is undamped (#3822 class).** `background.rs:1206-1226` — bad increments; `Effective|Settled|Unmatched` **removes** the counter (`:1224`). No decay, no hysteresis. An entry alternating bad/good never quarantines however long it is predominantly bad. The classifier input includes `topic_has_sessions` (`status.rs:355-356`), so classification can flip purely because sessions aged out — and the mechanism aging them out is cycle GC (`status.rs:1403-1576`) **in the same `run_maintenance` call**. The tick's own GC perturbs the tick's own classifier.

**I-13. `generation` is incremented unconditionally every tick** (`background.rs:1255`). It is the invalidation signal for the generation-cached snapshot pattern; bumping it every tick forces every downstream reader (search, briefing) to re-clone the full `HashMap` every 15 min even when nothing changed — defeating the caching the pattern exists to provide.

**I-14. Confidence refresh throughput silently capped far below corpus size.** `status.rs:1232-1292` — cap 500 (`coherence.rs:22`) AND a 200ms wall budget with per-entry N+1 awaits; abandonment logged at **`debug!`** only (`:1275-1281`). At typical SQLite write latency that admits ~100–200 rows/tick. For thousands of stale entries SL2 converges very slowly with nothing above `debug` saying so. Oldest-first sorting means progress is made, but the ceiling is invisible.

### COST

**I-15. Graph compaction re-embeds the entire corpus with no cap, and can self-perpetuate.** `status.rs:1361-1400` — gated on `graph_stale_ratio > 0.10`, then all `active_entries` fed to `adapter.embed_entries` in **one unbounded ONNX batch** (`:1366-1371`). Largest cost spike in the tick, no batch cap, no incremental path. If it exceeds the 120s timeout, the tick aborts, the stale ratio stays >0.10, and it retries identically next tick — a self-perpetuating saturation loop, warn-level only. Structurally the #280 failure mode (a phase whose cost grew past the timeout) in a new location.

**I-16. The active-entries table is fully read five times per tick.** (1) `load_active_entries_with_tags` (`status.rs:310-314`); (2) `load_entry_classification_meta` (`read.rs:1587-1597`) — second scan of the same `WHERE status=0` set with overlapping columns; (3) extraction's `query_by_status(Active)` (`background.rs:1791-1800`); (4)+(5) the two prior-computation scans (`status.rs:1306-1312`, `:1328-1335`) over data already in memory.

**I-17. Residual `compute_report`-inflation.** `effectiveness/mod.rs:315-380` computes `by_category` (5 full passes, `:323-330`), `by_source` (`:332`), `calibration` (`:333`, whose `calibration_rows` are fetched solely to feed it) and `unmatched` (`:357-368`) — the tick consumes only `all_entries` plus two audit-string lookup lists. `noisy_entries` is built with **no cap** (`:350-355`). #280 was fixed by introducing `load_maintenance_snapshot` to stop paying for `compute_report`'s unused phases; the snapshot still calls `build_report`, so a narrower version of the same inflation survives the fix.

**I-18. TypedGraphState rebuild is O(N+E) over full tables every tick, no cap** (`typed_graph.rs:93`, `:115`), bounded only by TICK_TIMEOUT. Combined with I-4 (edges grow monotonically), E grows without bound and this op's cost grows with it — the two compound.

**I-19. A tick interval of 0 panics the loop into an infinite 30s restart cycle.** `background.rs:96-101` — `parse_tick_interval_str("0")` returns `Ok(0)`; `read_tick_interval` (`:107-127`) applies no lower bound; `tokio::time::interval(Duration::from_secs(0))` panics. Supervisor catches, waits 30s, restarts into the same panic (`tick_loop.rs:109-115`, `background.rs:307-313`). Operator-triggerable via `UNIMATRIX_TICK_INTERVAL_SECS=0`.

**I-20. Missed-tick behavior is the tokio default (Burst) — a slow pass causes a catch-up storm.** Neither `background.rs:346` nor `tick_loop.rs:126` calls `set_missed_tick_behavior`. Under the serial per-slug loop, pass time scales with N slugs (each op carrying its own 120s timeout). Once a pass exceeds the interval, `interval.tick()` returns immediately per missed period, firing back-to-back passes precisely when the system is already saturated. Bears directly on C5 (`delivery:proven`) at N≥2.

**I-21. Per-tick allocations and dead work.** `DomainPackRegistry::with_builtin_claude_code()` constructed twice per tick and discarded both times (`background.rs:462-463`/`jobs.rs:52`, and `:1637`) — the path's own comment says the registry is never consulted (`:460-461`). Fresh `StatusService` per tick (`:464-475`). The `StatusReport` shell passed into `run_maintenance` (`:1160-1163`) takes three field writes then is dropped when `maintenance_tick` returns. Op 1m is a pure no-op stub. `fetch_observation_batch` takes the **write** pool for a read-only query (`:1617`).

**I-22. Malformed observation payloads dropped with no log at any level.** `background.rs:1667` — `serde_json::from_str(&s).ok()` converts parse failure to `None`; the record is still pushed with `input: None` and flows into extraction rules as if it legitimately had no input.

**I-23. Extraction gate rejections and fail-open admissions are invisible.** Rejections at `debug!` only (`background.rs:1730-1735`). In the rayon closure, embed failure (`:1813-1817`) and HNSW search failure (`:1820-1824`) both `passed.push(entry); continue` — **fail-open, entry admitted with no contradiction check, no log**. A missing embed adapter (`:1787`) skips gates 5–6 *and* the entire store step silently.

**I-24. NLI graph inference is off by default and its unavailability is invisible.** `nli_enabled` defaults **false** (`config.rs:960`, `:1023`) so Path B never runs out of the box (`nli_detection_tick.rs:566-569`); `get_provider()` failure returns at **`debug!`** (`:576-583`) — a permanently unloadable model produces nothing above `debug` forever.

### BENIGN (checked, no issue)

**B-1. `run_dead_knowledge_migration_v1` is correctly one-shot-gated — the dead-phase hypothesis does NOT hold.** `background.rs:1334-1339` reads the counter first and early-returns; post-completion cost is one indexed PK lookup (`counters.rs:39-50`); the scan at `:1345` is unreachable after first success. Two narrow non-terminating cases (`:1345-1354`, `:1384-1390`) retry next tick, and `.unwrap_or(0)` at `:1336` makes a counter-read error look like "never ran" — noted, low-consequence since the body is idempotent.

**B-2. Graph-state GC is NOT badly scattered.** Exactly one tick-owned `graph_edges` DELETE (`background.rs:811`). Others are the agent-facing MCP tool (`mcp/edge_write.rs`, 6 statements, all `source='agent'`-scoped or explicit) and full-reset import (`import/mod.rs:385,392`). The problem is not scattering — it is the **absence** of a count-aware deleter (I-4).

**B-3. Per-slug isolation holds structurally.** Every job touches only `ctx`'s handle set; handles are `Arc::clone`s of the serving `ServiceLayer`'s (`job.rs:151-170`, `Arc::ptr_eq` test at `:402-426`); per-slug counters distinct (`:439-447`). No cross-slug write path found. The per-slug correctness concern named in SCOPE is not realized on the daemon path.

## Unanswered questions

- **Actual tick wall-clock cost at production corpus size** — every cost figure here is a shape from code, not a measurement. *Out of scope for `directional`; needs a measurement spike.*
- **Whether I-15 actually triggers in practice** — requires sustained `graph_stale_ratio > 0.10` on a corpus where full re-embed exceeds 120s. *Requires measurement.*
- **Blast radius of I-4 on retrieval quality** — that edges are never demoted is established; whether accumulated stale edges measurably degrade PPR is not. *Needs a retrieval-quality eval, likely the nan-018 harness.*
- **Whether the stdio legacy path (I-7) is still exercised by real users** — divergence established; exposure depends on deployment share. *Blocked on data not in-repo.*
- **Interaction between I-2 and the ass-088 convergence tick (#870)** — SLN3 (#5538) explicitly defers several compaction residuals to #870; whether that work subsumes I-1/I-2 was not assessed, as SCOPE forbids re-litigating prior art. *Out of scope.*

## Out-of-Scope Discoveries

- **The tick is the sole supply line for two `delivery:proven` capability nodes** (RETAIN #5581, C5 #5550), yet their proofs are conditional on a background loop completing — a condition the capability map does not record. May warrant checking whether other `proven` nodes carry unrecorded liveness conditions.
- **Retain-on-error is applied inconsistently across the five Principle-7 caches.** TypedGraph/PhaseFreq/Effectiveness/Contradiction retain on error; ConfidenceState (I-3) overwrites with degenerate values and PhaseFreq (I-5) overwrites with empty on a *successful* thin rebuild. The absence of a stated convention for "what a cache does when its rebuild fails" looks like the shared root beneath I-3, I-5 and I-9 — but establishing that is root-cause work this spike is barred from.
- **`ExtractionContext` watermark is in-memory only** (`background.rs:1903-1905`, `job.rs:166`); restart replays the `observations` backlog from 0 at 1000 rows/tick. Extraction inserts as `Status::Active` (`:1880`), so whether replay duplicates entries depends entirely on the quality gates — not traced.
- **`ResourceClass` is declared but never read** (`job.rs:204-213`, documented as a forward hook). Inert today.
- **The 120s `TICK_TIMEOUT` is per-op, not per-pass.** With 9 jobs × N slugs, theoretical worst-case pass duration is 9 × 120 × N seconds against a 900s interval. No pass-level budget exists.

## Confidence statement

**Directional**, consistent with the approach and the constraint recorded in SCOPE.md. Every issue is identified by reading code and cited to `file:line`. Urgency ratings are reasoned from code semantics — not validated by reproduction, instrumentation, or measurement. No PoC built, no test run.

- **High confidence** (direct reading, unambiguous): I-1, I-2, I-4, I-6, I-7, I-8, I-10, I-13, I-16, I-17, I-19, I-21, I-22, I-24, and all three BENIGN findings.
- **Medium confidence** (mechanism established, real-world trigger conditions unverified): I-3, I-5, I-9, I-11, I-12, I-14, I-15, I-18, I-20, I-23.
- **Cost figures are shapes, not measurements.** Cap and default-config values are quoted from source; wall-clock estimates (e.g. "100–200 rows/tick" in I-14) are inference from typical latency and flagged as such.

Anything flagged here that warrants action should be re-investigated at `validated` or `empirical` confidence in a follow-up spike.
