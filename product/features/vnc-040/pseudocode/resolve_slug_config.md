# Component 2 — `resolve_slug_config` (per-slug overlay helper)

> NEW. **File home: `http_provision.rs`** (EXISTING module — see OVERVIEW module-home decision; no
> new `mod` wiring, no collision with the `main.rs` per-slug-loop edits).
> ADR-001 (#5209), ADR-003 (#5199), ADR-002 (#5206 §4 fallthrough). FR-01, FR-02, FR-06, FR-07,
> FR-08, FR-10. AC-02, AC-08a, AC-08b. R-01, R-03, R-10, R-11.

## Purpose

Sole owner of the per-slug overlay decision. Given the slug's dir and the daemon's already-resolved
global config, return either the global config unchanged (no file → byte-for-byte fallthrough) or a
merged-and-revalidated per-slug config (file present). Reuses `load_single_config` /
`validate_config` / `merge_configs` UNCHANGED — introduces no new load/merge/validate logic.

## Signature

```
fn resolve_slug_config<'a>(
    base_dir: &Path,
    slug: &ProjectSlug,
    global: &'a UnimatrixConfig,
) -> Result<Cow<'a, UnimatrixConfig>, ServerError>
```

- Return lifetime `'a` ties `Cow::Borrowed` to `global` (allocation-free fallthrough).
- `UnimatrixConfig` must be `Clone` for `Cow::Owned` and for the `merge_configs` call (it already is
  — `merge_configs` consumes owned values today).

## Pseudocode body

```
fn resolve_slug_config(base_dir, slug, global) -> Result<Cow<UnimatrixConfig>, ServerError>:

    // (1) Build the probe path — single-site, slug already allowlist-validated (no path escape).
    let path = base_dir.join(slug.as_str()).join("config.toml")    // matches http_provision.rs:159 derivation

    // (2) NO-FILE ARM — fallthrough sentinel (ADR-002 §4, FR-08, AC-02, R-03).
    //     Use a metadata probe, not a bare .exists() race, but DO NOT fail on NotFound:
    if not path_exists(path):                 // std::path::Path::exists(), or fs::metadata→is_file
        return Ok(Cow::Borrowed(global))      // NO merge, NO load, NO re-derivation. The global itself.

    // (3) FILE-PRESENT ARM — load → per-file validate → merge → post-merge validate (ADR-001 step 3).

    // 3a. Parse + hardening (REUSE — carries 64 KiB cap #2395 + #[cfg(unix)] 0o022 check, R-10):
    let slug_file: UnimatrixConfig =
        load_single_config(path)                         // -> Result<UnimatrixConfig, ConfigError>
            .map_err(|e| config_err(slug, path, e))?     // ServerError::Config naming the slug file

    // 3b. Per-file validation (FR-01, AC-08a):
    validate_config(&slug_file, path)
        .map_err(|e| config_err(slug, path, e))?

    // 3c. Merge — THIRD precedence layer (FR-01, FR-02). REUSE merge_configs UNCHANGED.
    //     LIVE SIGNATURE TAKES OWNED VALUES: merge_configs(global: UnimatrixConfig, project: UnimatrixConfig).
    //     `global` is borrowed here, so clone it once to feed the merge; `slug_file` is already owned.
    //     hash-pin global-wins (#4655) + instructions project-wins (config.rs:3863) ride INSIDE merge_configs.
    let merged: UnimatrixConfig = merge_configs(global.clone(), slug_file)

    // 3d. POST-MERGE re-validation (ADR-003, SR-01, FR-07, AC-08b, R-01) — MANDATORY, after merge,
    //     before return. Catches cross-field violations (fusion-weight sum, PPR, confidence,
    //     custom-preset, size bounds) that each file passes alone but the merge violates (#3905).
    validate_config(&merged, path)
        .map_err(|e| config_err(slug, path, e))?

    // 3e. Return owned merged config.
    Ok(Cow::Owned(merged))


// Error helper — every failure names the offending slug file, fails loud at STARTUP (NFR-05, R-11):
fn config_err(slug, path, e) -> ServerError:
    ServerError::Config(format!("per-slug config for slug '{}' at {}: {}", slug.as_str(), path.display(), e))
```

## State machine / lifecycle

Stateless, single-shot per slug. No persistent state, no async (reuses sync `load_single_config`).
Decision tree only: `no-file → Borrowed` | `file → load→validate→merge→validate → Owned | Err`.

## Initialization sequence

None — pure function. No constructor, no connection setup. Called per slug from the loop.

## Data flow

- **Inputs:** `base_dir: &Path`, `slug: &ProjectSlug`, `global: &UnimatrixConfig`.
- **Output:** `Cow<'a, UnimatrixConfig>` — `Borrowed(global)` (no file) | `Owned(merged)` (file).
- **Transformations:**
  - probe → `bool` (file present?)
  - TOML file → `UnimatrixConfig` (`load_single_config`)
  - `(global.clone(), slug_file)` → `merged` (`merge_configs` — consumes both)
  - `merged` validated in place (no transform), returned owned.
- **What crosses the boundary back to the loop:** the `Cow`. The loop derives the 7 overlayable
  values from `&*resolved`; it NEVER reads fields 0–2 (model handles / pool) from it.

## Error handling (FR-10, NFR-05, R-11)

| Failure | Source | Result |
|---------|--------|--------|
| Malformed TOML | `load_single_config` | `Err(ServerError::Config)` naming slug+path; startup-fatal |
| Oversized (>64 KiB) | `load_single_config` (#2395 cap) | `Err(...)` naming slug; startup-fatal (R-10) |
| World/group-writable (`mode()&0o022!=0`, unix) | `load_single_config` | `Err(...)` naming slug; startup-fatal (R-10) |
| Per-file invalid (unknown category, oversized instructions) | `validate_config(&slug_file)` | `Err(...)` naming slug (AC-08a) |
| Merge violates cross-field invariant | `validate_config(&merged)` | `Err(...)` naming slug (AC-08b, R-01) |
| No file | probe | `Ok(Cow::Borrowed)` — NOT an error |

- **No `.unwrap()` / `.expect()` anywhere** (NFR-06). All `?` propagate to the loop, which `?`s to
  startup. Failure is loud-at-startup, never request-time (#4583, R-11).
- The hash-pin divergence `tracing::warn` (AC-05, R-05) is emitted INSIDE `merge_configs`
  unchanged — `resolve_slug_config` does not add or suppress it.

## Reuse / non-rewrite obligations (flag to rust-dev)

- **SR-02 / R-02 (#4070):** `merge_configs`' inline `InferenceConfig {…}` literal is confirmed to
  list every field explicitly (no `..Default()` tail, `config.rs:3895-4260`). Re-confirm at impl
  time that the global→per-slug call shape exercises the SAME arm as global→project before trusting
  reuse. **No rewrite of `merge_configs`.**
- **Owned-arg correction (GAP):** brief/ARCHITECTURE §9 show `merge_configs(&..., &...)`; the LIVE
  fn takes OWNED values — hence `global.clone()` in 3c. One clone per slug-with-a-file, startup-only,
  negligible. Flag confirmed in OVERVIEW.

## Key test scenarios (hints for tester)

1. **AC-02 / R-03 #1-2** — no-file → `Cow::Borrowed(global)`; no `merge_configs` runs (construction
   proof); resolved == global value-equality.
2. **AC-08a / R-11** — each invalid input class (malformed TOML, unknown category, oversized
   instructions) → `ServerError::Config` at startup naming the slug file.
3. **AC-08b / R-01** — a per-slug file VALID per-file but whose merge violates the fusion-weight
   sum-of-six (or PPR/confidence/preset/size) → startup error from `validate_config(&merged)`. Prove
   per-file-only validation FAILS to catch it. Plus R-01 #3: assert `validate_config(&merged)` runs
   INSIDE the helper, after `merge_configs`, before return (construction proof).
4. **AC-03** — a file setting ONE overlayable key changes only that key; siblings fall through
   (per-key merge, not section-replace).
5. **AC-05 / R-05** — differing per-slug `*_sha256` → merged == global pin + `tracing::warn`
   (emitted by `merge_configs`, observed through this helper).
6. **R-10** — oversized (>64 KiB) and `#[cfg(unix)]` world/group-writable per-slug file rejected at
   load (the reused cap + permission check are EXERCISED on the per-slug path, not assumed).
7. **Edge** — empty / all-default per-slug file → merged == global (degenerate fallthrough, must not
   diverge from no-file semantics).
