# Agent Report — crt-054-agent-1-pseudocode (Stage 3a)

**Role**: Pseudocode specialist. Produced per-component pseudocode for crt-054 (producer-only).
**Status**: COMPLETE. 11 files (OVERVIEW + 10 components).

## Deliverables

All under `product/features/crt-054/pseudocode/`:
- `OVERVIEW.md` — shared types, cross-boundary data flow (Surface A/B + config + startup), build sequencing/waves, cross-cutting invariants.
- `activity-counters.md` (Component 1)
- `transcript-activity.md` (Component 2 — module + `SignatureScanner`)
- `apply-delta-fold.md` (Component 3 — fold call + buffer wiring, both routes)
- `activity-snapshot.md` (Component 4 — read surface + `ActivitySnapshot`)
- `activity-collector.md` (Component 5 — `activity_snapshots_for_feature`)
- `compaction-events-writer.md` (Component 6 — Surface A INSERT at `listener.rs:1854`)
- `compaction-events-migration.md` (Component 7 — table + 3-path migration)
- `compaction-insert-helper.md` (Component 8 — INSERT helper + named failure counter)
- `transcript-signals-config.md` (Component 9 — `[transcript_signals]` + `validate()`)
- `wave-b-precondition.md` (Component 10 — startup assert, both paths)

## Codebase verification (paths/seams confirmed, not assumed)

- `migration.rs:22` `CURRENT_SCHEMA_VERSION = 28`; v28 block at `:1384` carries the exact "if a v29 block lands, add `UPDATE counters SET value = 28`" note (`:1399-1404`) — folded into Component 7.
- `session_transcript.rs`: `apply_delta` `:150`, `high_water` field `:53`/accessor `:333`, `clear()` `:318`, existing `TranscriptSnapshot` manual metadata-only `Debug` `:112` (mirrored for `ActivitySnapshot`), `new` `:136`.
- `session.rs`: `take_transcripts_for_feature` `:469` (full body read — Component 5 mirrors it verbatim with one substitution), held-route branch `:388-401`, `increment_compaction` `:554`, `now_secs()` `:1176` (`.as_secs()` → seconds source for `compacted_at`).
- `listener.rs:1854` is exactly `session_registry.increment_compaction(session_id);`; handler is async; `session_state` Arc already held → no registry re-lookup for `high_water`.
- `store_ops.rs:52` `StoreService`; `services/mod.rs:239` `store_ops`. `counters.rs` provides durable `increment_counter` + `counters` table → reused for `compaction_events_insert_failed` (durable so crt-055 can read it for row-vs-increment drift; an in-process atomic would not survive restart).
- `db.rs:534` `create_tables_if_needed` (fresh-create path); `config.rs:71` `UnimatrixConfig`, `retention` `:87` (sibling site for `[transcript_signals]`); `main.rs:698/1234` `with_transcript_hold` (both Wave B paths).
- Scanner uses `regex::bytes::RegexSet` (deltas are arbitrary bytes, FR-B3 counts bytes — flagged in Component 2).

## Invariants honored

bytes-only (no `token_*`); content-opaque `ActivitySnapshot` (no byte field, metadata-only `Debug`, no `Display`, no `saw_compaction`/`reload` latch); `MAX_SIGNAL_CLASSES == 16` exactly; fold on BOTH routes via the embedded accumulator (no route-specific call); Surface A INSERT holds only the DB connection (`high_water` captured then guard dropped before INSERT); `compacted_at` Unix SECONDS with DDL comment; cast-free producer widths `u64`/`u32`; named counter `compaction_events_insert_failed`; next `CURRENT_SCHEMA_VERSION` bump for `compaction_events` ONLY (no `cycle_review_index`, no `SUMMARY_SCHEMA_VERSION`); survival-to-review (no zero/drop, `clear()` preserves accumulator); never fabricate a zero (undeclared sessions excluded by the collector's feature filter).

## Open questions / gaps flagged

1. **Schema version N = 29 vs 30** — left as `N` placeholder (Component 7). SM merge-order coordination point (#4095); not a design decision. Implementer resolves at the gate via `grep CURRENT_SCHEMA_VERSION migration.rs`.
2. **Default `error`/`refusal` regex literals** — Component 9 pins the SHAPE (two classes, indices 0/1, anchored `regex::bytes`); the literal patterns are deferred to delivery-time calibration (AC-10a, coordination item 3). Pseudocode uses `<calibrated ... pattern>` placeholders intentionally — this is the design-sanctioned deferral, not an unfilled gap.
3. **Scanner carrier** — the `Arc<SignatureScanner>` must be threaded into every `TranscriptBuffer::new` site (registered path, `transcript_hold.rs`, test constructors). The exact carrier (on `SessionRegistry` and/or `TranscriptHold`) is an implementation detail; Component 3 states the constraint (one shared scanner, threaded into every `new`) without over-specifying the plumbing. Implementer confirms all call sites by grep.
4. **`high_water as i64` at the Surface A INSERT** (Component 6) — this is the persist-boundary cast for a bounded value, distinct from the Surface-B cast-free rule (AC-14 applies only to `activity_snapshot()`'s counters). Noted explicitly so an AC-14 grep reviewer does not false-positive on it.
5. **AC-16 cross-gate seconds-boundary test ownership** — co-owned with crt-055 (coordination item 1); the SM assigns physical test ownership at the producer/consumer test-plan handoff. Not a pseudocode gap.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing`/`context_search` — the Unimatrix MCP tools were deferred and not loadable in this agent thread (`ToolSearch` returned no matching deferred tools for them); per the spec this is non-blocking, so I proceeded from the three sacred source documents + the 10 ADR references in the brief + crt-055 §"Producer contract" + direct codebase verification. No results retrieved; no entries stored (read-only tier).
- Deviations from established patterns: none. Pseudocode follows existing conventions — `take_transcripts_for_feature` two-phase lock (Component 5), `TranscriptSnapshot` metadata-only `Debug` (Component 4), `counters.rs` durable named counter (Component 8), three-path migration #4153 / `IF NOT EXISTS` idempotency #4092 (Component 7), pattern #3753 capture-then-drop-guard (Component 6).
