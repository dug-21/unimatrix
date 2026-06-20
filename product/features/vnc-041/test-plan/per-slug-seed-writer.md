# C3 — Per-Slug Seed Writer Test Plan (in `register`, State B + State C)

> File: `projects.rs` (`ProjectRegistry::register`). ADR-002 (#5236). Risks: **R-05 (Critical)**,
> R-10, R-13, R-03 (end-to-end half). ACs: **AC-02** (writes (b) at the resolver's exact path;
> round-trip), AC-05 (no-clobber on (b)). Tests extend `projects/tests.rs` (the `Fixture` harness with
> `ProjectRegistry::with_dirs`, `base_dir`, `config_data_dir`, `slug_dir`, `set_routing`).

## What this component is

Inside `ProjectRegistry::register`, after `ensure_project_stanza`, at BOTH success branches
(State C genesis + State B re-attach), write file **(b)** only:
```
path = per_slug_data_dir(&self.base_dir, &slug).join("config.toml")   // the SINGLE join site (SR-09)
body = render_per_slug_seed_toml()                                     // C2
write_if_absent(path, body)                                           // C1 no-clobber, best-effort
```
Writes ONLY (b); never touches the shared (a)≡(c) path-hash file; `ensure_project_stanza` UNCHANGED.

## Integration tests (the highest-value proofs)

### R-05 / AC-02 — (b) lands at exactly the resolver's path; round-trip
- `test_register_writes_b_at_per_slug_data_dir_path`
  Act: `registry.register("alpha")`. Assert: a file exists at
  `per_slug_data_dir(base_dir, "alpha").join("config.toml")` == `base_dir.join("alpha").join("config.toml")`
  — a SIBLING of the path-hash dir, not inside it (R-05 scenario 2, SR-09).
- `test_register_then_resolve_slug_config_reads_seeded_b` (THE AC-02 round-trip — load-bearing)
  Act: `register("alpha")`, then `resolve_slug_config(base_dir, slug("alpha"), &global)` with NO
  hand-placement step. Assert: the resolver returns `Cow::Owned` (file present arm) — it found and read
  the seeded (b). This is the empirical proof that the seed path == the resolver path (R-05 scenario 3).
- `test_register_seeded_b_is_resolver_loadable_pristine_no_divergence`
  After register, resolve and assert the resolved config equals `global` (pristine seed overlays
  nothing) and emits no WARN. (Couples to R-14.)

### R-05 — (a)/(c) byte-unchanged by the seed's presence
- `test_register_does_not_modify_shared_a_c_file`
  Arrange: pre-establish the path-hash `config.toml` via `set_routing(&["alpha"])` (the (a)≡(c) file with
  `[[projects]]`). Capture its bytes. Act: `register("alpha")` (writes (b), runs `ensure_project_stanza`).
  Assert: the (a)/(c) file's global knobs + `[[projects]]` stanza is byte-for-byte identical to a control
  `register` run with the per-slug seed disabled (or assert the seed never opens the (a)/(c) path). The
  two writers target different files — the seed never collides (R-05 scenario 1, SR-05).
- `test_register_b_path_is_sibling_not_inside_path_hash_dir`
  Assert `base_dir.join(slug)` (where (b) lives) != `config_data_dir` (where (a)/(c) lives) — structural
  proof the seed uses `per_slug_data_dir`, the single join site, not the config data dir (SR-09 forcing
  function).

### R-13 — seed on State B AND State C (re-attach not missed)
- `test_register_state_c_genesis_writes_b`
  Fresh slug, no store, no stanza ⇒ State C. Assert (b) written.
- `test_register_state_b_reattach_writes_b`
  Arrange: a slug whose store already exists (re-attach: `set_routing` + pre-create the store/db, or a
  second `register` after delete-without-purge — whatever State B the `Fixture` exposes). Act: `register`.
  Assert: (b) written (ADR-002 requires the seed at BOTH success branches — R-13 scenario 2). This is the
  specific gap the risk register calls out.
- `test_register_state_a_already_routed_errors_no_seed`
  Arrange: slug already registered + routed ⇒ State A. Act: `register`. Assert: loud error returned
  BEFORE any write; no (b) written, no partial write (R-13 scenario 3, no clobber on the error path).

### R-03 / AC-05 — no-clobber on (b) (operator file survives)
- `test_register_does_not_clobber_pre_placed_b`
  Arrange: pre-place `"# operator per-slug config\n"` at the (b) path BEFORE register. Act: `register`.
  Assert: (b) byte-for-byte unchanged (skip-if-exists via C1's `create_new`). AC-05 / R-03 scenario 2.
- `test_register_twice_does_not_overwrite_b`
  Act: `register("alpha")` twice (or re-attach). Assert: (b) content + mtime unchanged after the first
  write (idempotent; second seed is a no-op).

### R-10 — best-effort: seed failure does not fail register (C-09, NFR-07)
- `test_register_seed_write_failure_does_not_fail_register`
  Arrange: make the (b) target dir non-writable (chmod `0o555` on `base_dir/slug` parent, mirroring the
  existing read-only-dir pattern). Act: `register`. Assert: `register` still reaches its success path
  (Ok returned, store opened, stanza written) — seed failure is warn-and-continue, no error, no panic,
  no `.unwrap()`. Restore perms for cleanup.

## Security (from RISK-TEST-STRATEGY Security Risks)

- `test_register_rejects_hostile_slug_before_any_join`
  The (b) path joins `slug.as_str()`. `ProjectSlug` is a validated newtype (vnc-038). Assert a hostile
  slug (`"../../etc"`, contains separators) is rejected at `ProjectSlug` construction / `register`
  argument parsing, BEFORE any path join — so the seed cannot write outside the base. The seed reuses the
  SAME join the store + resolver use, inheriting slug validation. (May already be covered by an existing
  vnc-038/projects test — if so, reference it; do not duplicate.)

## Coverage requirement (RISK-TEST-STRATEGY R-05, R-13)

(b) written on State B AND State C, State A short-circuits with no seed; (b) at exactly
`per_slug_data_dir(base, slug).join("config.toml")`; resolver picks (b) up with zero hand-placement;
(a)/(c) byte-unchanged; one shared join site; pre-placed (b) survives; seed failure swallowed.
