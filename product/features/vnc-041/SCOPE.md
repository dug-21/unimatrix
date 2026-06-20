# vnc-041 — Config Seeding + Seam-Level WARN (C17 / vnc-040 Feature B)

> Feature B of the vnc-040 split (uni-zero 2026-06-19). Delivers capability
> **C17 — Installation provisions the proper config in place** (Unimatrix #5214).
> Tracked by GH **#801**. Feature A (vnc-040 / #799) delivered **C6** resolution; this is the
> seeding half that makes per-slug config discoverable and editable, plus the R-13 WARN that
> stops a hand-edited global-locked key from vanishing silently.

## Problem Statement
Per-slug config is now **resolvable** (Feature A) but not **provisioned**. Two gaps:

1. **Nothing seeds the files.** A fresh container `serve` never writes the annotated global
   `config.toml` — only `handle_version` (`init`/`version`) emits `DEFAULT_CONFIG_TOML` (the #783
   secondary observation). And `register <slug>` (vnc-038, `projects.rs` / `projects/config_write.rs`)
   creates the slug store + `[[projects]]` stanza but **no per-slug `config.toml`**. So an operator
   can't discover or edit cloud-global or per-project config without reverse-engineering the format
   and hand-placing files at `{base_dir}/{slug}/config.toml`.

2. **A hand-edited global-locked key vanishes silently (R-13).** Feature A's resolver
   (`resolve_slug_config`, `http_provision.rs`) overlays only per-slug-overlayable keys; a per-slug
   file that sets a global-locked section (transport, the `[embedding]` descriptor, `permissive`) is
   **ignored with no signal** — only an `*_sha256` pin divergence warns today. On hand-authored
   config (exactly Feature A's model: operator hand-places the file) silent-ignore is a support-ticket
   generator: the operator sets a key, it does nothing, nothing tells them why.

Who is affected: operators of multi-project (cloud / HTTP) deployments self-editing config.
Why now: Feature A shipped resolution; resolution without provisioning is invisible, and the silent
seam is felt the moment an operator hand-edits.

## Goals
1. **Global seed.** A fresh container `serve` writes an annotated global `config.toml` at the global
   path (showing the `DEFAULT_CONFIG_TOML` knobs) when one does not already exist. Independently
   buildable — does NOT depend on C6 (closes the #783 devex gap directly).
2. **Per-slug seed.** `register <slug>` writes an annotated `config.toml` at
   `{base_dir}/{slug}/config.toml` — the EXACT path Feature A's `resolve_slug_config` reads —
   seeded from defaults, when one does not already exist.
3. **Annotations render from Feature A's classification (the one-way A→B contract).** The seed's
   per-key annotations (which keys are per-slug-overlayable vs global-locked) RENDER from vnc-040's
   `PER_SLUG_CONFIG_CLASSIFICATION` registry / `is_per_slug_overlayable` (`infra/config.rs`) — B
   consumes the split, A owns it. B restates nothing; if A's classification changes, B's seed
   annotations follow.
4. **Seam-level WARN (R-13).** When `resolve_slug_config` encounters a per-slug file that sets a
   global-locked key (transport, `[embedding]` descriptor, `permissive`), emit a `tracing::warn`
   naming the ignored key + slug, instead of ignoring it silently. **WARN only — no rejection, no
   behavior change** beyond the log line. (The `*_sha256` divergence WARN already exists; this
   generalizes the signal to the rest of the locked surface.)
5. **Regression sentinel.** A local STDIO / single-project deployment's config behavior is
   byte-for-byte unchanged — no new file forced, the existing local seed path untouched.

## Non-Goals
- **Resolution / overlay logic** — shipped in Feature A (#799). B writes files + adds a WARN; it does
  not change how the overlay merges.
- **New config sections / new tunable knobs** — B seeds the EXISTING `UnimatrixConfig` surface.
- **Hot-reload** — seeding writes a file; the overlay still applies on restart (vnc-038 ADR-007).
- **Overwriting operator-authored config** — if a target file already exists, seeding must NOT
  clobber it (an operator may have hand-placed it for Feature A).
- **Rejecting a global-locked override** — R-13 is WARN, not error; rejection is explicitly out.
- **Multi-slug HTTP end-to-end harness** — infra-001 (#800), separate.

## Background Research

### Three-file disambiguation (read this first — these are routinely conflated)
This feature touches THREE config files. Two of them are the **same physical file**; one is distinct.
Confirmed against the code:

- **(a) GLOBAL annotated `config.toml`** — the `DEFAULT_CONFIG_TOML` seed target.
  Path: `{base}/.unimatrix/{path-hash}/config.toml` (the daemon path-hash dir).
  Resolution: `project::ensure_data_directory` (`unimatrix-engine/src/project.rs:146`) computes
  `paths.data_dir = {.unimatrix base}/{compute_project_hash()[..16]}`; the file is
  `paths.data_dir.join("config.toml")`. Container vs local is decided by `ensure_data_directory`'s
  `base_dir` arg: `None` ⇒ `dirs::home_dir()/.unimatrix` (local), `Some(/data)` ⇒ `/data/.unimatrix`
  (container). **Note (ADR-004, #5238):** the global-SEED gate is `if config.http.enabled`, not this
  `base_dir = Some(/data)` arg (live `serve` always passes `None`); the path resolution above stands.
  Writer: `write_default_config_if_absent` (`infra/config.rs:4836`), called **only** by
  `handle_version` (`main.rs:1958`, i.e. `init`/`version`). `serve` only READS it
  (`load_config_and_build_allowlist`, `main.rs:1806`) — never writes it (#783). **Goal 1, AC-01,
  AC-05 act on this file.**
- **(c) PROJECTS REGISTRY / `[[projects]]` stanza** — **this is the SAME physical file as (a).**
  Writer: `ensure_project_stanza` (`projects/config_write.rs:35`) writes to
  `config_data_dir.join("config.toml")` where `config_data_dir = paths.data_dir` (the path-hash dir;
  `projects.rs:182`). Called by `register <slug>` (`projects.rs:305`/`342`, vnc-038 ADR-007), atomic
  read-modify-write that PRESERVES all other sections. So global knobs (a) and `[[projects]]` routing
  (c) coexist in one path-hash `config.toml`. **No goal/AC in this feature writes (c)** — it is the
  vnc-038 prior art whose write site `register` already owns; vnc-041's per-slug seed (Goal 2) runs
  ALONGSIDE it in the same `register` flow but targets the distinct file (b).
- **(b) PER-SLUG `config.toml`** — the distinct file Feature A's resolver reads.
  Path: `{base_dir}/{slug}/config.toml` where `base_dir = paths.data_dir.parent()` = `{.unimatrix
  base}` — so the slug dir is a **SIBLING of the path-hash dir**, not inside it. Container:
  `/data/.unimatrix/{slug}/config.toml`; local: `~/.unimatrix/{slug}/config.toml`. Reader:
  `resolve_slug_config` (`http_provision.rs:310`, `path = base_dir.join(slug).join(PROJECT_CONFIG_NAME)`,
  `PROJECT_CONFIG_NAME = "config.toml"`). **Nothing writes (b) today** — that gap is Goal 2 / AC-02;
  Goals 3 (annotations) and 4 (WARN), AC-03/AC-04 act on this file's content + resolution.

### Other findings
- **Global-seed site:** `DEFAULT_CONFIG_TOML` is written only by `handle_version`; the container
  `serve` path never emits it (#783). The global path is the daemon path-hash `config.toml` (file (a)
  above).
- **Per-slug seed site:** `register <slug>` (`projects.rs` / `projects/config_write.rs`, vnc-038)
  already creates the slug store + `[[projects]]` stanza — the natural place to also write
  `{base_dir}/{slug}/config.toml`. `base_dir = paths.data_dir.parent()`; the slug dir is a sibling
  of the path-hash dir (vnc-040 SCOPE §"Per-slug config file location").
- **A→B contract source:** Feature A's `PER_SLUG_CONFIG_CLASSIFICATION` registry + the AC-11
  drift-guard (now derived from the `[knowledge]` struct after the F1 fix, commit `5e80febf`) is the
  single source of truth for per-slug-vs-global. B renders seed annotations from it.
- **WARN site:** `resolve_slug_config` in `http_provision.rs` — the same helper Feature A added.
  The locked surface to warn on = everything `is_per_slug_overlayable` returns false for (transport,
  `[embedding]` descriptor, `permissive`); the `*_sha256` global-wins WARN is the existing precedent
  to mirror.
- **Reuse:** seeding writes via the existing config-write machinery (`config_write.rs` /
  `DEFAULT_CONFIG_TOML` template); no new serializer.

## Proposed Approach
1. Container `serve` startup (OQ-4 RESOLVED — container path ONLY): if the global `config.toml` is
   absent at the resolved path-hash path, write the annotated `DEFAULT_CONFIG_TOML` template there
   (skip-if-exists). Local single-project `serve` forces no new file (regression sentinel, step 4).
2. `register <slug>` (OQ-2 RESOLVED — eager): after creating the store + stanza, if
   `{base_dir}/{slug}/config.toml` is absent, write an annotated per-slug seed (skip-if-exists).
   Annotations render from Feature A's classification registry: per-slug-overlayable keys editable,
   global-locked keys INCLUDED but commented-out + marked "managed globally" (OQ-1 RESOLVED).
3. `resolve_slug_config` (OQ-3 RESOLVED — per key, once per boot): when a per-slug file sets a key
   for which `is_per_slug_overlayable` is false, emit one `tracing::warn` per ignored locked key
   (once per boot, not per request) naming the key + slug; continue (the value is already ignored —
   this only adds the signal).
4. Hold the regression sentinel: gate that a single-project / local-STDIO config path forces no new
   file and behaves byte-for-byte as today.

## Acceptance Criteria
- AC-01: A fresh container `serve` (no global config present) writes an annotated global
  `config.toml` exposing the `DEFAULT_CONFIG_TOML` knobs at the EXACT path-hash path resolved by
  `project::ensure_data_directory` — i.e. `paths.data_dir.join("config.toml")`, which is
  `/data/.unimatrix/<path-hash>/config.toml` in container mode (`base_dir = Some(/data)`) and
  `~/.unimatrix/<path-hash>/config.toml` in local mode (`base_dir = None` ⇒ `dirs::home_dir()`),
  where `<path-hash>` = `compute_project_hash()[..16]`. A subsequent boot with the file present does
  NOT overwrite it (skip-if-exists). Per OQ-4 (RESOLVED): this seed is on the container `serve` path
  ONLY — local single-project `serve` forces no new file (see AC-06).
  **Gate correction (ADR-004, #5238):** the global-seed gate is `if config.http.enabled`, NOT the
  `base_dir = Some(/data)` argument — every live `serve` passes `base_dir = None`, so that framing is
  superseded; the resolved paths above are unchanged.
- AC-02: `register <slug>` writes `{base_dir}/{slug}/config.toml` — the exact path
  `resolve_slug_config` reads — and Feature A's resolver picks it up on the next restart with no
  hand-placement.
- AC-03: The per-slug seed's annotations match Feature A's classification field-for-field —
  per-slug-overlayable keys marked editable, global-locked keys INCLUDED but commented-out and marked
  "managed globally" (OQ-1 RESOLVED) — rendered from the registry (a classification change flips the
  annotation; proven, not restated).
- AC-04: A per-slug file setting a global-locked key (e.g. `[embedding].model`, a transport field,
  `permissive`) produces ONE `tracing::warn` PER IGNORED KEY, emitted once per boot (not per request;
  OQ-3 RESOLVED), naming the key + slug; the value remains ignored; daemon behavior is otherwise
  unchanged (WARN only, no rejection).
- AC-05: Seeding never clobbers an existing target file (global or per-slug) — operator-authored
  config survives a re-register / re-boot.
- AC-06: REGRESSION SENTINEL — a local STDIO / single-project deployment forces no new config file
  and its config behavior is byte-for-byte identical to pre-vnc-041.

## Constraints
- **A→B one-way contract:** annotations render FROM `PER_SLUG_CONFIG_CLASSIFICATION`; B never
  restates the split.
- **Skip-if-exists:** seeding is provisioning, never overwrite.
- **WARN, not error:** R-13 adds a log line only; no new rejection path, no behavior change.
- **Reuse:** existing config-write / template machinery; no new serializer or merge.
- **Restart-applies (vnc-038 ADR-007):** seeding writes a file; overlay still applies on restart.
- **Rust workspace rules:** ≤500 lines/file, no stubs, no `.unwrap()` in non-test, `tracing` for logs.

## Resolved Decisions (human concurred, 2026-06-20 — locked pre-design)
- **OQ-1 — RESOLVED. Global-locked keys ARE included in the per-slug seed**, rendered commented-out
  with a "managed globally" annotation (not omitted). Showing them documents the per-slug/global
  boundary for the operator. (Drives AC-03.)
- **OQ-2 — RESOLVED. The per-slug seed is EAGER at `register <slug>`** — written alongside the slug
  store + the `[[projects]]` stanza, in the same `register` flow (not lazy on first `serve`). Keeps
  all provisioning in one place. (Drives AC-02.)
- **OQ-3 — RESOLVED. WARN granularity is PER IGNORED KEY, emitted once per boot** (not per request).
  One `tracing::warn` per ignored locked key naming key + slug; no per-request log spam. (Drives
  AC-04.)
- **OQ-4 — RESOLVED. The global seed belongs to the container `serve` path ONLY.** Local
  single-project `serve` forces NO new config file — ties to the regression sentinel (AC-06).
  (Drives AC-01, AC-06.)

## Tracking
GH Issue **#801** (C17 / vnc-040 Feature B). Capability **C17** (Unimatrix #5214); pairs with
**C6** (#5148, resolution, shipped Feature A / #799). This SCOPE.md feeds the design session
(architecture + spec).
