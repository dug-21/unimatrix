## ADR-001: The seed-write primitive is the existing `create_new` no-clobber path, shared by both the global and per-slug seeds

### Context
vnc-041 writes two new files (global (a) on serve, per-slug (b) on register). SCOPE
Non-Goal "Overwriting operator-authored config" and AC-05 require **skip-if-exists**:
an operator may have hand-placed either file for Feature A, and a re-register / re-boot
must not clobber it. SR-01 (High): a naive check-then-write (TOCTOU) or a `File::create`
truncates an operator file before the existence guard fires (#665, #4567).

The codebase already contains TWO distinct write disciplines, and they are NOT
interchangeable for this purpose:

- `infra/config.rs:4836` `write_default_config_if_absent(path, force)` — `force=false`
  uses `OpenOptions::new().write(true).create_new(true).open(path)` (config.rs:4871-4874).
  `create_new(true)` is the atomic O_EXCL "fail if it already exists" primitive; an
  `AlreadyExists` error is treated as a silent no-op. This is exactly skip-if-exists with
  no TOCTOU window. It also creates the parent dir and logs-and-continues on any error.
- `projects/config_write.rs:100` `atomic_write` — temp + fsync + `std::fs::rename` over
  the target, where the temp is opened with `std::fs::File::create` (line 106). This is
  the correct primitive for an atomic *read-modify-write of an existing file* (the
  `[[projects]]` stanza), but as a *seed* primitive it is WRONG: `rename` unconditionally
  REPLACES the destination — it would clobber an operator file. `atomic_write` has no
  skip-if-exists semantics.

### Decision
Both vnc-041 seed writes use the `create_new`-based no-clobber primitive, NOT
`atomic_write`.

- The **global seed (a)** reuses `write_default_config_if_absent(&path, false)` verbatim
  (ADR-004). It already does precisely the right thing — it is the same function
  `handle_version` calls today; vnc-041 only adds a second caller (serve).
- The **per-slug seed (b)** needs a *different body* (classification-rendered, ADR-003),
  so it cannot call `write_default_config_if_absent` (which is hardwired to
  `DEFAULT_CONFIG_TOML`). It MUST replicate the same `OpenOptions::create_new(true)`
  discipline with its own rendered content. Concretely: factor a small content-parameterized
  helper, e.g. `write_if_absent(path: &Path, content: &str)`, that contains the exact
  parent-create + `create_new(true)` + `AlreadyExists`-is-noop + warn-and-continue logic,
  and have `write_default_config_if_absent` delegate to it with `DEFAULT_CONFIG_TOML`.
  This keeps ONE no-clobber implementation; the per-slug writer passes its rendered body.

NEVER use `fs::write`, `File::create`, or the `atomic_write` rename for a seed write —
each can truncate or replace an operator-authored file.

### Consequences
- Easier: one TOCTOU-safe no-clobber primitive; AC-05 holds by construction for both
  files (the O_EXCL open atomically refuses an existing file — no check-then-write gap).
- Easier: the global seed is literally the existing, already-tested function (config.rs
  tests at 11262–11346 cover create-when-absent, no-overwrite-no-force, force-overwrite,
  silent-on-write-fail) — zero new behavior to prove for (a).
- Cost: the per-slug seed re-uses the *primitive* but not the *function*, because its body
  differs; the small refactor (extract `write_if_absent`) keeps the no-clobber logic
  single-sourced rather than copy-pasting the OpenOptions dance.
- Deliberate exclusion: seeds are best-effort and do NOT fsync (unlike `atomic_write`).
  A seed is convenience provisioning; the resolver tolerates an absent (b) and serve
  tolerates an absent (a). Durability-before-rename matters for the `[[projects]]` routing
  truth (`atomic_write`'s job), not for a regenerable default template.
- Cross-references ADR-002 (per-slug writer), ADR-004 (global writer reuses the function).
