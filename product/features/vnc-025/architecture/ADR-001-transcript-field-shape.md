## ADR-001: Transcript Rides SessionState as `Arc<Mutex<TranscriptBuffer>>`

### Context

`SessionState` derives `Clone` and `SessionRegistry::get_state()` clones the whole struct
(`session.rs:223-226`) on hot paths — every `context_search` with a session
(`tools.rs:747`, `:1404`). A plain `Vec<u8>` transcript field would deep-copy up to 4 MiB per
search (SR-01, pattern #4737 — the single biggest structural constraint in scope). The risk
assessment mandates this as the first decision; every other design choice depends on it.

Alternatives considered:

- **Sibling map** (`Mutex<HashMap<String, TranscriptBuffer>>` beside `sessions`): keeps
  `SessionState` untouched but creates a two-map consistency obligation at every purge point
  (register/drain/sweep/clear) — a new class of leak bug for zero benefit.
- **Manual `Clone` impl skipping the field**: silent-miss trap whenever a future field is added
  to the 15-field struct; also makes a snapshot's transcript silently empty, which is a lie.
- **Non-cloning accessors only** (field private, no Clone): requires removing
  `derive(Clone)` from `SessionState` — invasive across `session.rs`/`listener.rs`/`tools.rs`.

### Decision

Add `pub transcript: Arc<Mutex<TranscriptBuffer>>` to `SessionState`. `derive(Clone, Debug)`
stays: cloning copies 8 bytes + a refcount bump (AC-10 satisfied structurally);
`TranscriptBuffer` provides a manual metadata-only `Debug` (ADR-002) so the derive compiles
without exposing content.

Lock discipline:

- Order is registry lock → buffer lock; never acquire the registry lock while holding a
  buffer lock.
- `apply_transcript_delta`: under the registry lock do lookup + `Arc::clone` +
  `last_activity_at` bump only (existing microsecond contract); release; then take the buffer
  lock for the merge memcpy. The worst-case 1 MiB copy (frame ceiling, Constraint 9) never
  blocks the global registry mutex — strictly better than the scope's own
  "≤64 KiB fits under the lock" assumption, which the frame ceiling invalidates.
- `get_state()` snapshots share the live buffer through the Arc; the PreCompact reader uses
  exactly this (no new registry read method needed).
- An Arc cloned immediately before key removal (drain/sweep) may merge into an orphaned
  buffer; it frees on last drop. Harmless under the fire-and-forget contract.

### Consequences

- Easier: AC-10 is true by construction; PreCompact reads the snapshot it already holds;
  purge stays structural (key removal drops the last Arc); no two-map bookkeeping.
- Harder: an inner mutex introduces a lock-ordering rule that reviewers must police; a
  snapshot's transcript is *live* (mutates after `get_state()`) — acceptable because the
  PreCompact block is the only reader and reads a point-in-time tail under the buffer lock.
- Cross-references: ADR-002 (buffer internals), ADR-004 (purge metadata read via the same
  handles).
