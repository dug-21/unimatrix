# vnc-041 — SPECIFICATION

> Config seeding (global + per-slug) plus seam-level WARN for global-locked keys.
> Capability **C17 — Installation provisions the proper config in place** (Unimatrix #5214).
> Feature B of the vnc-040 split. GH **#801**. Pairs with C6 (resolution, shipped Feature A / #799).
> Source: `product/features/vnc-041/SCOPE.md`, `SCOPE-RISK-ASSESSMENT.md`.

## Objective

Per-slug and cloud-global config are **resolvable** today (Feature A / vnc-040) but never
**provisioned** — no code writes the annotated global `config.toml` on a container `serve`, and
`register <slug>` never writes the per-slug `config.toml` the resolver reads. This feature seeds both
files (skip-if-exists, never clobbering operator-authored config), renders the seed's per-key
annotations from Feature A's canonical classification, and adds a seam-level `tracing::warn` so a
hand-edited global-locked key no longer vanishes silently (R-13). Local STDIO / single-project
behavior is left byte-for-byte unchanged.

## Domain Models

### The three config files (canonical disambiguation)

This feature touches THREE files; two are the **same physical file**, one is distinct. Conflating
them is the dominant failure mode (SR-05, SR-09), so the ubiquitous language below is normative.

| Tag | Name | Path | Writer (today) | This feature |
|-----|------|------|----------------|--------------|
| **(a)** | GLOBAL annotated `config.toml` | `{.unimatrix base}/{path-hash}/config.toml` (the daemon path-hash dir; `paths.data_dir.join("config.toml")`) | `write_default_config_if_absent` (`infra/config.rs:4836`), called ONLY by `handle_version` (`init`/`version`) | **Goal 1 / AC-01 / AC-05** — container `serve` seeds it |
| **(c)** | PROJECTS REGISTRY / `[[projects]]` stanza | **SAME physical file as (a)** | `ensure_project_stanza` (`projects/config_write.rs:35`) via `register` | **NOT written by this feature** — vnc-038 prior art; B must not touch (a)/(c) |
| **(b)** | PER-SLUG `config.toml` | `{base_dir}/{slug}/config.toml` where `base_dir = paths.data_dir.parent()` (SIBLING of the path-hash dir) | nothing writes it today | **Goal 2/3/4, AC-02/03/04** — `register` seeds it; resolver reads + WARNs on it |

### Ubiquitous language

- **path-hash dir** — `{.unimatrix base}/{compute_project_hash()[..16]}`, computed by
  `project::ensure_data_directory` (`project.rs:146`). `base_dir = None ⇒ dirs::home_dir()/.unimatrix`
  (local); `base_dir = Some(/data) ⇒ /data/.unimatrix` (container). Container-vs-local is decided
  **structurally** by this `base_dir` argument, not a runtime heuristic.
- **base_dir (per-slug)** — `paths.data_dir.parent()` = `{.unimatrix base}`. The per-slug dir is a
  **sibling** of the path-hash dir, never inside it.
- **`PER_SLUG_CONFIG_CLASSIFICATION`** — Feature A's canonical declarative registry
  (`infra/config.rs`): `&[ConfigKeyClass { key, disposition: PerSlugOverlayable | GlobalLocked }]`.
  The **single source of truth** for which keys are per-slug-overlayable vs global-locked (vnc-040
  ADR-004, #5217).
- **`is_per_slug_overlayable(key) -> bool`** — the predicate over the registry. `false` ⇒ the key is
  **global-locked**. This feature's annotation render AND WARN surface both derive from it at runtime;
  neither restates the split.
- **per-slug-overlayable key** — a key the resolver honors from a per-slug file (e.g.
  `knowledge.categories`, `confidence.weights`, `server.instructions`, `nli_top_k`, `nli_enabled`).
- **global-locked key** — a key the resolver ignores from a per-slug file (transport/daemon sections,
  the `[embedding]` descriptor / `inference.embedding_model_sha256`, `inference.nli_model_sha256`,
  `permissive`). Note `permissive` has no clean `UnimatrixConfig` field to render from (SR-03).
- **seed** — write the annotated default config to a target file **only if absent**; provisioning,
  never overwrite.
- **skip-if-exists** — the no-clobber guard: a seed write must not truncate or overwrite an existing
  target (operator may have hand-placed it for Feature A).
- **A→B one-way contract** — Feature A owns the classification; Feature B (this feature) consumes it.
  If A's classification changes, B's annotations + WARN surface follow automatically; B restates
  nothing.
- **regression sentinel** — the AC-06 guard that local STDIO / single-project config behavior is
  byte-for-byte identical to pre-vnc-041 (no new file forced).
- **WARN (R-13)** — one `tracing::warn` per ignored global-locked key, once per boot, naming the key
  + slug, at the resolver seam. WARN only: no rejection, no behavior change beyond the log line.

## Functional Requirements

- **FR-01 — Global seed write.** On the container `serve` startup path, when file (a) is absent at the
  resolved path-hash path, the daemon writes the annotated `DEFAULT_CONFIG_TOML` template there.
  (Goal 1, AC-01) *Test:* container `serve` with no (a) present ⇒ (a) exists afterward at
  `/data/.unimatrix/<path-hash>/config.toml` and exposes the `DEFAULT_CONFIG_TOML` knobs.
- **FR-02 — Global seed is container-only.** The global seed fires ONLY on the container path,
  decided structurally by `ensure_data_directory`'s `base_dir = Some(/data)` argument (not a runtime
  flag). The local single-project `serve` path forces no new file. (Goal 1, OQ-4, AC-01, AC-06; SR-04)
  *Test:* local `serve` with no (a) present writes ZERO config files.
- **FR-03 — Per-slug seed write.** `register <slug>` writes file (b) at `{base_dir}/{slug}/config.toml`
  when absent — the exact path `resolve_slug_config` reads
  (`base_dir.join(slug).join(PROJECT_CONFIG_NAME)`, `PROJECT_CONFIG_NAME = "config.toml"`), reusing
  the resolver's path construction, not recomputing it. (Goal 2, OQ-2, AC-02; SR-09) *Test:* after
  `register <slug>`, (b) exists at the resolver path and a subsequent restart's `resolve_slug_config`
  picks it up with no hand-placement.
- **FR-04 — Per-slug seed is eager at register.** The per-slug seed is written in the same `register`
  flow as the slug store + `[[projects]]` stanza (eager), not lazily on first `serve`. (Goal 2, OQ-2,
  AC-02)
- **FR-05 — Per-slug seed targets the distinct file only.** The per-slug seed writes file (b) ONLY.
  It must NOT write or modify file (a)/(c) (the shared path-hash `config.toml`); `register`'s existing
  `ensure_project_stanza` write to (c) is untouched and the two writers never overlap. (Goal 2;
  SR-05) *Test:* after `register <slug>`, (a)/(c) global knobs and `[[projects]]` stanza are
  byte-unchanged relative to a register without the per-slug seed.
- **FR-06 — Annotations render from the classification.** The per-slug seed's per-key annotations are
  produced by iterating `PER_SLUG_CONFIG_CLASSIFICATION` / calling `is_per_slug_overlayable` at write
  time — never a hand-listed key set. (Goal 3, AC-03; SR-02, SR-03, SR-07)
- **FR-07 — Per-slug-overlayable keys rendered editable.** Keys for which `is_per_slug_overlayable`
  is true are rendered as editable entries in the per-slug seed. (Goal 3, AC-03)
- **FR-08 — Global-locked keys rendered commented-out + "managed globally".** Keys for which
  `is_per_slug_overlayable` is false are INCLUDED in the per-slug seed but commented out and annotated
  "managed globally" (not omitted). Field-less locks (e.g. `permissive`, which has no `UnimatrixConfig`
  field) are handled explicitly rather than silently dropped. (Goal 3, OQ-1, AC-03; SR-03) *Test:* a
  classification flip of one key (overlayable↔locked) flips that key's seed annotation —
  proven, not restated.
- **FR-09 — Seam-level WARN for ignored global-locked keys.** When `resolve_slug_config` encounters a
  per-slug file (b) that sets a key for which `is_per_slug_overlayable` is false, it emits ONE
  `tracing::warn` naming the ignored key + slug. (Goal 4, AC-04; SR-02) *Test:* a per-slug file
  setting `[embedding].model` / a transport field / `permissive` emits a WARN naming that key + slug.
- **FR-10 — WARN derives from the classification.** The WARN surface (the set of keys that trigger a
  warning) is exactly `{ key : is_per_slug_overlayable(key) == false }`, evaluated at runtime — never
  a hand-enumerated locked list. (Goal 4, Constraints; SR-02, SR-07)
- **FR-11 — WARN granularity: per key, once per boot.** At most one WARN is emitted per ignored
  global-locked key per daemon boot — not per request. (Goal 4, OQ-3, AC-04; SR-06) *Test:* repeated
  `resolve_slug_config` calls for the same slug+key within one boot emit the WARN at most once.
- **FR-12 — WARN is signal-only.** The WARN adds a log line only: the offending value remains ignored
  (Feature A's existing behavior), no new rejection path, no resolution/overlay change. (Goal 4,
  Constraints, AC-04; SR-06) *Test:* resolution output with a global-locked override present is
  identical with and without the WARN code path (only logs differ).
- **FR-13 — Skip-if-exists (no clobber).** Both seeds (global and per-slug) write only when the target
  is absent and never truncate/overwrite an existing target. The write uses an atomic no-clobber
  primitive (`OpenOptions::create_new`), not check-then-`fs::write`/`File::create`. (Goals 1+2,
  Constraints, AC-05; SR-01) *Test:* a target file with operator content survives a re-register /
  re-boot byte-for-byte.
- **FR-14 — Shared no-clobber seed primitive.** The global and per-slug seed writes use ONE shared
  `create_new`-based seed-write primitive (reusing `write_default_config_if_absent`'s no-clobber
  path), not two divergent write sites. (Constraints "Reuse"; SR-01, SR-05)
- **FR-15 — Reuse existing template/write machinery.** Seeding uses the existing config-write
  machinery (`config_write.rs` / `DEFAULT_CONFIG_TOML` template); no new TOML serializer and no new
  merge logic. (Constraints "Reuse")

## Non-Functional Requirements

- **NFR-01 — Local/STDIO regression: zero files, byte-for-byte.** A local STDIO / single-project
  deployment forces no new config file and its config behavior is byte-for-byte identical to
  pre-vnc-041. Asserted by a sentinel test that counts files written on the local path (must be 0).
  (AC-06; SR-04; vnc-040 ADR-002 byte-for-byte-by-construction, #5206)
- **NFR-02 — No new config surface.** This feature seeds the EXISTING `UnimatrixConfig` surface only:
  no new sections, no new tunable knobs. (Non-Goals; SR-04) Adding a knob would break NFR-01 and the
  A-owned classification simultaneously.
- **NFR-03 — Single classification consumption point.** Annotation render and WARN surface both bind
  to `is_per_slug_overlayable` at runtime so an A-side classification change cannot silently diverge B
  (drift is structurally prevented, not test-policed). (Constraints; SR-07)
- **NFR-04 — Atomicity / no TOCTOU on seed writes.** The seed-write primitive must be atomic w.r.t.
  the no-clobber guard (no check-then-write window that could truncate an operator file under a
  concurrent register/boot). (SR-01, #665)
- **NFR-05 — Restart-applies (no hot-reload).** Seeding writes a file; the overlay still applies on
  the next restart (vnc-038 ADR-007). The per-slug seed need not take effect within the writing
  process. (Non-Goals, Constraints)
- **NFR-06 — Log discipline.** All seam signals use `tracing` (no `println!`/`eprintln!`); WARN level
  for ignored global-locked keys, mirroring the existing `*_sha256` global-wins WARN precedent.
  (Constraints)
- **NFR-07 — Workspace rules.** ≤500 lines/file, no stubs (`todo!()`/`unimplemented!()`/TODO), no
  `.unwrap()` in non-test code. (Constraints)

## User Workflows

### W1 — Operator deploys a fresh container and edits global config
1. Operator runs the container `serve` for the first time; no (a) present.
2. Daemon seeds the annotated global `config.toml` at the path-hash path (FR-01).
3. Operator discovers and edits the file; restarts; config applies.
4. On restart, (a) is present ⇒ seed is skipped (FR-13); operator edits survive (AC-05).

### W2 — Operator registers a project and tunes per-slug config
1. Operator runs `register <slug>`; the slug store + `[[projects]]` stanza are created (vnc-038).
2. In the same flow, the per-slug seed writes (b) at `{base_dir}/{slug}/config.toml` (FR-03/04),
   per-slug-overlayable keys editable, global-locked keys commented out + "managed globally" (FR-08).
3. Operator edits per-slug-overlayable keys; restarts; `resolve_slug_config` overlays them.

### W3 — Operator hand-edits a global-locked key in a per-slug file
1. Operator uncomments/sets a global-locked key (e.g. `[embedding].model`) in (b).
2. On next boot, `resolve_slug_config` ignores the value (Feature A behavior) AND emits one
   `tracing::warn` naming the key + slug (FR-09/11/12).
3. Operator sees the WARN in logs and understands why the key did nothing — no silent vanish (R-13).

### W4 — Local single-project operator (regression-protected)
1. Operator runs local STDIO `serve`; no global seed is written, no per-slug seed path engaged.
2. Config behavior is identical to pre-vnc-041 (NFR-01, AC-06).

## Acceptance Criteria

| AC-ID | Criterion | Verification method |
|-------|-----------|---------------------|
| **AC-01** | A fresh container `serve` (no (a) present) writes an annotated global `config.toml` exposing the `DEFAULT_CONFIG_TOML` knobs at the EXACT path `project::ensure_data_directory` resolves — `paths.data_dir.join("config.toml")` = `/data/.unimatrix/<path-hash>/config.toml` in container mode (`base_dir = Some(/data)`), `~/.unimatrix/<path-hash>/config.toml` in local mode (`base_dir = None`), `<path-hash> = compute_project_hash()[..16]`. A subsequent boot with the file present does NOT overwrite it. Container `serve` path ONLY (OQ-4). | Integration test: container `serve` with empty `/data`, assert (a) exists at the resolved path-hash path with `DEFAULT_CONFIG_TOML` knobs; second boot asserts mtime/content unchanged. (FR-01, FR-02, FR-13) |
| **AC-02** | `register <slug>` writes `{base_dir}/{slug}/config.toml` — the exact path `resolve_slug_config` reads — and Feature A's resolver picks it up on the next restart with no hand-placement. | Test: run `register <slug>`, assert (b) exists at `base_dir.join(slug).join("config.toml")`; then run `resolve_slug_config` for that slug and assert it reads the seeded file (no hand-placement step). (FR-03, FR-04, FR-05) |
| **AC-03** | The per-slug seed's annotations match Feature A's classification field-for-field — per-slug-overlayable keys editable, global-locked keys INCLUDED but commented-out and marked "managed globally" — rendered from the registry. A classification change flips the annotation. | Test: parse the seeded (b); for every key in `PER_SLUG_CONFIG_CLASSIFICATION`, assert overlayable ⇒ editable entry, locked ⇒ commented-out + "managed globally". Plus a flip test: stub one key's disposition overlayable↔locked and assert the rendered annotation flips (proven, not restated). (FR-06, FR-07, FR-08; SR-02/03/07) |
| **AC-04** | A per-slug file setting a global-locked key (e.g. `[embedding].model`, a transport field, `permissive`) produces ONE `tracing::warn` PER IGNORED KEY, once per boot (not per request), naming the key + slug; the value remains ignored; daemon behavior is otherwise unchanged. | Test: per-slug file (b) sets a locked key; capture `tracing` output across repeated `resolve_slug_config` calls in one boot; assert exactly one WARN per ignored key naming key+slug, value still ignored, resolution output identical to the no-WARN path. (FR-09, FR-10, FR-11, FR-12; SR-06) |
| **AC-05** | Seeding never clobbers an existing target file (global or per-slug) — operator-authored config survives a re-register / re-boot. | Test: pre-place operator content in (a) and (b); run container `serve` + `register <slug>`; assert both files byte-for-byte unchanged. Uses the `create_new` no-clobber primitive. (FR-13, FR-14; SR-01) |
| **AC-06** | REGRESSION SENTINEL — a local STDIO / single-project deployment forces no new config file and its config behavior is byte-for-byte identical to pre-vnc-041. | Sentinel test: local `serve` (`base_dir = None`) with empty home `.unimatrix`; assert ZERO config files written and resolution behavior matches the pre-vnc-041 baseline. (NFR-01, FR-02; SR-04) |

## Constraints

- **C-01 — A→B one-way contract.** Annotations and WARN surface render FROM
  `PER_SLUG_CONFIG_CLASSIFICATION` / `is_per_slug_overlayable`; B never restates the split (SR-02,
  SR-07). Assumption to confirm with architect: `is_per_slug_overlayable` is callable from BOTH the
  `register` seed site and the `resolve_slug_config` WARN site.
- **C-02 — Skip-if-exists / atomic no-clobber.** Seeding is provisioning, never overwrite; use
  `OpenOptions::create_new`, never `fs::write`/`File::create` on a seed write (SR-01).
- **C-03 — WARN, not error.** R-13 adds a log line only; no new rejection path, no behavior change
  (SR-06).
- **C-04 — B stays off the shared (a)≡(c) file.** B writes file (b) only; the global seed (Goal 1) is
  `serve`-time, never inside `register`, so the (a)/(c) writer and the (b) writer never overlap
  (SR-05).
- **C-05 — Container-only is structural.** The global seed is gated by `base_dir = Some(/data)`, not a
  runtime flag (SR-04).
- **C-06 — Reuse existing machinery.** Existing config-write / `DEFAULT_CONFIG_TOML` template; no new
  serializer, no new merge logic.
- **C-07 — Restart-applies (vnc-038 ADR-007).** Seeding writes a file; overlay applies on restart.
- **C-08 — `register` is the sole per-slug provisioning point.** The eager seed assumes no slug is
  created by any path other than `register` (assumption to confirm).
- **C-09 — Workspace rules.** ≤500 lines/file, no stubs, no `.unwrap()` in non-test, `tracing` logs.

## Dependencies

- **Feature A (vnc-040 / #799)** — `PER_SLUG_CONFIG_CLASSIFICATION`, `is_per_slug_overlayable`,
  `resolve_slug_config` (`http_provision.rs`), `merge_configs` (`infra/config.rs`). The A→B contract
  source. Post-merge F1 fix `5e80febf` derives the `[knowledge]`-section exhaustiveness from the type.
- **vnc-038** — `register <slug>` flow (`projects.rs`, `projects/config_write.rs`),
  `ensure_project_stanza`, ADR-007 (restart-applies).
- **Existing config machinery** — `write_default_config_if_absent` (`infra/config.rs:4836`, the
  no-clobber `create_new` primitive to reuse), `DEFAULT_CONFIG_TOML` template, `handle_version`
  (`main.rs:1958`), `project::ensure_data_directory` (`project.rs:146`),
  `load_config_and_build_allowlist` (`main.rs:1806`), `PROJECT_CONFIG_NAME`.
- **`tracing`** — WARN-level logging, mirroring the existing `*_sha256` global-wins WARN precedent.

## NOT in Scope

- **Resolution / overlay merge logic** — shipped in Feature A (#799). B writes files + adds a WARN; it
  does not change how the overlay merges.
- **New config sections / new tunable knobs** — B seeds the EXISTING `UnimatrixConfig` surface only.
- **Hot-reload** — seeding writes a file; overlay applies on restart (vnc-038 ADR-007).
- **Overwriting operator-authored config** — skip-if-exists; never clobber a hand-placed file.
- **Rejecting a global-locked override** — R-13 is WARN, not error.
- **Writing the shared (a)/(c) path-hash file from `register`** — the `[[projects]]` stanza write
  stays vnc-038's; B writes only (b).
- **Multi-slug HTTP end-to-end harness** — infra-001 (#800), separate.
- **A global seed on the local single-project `serve` path** — explicitly excluded by OQ-4 / AC-06.

## Open Questions (for architect)

- **OQ-A (architect, confirm).** Is `is_per_slug_overlayable` / `PER_SLUG_CONFIG_CLASSIFICATION`
  callable from BOTH the `register` seed site and the `resolve_slug_config` WARN site without
  re-exporting or restating the split? (C-01; SCOPE-RISK assumption.) If not, the architect must
  expose it — restating the split is prohibited.
- **OQ-B (architect).** Rendering the "managed globally" annotation for **field-less** locks —
  `permissive` (no `UnimatrixConfig` field) and the embedding descriptor (only
  `inference.embedding_model_sha256`, no `[embedding].model`) — needs an explicit render strategy from
  the classification registry, since there is no struct field to template from (SR-03). How does the
  registry map such keys to seed lines?
- **OQ-C (architect).** WARN once-per-boot dedup state — where does the "already warned this
  key+slug this boot" set live (per-daemon, per-slug-resolver, process-global)? Must survive repeated
  `resolve_slug_config` calls within a boot but reset across boots (FR-11).
- **OQ-D (architect).** `register`/`handle_version`/`Command` signature audit before any change
  (SR-08): prefer additive call sites over `Command` variant shape changes to avoid breaking
  `matches!` arms and `main_tests.rs`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced vnc-040 ADR-004 (#5217, the A→B classification
  contract: verdict table + merge_configs + B's seed annotations all DERIVE from
  `PER_SLUG_CONFIG_CLASSIFICATION`; F1 post-merge correction makes `[knowledge]`-section exhaustiveness
  compiler-derived) and ADR-002 (#5206, byte-for-byte fallthrough by construction; names the
  seam-level WARN this feature delivers as the previously-"optional future" enhancement, and records
  the silent-ignore residual R-13 closes). Both confirm: B consumes the split at runtime, never
  restates it.
