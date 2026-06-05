# vnc-025 Implementation Brief — Server-Side Session Transcript Buffer (Stream Wiring + Like-for-Like PreCompact Delivery, F2)

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-025/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-025/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-025/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-025/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-025/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-025/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-025/ACCEPTANCE-MAP.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| transcript-buffer (`TranscriptBuffer`, `TranscriptPurgeRecord`, `session_key`) | pseudocode/transcript-buffer.md | test-plan/transcript-buffer.md |
| transcript-block (shared extraction core, `from_path` + `from_bytes`) | pseudocode/transcript-block.md | test-plan/transcript-block.md |
| registry-wiring (`apply_transcript_delta`, `clear_transcripts_for_feature`, drain/sweep changes, `with_transcript_cap`) | pseudocode/registry-wiring.md | test-plan/registry-wiring.md |
| dispatch-wiring (single arm + batch tee, `handle_compact_payload` prepend) | pseudocode/dispatch-wiring.md | test-plan/dispatch-wiring.md |
| purge-audit (three purge points, `transcript_session_purged` emission) | pseudocode/purge-audit.md | test-plan/purge-audit.md |
| config-knob (`transcript_buffer_max_bytes` + validate + project-wins merge) | pseudocode/config-knob.md | test-plan/config-knob.md |
| cycle-review-purge (tools.rs handler gate + clear call) | pseudocode/cycle-review-purge.md | test-plan/cycle-review-purge.md |

Pseudocode and test-plan files are produced in Session 2 Stage 3a; this map lists expected
components from the architecture — actual paths are confirmed during delivery.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Replace vnc-024's two accept-and-drop guard points with a per-session, in-memory,
never-persisted `TranscriptBuffer` fed by an offset-bounded idempotent merge of
`transcript_delta` events, add its three-point purge lifecycle with content-free audit, and
build the server-side PreCompact transcript-tail block from the buffer — closing the remote
PreCompact-fidelity gap (#4676) with structural parity to today's local Rust hook. Ships dark
(no client streams deltas until F3); distillation is crt-052 (#689).

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| SessionState field shape | `Arc<Mutex<TranscriptBuffer>>` field; registry lock does lookup + Arc clone + activity bump only; memcpy under buffer lock; lock order registry → buffer, never reverse | SR-01 (first decision, mandated) | architecture/ADR-001-transcript-field-shape.md (Unimatrix #4739) |
| Buffer representation | Contiguous span + hole ranges (capped 64, collapse-to-newest at 65th); ring-tail overflow; elision is metadata only (`elided_bytes` + `base_offset`), never spliced bytes; content-opaque manual Debug, no Display; tail-window equivalence under overflow | Resolved decisions 1+2; OQ-2/OQ-3 | architecture/ADR-002-transcript-buffer-representation.md (Unimatrix #4740) |
| Dispatch wiring | Tee deltas to merge before the untouched vnc-024 batch filter; reuse sanitized RecordEvent entry (no parallel path); always-Ack for every delta outcome | SR-07, SR-08 | architecture/ADR-003-dispatch-tee-before-filter.md (Unimatrix #4741) |
| Purge + audit lifecycle | Metadata under lock, audit emitted after release via `log_event_async`, fire-and-forget; purge never depends on audit; `clear_transcripts_for_feature` is the named crt-052 seam (counts-only today, take-shaped later); zero-byte purges emit nothing | SR-03, SR-04 | architecture/ADR-004-purge-audit-lifecycle.md (Unimatrix #4742) |
| PreCompact parity | One shared extraction core `uds/transcript_block.rs` — `extract_transcript_block(path)` (hook re-imports) + `extract_transcript_block_from_bytes` (server); parity structural; golden test, no hand-written expectation | SR-05, A3, OQ-1 | architecture/ADR-005-shared-transcript-block-extraction.md (Unimatrix #4743) |
| Config knob | `RetentionConfig.transcript_buffer_max_bytes`, serde default 4 MiB, `validate()` rejects < 65,536; injected via `SessionRegistry::with_transcript_cap`; `new()` keeps default for tests; project-wins merge arm added | Resolved decision 1 | architecture/ADR-006-buffer-cap-config.md (Unimatrix #4744) |
| Enterprise key seam | `session_key(tenant, project, session_id)` constructor in the new module; OSS returns `session_id` unchanged; only new vnc-025 paths route through it; no call-site re-key | Resolved decision 3 | architecture/ADR-007-session-key-seam.md (Unimatrix #4745) |
| Arithmetic + poison policy | Layer 1: all offset arithmetic `checked_*`/`saturating_*`; overflowing delta dropped whole (no partial clip — do NOT "improve" this); no raw `offset as usize`. Layer 2: no `lock().unwrap()` on buffer mutex — `into_inner()` + `clear()` treat-as-empty recovery at every site; always-Ack preserved; PreCompact degrades to empty-buffer path | R-02, R-06, NFR-09 | architecture/ADR-008-arithmetic-overflow-poison-policy.md (Unimatrix #4746) |

## Files to Create / Modify

All paths under `crates/unimatrix-server/src/` unless noted.

**New**
- `infra/session_transcript.rs` — `TranscriptBuffer` (span + holes + ring-tail + `contiguous_tail` + metadata accessors + manual Debug), `TranscriptPurgeRecord`, `session_key()` seam. ≤500 lines.
- `uds/transcript_block.rs` — extraction core moved verbatim-where-possible from `hook.rs`: `ExchangeTurn`, `build_exchange_pairs`, `format_turn`, constants, `extract_transcript_block(path)`, new `extract_transcript_block_from_bytes`, `prepend_transcript`.

**Modified (thin wiring only — all three host files are over the 500-line cap)**
- `infra/session.rs` — `SessionState.transcript: Arc<Mutex<TranscriptBuffer>>`; new methods `apply_transcript_delta`, `clear_transcripts_for_feature`; signature changes: `drain_and_signal_session -> Option<(SignalOutput, Option<TranscriptPurgeRecord>)>`, `sweep_stale_sessions -> (Vec<SweepResult>, Vec<TranscriptPurgeRecord>)`; new ctor `with_transcript_cap(usize)`.
- `uds/listener.rs` — single-arm drop body → merge call (`:774`); batch-arm tee loop before the untouched filter (`:1009`); `handle_compact_payload` (`:1504`) tail-block build + prepend before token_count; drain/sweep call-site updates (`:1796/:1814`); purge audit emission in `handle_session_close` (thread existing `Arc<AuditLog>` in).
- `uds/hook.rs` — extraction internals removed; call sites (`:220/:252/:295`) re-import from `transcript_block.rs`. Behavior unchanged; existing test suite passes unmodified (R-14: pre/post test-name inventory + constant pins).
- `mcp/tools.rs` — `context_cycle_review` handler (`:1918`): `match transcript_retention { PurgeOnCycleClose => clear_transcripts_for_feature(...) }`, audit via `log_event_async`.
- `infra/config.rs` — `RetentionConfig.transcript_buffer_max_bytes` beside `transcript_retention` (`:1561`); `validate()` floor; project-wins merge arm (`:3376`).
- `server.rs:335`, `main.rs:645/:1068` — switch to `with_transcript_cap(cfg.retention.transcript_buffer_max_bytes)`.
- `http/router.rs` — **unchanged** (`prefix_session_id` already preserves `event_type`; convergence proven by tests, pattern #4725).

## Data Structures

```rust
pub struct TranscriptBuffer {
    base_offset: u64,        // logical offset of data[0]
    data: Vec<u8>,           // spans [base_offset, base_offset + data.len())
    holes: Vec<(u64, u64)>,  // unwritten sub-ranges within span (capped 64)
    high_water: u64,         // max(offset + len) ever seen — monotonic
    elided_bytes: u64,       // ring-tail-dropped + below-base-clipped bytes
    max_bytes: usize,        // cap, injected at construction
}

pub struct TranscriptPurgeRecord { pub session_id: String, pub bytes_purged: u64 }
```

Wire (frozen, consume-only): `TranscriptDeltaPayload { offset: u64, bytes: String }`
(`wire.rs:284`); `TRANSCRIPT_DELTA_EVENT = "transcript_delta"` (`wire.rs:46`).

Audit event shape: `operation: "transcript_session_purged"`, `agent_id: "server"`,
`detail: "bytes=<n> trigger=<session_close|stale_sweep|cycle_review>"`, `outcome: Success`,
`target_ids: []` — never content (mirrors `uds_auth_failure`, `listener.rs:409`).

## Function Signatures

```rust
// infra/session_transcript.rs
impl TranscriptBuffer {
    fn apply_delta(&mut self, offset: u64, bytes: &[u8]);              // no return, never panics
    fn contiguous_tail(&self, window: usize) -> Option<Vec<u8>>;       // never crosses a hole
    fn len(&self) -> usize;  fn high_water(&self) -> u64;
    fn elided_bytes(&self) -> u64;  fn clear(&mut self) -> u64;        // returns bytes purged
}
pub fn session_key(tenant: &str, project: &str, session_id: &str) -> String; // OSS: id unchanged

// infra/session.rs
impl SessionRegistry {
    fn with_transcript_cap(max_bytes: usize) -> Self;                  // new() = 4 MiB default
    fn apply_transcript_delta(&self, session_id: &str, offset: u64, bytes: &[u8]); // silent no-op unregistered
    fn clear_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptPurgeRecord>;
    fn drain_and_signal_session(...) -> Option<(SignalOutput, Option<TranscriptPurgeRecord>)>; // SignalOutput shape untouched
    fn sweep_stale_sessions(&self) -> (Vec<SweepResult>, Vec<TranscriptPurgeRecord>); // incl. silently-evicted
}

// uds/transcript_block.rs
pub fn extract_transcript_block(path: &str) -> Option<String>;          // moved, unchanged
pub fn extract_transcript_block_from_bytes(bytes: &[u8]) -> Option<String>; // new
pub fn prepend_transcript(transcript: Option<&str>, briefing: &str) -> String; // moved, unchanged
pub const MAX_PRECOMPACT_BYTES: usize = 3000;
pub const TAIL_MULTIPLIER: usize = 4;                                   // window = 12,000
```

## Constraints (binding)

1. **Secrets posture is architectural** (#4721): no redactor exists; in-memory + purge IS the guarantee. Any persist/spill/log of raw transcript = review rejection (NFR-01, AC-12 hard gate).
2. **Hot-path clone cost**: transcript never rides wholesale `SessionState` clones — Arc handle is structural proof (AC-10).
3. **Registry mutex discipline**: bounded in-memory work only under lock; no I/O/await; memcpy happens under the per-session buffer lock, never the registry lock.
4. **Fire-and-forget**: every delta outcome returns `Ack` — malformed, unregistered, over-cap, even poisoned-mutex recovery. Never `Error`.
5. **Batch filter is load-bearing**: the filter line at `listener.rs:1009` is not edited, moved, or simplified — tee before it. vnc-024's zero-rows test runs **unmodified**.
6. **500-line rule**: new logic in new focused modules; over-limit files get thin wiring only.
7. **Exhaustive `TranscriptRetention` match** — enterprise seam, never an assumed variant.
8. **Ships dark**: all verification test-driven; empty-buffer PreCompact is the only live-before-F3 behavior.
9. **1 MiB frame ceiling** is the per-delta bound; do not trust the client's 64 KiB soft cap.
10. **Wire frozen**: no changes to payload, event-type string, or ts-rs bindings.
11. **No global memory cap** (human-accepted): per-session cap × N; 4 h sweep is the backstop. Evidence trigger to revisit: >32 concurrent sessions or >256 MiB resident transcript memory.
12. **Never-panics contract (NFR-09 / ADR-008)**: no wire-reachable input panics inside `TranscriptBuffer`; drop-whole on overflow is deliberate — do not partial-clip; no bare `unwrap()` on the buffer mutex.

## Dependencies

- **Crates**: none new (AC-13). std + existing `tokio`/`tracing` only.
- **vnc-024 (shipped, `70b3aeb7`)**: wire surface, drop arms being replaced, `transcript_retention` config (first consumer), zero-rows test.
- **Existing components**: `SessionRegistry`/`SessionState`, UDS dispatch, HTTP `/observe` router, `AuditLog`/`AuditEvent` + `gc_audit_log`, `context_cycle_review` handler, `hook.rs` extraction internals (moving).
- **Forward consumers (out of scope)**: crt-052 (#689) reads via `clear_transcripts_for_feature`; F3 TS client produces deltas. Delta-content contract for F3: raw JSONL transcript-file bytes, file byte offsets.

## NOT in Scope

- Distillation / reconstruction fallback (crt-052 #689) — nothing reads the buffer except PreCompact.
- TS client / delta production — F3.
- Enterprise acknowledged-delivery audit path (ass-069 Q7).
- Honoring `RetainDays`; any disk spill, crash recovery, or transcript persistence.
- Changes to the 23 detection rules or cycle-review output.
- Interpretive transcript parsing (the mechanical JSONL→exchange-turn formatting in the shared core is required by parity and is NOT this exclusion — ADR-005).
- Activating `CompactPayload.transcript_excerpt` (stays ignored legacy).
- Registry re-key to a composite key type (seam only).
- Attribution-heuristic retirement; OAuth multi-tenant prefixing; global memory cap.

## Key Test Obligations (from RISK-TEST-STRATEGY.md)

- **R-01 (Critical)**: `apply_delta` permutation/hole-surgery harness — densest test surface; derive expectations programmatically (#2984), reuse ass-069 PoC fixtures.
- **Hard gates**: vnc-024 zero-rows test unmodified (R-04); sentinel-based content-leak tests + static grep gate, both not either (R-05); golden parity test `from_path` vs streamed-shuffled `from_bytes` + empty-buffer byte-identity (R-09); fuzz-ish (offset, len) no-panic test incl. near-`u64::MAX` + explicit poisoned-mutex test (R-02/R-06, NFR-09).
- **Named mandatory case**: silently-evicted session (empty `injection_history`, non-empty buffer) still gets a `TranscriptPurgeRecord` + audit row (R-08).
- **hook.rs move**: pre/post test-name inventory, constants pinned (`3000`/`4`) (R-14).

## Alignment Status

From ALIGNMENT-REPORT.md (2026-06-05): 4 PASS, 2 WARN, 1 VARIANCE, 0 FAIL.

- **VARIANCE 1 (human-accepted 2026-06-05)**: SCOPE AC-02 promised full-content convergence regardless of arrival order; spec FR-02/AC-02 weaken this **under overflow** to tail-window equivalence (full-content equality below the cap; below-floor deltas are defined no-ops once ring-tail advances). Derived from human-approved scope decisions 1+2 (ring-tail; no covered-range replay buffering); the only vnc-025 reader (12 KB PreCompact tail) is fully served. R-03 tests pin the guarantee. crt-052 inherits these semantics — confirm before implementation proceeds on AC-02.
- **WARN 1 (remediated)**: RISK-TEST-STRATEGY was stale relative to ADR-008 — refreshed; it now cites ADR-001..008 and states the poison policy is pinned.
- **WARN 2 (remediated)**: the no-panic/poison-recovery contract was ADR-only — spec NFR-09 added carrying it as a requirement.
- Justified scope additions (risk-covered, no action): hook.rs extraction-core move (ADR-005, R-14), ADR-008 hardening (R-02/R-06), config validate floor + ctor.

## Tracking

GitHub Issue: https://github.com/dug-21/unimatrix/issues/670
