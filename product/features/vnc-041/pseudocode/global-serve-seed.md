# C4 — Global serve-time seed (in `if config.http.enabled`)

> ADR-004. Crate: `unimatrix-server`, file `main.rs` (`tokio_main_daemon`). Drives AC-01/AC-06, addresses SR-04, R-01, R-02, R-11.

## Purpose

On a container `serve`, write the annotated global config (a) at the path-hash path when absent
(skip-if-exists), closing the #783 devex gap where only `init`/`version` seed (a) and `serve` only READS it.
The seed is **container-only by structural branch placement**: it lives INSIDE the `if config.http.enabled`
block, so the local STDIO/UDS `else` branch has NO seed call site (AC-06 holds by construction, R-01/R-02).

## Critical correction this component encodes (ADR-004, vs SCOPE/SR-04)

The gate is **`if config.http.enabled`**, NOT `ensure_data_directory`'s `base_dir` argument. Every live
`serve` call passes `base_dir = None` (main.rs:599/1347/1779/529/546) — a `base_dir`-keyed seed would
NEVER fire. Container-vs-local is decided by `dirs::home_dir()` (HOME=`/data` in the container) inside
`ensure_data_directory`. `if config.http.enabled` (main.rs:1011) is the real container/multi-project seam;
`require_http_for_projects` (main.rs:670) already ties `[[projects]]` to it.

## New / modified functions

No new function. C4 is a single additive call site inside the existing `if config.http.enabled` block in
`tokio_main_daemon`, placed BEFORE (or at the head of) the per-slug loop.

```
// main.rs, tokio_main_daemon — existing structure:
paths  = ensure_data_directory(None, None)?                 // base_dir = None on the live serve path
config = load_config_and_build_allowlist(&paths.data_dir)?  // reads (a) today; UNCHANGED

if config.http.enabled {                                    // ◄── STRUCTURAL container gate (main.rs:1011)
    // ◄── NEW (C4): seed (a) before the per-slug loop. Reuse the existing function with the
    //     existing template (NOT the C2 per-slug body — (a) is the daemon's own config).
    config::write_default_config_if_absent(&paths.data_dir.join("config.toml"), false);

    // ... existing per-slug loop (resolve_slug_config per slug — C5 WARN pass rides inside) ...
} else {
    // local STDIO/UDS path — NO seed call site exists here (AC-06 sentinel). Do NOT add one.
}
```

Path is exactly `paths.data_dir.join("config.toml")` — the same (a) path `handle_version` writes and
`load_config_and_build_allowlist` reads. `false` selects the no-clobber `create_new` branch (delegates to
C1's `write_if_absent`).

## State machine / lifecycle

None. One call per boot inside the HTTP branch.

## Initialization sequence

C4 runs during daemon boot, after `ensure_data_directory` (dir already created) and
`load_config_and_build_allowlist` (so `config.http.enabled` is resolved, env override applied), and before
the per-slug resolution loop. Order vs `handle_version`: irrelevant — `create_new` makes whichever writer
runs first the winner, the other a no-op (R-11).

## Data flow

- **Inputs:** `paths.data_dir` (from `ensure_data_directory`), `config.http.enabled` (the gate).
- **Output:** `()` — side effect: file (a) created at `paths.data_dir.join("config.toml")` iff absent AND
  `http.enabled`.
- **Transformations:** none — reuses `DEFAULT_CONFIG_TOML` verbatim via `write_default_config_if_absent`.
- The local `else` branch produces zero file writes (no call site).

## Why the global seed uses `DEFAULT_CONFIG_TOML`, not C2

File (a) is the daemon's OWN global config, not a per-slug overlay. Its annotations are the existing
template's. C2's classification legend is for the per-slug file (b) only. C4 reuses
`write_default_config_if_absent` (which writes `DEFAULT_CONFIG_TOML`) verbatim — the only new behavior is a
second caller on the HTTP branch (ADR-004).

## Error handling

- **Best-effort (C-09, R-10):** `write_default_config_if_absent` is infallible (warns internally), so the
  call needs no `?` and a write failure on (a) does NOT abort `serve` startup. `serve` tolerates an absent
  (a) (loads from defaults via `load_config_and_build_allowlist`).
- No `.unwrap()`, no panic (NFR-07).

## Key test scenarios (hints — see RISK-TEST-STRATEGY R-01, R-02, R-11)

- **R-01 #2 / AC-01:** `serve` with `config.http.enabled == true` and empty `/data` ⇒ (a) IS written at
  `paths.data_dir.join("config.toml")` exposing `DEFAULT_CONFIG_TOML` knobs; a second boot does NOT overwrite
  (mtime/content unchanged).
- **R-01 #1 / R-02 #1 / AC-06 (the regression sentinel — CRITICAL):** `serve` with
  `config.http.enabled == false` (local/STDIO) and empty home `.unimatrix` ⇒ ZERO config files written
  (file-count delta == 0, empirically, not by reading the branch).
- **R-01 #3 (structural):** the seed call is lexically inside the `if config.http.enabled` block; the
  `else` branch contains no seed call site.
- **R-01 #4:** with `base_dir = None` (live serve value) and `http.enabled == true`, the seed STILL fires —
  proving the gate is `http.enabled`, not `base_dir`.
- **R-02 #3 (negative control):** the same sentinel harness on the container path shows a non-zero delta —
  proving the sentinel detects writes and is not trivially passing.
- **R-11:** `init` then container `serve` ⇒ (a) written once by init, serve no-ops; and the reverse order —
  whichever runs first wins, the other no-ops (`create_new`).
- Container `serve` where (a) exists but is operator-edited ⇒ skip, edits survive (AC-05).

## Open questions / gaps

None. The gate, path, and reuse are fully specified by ADR-004 against verified call sites.
```
