# Risk-Based Test Strategy: vnc-041

> Config seeding (global (a) + per-slug (b)) + seam-level WARN for global-locked keys.
> Capability **C17** (Unimatrix #5214). Feature B of the vnc-040 split. GH **#801**.
> Sources: `SCOPE.md`, `architecture/ARCHITECTURE.md`, ADR-001..005, `specification/SPECIFICATION.md`,
> `SCOPE-RISK-ASSESSMENT.md`. Crate is **`unimatrix-server`** (the binary crate), NOT `unimatrix-engine`.

## Architect corrections this strategy risk-tests
1. **Code lives in `unimatrix-server`** — all new sites (`main.rs`, `projects.rs`, `infra/config.rs`,
   `http_provision.rs`) are in the server binary crate; `unimatrix-engine/src/project.rs` is a
   read-only dependency. Tests target the server crate.
2. **The global-seed gate is `if config.http.enabled`, NOT `base_dir = Some(/data)`** (ADR-004). The
   `base_dir` arg is `None` on every live `serve` call site (main.rs:599/1347/1779/529/546) — a seed
   keyed on it would NEVER fire. `http.enabled` is the SOLE structural safeguard against regressing
   the local / single-project majority (AC-06). This is the highest-risk seam in the feature and is
   risk-tested as such (R-01, R-02).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Global seed fires on the local / single-project `serve` path — regresses the majority (wrong/missing `http.enabled` gate, or gate placed where `base_dir`-keyed logic never fires) | High | Med | **Critical** |
| R-02 | AC-06 sentinel passes by structural reasoning but not empirically — local path actually writes a file under some config (e.g. `http.enabled` true locally, or seed leaks outside the `else` branch) | High | Med | **Critical** |
| R-03 | Seed write is not atomic-no-clobber (TOCTOU / `fs::write` / `File::create` / `atomic_write` rename) and truncates or replaces an operator-authored (a) or (b) | High | Med | **Critical** |
| R-04 | WARN/annotation "locked surface" hand-enumerated in B and drifts from A's `PER_SLUG_CONFIG_CLASSIFICATION` — mis-warns or misses keys (`permissive` field-less, transport construction-locked, `*_sha256` merge-locked) | High | High | **Critical** |
| R-05 | Per-slug seed writes the shared (a)≡(c) path-hash file (or wrong base) instead of the distinct (b) — clobbers `[[projects]]`/global knobs, or the resolver can't find (b) | High | Med | **Critical** |
| R-06 | A→B classification drift over time: A flips a key disposition; B's seed annotation and WARN surface silently diverge (no single runtime consumption point) | High | Med | High |
| R-07 | Raw-TOML WARN parse introduces a NEW error path or alters resolution — turns a parseable per-slug file into a `ServerError`, or changes the merged `Cow<UnimatrixConfig>` output (R-13 must be WARN-only) | High | Med | High |
| R-08 | WARN granularity wrong: per-request spam instead of once-per-boot-per-key, or dedup state leaks across boots / across slugs | Med | Med | High |
| R-09 | `Command`/`handle_version`/`register` signature or `Command` match-arm ripple breaks `main_tests.rs` and existing config tests (config.rs:11262–11346) | Med | Med | Med |
| R-10 | Best-effort seed write failure (permission, full disk, read-only FS) fails or panics `register`/`serve` instead of warn-and-continue | Med | Low | Med |
| R-11 | `handle_version` (init/version) and serve both seed (a); a race or ordering bug between the two writers corrupts or double-writes the file | Med | Low | Low |
| R-12 | Field-less / shape-mismatched locks (`permissive`, embedding descriptor with only `*_sha256`, `tls`/`http`) render a bogus editable knob or panic the renderer | Med | Med | Med |
| R-13 | Per-slug seed written only on State C (genesis) and missed on State B (re-attach) — a re-registered slug never gets (b) | Med | Med | Med |
| R-14 | Seeded (b) body is itself not parseable / not resolver-loadable — operator restart fails or the seed round-trips into a validation error | Med | Low | Med |

## Risk-to-Scenario Mapping

### R-01: Global seed regresses the local / single-project majority
**Severity**: High **Likelihood**: Med
**Impact**: Every local STDIO user gets a config file they never had (#783-class devex regression in reverse); AC-06 breaks; trust in "byte-for-byte unchanged" is lost. This is the #4583 silent-fallback class of bug (#5206 evidence) re-introduced.

**Test Scenarios**:
1. `serve` with `config.http.enabled == false` (local/STDIO) and an empty home `.unimatrix` ⇒ assert ZERO config files written at the path-hash path (and anywhere).
2. `serve` with `config.http.enabled == true` and empty `/data` ⇒ assert (a) IS written at `paths.data_dir.join("config.toml")`.
3. Structural assertion: the seed call is lexically inside the `if config.http.enabled` block — the `else` branch contains no seed call site (code-presence test / the SR-08 ripple audit confirms placement).
4. Confirm the seed does NOT key off the `base_dir` argument: with `base_dir = None` (the live serve value) and `http.enabled == true`, the seed STILL fires (proves the gate is `http.enabled`, not `base_dir`).

**Coverage Requirement**: Both branches of `http.enabled` exercised on the serve path with empty config dirs; the local branch must be proven to write zero files empirically (file count == 0), not asserted by reading the branch.

### R-02: AC-06 sentinel green by reasoning, not empirically
**Severity**: High **Likelihood**: Med
**Impact**: The sentinel is the only forcing function for SR-04. A test that asserts the structure but never counts files lets a real regression ship. Unimatrix #4876: gate-integrity / error-propagation claims must be verified empirically, not by structural reasoning alone.

**Test Scenarios**:
1. Sentinel test counts files in the home `.unimatrix` tree before and after a local `serve` boot ⇒ delta == 0 (no new config file, no per-slug dir).
2. Resolution-behavior baseline: capture the local config-load + resolution result pre-vnc-041 (or with the seed code path disabled) and assert it is byte-for-byte / value-for-value identical with vnc-041 present on the local path.
3. Negative control: the SAME sentinel harness on the container (`http.enabled`) path SHOWS a non-zero delta — proving the sentinel actually detects writes and isn't trivially passing.

**Coverage Requirement**: A file-count delta assertion (== 0 local, > 0 container) plus a resolution-output equivalence assertion against a pre-vnc-041 baseline.

### R-03: Seed write not atomic-no-clobber (clobbers operator config)
**Severity**: High **Likelihood**: Med
**Impact**: Operator's hand-placed (a) or (b) (placed for Feature A) is truncated/replaced on re-boot or re-register — silent data loss of intentional config. Violates AC-05, the #665 TOCTOU class.

**Test Scenarios**:
1. Pre-place operator content in (a); run container `serve`; assert (a) byte-for-byte unchanged (skip-if-exists).
2. Pre-place operator content in (b); run `register <slug>`; assert (b) byte-for-byte unchanged.
3. Re-register / second-boot idempotency: run the seed path twice; second run never overwrites (mtime + content unchanged after first write).
4. Primitive assertion: the per-slug writer uses the shared `create_new(true)` `write_if_absent` helper (ADR-001), NOT `fs::write`/`File::create`/`atomic_write` rename — confirm `AlreadyExists` is treated as a silent no-op.
5. TOCTOU: there is no check-then-write window — the existence guard IS the `O_EXCL` open (one syscall), proven by the absence of a separate `path.exists()` precheck before the write.

**Coverage Requirement**: Byte-for-byte survival of pre-placed content for BOTH files; the no-clobber primitive is `create_new`-based and single-sourced (one `write_if_absent`); no `path.exists()` precheck gating the write.

### R-04: Locked surface hand-enumerated and drifts from A's classification
**Severity**: High **Likelihood**: High
**Impact**: A hardcoded locked list in B mis-warns (warns on an overlayable key) or misses (silent-ignores a locked key — the exact R-13 failure this feature exists to close). The heterogeneous locks (`permissive` field-less, transport construction-locked, `*_sha256` merge-locked) make a hand-list especially fragile (SR-02).

**Test Scenarios**:
1. Annotation render: for EVERY entry in `PER_SLUG_CONFIG_CLASSIFICATION`, assert overlayable ⇒ editable line, locked ⇒ commented-out + "managed globally" line in the seeded (b). No key outside the registry, none missing.
2. WARN surface: for each `GlobalLocked` key set in a per-slug file, assert a WARN fires; for each `PerSlugOverlayable` key set, assert NO WARN.
3. **Flip test (AC-03 "proven, not restated")**: stub/flip one key's `OverlayDisposition` (overlayable↔locked) and assert BOTH the rendered seed annotation AND the WARN behavior for that key flip — proving both derive from the registry at runtime.
4. Unknown-key behavior (ADR-005): a non-registry / typo'd key set in (b) ALSO warns (conservative `is_per_slug_overlayable == false` default) — assert and document.

**Coverage Requirement**: Annotation + WARN both bind to `is_per_slug_overlayable` at runtime; the flip test must move BOTH derived behaviors; no hand-enumerated key list exists in B (grep/structural confirm).

### R-05: Per-slug seed touches the shared (a)≡(c) file or wrong base
**Severity**: High **Likelihood**: Med
**Impact**: Two writers in the `register` flow (`ensure_project_stanza` on (a)/(c), the per-slug seed) collide ⇒ clobbered `[[projects]]` routing or global knobs (SR-05); OR a wrong base computation writes (b) into the path-hash dir where the resolver can't read it (SR-09).

**Test Scenarios**:
1. After `register <slug>`, assert (a)/(c) (the path-hash `config.toml`: global knobs + `[[projects]]` stanza) is byte-for-byte identical to a `register` run with the per-slug seed disabled (the seed never touched (a)/(c)).
2. After `register <slug>`, assert (b) exists at exactly `per_slug_data_dir(base_dir, slug).join("config.toml")` = `base_dir.join(slug).join(PROJECT_CONFIG_NAME)` — the SAME join `resolve_slug_config` uses (sibling of path-hash dir, not inside it).
3. Round-trip: after `register <slug>`, run `resolve_slug_config` for that slug and assert it READS the seeded (b) with no hand-placement step (AC-02 — proves the path matches the resolver's).
4. Path-join single-site assertion: the seed reuses `per_slug_data_dir` (projects.rs:122), not a recomputed base (SR-09 forcing function).

**Coverage Requirement**: (a)/(c) byte-unchanged across register-with-seed; (b) at the resolver's exact path; resolver picks (b) up with zero hand-placement; one shared join site.

### R-06: A→B classification drift over time
**Severity**: High **Likelihood**: Med
**Impact**: A future A-side classification change silently diverges B's seed annotations and WARN surface — re-opening the drift R-13/SR-02 were designed to prevent.

**Test Scenarios**:
1. NFR-03 structural test: confirm annotation render (C2) and WARN (C5) each consult `is_per_slug_overlayable` / iterate `PER_SLUG_CONFIG_CLASSIFICATION` at runtime — a single consumption surface, no second copy of the split in B.
2. The R-04 flip test doubles as the drift guard — one entry flip moves both behaviors.
3. `OverlayDisposition` exhaustiveness: adding a new variant is a compile break in `render_per_slug_seed_toml` (ADR-003 forcing function) — confirm the render `match` is exhaustive, not a catch-all.

**Coverage Requirement**: Both consumers bind at runtime; a disposition variant addition fails to compile until B classifies it.

### R-07: WARN raw-parse introduces a new error path / alters resolution
**Severity**: High **Likelihood**: Med
**Impact**: R-13 must be WARN-ONLY. A second raw `toml::Value` parse that errors, or that feeds resolution, could turn a parseable per-slug file into a `ServerError` or change the merged config — a behavior change masquerading as a log line (SR-06).

**Test Scenarios**:
1. Equivalence: resolution output (`Cow<UnimatrixConfig>`) for a per-slug file with a global-locked override present is byte/value-identical WITH and WITHOUT the WARN code path — only logs differ (FR-12).
2. The locked override value remains IGNORED (Feature A behavior) after the WARN — assert the merged value is the global, not the per-slug, value.
3. Raw-parse failure degradation: a per-slug file the raw pass cannot inspect does NOT add a new error — the existing `load_single_config` in the same arm is the sole error source (a malformed file still surfaces the existing loud, slug-named `ServerError::Config`, not a new one from the WARN pass).
4. No-file arm untouched: a slug with no (b) file produces no WARN and byte-for-byte fallthrough (vnc-040 AC-02 sentinel still green).

**Coverage Requirement**: Resolution output identical with/without WARN; no new error variant reachable from the WARN pass; the no-file arm is unmodified.

### R-08: WARN granularity / dedup-state scope wrong
**Severity**: Med **Likelihood**: Med
**Impact**: Per-request WARN spam floods logs (defeats the signal), or dedup state leaks across boots (a real new override never re-warns) or across slugs (one slug's WARN suppresses another's).

**Test Scenarios**:
1. Repeated `resolve_slug_config` calls for the same slug+key within one boot ⇒ exactly one WARN (FR-11). Note: ADR-005 states the resolver runs once per slug per boot (per-slug loop, main.rs:1089), so confirm whether dedup is even reachable, and where the once-per-boot state lives (OQ-C).
2. Two different slugs each setting the same locked key ⇒ a distinct WARN per slug (key+slug named — no cross-slug suppression).
3. Dedup state resets across boots: a fresh boot re-emits the WARN (state is not persisted).

**Coverage Requirement**: At most one WARN per (slug, key) per boot; per-slug and per-boot isolation of any dedup state.

### R-09: Signature / match-arm ripple breaks existing tests
**Severity**: Med **Likelihood**: Med
**Impact**: An over-broad change (new `Command` variant shape, changed `register`/`handle_version` signature) breaks `matches!` arms, `main_tests.rs:33`, and the four `write_default_config_if_absent` tests (config.rs:11262–11346) — gate-3a churn or a CI miss.

**Test Scenarios**:
1. Existing `write_default_config_if_absent` tests (create-when-absent, no-overwrite-no-force, force-overwrite, silent-on-write-fail) still pass after the `write_if_absent` extraction/delegation (ADR-001).
2. `main_tests.rs` `Command::Version` arm and any `register` call-site tests compile/pass unchanged (additive call sites only — OQ-D / SR-08).
3. New callers are additive — `register` and `handle_version` signatures unchanged.

**Coverage Requirement**: All pre-existing config-write and Command-match tests green; no signature/variant shape change.

### R-10: Best-effort seed failure fails the command
**Severity**: Med **Likelihood**: Low
**Impact**: A read-only FS / permission / full-disk seed-write failure aborts or panics `register`/`serve` — provisioning convenience gating a hash-chain-critical registration or the daemon boot.

**Test Scenarios**:
1. Seed write into a non-writable target dir ⇒ `register` still reaches its success path (warn-and-continue, no error returned, no panic). No `.unwrap()` on the seed write (NFR-07).
2. Same for the global seed: a write failure on (a) does NOT abort `serve` startup.
3. After a failed seed, the resolver tolerates the absent (b) (no-file arm) and `serve` tolerates the absent (a) (loads from defaults).

**Coverage Requirement**: Seed-write failure is logged and swallowed on both sites; command/daemon proceeds; no panic, no `.unwrap()`.

### R-11: Dual (a) writers (handle_version + serve) conflict
**Severity**: Med **Likelihood**: Low
**Impact**: `init`/`version` and `serve` both seed (a) via the same function; an ordering/race bug could double-write or corrupt.

**Test Scenarios**:
1. `init` then container `serve` ⇒ (a) written once by init, serve no-ops (skip-if-exists); content is the init-written file.
2. Reverse order (serve seeds first, later `version` runs) ⇒ same file, second caller no-ops. Whichever runs first wins (ADR-004).

**Coverage Requirement**: Idempotent across both (a) writers; the `create_new` primitive makes the second a no-op regardless of order.

### R-12: Field-less / shape-mismatched locks mis-render
**Severity**: Med **Likelihood**: Med
**Impact**: `permissive` (no `UnimatrixConfig` field), the embedding descriptor (only `inference.embedding_model_sha256`, no `[embedding].model` field), `tls`/`http` (transport, never read at the seam) — the renderer could emit a bogus editable knob or panic dereferencing a missing field (SR-03).

**Test Scenarios**:
1. Assert `permissive`, `tls`, `http`, the `*_sha256` descriptors render in the legend as "managed globally; value ignored" with NO editable knob emitted.
2. Renderer does not panic / does not require a struct field for any field-less classification entry (legend is keyed on the registry `key` string + disposition, not a struct field — ADR-003).
3. The legend lists EXACTLY the registry's dotted keys (e.g. `inference.w_sim`, `permissive`) — count matches the registry length.

**Coverage Requirement**: Every field-less lock renders as a documented "managed globally" legend line, never an editable knob, never a panic.

### R-13: Per-slug seed missed on State B (re-attach)
**Severity**: Med **Likelihood**: Med
**Impact**: A slug re-registered (State B) after a fresh checkout / store re-attach never gets (b) — the operator who deletes and re-registers, or attaches to an existing store, has no per-slug seed (AC-02 partial failure).

**Test Scenarios**:
1. State C (genesis) `register <slug>` ⇒ (b) written.
2. State B (re-attach: store already exists, no stanza or re-register) `register <slug>` ⇒ (b) written (ADR-002 requires the seed at BOTH success branches).
3. State A (already registered + routed) ⇒ loud error before any write, no seed (no clobber, no partial write).

**Coverage Requirement**: (b) written on State B AND State C; State A short-circuits with no seed.

### R-14: Seeded (b) body not resolver-loadable
**Severity**: Med **Likelihood**: Low
**Impact**: The rendered seed (legend + reused `DEFAULT_CONFIG_TOML`) is itself unparseable or fails validation, so an operator's first restart after `register` errors on its own seed.

**Test Scenarios**:
1. Parse the freshly-seeded (b) as TOML ⇒ succeeds (the legend lines are comments, the body is the proven `DEFAULT_CONFIG_TOML`).
2. Feed the seeded (b) through `resolve_slug_config` ⇒ resolves with no error and no WARN (a pristine seed sets no global-locked key; all commented-out).
3. Round-trip: seed → resolve → assert the resolved config equals the global (the seed overlays nothing until an operator edits it).

**Coverage Requirement**: A pristine seed parses, resolves cleanly, and emits no WARN.

## Integration Risks

- **(b) path must equal the resolver path** (R-05): the single highest-value integration assertion is `register` → `resolve_slug_config` round-trip with no hand-placement (AC-02). Reusing `per_slug_data_dir` is the structural guarantee; the round-trip test is the empirical proof.
- **Two writers in `register`** (R-05): `ensure_project_stanza` (a≡c) and the per-slug seed (b) run in the same flow on different paths. Test that (a)/(c) is byte-unchanged by the seed's presence.
- **Two (a) writers across commands** (R-11): `handle_version` and serve both seed (a); idempotency by `create_new`.
- **A→B registry binding** (R-04, R-06): render (C2) and WARN (C5) both consume the same `infra/config.rs` registry; the flip test exercises the binding for both.
- **WARN pass vs existing `*_sha256` merge WARN** (ADR-005 §4): a set-and-diverging `*_sha256` may log twice (the new pass + the existing merge warn). Test that this duplicate is present-and-acceptable, not a defect — and that neither path errors.

## Edge Cases

- Empty per-slug file (b) (zero keys set) ⇒ no WARN, resolves to global.
- (b) sets only per-slug-overlayable keys ⇒ no WARN, overlay applies.
- (b) sets a key that is BOTH present in the template AND classified locked (e.g. an uncommented `[embedding]` descriptor) ⇒ WARN fires, value ignored.
- Unknown / typo'd key in (b) ⇒ WARN fires (conservative default, ADR-005) — explicitly assert.
- `register` with the per-slug dir already containing a hand-placed (b) ⇒ skip-if-exists, operator file survives.
- Container serve where (a) exists but is operator-edited ⇒ skip, edits survive (AC-05).
- Concurrent `register` of the same slug (two processes) ⇒ `create_new` makes one win, the other no-ops (no clobber, no corruption).
- `http.enabled == true` on a LOCAL host (misconfiguration) ⇒ seed fires (gate is `http.enabled`, by design); document that `http.enabled` is the definition of "container path," not host detection.
- Maximum-size (b) (~64 KiB, ADR-005) raw-parse cost ⇒ once-per-boot, negligible; no per-request parse.

## Security Risks

Untrusted/external input surfaces for this feature:

- **Per-slug `config.toml` (b) is operator-authored, file-system input parsed twice** (typed + raw `toml::Value`, ADR-005). Risk: a malformed or hostile TOML triggering a panic or a new error path. Mitigation/test: the raw parse must NOT add a failure mode — `load_single_config` remains the sole error source; the WARN pass degrades to no-warn on an uninspectable file (R-07). Assert a malformed (b) cannot crash the resolver via the WARN pass.
- **Path construction for (b)** — the slug is operator-supplied. `per_slug_data_dir(base, slug)` joins `slug.as_str()`. Risk: path traversal if `slug` were not validated (e.g. `../../etc`). Blast radius: a seed write outside the `.unimatrix` base. Mitigation: `ProjectSlug` is a validated newtype (vnc-038); the seed reuses the SAME join the store + resolver already use, so it inherits slug validation — confirm `ProjectSlug` validation rejects traversal/separators (test a hostile slug is rejected at construction, before any join).
- **Seed-write blast radius** — `create_new` cannot overwrite an existing file (no destructive write of arbitrary paths); a failed/denied write is swallowed (R-10). Worst case is a missing convenience file, never data loss or escalation.
- **WARN log content** — the WARN names key + slug. Both are bounded identifiers (registry keys are static; slug is a validated newtype). No untrusted value payload is logged (mirrors the #4749 content-free-logging pattern: log key/disposition, never the operator's set VALUE).

## Failure Modes

- **Seed write fails (permission / full disk / read-only FS)** ⇒ `tracing::warn` and continue; `register`/`serve` succeed; resolver/serve tolerate the absent file (R-10). Provisioning never gates the daemon or the hash-chain-critical registration.
- **Raw WARN parse fails** ⇒ no WARN emitted; the existing typed parse surfaces the loud slug-named `ServerError::Config` (R-07). The WARN pass never converts a parseable file into an error.
- **Target file already exists** ⇒ silent no-op (skip-if-exists), operator content survives (R-03, AC-05).
- **Classification has a new disposition variant** ⇒ compile break in the renderer (intended forcing function, R-06/ADR-003) — fail closed at build time, not silent at runtime.
- **Local path reached with seed code** ⇒ impossible by branch placement (`else` of `http.enabled` has no seed call) — AC-06 holds by construction, sentinel-verified empirically (R-01, R-02).

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (no-clobber not atomic / TOCTOU) | R-03, R-11 | ADR-001: shared `create_new` `write_if_absent` primitive; no `fs::write`/`File::create`/`atomic_write` on a seed. Tested by byte-for-byte survival + no-precheck assertion. |
| SR-02 (WARN locked surface hand-enumerated, drifts) | R-04, R-06 | ADR-005: WARN derives from `is_per_slug_overlayable == false` at runtime; flip test proves binding. |
| SR-03 (annotation renderer field-less locks) | R-04, R-12 | ADR-003: legend rendered by iterating the registry; field-less locks render "managed globally" with no knob; renderer never dereferences a struct field. |
| SR-04 (global seed regresses local majority) | R-01, R-02 | ADR-004 CORRECTION: gate is `if config.http.enabled` (NOT `base_dir`); local `else` branch has no seed call; AC-06 sentinel counts files == 0 empirically (#4876). |
| SR-05 (shared (a)≡(c) two-writer clobber) | R-05 | ADR-002: per-slug seed writes (b) ONLY; global seed is serve-time, never inside `register`; the two writers target different paths. Tested by (a)/(c) byte-unchanged. |
| SR-06 (WARN scope creeps to rejection / behavior change) | R-07, R-08 | ADR-005: WARN-only; resolution output identical with/without WARN; no new error path; once-per-boot-per-key. |
| SR-07 (A→B classification drift) | R-06, R-04 | NFR-03: single runtime consumption point for render + WARN; flip test moves both; `OverlayDisposition` exhaustiveness is a compile forcing function. |
| SR-08 (variant/signature ripple) | R-09 | OQ-D audit: additive call sites only; existing `write_default_config_if_absent` + `main_tests.rs` + register call-site tests stay green. |
| SR-09 (per-slug dir is sibling; wrong base) | R-05 | ADR-002: reuse `per_slug_data_dir` single join site; round-trip `register`→`resolve_slug_config` proves (b) lands where the resolver reads. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01..R-05) | ~21 scenarios — full coverage; both `http.enabled` branches empirically file-counted, byte-for-byte no-clobber for (a) and (b), flip test for the A→B binding, register→resolver round-trip |
| High | 3 (R-06, R-07, R-08) | ~10 scenarios — resolution-output equivalence with/without WARN, runtime binding + exhaustiveness, once-per-boot-per-key dedup scope |
| Medium | 5 (R-09, R-10, R-12, R-13, R-14) | ~13 scenarios — existing-test ripple, best-effort failure swallow, field-less render, State B+C seed, pristine-seed round-trip |
| Low | 1 (R-11) | ~2 scenarios — dual (a) writer idempotency both orders |

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search for config-seed/TOCTOU/regression lessons and risk patterns -- low-relevance results; most applicable: #4876 (gate-integrity claims must be verified empirically, not structurally -- drives R-02 sentinel design), #4881 (fire-and-forget write race / flaky tests -- informs R-10 best-effort posture), #4749 (content-free logging -- informs the Security WARN-content assessment), #1118 (sentinel markers for idempotent file mutation -- informs R-03/R-11).
- Stored: nothing novel to store -- the risks here are feature-specific; no cross-feature (2+) pattern emerged beyond the already-stored #4876 gate-integrity lesson and #4749 content-free-logging pattern, which this strategy reuses rather than duplicates.
