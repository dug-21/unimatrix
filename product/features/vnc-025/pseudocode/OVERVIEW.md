# vnc-025 Pseudocode Overview

Source of truth: ARCHITECTURE.md + ADR-001..008, SPECIFICATION.md (FR-01..21, NFR-01..09),
RISK-TEST-STRATEGY.md (R-01..R-15). All code line references verified against main 2026-06-05.

## Components

| Component | File | New/Modified | Pseudocode |
|-----------|------|--------------|-----------|
| transcript-buffer | `crates/unimatrix-server/src/infra/session_transcript.rs` | New (≤500 lines) | transcript-buffer.md |
| transcript-block | `crates/unimatrix-server/src/uds/transcript_block.rs` | New (moved from hook.rs) | transcript-block.md |
| registry-wiring | `crates/unimatrix-server/src/infra/session.rs` | Modified (thin wiring) | registry-wiring.md |
| dispatch-wiring | `crates/unimatrix-server/src/uds/listener.rs` | Modified (thin wiring) | dispatch-wiring.md |
| purge-audit | `crates/unimatrix-server/src/uds/listener.rs` (+ helper) | Modified | purge-audit.md |
| config-knob | `crates/unimatrix-server/src/infra/config.rs`, `server.rs`, `main.rs` | Modified | config-knob.md |
| cycle-review-purge | `crates/unimatrix-server/src/mcp/tools.rs`, `server.rs`, `main.rs` | Modified | cycle-review-purge.md |

## Shared Types (defined once, used across components)

```rust
// infra/session_transcript.rs (transcript-buffer component)
pub struct TranscriptBuffer {
    base_offset: u64,        // logical offset of data[0]
    data: Vec<u8>,           // spans [base_offset, base_offset + data.len()); always ≤ max_bytes
    holes: Vec<(u64, u64)>,  // disjoint, sorted, unwritten sub-ranges within span; ≤ 64 entries
    high_water: u64,         // max(offset + len) ever seen — monotonic
    elided_bytes: u64,       // ring-tail-dropped + below-base-clipped bytes (metadata, never content)
    max_bytes: usize,        // cap, injected at construction (ADR-006)
}

pub struct TranscriptPurgeRecord { pub session_id: String, pub bytes_purged: u64 }

pub fn session_key(tenant: &str, project: &str, session_id: &str) -> String; // OSS: id unchanged (ADR-007)
```

Wire (frozen, consume-only — NFR-08): `TranscriptDeltaPayload { offset: u64, bytes: String }`
(`unimatrix-engine/src/wire.rs:284`), `TRANSCRIPT_DELTA_EVENT = "transcript_delta"` (`wire.rs:46`).

Audit event (pinned shape, ADR-004): `operation: "transcript_session_purged"`,
`agent_id: "server"`, `session_id: <purged id>`, `outcome: Outcome::Success`, `target_ids: []`,
`detail: "bytes=<n> trigger=<session_close|stale_sweep|cycle_review>"` — never content.

## Data Flow

```
delta (UDS single/batch, HTTP /observe via prefix_session_id — router.rs UNCHANGED)
  → dispatch-wiring (tee/route inside existing sanitized arms; always Ack)
  → registry-wiring: SessionRegistry::apply_transcript_delta
       registry lock: lookup + Arc clone + last_activity_at bump only
       buffer lock:   TranscriptBuffer::apply_delta (memcpy here, never under registry lock)
  → transcript-buffer (span + holes + ring-tail; checked arithmetic; poison → treat-as-empty)

PreCompact read (the ONLY reader):
  handle_compact_payload step-2 snapshot (already held) → transcript Arc
  → contiguous_tail(MAX_PRECOMPACT_BYTES * TAIL_MULTIPLIER = 12,000)
  → transcript-block: extract_transcript_block_from_bytes → prepend_transcript
  → token_count computed AFTER prepend; empty/None → byte-identical to pre-vnc-025

Purge (three points, all clear() the buffer and emit content-free audit after lock release):
  drain_and_signal_session → Option<(SignalOutput, Option<TranscriptPurgeRecord>)>  [session_close]
  sweep_stale_sessions     → (Vec<SweepResult>, Vec<TranscriptPurgeRecord>)         [stale_sweep]
  clear_transcripts_for_feature(feature_cycle) → Vec<TranscriptPurgeRecord>         [cycle_review]
```

## Lock Discipline (binding, ADR-001)

- Order: registry lock → buffer lock. NEVER acquire registry lock while holding a buffer lock.
- Registry lock: lookup + `Arc::clone` + scalar bump only. The ≤1 MiB worst-case memcpy happens
  under the per-session buffer lock only.
- Every buffer-mutex lock site uses poison recovery: on `Err(poisoned)` → `into_inner()` +
  `clear()` the buffer before proceeding (treat-as-empty, ADR-008 Layer 2). No bare
  `lock().unwrap()` on the buffer mutex anywhere.

## Sequencing Constraints (build order)

1. **transcript-buffer** + **transcript-block** — pure new modules, no dependents, parallelizable.
2. **config-knob** — config field + validate + merge arm (independent of 1).
3. **registry-wiring** — depends on transcript-buffer types; changes drain/sweep signatures.
4. **dispatch-wiring** + **purge-audit** — depend on 3 (call sites of changed signatures) and on
   transcript-block (PreCompact). purge-audit and dispatch-wiring touch the same file
   (`listener.rs`); implement together or sequentially.
5. **cycle-review-purge** — depends on 3 (`clear_transcripts_for_feature`) and the
   `retention_config` field added per config-knob/cycle-review-purge.

## Cross-Cutting Constraints (apply to every component)

- **Secrets posture (NFR-01, AC-12 hard gate)**: no persist/spill/log of raw transcript bytes.
  No `tracing` calls in the two new modules touch content. No `Display`; manual metadata-only
  `Debug`.
- **Always-Ack (ADR-003)**: every delta outcome — merged, unregistered, malformed, over-cap,
  poison-recovered — returns `HookResponse::Ack`.
- **Never-panics (NFR-09, ADR-008)**: all offset arithmetic `checked_*`/`saturating_*`;
  overflow → whole delta silently dropped (do NOT partial-clip); no raw `offset as usize` —
  u64→usize only on span-relative values proven ≤ `max_bytes`.
- **Batch filter untouched (Constraint 5)**: the filter line at `listener.rs:1009` is not
  edited, moved, or simplified. vnc-024 zero-rows test runs unmodified.
- **Wire frozen (NFR-08)**: no changes to payload struct, event-type string, or ts-rs bindings.
- **500-line rule**: new logic in the two new modules; host files get thin wiring only.
- **New vnc-025 key paths route through `session_key("default", "", id)`** (ADR-007); existing
  call sites untouched.

## Open Questions / Gaps (flagged, not blocking)

1. **Mid-session re-registration wipes the transcript.** `register_session` overwrites
   `SessionState` and is also called on `cycle_start` via `handle_cycle_event` for
   already-live sessions. With the new field, that overwrite replaces the buffer Arc with a
   fresh empty one — a silent, unaudited purge. The architecture pins fresh-empty after
   *drain*; live re-registration is unaddressed. Ships dark (no client streams until F3), so
   pseudocode keeps the simple overwrite — implementer/tester should confirm with the SM
   whether this is acceptable as an F3 contract note or whether `register_session` should
   preserve an existing transcript Arc on overwrite.
2. **Purge on review error paths.** Spec W4 says the purge happens when "context_cycle_review
   runs"; pseudocode pins purge only after a *successful* review (error paths keep
   transcripts for retry). Confirm at Gate 3a if purge-on-error was intended.
3. **`server.rs:335` construction site.** The brief lists it among the three
   `with_transcript_cap` switch sites, but it is the test-server constructor (production
   daemon/stdio paths construct in `main.rs:645/:1068` and overwrite config fields per the
   #561 pattern). config-knob.md follows the brief with a verify-in-review note.
4. **`retention_config` field on `UnimatrixServer` is a new wiring requirement** (not named in
   the architecture's file list): the cycle-review handler needs `transcript_retention` at
   runtime; specified in cycle-review-purge.md following the `store_config` precedent (#561).
5. **Pseudocode-level pins within ADR latitude** (tester must pin via tests; crt-052
   inherits): `clear()` sets `base_offset = high_water` (clean resumed-stream semantics,
   R-10.2); `bytes_purged` = span length (`len()`, per ADR-004's "reading buffer.len()");
   `elided_bytes` unchanged by `clear()`; zero-length deltas update only `high_water`;
   `contiguous_tail(0)` → `None`; ring-tail/collapse elision counts received (non-hole) bytes
   while below-floor clips count per-delta clipped lengths (R-03.4 no-double-count rule).
