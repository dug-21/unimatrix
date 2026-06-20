> **⚠️ SUPERSEDED (2026-06-15, uni-zero review) — DO NOT DELIVER.**
> This feature's premise is stale. Its only consumer, **crt-054**, detects compaction via
> an in-stream content-marker regex latch (`saw_compaction` → `reload_after_compaction`,
> ARCHITECTURE.md:65/335), consumes **no byte offset**, lists **no dependency on vnc-036**,
> and explicitly dropped offset/boundary derivation for v1 (ARCHITECTURE.md:338). Even if a
> compaction offset were wanted later, the server already exposes per-session
> `TranscriptBuffer.high_water` (`session_transcript.rs:53/333`) reachable in
> `handle_compact_payload` — so the wire/client change here is avoidable in all cases.
> See the note on crt-054 #752. Retire rather than deliver; reopen only if crt-054's owner
> confirms a genuine offset need.

# vnc-036 — Transcript Stream-Position on the CompactPayload Wire Frame

> A minimal, isolated wire-protocol prerequisite for **crt-054**. The edge client
> writes the current transcript byte-offset at PreCompact; the server receives it and
> exposes it in session state. **Nothing more** — no consumption logic, no DB
> persistence, no fold/re-read counting. Those are crt-054's job.

## Problem Statement

crt-054 ("Activity Accumulator", ADR-001 #4999) folds per-cycle activity signals from
the streamed transcript — including a durable reload counter — at `apply_delta`. To count
**re-reads after each compaction**, crt-054 needs to know *where in the transcript stream a
compaction occurred*: the byte-offset that delimits "before this compaction" from "after".

Today the `CompactPayload` frame — the synchronous PreCompact request — carries **no stream
position**. The server increments a bare `compaction_count` (`infra/session.rs:554-557`,
called at `listener.rs:1854`) but records nothing about *where* in the byte stream each
compaction landed. The information exists on the client (`~/.unimatrix/{hash}/offsets/{key}.json`,
the same per-session offset that `delta.js` advances) but never reaches the server at
PreCompact, because the PreCompact builder is a pure function with no offset input.

This feature closes that one gap: thread the client's current offset into the
`CompactPayload` frame, receive it server-side, and surface it on `SessionState` so crt-054
can read it. Without this, crt-054 cannot delimit per-compaction activity windows.

## Goals

1. Add an optional transcript stream-position field to the `CompactPayload` wire variant
   (`crates/unimatrix-engine/src/wire.rs:172`), serde-optional and absent-on-wire when `None`.
2. The edge client populates the field at PreCompact with the session's current persisted
   offset, fail-open (missing/stale/corrupt offset → field omitted, never throws).
3. The server receives the field in `handle_compact_payload` and **exposes** it in
   `SessionState` (the exact shape is OQ-01 — the central open question below).
4. ts-rs binding parity + parity fixtures updated additively; frozen-hook frames that send
   no offset stay byte-identical (skip-serializing-if discipline).
5. Confirm and absorb the construction/match-site blast radius across `unimatrix-engine` and
   `unimatrix-server` (~50 sites) with mechanical single-line edits.

## Non-Goals

These are **explicitly OUT** — this feature only *writes + exposes* the value:

- **No consumption of the field.** No re-read counting, no per-compaction activity
  delimiting, no fold logic. That is crt-054 (ADR-001 #4999).
- **No DB persistence.** The offset lives in in-memory `SessionState` only — same lifetime
  contract as `compaction_count`, `category_counts`, `confirmed_entries` (reset on
  `register_session`, never written to disk). crt-054 owns any durable accumulation.
- **No change to the existing offset machinery.** `state.js` offset read/write, `delta.js`
  advance rule, the offset file format, and pruning are untouched. We *read* the offset; we
  do not change how it is produced.
- **No change to `compaction_count`** semantics or its increment site.
- **No change to the transcript buffer, `apply_delta`, or any TranscriptBuffer field.**
- **No new MCP tool, no read API for the offset.** crt-054 reads `SessionState` directly,
  exactly as `handle_compact_payload` already reads `compaction_count`.
- **No HTTP-transport behavior change** beyond the additive field. The Rust hook
  (`hook.rs:516`) and HTTP path send `None` (no offset source there) — frozen-frame parity.

## Background Research (grounded in code)

### Wire frame today
`HookRequest::CompactPayload` (`crates/unimatrix-engine/src/wire.rs:172-195`) carries:
`session_id`, `injected_entry_ids`, `role`, `feature`, `token_limit`, `transcript_excerpt`,
`accept`. The last two are the **precedent pattern**: both are
`#[serde(default, skip_serializing_if = "Option::is_none")] Option<T>`, added later, absent
on the wire when `None`, ignored by the handler when not consumed. The new offset field
follows this pattern exactly (transcript_excerpt is the closest analog — added for a future
consumer, `None` over the default transport).

### Server receive path
`listener.rs:1430-1463` destructures `CompactPayload` (currently `_`-ignores
`injected_entry_ids`, `transcript_excerpt`, `accept`) and calls `handle_compact_payload`
(`listener.rs:1737-1862`). That handler resolves `session_state` via
`session_registry.get_state(session_id)` (`:1754`), reads `compaction_count` (`:1760-1763`),
and at `:1854` calls `session_registry.increment_compaction(session_id)`. The new field
threads through the same destructure → handler-param → registry-mutator path that
`compaction_count` already uses.

### SessionState
`crates/unimatrix-server/src/infra/session.rs:146-196`. In-memory only, `Clone + Debug`,
reset on register. `compaction_count: u32` (`:158`) is the existing per-session compaction
scalar; `increment_compaction` (`:554-557`) is its mutator. The new exposure surface sits
alongside these (shape = OQ-01).

### Client offset machinery
- The offset is persisted per session at `~/.unimatrix/{hash}/offsets/{session_key}.json`
  as `{ "offset": N, "updated": <secs> }`, read by `state.readOffset(stateDir, sessionId)`
  (`state.js:97-107`) which already **fail-opens to 0** on missing/corrupt/negative/unsafe.
- `delta.js:maybeSendDelta` (`:209-210`) reads this offset and advances it
  (`offset + byteLength(bytes)`) **only on delta send success** (`:239`). So the persisted
  offset is the **last-shipped** stream position — exactly the "where in the stream" anchor
  crt-054 wants, modulo unsent growth (see OQ-02 caveat).
- **`build-request.js` is pure** (`build-request.js:42`, doc'd "no I/O except process.ppid/cwd").
  The PreCompact arm (`:94-103`) builds `CompactPayload` with no offset and no I/O. The
  PreCompact HookInput carries `transcript_path` but **no offset** — so the offset must be
  threaded in from outside the pure builder.
- **Threading seam already exists.** `index.js:366` calls `buildRequest` (pure), then
  **immediately mutates `request` with I/O** for the SubagentStart case (`:379-389`, a
  transcript file read). This is the established "pure build, then decorate with I/O in
  index.js" pattern. `config.stateDir` and `sessionId` are both in scope at that point
  (`:355`, `:277`). A post-build PreCompact decoration that reads
  `state.readOffset(config.stateDir, sessionId)` and stamps it onto the frame is the
  smallest honest change — it keeps `build-request.js` pure and reuses the
  already-fail-open `readOffset`.

### Blast radius (confirmed)
`grep` over `crates/`: **52** code sites list the full `CompactPayload` field set (full
struct-literal constructions → E0063 on a new required field; full exhaustive destructures →
E0027 without `..`), split across `unimatrix-engine/src/wire.rs` (~18 test sites) and
`unimatrix-server` (hook.rs, listener.rs, listener tests, parity corpus, http router tests,
test_support). **9** sites already use `..` and need **no change**. This matches the
briefing's ~45 estimate (same order of magnitude). Every breaking site is a **mechanical
one-line edit** (add `field: None,` to a literal, or add the field / `..` to a destructure).
Confirm the exact count with `cargo test --workspace --no-run` during delivery.

### Knowledge base
- crt-054 ADR-001 (#4999) — the downstream consumer; defines what "where a compaction
  occurred" is used for (reload counter, per-cycle activity fold). Read; informs OQ-01.
- vnc-022 ADR-005 (`transcript_excerpt` forward-compat field) and vnc-027 ADR-001
  (`accept` field) — the two precedent optional-field additions to this exact variant.
- vnc-025 ADR-002 #4740 (TranscriptBuffer content-opacity) — confirms we must not touch
  buffer internals; the offset is metadata, not content.

## Proposed Approach

1. **Wire field** — add to `CompactPayload`:
   `#[serde(default, skip_serializing_if = "Option::is_none")] transcript_offset: Option<u64>`
   (name/type are OQ-03). Byte-offset `u64` mirrors `delta.js` offsets (`Number.isSafeInteger`
   on the JS side; `u64` server side). Optional so the HTTP/Rust-hook paths that have no
   offset send nothing and stay byte-identical.
2. **Client** — in `index.js`, after the pure `buildRequest` (mirroring the SubagentStart
   decoration at `:379-389`): when `request.type === "CompactPayload"`, set
   `request.transcript_offset = state.readOffset(config.stateDir, sessionId)`. `readOffset`
   already fail-opens to 0; decide whether 0/absent-file should serialize `0` or omit the
   field (OQ-02). Keep `build-request.js` pure.
3. **Server** — destructure the field at `listener.rs:1430` (replace the `_`-style ignore
   pattern), pass it to `handle_compact_payload`, and store it on `SessionState` via a new
   registry mutator (shape = OQ-01), co-located with `increment_compaction`.
4. **Bindings/fixtures** — regenerate ts-rs `HookRequest.ts`; add/extend parity fixtures
   (`request_compact_payload*.json`) additively — one with the field present, existing
   no-field fixtures unchanged to pin frozen-frame parity.
5. **Blast radius** — single-line edits across the ~52 sites. A brace-balanced bulk
   insertion is **not** recommended unmodified: construction sites need `field: None,` while
   destructures need either the field bound or a `..` — different edits, so a blind insert
   would mis-edit destructures (OQ-04). Prefer per-site or two-pass (constructions then
   destructures), or convert ignorable test destructures to `..`.

## Acceptance Criteria

- AC-01: `CompactPayload` carries an optional transcript stream-position field, serde
  `default` + `skip_serializing_if = "Option::is_none"`; `None` is absent on the wire.
- AC-02: An existing `CompactPayload` frame with no offset field deserializes successfully
  (forward/back compat), and a `None`-offset frame serializes byte-identically to a
  pre-vnc-036 frame (frozen-hook parity — Rust hook and HTTP paths unaffected).
- AC-03: At PreCompact, the edge client populates the field from the session's persisted
  offset (`state.readOffset`), and `build-request.js` remains a pure function (the offset is
  applied by the index.js decoration step, not inside the builder).
- AC-04: When the offset file is missing/corrupt/stale, the client fails open per the
  resolved OQ-02 policy and never throws; the PreCompact send still proceeds.
- AC-05: `handle_compact_payload` receives the offset and stores it on `SessionState` per
  the resolved OQ-01 shape; the value is readable from `SessionState` the same way
  `compaction_count` is.
- AC-06: ts-rs binding (`HookRequest.ts`) and parity fixtures are updated; a new fixture
  exercises the field present, and existing no-field fixtures remain byte-unchanged.
- AC-07: `cargo build --workspace` and `cargo test --workspace` pass — all construction and
  match sites updated; no E0063/E0027.
- AC-08: No consumption of the field exists in this feature (no re-read count, no fold, no DB
  write) — verifiable by absence; the value is exposed and nothing more.

## Constraints

- **Optional-field discipline** is load-bearing: `skip_serializing_if` is what preserves
  frozen-frame parity for the HTTP/Rust-hook paths that have no offset (AC-02). Mirror
  `transcript_excerpt`/`accept` exactly.
- **`build-request.js` purity must be preserved** (documented invariant; parity port of
  `hook.rs::build_request`). The offset enters via index.js decoration, not the builder.
- **In-memory-only on the server** — `SessionState` is never persisted (matches
  `compaction_count`); no schema/migration. Do not introduce a DB write.
- **Blast radius is mechanical but wide** (~52 sites across two crates) — destructures and
  constructions need different edits; bulk insertion is hazardous (OQ-04).
- **Type ceiling**: offsets are byte counts of a transcript file; `u64` server-side,
  `Number.isSafeInteger`-bounded client-side (2^53). Consistent with `delta.js`.
- **Crate naming** — wire struct in `unimatrix-engine`; handler + `SessionState` in
  `unimatrix-server`; client in `packages/unimatrix/lib/hook-client`.
- **Test infra is cumulative** — extend the existing wire round-trip / parity-corpus /
  listener transcript fixtures; do not scaffold isolated harnesses.
- **No `.unwrap()` in non-test code, ≤500 lines/file** (workspace rules).

## Open Questions

> **OQ-01 is the central scope question and needs a human decision before design proceeds.**

- **OQ-01 — "Make available": what shape on SessionState? (CENTRAL — needs decision.)**
  Options:
  - **(a) Single latest offset** — `last_compaction_offset: Option<u64>`, overwritten each
    PreCompact. Simplest; mirrors how `compaction_count` is a single scalar.
  - **(b) List of offsets** — `compaction_offsets: Vec<u64>`, one per compaction, parallel
    to `compaction_count`. Lets crt-054 reconstruct *every* per-compaction window in one
    session, not just the most recent.
  - **(c) Latest offset alongside count, with crt-054 doing the folding** — expose `(count,
    latest_offset)` and let crt-054 accumulate windows incrementally at its own fold seam.
  crt-054's ADR-001 (#4999) folds **incrementally at `apply_delta`** and keeps a durable
  reload counter — it does not obviously need a server-held *history* of offsets, because it
  accumulates as deltas arrive. That argues for **(a) or (c)** (expose the latest
  compaction boundary; crt-054 folds re-reads against it). A `Vec` (b) risks unbounded
  in-memory growth for long sessions and duplicates state crt-054 already accumulates.
  **Recommendation to confirm with human + crt-054 owner: (a) single latest offset**, the
  minimal exposure consistent with "write + expose, nothing more." Confirm against crt-054's
  actual read needs before locking.

- **OQ-02 — Fail-open / zero-vs-omit at PreCompact.** `readOffset` returns `0` for
  missing/corrupt/stale files (and `0` is also a legitimate start-of-stream offset). Should
  the client (i) always send the numeric offset including `0`, or (ii) omit the field when
  the offset file is absent (distinguishing "no offset known" from "offset is 0")? Sending
  `0` is simplest and `readOffset` is already fail-open; omitting requires a
  file-exists probe `readOffset` doesn't expose. Also: the persisted offset is the
  *last-shipped* position — unsent transcript growth since the last delta is not reflected.
  Is "last-shipped offset" the correct semantic for crt-054, or does it need "current file
  length"? (Likely last-shipped is correct and matches the delta stream crt-054 folds, but
  confirm.)

- **OQ-03 — Field name + type.** Proposed `transcript_offset: Option<u64>`. Alternatives:
  `compaction_offset`, `stream_position`, `transcript_byte_offset`. Pick one consistent with
  crt-054's vocabulary (ADR-001 uses "byte throughput"/"offset"). `u64` confirmed.

- **OQ-04 — Bulk-edit strategy for the ~52 sites.** Per-site edits, or a two-pass
  (constructions add `field: None,`; destructures get `..` or the bound field), or convert
  ignorable test destructures to `..` to shrink the surface permanently? A single
  brace-balanced bulk insert is **not** safe across both site classes. Recommend: convert
  test destructures that ignore the field to `..` (immunizing them against future additions
  too), and add `field: None,` to constructions.

- **OQ-05 — crt-054 dependency direction confirmation.** This feature is a *prerequisite*;
  crt-054 consumes the exposed value. Confirm vnc-036 lands before crt-054's read code, and
  that OQ-01's shape matches what crt-054's owner expects to read from `SessionState`.

## Dependencies

- **crt-054** (#4999, ADR-001) — the downstream consumer. This feature exists to unblock it;
  OQ-01 must be reconciled with crt-054's read needs.
- **vnc-022** (ADR-005, `transcript_excerpt`) + **vnc-027** (ADR-001, `accept`) — precedent
  optional-field additions to `CompactPayload`; the serde/parity pattern to copy.
- **vnc-024/025/026** transcript-delta + offset stack — produces the offset this feature
  reads (`state.js`, `delta.js`); unchanged here, depended upon.
- **`unimatrix-engine` wire** + **ts-rs bindings/parity corpus** — additive field + fixture.

## Tracking

- GitHub Issue: _(to be created in Session 1)_
