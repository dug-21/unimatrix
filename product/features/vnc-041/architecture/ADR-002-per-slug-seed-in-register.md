## ADR-002: The per-slug seed is eager inside `register`, writes ONLY file (b), and never touches the shared (a)≡(c) file

### Context
SCOPE Goal 2 / AC-02 / OQ-2 (RESOLVED, eager): `register <slug>` must write an annotated
per-slug `config.toml` at the EXACT path Feature A's `resolve_slug_config` reads, so the
operator can discover and edit per-project config without hand-placement. OQ-2 locked the
write to `register` (eager), keeping all provisioning in one command.

The hazard is SR-05 (High): the global config (a) and the `[[projects]]` routing registry
(c) are the SAME physical file — `config_data_dir.join("config.toml")`,
`config_data_dir == paths.data_dir` (projects.rs:182). `register` already writes that file
via `ensure_project_stanza` (vnc-038 ADR-007, atomic RMW that preserves all other
sections). If the per-slug seed also wrote into the path-hash file, two writers in the same
`register` flow could clobber `[[projects]]` or global knobs.

The per-slug file (b) is a DIFFERENT path: `{base_dir}/{slug}/config.toml`, where
`base_dir = paths.data_dir.parent()` — a SIBLING of the path-hash dir (SR-09). The single
per-slug path-join site already exists: `per_slug_data_dir(base, slug)` (projects.rs:122),
which `register` itself uses for the store dir and which mirrors the resolver's
`base_dir.join(slug.as_str()).join(PROJECT_CONFIG_NAME)` (http_provision.rs:317).

### Decision
The per-slug seed is written inside `ProjectRegistry::register`, at the two store-success
branches, targeting ONLY file (b):

1. **Path** = `per_slug_data_dir(&self.base_dir, &slug).join("config.toml")` — REUSE the
   existing single join site (SR-09); never recompute the base or re-derive the path. This
   is byte-identical to what `resolve_slug_config` reads, so the resolver picks it up on
   the next restart with no hand-placement (AC-02).
2. **When** = after the store is open in BOTH success states, mirroring `ensure_project_stanza`:
   - State C (genesis): after `Store::open` (genesis) + `ensure_project_stanza`.
   - State B (re-attach): after `Store::open` (re-attach) + `ensure_project_stanza`.
   - State A (already registered + routed) returns a loud error before any write — no seed.
3. **How** = the no-clobber `create_new` primitive (ADR-001) with the
   classification-rendered body (ADR-003); skip-if-exists (AC-05). An existing operator (b)
   survives a re-register.
4. **Isolation** = the per-slug seed writes (b) and ONLY (b). It does NOT call
   `write_default_config_if_absent` on the path-hash file, does NOT touch `[[projects]]`,
   and `ensure_project_stanza` (which owns (a)/(c)) is UNCHANGED. The two writers in
   `register` target different files; they cannot collide.
5. **Posture** = best-effort. A seed-write failure logs a `tracing::warn` and `register`
   continues to its success message — provisioning convenience must never fail the
   hash-chain-critical registration nor the routing-intent write.

### Consequences
- Easier: one command (`register`) provisions the routing intent (a≡c) AND the editable
  per-slug config (b); the operator never hand-places (b) (AC-02).
- Easier: SR-05 is structurally avoided — (b) and (a)/(c) are different paths written by
  different code; no read-modify-write contention, no clobber surface.
- Easier: SR-09 avoided — reusing `per_slug_data_dir` guarantees the seed lands exactly
  where the resolver looks; a wrong-base bug is impossible because there is one join site.
- Cost: two call sites (State B and State C) get the seed call. They are additive — no
  change to `register`'s signature or to `ensure_project_stanza` — so the SR-08 ripple is
  bounded (no `Command`/match-arm shape change).
- Restart-applies (vnc-038 ADR-007 / SCOPE Constraint): seeding writes (b); the overlay
  applies on the next `serve`, not live. No hot-reload (Non-Goal).
- Cross-references ADR-001 (no-clobber primitive), ADR-003 (rendered body), vnc-038 ADR-007
  (the `register` write site this runs alongside), vnc-040 ADR-001 (the resolver path (b)
  must match).
