## ADR-005: Modularity — new-module-with-thin-wiring; no ad-hoc monolith split; scheduled decompositions

### Context
PL-10 (#5715) / #693: no module grows unbounded; a design-phase gate must flag any file a feature
pushes over the 500 **code-line** cap (tests excluded) or that is already over-cap and being modified,
and require an accounted plan — never a silent waiver, never untested mid-delivery surgery. SCOPE
Open Question 2 routes the carve call here and warns the raw counts (background.rs 5075, listener.rs
10131, hook.rs 4403) are **raw lines, not code-lines** — re-measure first.

Re-measured code-lines (block comments + `#[cfg(test)]` modules excluded):

| File | Code-lines | Over cap | C18 edits |
|---|---|---|---|
| `uds/hook.rs` | 804 | yes | yes (provider arm) |
| `uds/listener.rs` | 2504 | yes | yes (write-path INSERT) |
| `services/observation.rs` | 975 | yes | yes (read path) |
| `unimatrix-store/src/db.rs` | ~1250 | yes | yes (CREATE +2 cols) |
| `unimatrix-store/src/migration.rs` | ~1500 | yes | yes (additive ALTER) |
| `background.rs` (#966) | 1389 | yes | **NO** |
| `wire.rs` | 237 | no | yes (add model_id) |
| `domain/mod.rs` | 106 | no | yes (opencode pack) |
| `unimatrix-store/src/observations.rs` | 185 | no | yes (read SELECT) |

Key correction: **background.rs is not touched by C18.** Its `INSERT INTO observations` (line 2960)
sits inside the `#[cfg(test)]` module (starts line 1943); the production write path is `listener.rs`.
The SCOPE #966 (background.rs) and #965 (listener.rs) carve candidates rested on the raw-line misread.

### Decision
1. **No carve of background.rs** — C18 does not modify its production code; PL-10 requires no action on
   a file the feature does not touch. It stays over-cap and untouched.
2. **No ad-hoc mid-delivery split** of listener.rs (2504), hook.rs (804), observation.rs (975), db.rs,
   or migration.rs. Splitting a 2504-line monolith with no dedicated safety net during feature delivery
   is untested surgery — a risk PL-10 and SCOPE both forbid.
3. **New-module-with-thin-wiring** for all net-new logic:
   - C2 OpenCode event-name canonicalization → new `uds/hook/opencode.rs`; hook.rs gets a const entry
     + one delegating match arm.
   - C9 installer OpenCode branch → new `packages/unimatrix/lib/opencode-install.js`; init.js gets one
     call.
   - C1 plugin → net-new module tree, ships at/under cap.
   The over-cap files receive only minimal wiring: `"opencode"` in `KNOWN_PROVIDERS`; a `source_domain`/
   `model_id` column pair in the two `listener.rs` INSERTs; SELECT + prefer-stored logic in
   `observation.rs`/`observations.rs`; an additive ALTER block in migration.rs; two columns in the db.rs
   CREATE.
4. **Scheduled decomposition, not executed** — per PL-10 done_when(2), the delivery-phase check records
   each over-cap file C18 modifies (hook.rs, listener.rs, observation.rs, db.rs, migration.rs) and
   files or updates a refactor issue with a test plan (the parity corpus + existing listener/observation
   test suites are the safety net a future decomposition builds on). Deferred to a dedicated cycle.
5. New modules ship at/under the cap (done_when 3).

### Consequences
Easier: C18's footprint in each monolith is a handful of wiring lines; no risky refactor blocks
delivery; the over-cap debt is tracked, not silently waived. Harder: five over-cap files still grow
(slightly); the scheduled-decomposition issues are real follow-up debt someone must schedule; a new
`hook/opencode.rs` sub-module adds a module boundary the normalizer must delegate across.
Cross-references ADR-004 (C2 new module), ADR-006 (C9 new module).
