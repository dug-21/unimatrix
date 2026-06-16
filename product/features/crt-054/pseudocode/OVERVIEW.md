# crt-054 Pseudocode Overview — Transcript-Fold Producer

**Feature**: crt-054 (#752) — producer half. Two raw inputs for crt-055 (#755): Surface A (durable `compaction_events` table) + Surface B (in-memory `activity_snapshot()` fold) + the `[transcript_signals]` config that feeds B.
**Binding contract**: `product/features/crt-055/SCOPE.md` §"Producer contract" — authoritative for every field. On conflict, the contract wins.
**ADRs honored**: ADR-001..ADR-010 (architecture/ADR-*.md).

This overview is the integration spine: shared types, the data flow across component boundaries, and the build/sequencing constraints. Per-component bodies live in the sibling files.

---

## Components (10) and their files

| # | Component | File | Crate / file (confirmed) |
|---|-----------|------|--------------------------|
| 1 | `ActivityCounters` (fold accumulator) | `activity-counters.md` | `unimatrix-server/src/infra/transcript_activity.rs` (new) |
| 2 | `transcript_activity` module + `SignatureScanner` | `transcript-activity.md` | `unimatrix-server/src/infra/transcript_activity.rs` (new) |
| 3 | `apply_delta` fold call (both routes) | `apply-delta-fold.md` | `unimatrix-server/src/infra/session_transcript.rs:150` (modify) |
| 4 | `activity_snapshot()` + `ActivitySnapshot` read surface | `activity-snapshot.md` | `unimatrix-server/src/infra/session_transcript.rs` (modify) + `transcript_activity.rs` |
| 5 | activity collector (`activity_snapshots_for_feature`) | `activity-collector.md` | `unimatrix-server/src/infra/session.rs` (modify, mirrors `take_transcripts_for_feature` @ :469) |
| 6 | `compaction_events` writer (at `handle_compact_payload`) | `compaction-events-writer.md` | `unimatrix-server/src/uds/listener.rs:1854` (modify) |
| 7 | `compaction_events` table + migration | `compaction-events-migration.md` | `unimatrix-store/src/migration.rs` (:22, run_main_migrations) + `db.rs:534` (modify) |
| 8 | compaction-INSERT helper + failure counter | `compaction-insert-helper.md` | `unimatrix-store/src/write_ext.rs` (or `write.rs`) + `services/store_ops.rs` thin wrapper (new) |
| 9 | `[transcript_signals]` config + `validate()` | `transcript-signals-config.md` | `unimatrix-server/src/infra/config.rs` (modify) |
| 10 | Wave B startup precondition assert | `wave-b-precondition.md` | `unimatrix-server/src/main.rs` (~:698 daemon, ~:1234 mirror) (modify) |

---

## Shared types (single source — used verbatim by component files)

```
// PINNED shared constant. MUST equal crt-055's constant exactly — it crosses the
// boundary via ActivitySnapshot.class_counts. v1 indices: 0 = error, 1 = refusal.
const MAX_SIGNAL_CLASSES: usize = 16            // EXACTLY 16, not "<= 16" (NFR-6)

// In-memory fold accumulator. Scalars only — Copy-safe; never a content field.
struct ActivityCounters {
    bytes_total:  u64                            // monotonic sum of delta payload lengths
    delta_count:  u32                            // +1 per delta merged
    class_counts: [u32; MAX_SIGNAL_CLASSES]      // per-class match counts (config order)
}

// Counters-only read surface returned by activity_snapshot().
// No Vec<u8>/String/&[u8]; NO Display; metadata-only (manual) Debug.
#[derive(Clone, Copy)]
struct ActivitySnapshot {
    bytes_total:  u64
    delta_count:  u32
    class_counts: [u32; MAX_SIGNAL_CLASSES]
}

// compaction_events table (Surface A) — content-free, insert-only:
//   id           INTEGER PRIMARY KEY
//   session_id   TEXT    NOT NULL
//   compacted_at INTEGER NOT NULL              -- Unix SECONDS (DDL comment documents the unit)
//   high_water   INTEGER NOT NULL DEFAULT 0
//   INDEX idx_compaction_events_session ON compaction_events(session_id)

// [transcript_signals] config entry — sibling to [retention], #[serde(default)]:
struct TranscriptSignal {
    class_name: String
    pattern:    String
    enabled:    bool
}

// Durable failure counter (existing `counters` table; helpers in unimatrix-store/src/counters.rs):
const COMPACTION_EVENTS_INSERT_FAILED: &str = "compaction_events_insert_failed"
```

Width contract (crt-055 §"Surface B"): producer is **cast-free** — `bytes_total: u64`, `delta_count: u32`, `class_counts: [u32; N]` are emitted at native widths. The checked/saturating `→ i64` conversion is crt-055's, at persist. crt-054 performs NO `as i64`/narrowing on these (NFR-5, AC-14).

---

## Data flow across boundaries

### Surface B — in-memory fold (ingest → review)

```
delta bytes
   │  (apply_delta_to_session, session.rs)
   ▼
Phase1 registry lock resolves Arc<Mutex<TranscriptBuffer>>:
   registered route  (session.rs:400-401)   ─┐
   held route        (session.rs:388-395 via held_arc_for_session) ─┤  same Arc → same buffer
   ▼                                          ┘
Phase2 buffer lock: buf.apply_delta(offset, bytes)   (session_transcript.rs:150)
   └─ after merge: self.activity.fold(bytes, &self.scanner)   [Component 3]
        bytes_total += bytes.len(); delta_count += 1;
        scanner.scan(bytes) → for each matched class i: class_counts[i] += 1   [Components 1,2]
   ▼ (buffer + accumulator ride the crt-052 Wave B hold across drains — never zeroed by crt-054)
crt-055 at review:
   activity_snapshots_for_feature(feature_cycle)   [Component 5, mirrors take_transcripts_for_feature]
     → per session: buf.activity_snapshot() → ActivitySnapshot   [Component 4]
   (read BEFORE purge_cycle_transcripts; crt-055 sums and lands columns)
```

The accumulator lives **inside** `TranscriptBuffer`, so registered AND held routes fold the *same* accumulator by construction (ADR-001) — no per-route wiring; this is the believable-zero guard's structural basis.

### Surface A — durable compaction event (handler → review)

```
handle_compact_payload (listener.rs:1737)
   ... briefing build, tail read under buffer lock (guard dropped at :1835) ...
   session_registry.increment_compaction(session_id)   (listener.rs:1854, existing)
   ▼  CO-LOCATED immediately after, NO registry/session/buffer lock held across it:   [Component 6]
   high_water = { lock buffer via shared Arc; read buf.high_water(); DROP guard }
   compacted_at_secs = now_secs()   (server wall clock, .as_secs())
   services.store_ops.insert_compaction_event(session_id, compacted_at_secs, high_water)   [Component 8]
     ├─ Ok  → durable row { session_id, compacted_at (SECONDS), high_water }
     └─ Err → increment durable counter "compaction_events_insert_failed",
              log ids/counts only (no content); compaction ACK proceeds (non-blocking)
   ▼
crt-055 at review: SELECT compaction_events BY session_id; gate PostToolUse read ts (÷1000) vs compacted_at
```

### Config → scanner (startup)

```
[transcript_signals] config (config.rs)   [Component 9]
   → validate() at load: reject > MAX_SIGNAL_CLASSES enabled, invalid regex, duplicate class_name (LOUD)
   → enabled entries in config order → SignatureScanner::compile → one shared RegexSet   [Component 2]
   → scanner threaded into every TranscriptBuffer::new construction site   [Components 2,3]
```

### Startup precondition (Wave B)

```
main.rs (~:698 daemon, ~:1234 mirror)   [Component 10]
   after with_transcript_hold(...): assert the HeldBufferScan handle is wired;
   fail LOUD at startup if absent (Surface B survival depends on the hold — ADR-010).
```

---

## Sequencing / build constraints (for Stage 3b waves)

1. **Components 1, 2 (`transcript_activity.rs`) first** — they define `ActivityCounters`, `ActivitySnapshot`, `MAX_SIGNAL_CLASSES`, `SignatureScanner`. Everything else depends on these types.
2. **Component 9 (config) before 2's wiring** — the scanner is compiled from validated config; construction sites need a scanner to pass in.
3. **Component 3 (`apply_delta` fold) + 4 (`activity_snapshot()`) depend on 1/2** — they embed the accumulator + scanner into `TranscriptBuffer` and add the read surface. These two co-edit `session_transcript.rs`; land together to keep the file coherent and under the 500-line cap (the buffer module is already near the cap → the *logic* lives in `transcript_activity.rs`; `session_transcript.rs` gains only fields + thin call/accessor).
4. **Component 5 (collector) depends on 4** — needs `activity_snapshot()` + `ActivitySnapshot`.
5. **Component 7 (table+migration) and 8 (helper) are the Surface A spine** — 7 before 6 (the writer needs the table); 8 before 6 (the writer calls the helper). 7 and 8 are independent of Surface B.
6. **Component 6 (writer) last on Surface A** — depends on 7 + 8.
7. **Component 10 (Wave B assert) independent** — touches only `main.rs`; can land any time, but its test depends on the hold wiring already present (it is, from crt-052).

**Wave A (parallel-safe):** {1,2}, {9}, {7,8}. **Wave B:** {3,4} (after 1,2,9), {6} (after 7,8). **Wave C:** {5} (after 4), {10}.

---

## Cross-cutting invariants (every component file restates the ones it touches)

- **Bytes-only** — never tokens, never cost; no `token_*` symbol anywhere (NFR-2, AC-15).
- **Content-opaque** — `ActivitySnapshot` has no byte-bearing field, metadata-only `Debug`, no `Display` (NFR-1, AC-08). Same posture as the existing `TranscriptSnapshot` Debug (`session_transcript.rs:112`).
- **`MAX_SIGNAL_CLASSES == 16` exactly** — pinned, equals crt-055's (NFR-6, AC-11).
- **Fold on BOTH routes via the same embedded accumulator** (ADR-001, FR-B2, AC-06) — no route-specific fold call.
- **Surface A INSERT holds only the DB connection** — no registry/session/buffer lock across it; `high_water` captured then guard dropped (ADR-007, NFR-4, AC-04, pattern #3753).
- **`compacted_at` in Unix SECONDS** (ADR-007, AC-01a) — server wall clock; the `ts/1000` gate normalization is crt-055's.
- **Cast-free producer widths** `u64`/`u32` (NFR-5, AC-14).
- **Named failure counter** `compaction_events_insert_failed` on INSERT failure — not a generic log (ADR-007 §6, R-15, AC-04a).
- **Next `CURRENT_SCHEMA_VERSION` bump (28 → 29/30 by merge order) for `compaction_events` ONLY** — do NOT touch `cycle_review_index` or `SUMMARY_SCHEMA_VERSION` (ADR-008, NFR-8, AC-15).
- **Survival to review** — crt-054 never zeroes/drops the fold before the crt-052 purge (ADR-006, FR-B9, AC-07).
- **Never fabricate a zero** — undeclared/purged sessions contribute no entry; Surface A row is written regardless of declaration (ADR-004, FR-B10/FR-A5, AC-03/AC-12).
