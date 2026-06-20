# Gate 3a Report: vnc-041

> Gate: 3a (Design Review)
> Date: 2026-06-20
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | C1–C5 map 1:1 to the architecture component table; crate/file/ADR all consistent; interfaces match §7 integration surface. |
| 2. Specification coverage | PASS | FR-01..FR-15, NFR-01..NFR-07, AC-01..AC-06 all have corresponding pseudocode; no scope additions. |
| 3. Risk coverage | PASS | Every R-01..R-14 maps to test-plan scenarios; flip test + empirical sentinel + negative control all planned. |
| 4. Interface consistency | PASS | OVERVIEW shared types match per-component usage and live `infra/config.rs`; data flow coherent across files. |
| 5. Knowledge stewardship | PASS | All design-phase agent reports carry `## Knowledge Stewardship` with `Queried:` + `Stored:`/reasoned-decline entries. |

## Load-bearing invariant verification (spawn-prompt focus)

| Invariant | Status | Evidence |
|-----------|--------|----------|
| C1 `create_new(true)`, AlreadyExists no-op, NO `path.exists()` precheck, force=true overwrite arm preserved | PASS | seed-write-primitive.md §"write_if_absent" step 3 "NO path.exists() precheck — O_EXCL IS the guard"; `AlreadyExists =>` silent no-op; `write_default_config_if_absent` keeps the `if force: fs::write` arm unchanged; test plan keeps `test_write_default_config_overwrites_with_force` green (seed-write-primitive.md test plan §R-09). |
| C2 EXHAUSTIVE `OverlayDisposition` match, no catch-all; field-less locks render no editable knob | PASS | per-slug-seed-renderer.md: `match entry.disposition` with both variants and explicit "NO `_ =>` arm"; §"Field-less locks" — legend keyed on registry `key` string + disposition, "no editable knob emitted." |
| C3 seeds ONLY (b) at State B AND State C, never (a)≡(c); best-effort | PASS | per-slug-seed-writer.md state table (B=YES, C=YES, A=NO), §"Isolation invariant" writes (b) only via `per_slug_data_dir`, never `config_data_dir`; `ensure_project_stanza` unchanged; best-effort (no `?`). |
| C4 global seed gated by `if config.http.enabled` (NOT base_dir); local else has NO seed call | PASS | global-serve-seed.md: seed call lexically inside `if config.http.enabled`, "else branch — NO seed call site exists here." |
| C5 WARN from `is_per_slug_overlayable==false`, WARN-only, content-free (key+slug), once per locked key per boot | PASS | seam-warn.md: derives from `!is_per_slug_overlayable(&key)`; "CONTENT-FREE ... NEVER the operator's set VALUE"; §"WARN-only invariant" return type/merge/errors unchanged; §dedup once-per-resolution==once-per-boot. |
| Test plans: A→B flip test (one flip moves BOTH C2 annotation and C5 WARN) | PASS | OVERVIEW.md §4 "two proven-not-restated centerpieces" #1; planned in both per-slug-seed-renderer.md (`test_render_legend_flips_when_disposition_flips`) and seam-warn.md (`test_resolve_warn_behavior_flips_when_disposition_flips`). |
| Test plans: AC-06 empirical zero-files sentinel WITH negative control (container delta>0) | PASS | global-serve-seed.md: `test_local_serve_writes_zero_new_config_files` (delta==0) + MANDATORY `test_container_serve_writes_one_config_file_negative_control` (delta>0). |

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**: The pseudocode OVERVIEW component table (C1 `infra/config.rs`, C2 `infra/config.rs`,
C3 `projects.rs`, C4 `main.rs`, C5 `http_provision.rs`) is byte-consistent with ARCHITECTURE §3
component breakdown and §7 integration surface. The A→B one-way contract (Feature A owns
`PER_SLUG_CONFIG_CLASSIFICATION`; B consumes via `is_per_slug_overlayable` at runtime) is restated
identically in OVERVIEW §"Shared types" and §"Cross-cutting constraints." ADR-001..005 are referenced
per-component and match the ADR index (ARCHITECTURE §8). The ADR-004 correction (gate is
`config.http.enabled`, not `base_dir`) is carried through C4 and the risk strategy verbatim. No
component boundary, interface, or technology choice diverges from the approved architecture.

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: Every functional requirement has corresponding pseudocode:
FR-01/02 → C4; FR-03/04/05 → C3; FR-06/07/08 → C2; FR-09/10/11/12 → C5; FR-13/14 → C1;
FR-15 (reuse template) → C2/C4 reuse of `DEFAULT_CONFIG_TOML`. NFRs are addressed:
NFR-01 (zero-files sentinel) → C4 test plan; NFR-03 (single classification consumption) → C2+C5 both
bind to the registry at runtime; NFR-04 (atomic no-TOCTOU) → C1 `create_new`, no precheck;
NFR-07 (≤500 lines, no stubs, no `.unwrap()`) → cross-cutting constraints in every file. No scope
additions: no new serializer, no new config knob, no new merge logic — confirmed against the
Non-Goals list. FR-08's "global-locked keys INCLUDED but commented-out" is satisfied by the legend
(commented) plus the reused template body, whose field values are already all comment lines (verified
against live `DEFAULT_CONFIG_TOML`, config.rs:4605+) — no uncommented editable value is emitted for a
locked key, so no contradiction with FR-08.

### Check 3 — Risk coverage
**Status**: PASS
**Evidence**: The test-plan OVERVIEW §3 risk→test table maps all 14 risks (R-01..R-14) to owning
component plans; the 5 Critical risks (R-01..R-05) each have explicit empirical scenarios. The two
mandatory "proven, not restated" centerpieces are both planned (A→B flip in C2+C5; AC-06 empirical
sentinel + negative control in C4). Integration scenarios (register→resolve round-trip, R-05) and
edge cases (empty (b), unknown-key WARN, malformed-TOML degradation) are present. Risk priorities are
reflected in plan emphasis (Critical risks carry the load-bearing AC anchors). Security risks
(hostile slug rejection, content-free WARN, double-parse no-crash) are explicitly planned.

### Check 4 — Interface consistency
**Status**: PASS
**Evidence**: OVERVIEW §"Shared types" (`PER_SLUG_CONFIG_CLASSIFICATION` :4447, `ConfigKeyClass`
:4428, `OverlayDisposition` :4413, `is_per_slug_overlayable` :4552, `DEFAULT_CONFIG_TOML` :4605)
matches the live source verbatim (verified). C2's renderer and C5's WARN both consume the same
registry — no second copy of the split. C3's path construction (`per_slug_data_dir(base_dir, slug)
.join("config.toml")`) is the same join C5/the resolver reads, ensuring the round-trip. The
`write_if_absent` visibility (`pub(crate)`) needed by C3 is consistently flagged in both C1 and C3
pseudocode as a 3b confirmation item — no contradiction, only a modifier to settle. Data flow
(serve → C4 seed → per-slug loop → C5 WARN; register State B/C → C3 seed) is coherent across files.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: All design-phase agent reports contain a `## Knowledge Stewardship` block.
- architect (active-storage): `Queried:` context_briefing; `Stored:` entries #5235–#5239 + edges. PASS.
- risk-strategist (RISK-TEST-STRATEGY.md §Knowledge Stewardship): `Queried:` context_search;
  `Stored:` "nothing novel to store -- {reason}" with a stated reason. PASS.
- pseudocode (read-only): `Queried:` context_search (with relevance assessment); declined-store with
  reason. PASS.
- testplan: `Queried:` context_briefing + context_search; reasoned decline. PASS.
No missing blocks; all "nothing novel" declines carry a reason. No WARN.

## Minor observations (non-blocking, for 3b — not FAIL/WARN)

1. **C5 `flatten_present_keys` granularity for table-shaped locks.** The registry classifies `tls`
   and `http` as bare section names, but `flatten_present_keys` emits `section.subkey` for any table.
   A per-slug file setting `[tls]`/`[http]` would yield keys like `tls.cert_path`, which
   `is_per_slug_overlayable` treats as unknown → conservative `false` → WARN still fires (correct
   outcome per FR-09 + the unknown-key edge case). The WARN names the subkey rather than the section.
   This is a granularity nuance, not a correctness gap; the pseudocode already flags it as a 3b
   confirmation item ("confirm against the registry in 3b that no classified key needs deeper
   nesting"). Appropriately deferred — does not block 3a.

2. **C4 harness reachability** (function-level vs full daemon boot) and **C5 dedup-map necessity**
   are flagged open questions in the test-plan report; both are resolvable in 3b and do not affect
   the mandatory empirical assertions, which are required at any harness depth.

## Rework Required
None.

## Scope Concerns
None.
