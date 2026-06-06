## ADR-003: Client State Dir + Disk Event Queue Mini-Spec (Bounds, Eviction, Lock-Free Writes, Purge)

### Context

RQ-1 includes a minimal disk queue (enqueue-on-failure, replay-before-send); RQ-2 fixes
the state root at `~/.unimatrix/{hash}/`. SR-05 (Medium/High) warns "minimal" queues ship
without bounds, corruption recovery, or concurrent-spawn safety; SR-06 (High/Medium)
warns queued payloads are secrets-adjacent and unencrypted; SR-03 warns unbounded replay
erodes the spawn budget. The Rust `EventQueue` (`event_queue.rs`: `pending-{ts}.jsonl`
append files, 1000 events/file, 10 files, 7-day prune) shares append-files between
writers — fine for Rust's serialized usage, but concurrent Node hook spawns interleaving
appends to one file risk torn lines. The JS client needs its own spec, not a blind port.

### Decision

**State dir layout** — `~/.unimatrix/{hash}/hook-client/` (subdir of the Rust scheme's
project dir; `{hash}` = first 16 hex of SHA-256 of project-root path, identical algorithm
to `project.rs::compute_project_hash`). Created with dir mode 0700; all files 0600. F4
(UDS client) inherits this dir unchanged.

```
~/.unimatrix/{hash}/hook-client/
  offsets/{session_key}.json    # { "offset": N, "updated": <unix secs> }
  queue/{ts_ms}-{pid}-{seq}.json  # one HookRequest frame per file
  health.json                   # ADR-005 breadcrumb
```

`session_key` = session_id if it matches `^[A-Za-z0-9_-]{1,64}$`, else
`sha256(session_id).slice(0,16)` — no path traversal via attacker-shaped session ids.

**Writes are lock-free by construction**:
- Queue enqueue: one frame per file, created with `fs.writeFileSync(path, data, { flag:
  "wx" })` (O_CREAT|O_EXCL). No shared mutable file → no torn writes, no locking.
  Name collision (same ms, same pid) bumps `seq`.
- Offsets and health.json: write temp file + `fs.renameSync` (atomic on POSIX). Concurrent
  spawns of one session are last-writer-wins; the worst case is a re-shipped span, which
  F2's offset-bounded merge dedupes (idempotent — ass-069 Q1).

**Bounds and eviction** (checked at enqueue):
- max 500 queue files AND max 5 MiB total; beyond either → delete oldest first
  (drop-oldest; lexicographic order on the `{ts_ms}` prefix is age order).
- Age prune: queue files older than **24 hours** deleted (vs Rust's 7 days — deliberate:
  queued `RecordEvent` payloads carry `tool_input`/`tool_response`, secrets-adjacent;
  short retention is the SR-06 mitigation for what does reach disk). Offset files prune
  at 7 days since `updated`; the session's offset file is deleted on successful
  SessionClose send.
- An outage longer than 24 h loses queued telemetry — accepted under the degradation
  contract (content loss, never mis-attribution).

**Replay** (replay-before-send, RQ-1):
- Runs only on fire-and-forget spawns — never the sync trio (SR-03).
- Per-spawn budget: at most **32 frames or 256 KiB** sent, oldest first; delete each file
  only after a 2xx; stop at first failure (leave remainder); leftover replays next spawn.
- Corrupt frame file (unparseable JSON) → delete and continue (poison-pill immunity).

**Content posture** (at-rest statement, SR-06): the queue persists only non-delta
fire-and-forget frames. `transcript_delta` frames are **never** written to disk
(ADR-004) — raw conversation bytes have zero at-rest footprint. What is queued
(tool_input/tool_response excerpts inside RecordEvent payloads) matches the existing
Rust queue's exposure, tightened by 0600/0700 modes and the 24 h prune. No encryption at
rest in F3 — same posture as the shipped Rust queue; enterprise at-least-once/encrypted
delivery is the named ass-069 Q7 gap, out of scope.

**Queue failures never affect exit code or stdout** (AC-15): every queue operation is
wrapped; errors go to the breadcrumb + stderr only.

### Consequences

- Easier: no file locking anywhere; bounded disk and bounded replay latency make AC-13/
  SR-03 measurable; one-frame-per-file makes corruption local to one event; F4 reuses the
  dir and format as-is.
- Harder: many small files instead of one JSONL (readdir cost — trivial at ≤500 files);
  ordering granularity is ms+pid+seq, not strictly total across processes (sufficient:
  F2 merge is offset-keyed for deltas and order-tolerant for observations).
- Diverges from the Rust queue's file format — intentional; the two queues never share a
  directory (`event-queue/` vs `hook-client/queue/`), so no cross-format reads occur.
