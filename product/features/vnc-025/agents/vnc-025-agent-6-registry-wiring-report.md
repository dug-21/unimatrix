# Agent Report: vnc-025-agent-6-registry-wiring (Wave 2 — registry-wiring)

## Files Modified

- `crates/unimatrix-server/src/infra/session.rs` — thin wiring + tests:
  - `SessionState.transcript: Arc<Mutex<TranscriptBuffer>>` (ADR-001; `derive(Clone, Debug)` unchanged)
  - `SessionRegistry.transcript_cap: usize`; `with_transcript_cap(max_bytes)` ctor; `new()` delegates with the 4 MiB default (ADR-006)
  - `register_session` allocates a fresh buffer per registration (re-registration replaces the Arc — no ghost content)
  - `apply_transcript_delta(&self, session_id, offset, bytes)` — silent no-op for unregistered sessions; registry lock = lookup + Arc clone + activity bump only; memcpy under buffer lock (ADR-001/NFR-03); routed through `session_key("default", "", ...)` (ADR-007)
  - `clear_transcripts_for_feature(&self, feature_cycle) -> Vec<TranscriptPurgeRecord>` — Arcs cloned under registry lock, cleared after release; sessions stay registered (crt-052 seam, ADR-004)
  - `drain_and_signal_session -> Option<(SignalOutput, Option<TranscriptPurgeRecord>)>` — SignalOutput shape untouched (Wave 0 baselines `signal_output_drain.txt` / `signal_record_wire.json` pass unmodified)
  - `sweep_stale_sessions -> (Vec<SweepResult>, Vec<TranscriptPurgeRecord>)` — purge record for every evicted session including silently-evicted (R-08.1)
  - `lock_buffer` (pub(crate)) + `purge_record_for` helpers — ADR-008 Layer 2 `into_inner()` + `clear()` at every buffer lock site; no bare unwrap on the buffer mutex
- `crates/unimatrix-server/src/uds/listener.rs` — mechanical signature adaptations only (`:1796` sweep tuple destructure ignoring purges; `:1814` drain `.map(|(output, _)| output)`). Audit emission deliberately NOT added — Wave 3 (purge-audit) owns it.
- `crates/unimatrix-server/src/services/status.rs` — `:1583` sweep tuple destructure, purges ignored (same Wave 3 note).
- `crates/unimatrix-server/src/main.rs` — both ctor sites (drifted to `:645`→`:646` region and `:1068`→`:1074` region) switched to `with_transcript_cap(config.retention.transcript_buffer_max_bytes)`. `server.rs:335` test ctor NOT switched (Gate 3a W1).
- `crates/unimatrix-server/src/services/index_briefing.rs` — test helper `make_session_state` gained the required `transcript` field (compile-only fix).

## Tests

- New tests per test-plan/registry-wiring.md §1–§5 in the `session.rs` test module, including both MANDATORY cases:
  - `test_sweep_silently_evicted_session_yields_purge_record` (R-08.1)
  - `test_poisoned_buffer_mutex_recovery` (R-06.2 — merge/read/drain/sweep lock-site classes)
  - plus delta merge, unregistered no-op, drain shapes, sweep purge records, zero-byte suppression, concurrency no-deadlock, clear-under-stream, clear matrix (R-10.1), orphaned-Arc merge (R-06.4), sweep×cycle-review race, Arc::ptr_eq clone-cost guard (AC-10), ctor cap propagation + 4 MiB default.
- `infra::session` module: 143 passed, 0 failed.
- `unimatrix-server --lib` full suite: 3570 passed, 0 failed.
- Workspace suite: one failure of pre-existing flake `uds::listener::tests::col018_topic_signal_from_file_path` under full parallel load (fixed 50 ms sleep waiting on a spawned observation write); passes in isolation and on lib-suite re-run, with and without these changes. Unrelated to this component.
- `cargo build --workspace` clean; `cargo fmt` applied; clippy warnings in touched files are pre-existing crate-wide lint noise from a newer clippy (`new_without_default`, `collapsible_if` on pre-vnc-025 code); my additions are clean.

## Deviations from Pseudocode (flagged, not silent)

1. **`Mutex::clear_poison()` added in both poison-recovery arms** (`lock_buffer`, `purge_record_for`). The pseudocode's recovery (`into_inner()` + `clear()`) alone re-triggers on EVERY later lock because std poisoning is permanent — each new delta would be silently purged and reads would always see empty, violating the test plan's pinned "subsequent deltas accumulate" behavior (R-06.2). `clear_poison()` makes the treat-as-empty recovery one-shot, which is the ADR-008 intent. The R-06.2 test failed without it and passes with it. dispatch-wiring (Wave 3) should reuse `pub(crate) lock_buffer` rather than replicating, or replicate including clear_poison.
2. `test_apply_transcript_delta_no_memcpy_under_registry_lock` (NFR-03) is a structural review gate per the test plan — recorded as a code comment at the gate location, no runtime assertion (none is practical).

## Issues / Notes for Delivery Leader

- Running `cargo test --workspace` regenerates `crates/unimatrix-engine/bindings/fixtures/request_hookinput_flatten.json` and `request_record_event.json` with JSON key-order churn only; I restored them (`git checkout`) to keep the wave diff clean. If they reappear dirty after test runs, it is non-semantic.
- No commits made (delivery leader commits the wave). No listener.rs/tools.rs behavioral changes — Wave 3 surface untouched beyond the two mechanical destructures noted above.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced ADR-001 (#4739), ADR-006 (#4744), ADR-004 (#4742), ADR-008 (#4746) and the SessionState clone-cost pattern (#4737); all applied. context_search for vnc-025 decisions confirmed ADR-007 seam (#4745).
- Stored: entry #4748 "Pair Mutex::clear_poison() with into_inner() in treat-as-empty poison recovery" via /uni-store-pattern (the deviation-1 gotcha — compiles fine, only caught by an accumulation test).
