# Scope Risk Assessment: vnc-026

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `build-request.js` parity: porting nontrivial logic from the 4,183-line `hook.rs` (PostToolUse rework/failure extraction, MIN_QUERY_WORDS gate, SubagentStart JSONL tail-parse) — Rust/JS behavioral divergence in edge cases (malformed JSONL, UTF-8 boundaries, missing fields) | High | High | Architect: enumerate the exact `hook.rs` functions ported and their edge-case inventory in the design; spec: parity corpus (AC-01/AC-05) must include adversarial cases, not just happy paths. Flagged primary risk by accepted review. |
| SR-02 | Byte-identical JSON serialization across languages: `serde_json` vs `JSON.stringify` differ on field order guarantees, number formatting, and unicode escaping — AC-04 demands byte parity for the `hookSpecificOutput` envelope | High | Medium | Spec: pin the envelope to a literal template string, not object serialization; treat fixtures (`bindings/fixtures/`) as the only authority, per the frozen-wire constraint. |
| SR-03 | AC-13 spawn budget (p50 ≤ ~12 ms incl. state-dir hash) measured only on the reference env; queue replay-before-send adds unbounded latency to the first spawn after an outage | Medium | Medium | Architect: bound replay work per spawn (max frames/bytes per replay batch); keep replay strictly off the sync trio path. |
| SR-04 | Delta size math: 64 KiB raw bytes inflate under JSON string-escaping inside the 1 MiB body guard; binary/control chars in transcripts inflate worst-case ~6x | Medium | Medium | Spec: define the cap against the *serialized* frame (AC-07 hints at this — make it explicit) and add a post-serialization size check. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | "Minimal" disk queue (RQ-1) is a scope-creep magnet: ordering, growth bound, corruption recovery, and concurrent-spawn locking are all unspecified; concurrent hook spawns in one session can interleave queue writes and `last_offset` updates | Medium | High | Spec: define queue bounds (max entries/bytes/age), drop-oldest policy, and single-writer strategy (e.g., O_EXCL per-frame files, no shared mutable file). Evidence: gate failures cluster where "minimal" components ship without failure-path tests (#4473). |
| SR-06 | Queued `transcript_delta` frames persist raw conversation bytes (potentially secrets) unencrypted in `~/.unimatrix/{hash}/`, with no retention/cleanup defined in scope | High | Medium | Architect: define queue retention + purge-on-replay and SessionClose/queue-expiry cleanup; document the at-rest posture. Evidence: #4711 — raw-conversation event types must not inherit generic disk fall-through. |
| SR-07 | RQ-8 couples this remote feature to local-mode `HOOK_EVENTS` changes — a regression there breaks every existing local install, far beyond F3's blast radius | Medium | Low | Keep the fix to the one-line list + matchers exactly as scoped; AC-16 regression test must cover re-run recognition of pre-existing local configs. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | `merge-settings.js` ownership regexes don't match `node …/hook-client/index.js` commands; mixed upgrades (local→remote, re-runs, F5 later) risk duplicated/orphaned hook entries | Medium | High | Extend `UNIMATRIX_PATTERNS` per the established prefix-match pattern (#1195, ADR #1201); AC-11 re-run tests must cover configs containing *old-style* unimatrix entries. |
| SR-09 | Token/URL resolution at hook spawn time: the client must locate `.claude/settings.local.json` from the hook's cwd; wrong-root resolution or missing token fails silently (exit-0 mandate) | Medium | Medium | Architect: specify the config-discovery algorithm (cwd walk vs env var precedence) and what `init` validates; spec: AC for spawn-time resolution from a subdirectory cwd. |
| SR-10 | Fail-open-everywhere makes misconfiguration invisible: an expired token or wrong URL means remote sessions silently lose all learning indefinitely — no user-visible signal exists in scope | Medium | High | Architect: consider a cheap local signal (queue growth as the observable, stderr-to-log, or `init` Ping as the only checkpoint — make the trade-off an explicit ADR). Evidence: #4473 — warn+continue posture masks failure paths. |
| SR-11 | Delivery gate depends on vnc-025 (#670, in flight): F2 review rework could shift buffer/merge semantics the Layer-1 parity harness pre-populates against | Medium | Medium | Design against the F2 *wire contract* (frozen, F1 types) not its internals; isolate buffer pre-population behind one test helper so F2 changes localize. |

## Assumptions

- **A-1** (SCOPE.md Background/ass-068): ~12 ms Node spawn holds on user machines, not just the reference env — AC-13 only re-validates on reference hardware. If wrong on slow filesystems (network homes), the state-dir hash + queue check erode the sync budget.
- **A-2** (SCOPE.md Constraints): vnc-025 merges without semantic drift to the offset-bounded idempotent merge; AC-05/AC-10 gates are unrunnable against F1-only servers.
- **A-3** (SCOPE.md Proposed Approach): the 18 committed contract fixtures are sufficient parity authority — any `build_request` behavior not represented in a fixture is invisible to AC-14 and only caught by AC-01's Rust-comparison corpus.
- **A-4** (SCOPE.md Goal 3): Claude Code transcripts remain append-only JSONL; `[last_offset, file_len)` reads assume the file never rewrites earlier bytes (compaction rewriting the file would silently corrupt deltas).

## Design Recommendations

1. **SR-01/SR-02**: Make the parity corpus the design's first artifact — generated from the Rust hook as the oracle (golden outputs committed), covering the defensive/malformed paths in `hook.rs::run()`, before any client module is designed.
2. **SR-05/SR-06**: Give the disk queue a one-page mini-spec: bounds, eviction, locking, at-rest content posture, cleanup lifecycle. It is the only stateful, secrets-adjacent component in an otherwise stateless client.
3. **SR-08/SR-09**: Treat `init --remote` + spawn-time config discovery as one integration surface; test matrix must include re-runs over old-style configs and spawns from non-root cwds.
4. **SR-10**: Decide the observability trade-off explicitly (ADR): full silence vs minimal local breadcrumb when remote delivery is failing.
5. **A-4**: Spec a cheap guard — if `file_len < last_offset` (truncation/rewrite), reset offset rather than ship garbage.
