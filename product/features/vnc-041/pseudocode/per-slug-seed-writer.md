# C3 — Per-slug seed writer (in `register`, State B + State C)

> ADR-002. Crate: `unimatrix-server`, file `projects.rs`. Drives AC-02/AC-05, addresses SR-05/SR-09, R-05, R-10, R-13.

## Purpose

Eagerly write the per-slug file (b) inside `ProjectRegistry::register`, at BOTH store-success branches
(State B re-attach + State C genesis), AFTER `ensure_project_stanza`. The seed targets **only file (b)** —
the sibling per-slug path — never the shared (a)≡(c) path-hash file. Best-effort: a seed-write failure
warns and `register` proceeds to its success message.

## Anchor (verified, projects.rs:267–348)

`register(raw_slug)` after validation computes `dir = per_slug_data_dir(&self.base_dir, &slug)` (line 269),
then branches:
- **State A** (data_exists && is_routed): loud error, returns BEFORE any write — NO seed.
- **State B** (data_exists, not routed): `Store::open` (re-attach) → `ensure_project_stanza(&slug)` (line 305)
  → `println!` → `return Ok(())`. **Insert the seed call after line 305, before the `println!`.**
- **State C** (genesis): `create_dir_all` → `Store::open` → `ensure_project_stanza(&slug)` (line 342)
  → `println!` → `Ok(())`. **Insert the seed call after line 342, before the `println!`.**

`self.base_dir` and the validated `&slug` are in scope at both sites; `dir` is already
`per_slug_data_dir(&self.base_dir, &slug)`.

## New / modified functions

### `write_per_slug_seed` (NEW — private method or free fn)

A small helper so the call site is one line at both branches (avoids duplicating the path-join + render).

```
fn write_per_slug_seed(&self, slug: &ProjectSlug):
    // PATH: REUSE the single per-slug join site (SR-09). NEVER recompute base or re-derive.
    // This is byte-identical to what resolve_slug_config reads:
    //   base_dir.join(slug.as_str()).join(PROJECT_CONFIG_NAME)
    path = per_slug_data_dir(&self.base_dir, slug).join("config.toml")

    // BODY: classification-derived (C2). DERIVES from the registry at runtime.
    body = config::render_per_slug_seed_toml()

    // WRITE: shared no-clobber primitive (C1). skip-if-exists; best-effort; returns ().
    // write_if_absent is infallible (warn-and-continue internally), so there is nothing to
    // propagate — register's hash-chain-critical steps already completed above.
    config::write_if_absent(&path, &body)
```

NOTE on `write_if_absent` visibility: it is module-private in `infra/config.rs` (C1). C3 lives in
`projects.rs`, same crate. Expose it as `pub(crate)` so the `register` site can call it directly with the
C2-rendered body. (`write_default_config_if_absent` cannot be used here — it is hardwired to
`DEFAULT_CONFIG_TOML`; the per-slug body differs. ADR-001.)

### `register` call-site changes (signature UNCHANGED — additive only)

```
// State B (re-attach), after ensure_project_stanza(&slug) (line 305), before the println!:
self.ensure_project_stanza(&slug)?;
self.write_per_slug_seed(&slug);          // ◄── NEW (C3). Best-effort; no `?`.
println!("re-attached project '{slug}' ... Restart to apply.", ...)
return Ok(())

// State C (genesis), after ensure_project_stanza(&slug) (line 342), before the println!:
self.ensure_project_stanza(&slug)?;
self.write_per_slug_seed(&slug);          // ◄── NEW (C3). Best-effort; no `?`.
println!("registered project '{slug}' ... Restart to apply.", ...)
Ok(())
```

State A is untouched — it returns before any write, so no seed (R-13 #3).

## State machine (which register state seeds)

| State | Condition | Seed (b)? | Why |
|-------|-----------|-----------|-----|
| A | data_exists && is_routed | NO | loud error before any write; no clobber, no partial write |
| B | data_exists, !is_routed (re-attach) | YES | a re-registered slug must get (b) (R-13) |
| C | !data_exists (genesis) | YES | fresh registration provisions (b) |

The seed runs AFTER `ensure_project_stanza` at B and C, mirroring its placement. Ordering invariant:
store-open → routing intent (a≡c) → per-slug seed (b). The seed is the LAST step and best-effort, so it
never affects the hash-chain or routing writes.

## Initialization sequence

None new — `ProjectRegistry` construction is unchanged. C3 adds two call sites + one helper method.

## Data flow

- **Inputs:** `&self.base_dir` (the per-slug base, sibling of path-hash dir), validated `&slug`.
- **Output:** `()` — side effect: file (b) created at `{base_dir}/{slug}/config.toml` iff absent.
- **Transformations:** `(base_dir, slug)` → path via `per_slug_data_dir`; registry → body via C2;
  body written by C1.

## Isolation invariant (SR-05, R-05) — the dominant failure mode

C3 writes **(b) and ONLY (b)**:
- It calls `per_slug_data_dir(base_dir, slug).join("config.toml")` — the SIBLING per-slug path, never
  `config_data_dir.join("config.toml")` (the path-hash (a)≡(c) file).
- It does NOT call `write_default_config_if_absent` on the path-hash file, does NOT touch `[[projects]]`,
  and leaves `ensure_project_stanza` (the (a)/(c) owner) UNCHANGED.
- The two writers in `register` (`ensure_project_stanza` on (a)/(c), C3 on (b)) target different paths —
  no read-modify-write contention, no clobber surface.

## Error handling

- **Best-effort (C-09, R-10):** `write_if_absent` is infallible (warns internally). C3 makes NO `?` call
  on the seed and returns nothing from it — `register` always reaches its success `println!` and `Ok(())`.
- A seed-write failure (permission, full disk, read-only FS) logs a warn (inside C1) and registration
  succeeds; the resolver tolerates an absent (b) via its no-file arm.
- No `.unwrap()`, no panic on the seed path (NFR-07).

## Key test scenarios (hints — see RISK-TEST-STRATEGY R-05, R-10, R-13)

- **R-13 #1/#2 (State B + C, the core requirement):** State C genesis `register <slug>` ⇒ (b) written;
  State B re-attach `register <slug>` ⇒ (b) written. State A ⇒ loud error, NO seed (R-13 #3).
- **R-05 #2 / AC-02:** after `register <slug>`, (b) exists at exactly
  `per_slug_data_dir(base_dir, slug).join("config.toml")` = `base_dir.join(slug).join(PROJECT_CONFIG_NAME)`.
- **R-05 #1 (isolation):** after `register <slug>`, the path-hash (a)≡(c) file (global knobs +
  `[[projects]]` stanza) is byte-for-byte identical to a `register` run with the seed disabled.
- **R-05 #3 (round-trip — highest-value integration assertion):** after `register <slug>`, run
  `resolve_slug_config` for that slug and assert it READS the seeded (b) with NO hand-placement.
- **R-05 #4:** the seed reuses `per_slug_data_dir` (one join site) — structural/grep confirm, no recomputed base.
- **AC-05 / R-03 #2:** pre-place operator content in (b); `register <slug>` ⇒ (b) byte-for-byte unchanged.
- **R-10 #1:** seed write into a non-writable per-slug dir ⇒ `register` still reaches success (warn-and-continue,
  no error returned, no panic).
- Concurrent `register` of the same slug ⇒ `create_new` makes one win, the other no-ops (no corruption).

## Open questions / gaps

- **`write_if_absent` visibility.** C1 spec leaves `write_if_absent` module-private; C3 needs it from
  `projects.rs`. Resolution: declare it `pub(crate)` in `infra/config.rs`. Flagged so C1's implementer
  and C3's implementer agree on the visibility (no signature change, just the modifier). Confirm during 3b.
```
