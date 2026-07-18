## ADR-003: Live-PID-only import refusal makes the shutdown vector-clobber structurally unreachable

### Context
Import into a live daemon's slug is corrupting in a way that *looks* like success (SR-03, C-4). `build_project_server` loads each slug's vector index at boot only (`http_provision.rs:196-224`); at shutdown the daemon dumps every per-slug index back to disk (`for (index, dir) in &handles.per_slug_vectors { index.dump(dir) }`, `infra/shutdown.rs:189-197`, #823). So: `import --slug` rebuilds `{slug}/vector` on disk → the daemon still holds its stale boot-time index in memory → on the next shutdown it **overwrites the freshly rebuilt index with the stale one**. DB rows survive; vector search silently reverts. The existing pre-flight has the right signal but the wrong strength: the PID check (`import/mod.rs:268-273`) is warning-only (SR-07), which lets an operator proceed straight into this clobber. The PID file lives in the path-hash `data_dir` and is the **daemon's** PID (one daemon serves all slugs), which is correct.

### Decision
`import --slug` **hard-errors** when a live daemon PID is present — full stop, no warning-past (OQ-1). The predicate is **live-PID-only**:

```
if let Some(pid) = read_pid_file(&paths.pid_path)        # paths = path-hash, base-scoped daemon PID
   && is_process_alive(pid) / is_unimatrix_process(pid)  # infra/pidfile.rs — liveness, not mere file presence
{ return Err(...) }                                       # refuse; create nothing, write nothing
```

- Reuses `infra/pidfile.rs` primitives — `read_pid_file` + `is_process_alive` (kill -0); `is_unimatrix_process` (`/proc/{pid}/cmdline`) preferred where the stronger daemon-identity check matters, mirroring `bridge.rs:72`. **Liveness, not file existence** — a stale PID file from a crashed daemon must not refuse the import.
- The `"AND slug in [[projects]]"` half of any earlier predicate is **dropped**: config is read only at boot (#5079), so a stanza cannot prove the daemon holds the store (`register` writes the stanza *before* the next restart), and gating on it would refuse the documented `register → stop → import → start` sequence.
- The error (AC-13) names the **resolved path-hash PID path** (`paths.pid_path`) and the remedy `stop → import → start`. Covered by a test asserting the refusal fires while a live PID is present.
- This **reverses SR-07's warning-only stance for this one flag only**. A `--force`-style override is out of scope. `--slug`-less import keeps its warning-only PID behavior unchanged (AC-05).

### Consequences
Easier: the shutdown-clobber is **structurally unreachable** — import cannot run while any daemon is up, so no live daemon can hold a stale index over a rebuilt one (SR-03 closed by construction, not by operator discipline). The supported sequence `register → stop → import --slug → start` never trips the gate, because `stop` clears the live PID. The verdict is provable from `start` onward (ADR-004, AC-12), not from disk state.

Harder: import now depends on the assumption "one daemon serves all slugs; its PID is base-scoped in the path-hash `data_dir`" — true today; if a per-slug daemon model ever ships, this predicate must be revisited. A crashed daemon leaving a *live* PID of a reused OS PID is a theoretical false-refuse; `is_unimatrix_process` narrows it to unimatrix binaries, making it negligible. The operator must take the daemon down to restore — accepted; it is the documented, canonical sequence (OQ-3).
