# C1 — Seed-write primitive (`write_if_absent`, `write_default_config_if_absent` delegate)

> ADR-001. Crate: `unimatrix-server`, file `infra/config.rs`. Drives AC-05, addresses SR-01 / R-03, R-09, R-11.

## Purpose

Provide ONE TOCTOU-safe, no-clobber, content-parameterized seed-write primitive used by BOTH seeds:
the global seed (a) via the existing `write_default_config_if_absent`, and the per-slug seed (b) via C3.
"Skip-if-exists" is guaranteed atomically by `OpenOptions::create_new(true)` (O_EXCL) — there is NO
check-then-write window that could truncate an operator-authored file.

## Current state (verified, config.rs:4836–4902)

`write_default_config_if_absent(path, force)` already contains the exact logic to extract:
parent `create_dir_all` → if `!force`, `OpenOptions::new().write(true).create_new(true).open(path)` →
`write_all(DEFAULT_CONFIG_TOML.as_bytes())` → `AlreadyExists` is a silent no-op → any other error is a
`tracing::warn` and return. The `force = true` branch (`fs::write` overwrite) is used by `handle_version`
with `force` and MUST be preserved on `write_default_config_if_absent`.

## Decision: extract the no-force path into `write_if_absent`, delegate

`write_if_absent(path, content)` owns the parent-create + `create_new(true)` + `AlreadyExists`-noop +
warn-and-continue logic, parameterized on `content`. `write_default_config_if_absent`'s `force = false`
branch delegates to it with `DEFAULT_CONFIG_TOML`. The `force = true` branch stays in
`write_default_config_if_absent` unchanged (it is an intentional overwrite, NOT a seed — never delegate it).

## New / modified functions

### `write_if_absent` (NEW — module-private)

```
fn write_if_absent(path: &Path, content: &str):
    // 1. Determine parent; cannot-determine is warn-and-return (mirrors existing).
    parent = match path.parent():
        Some(p) => p
        None    => tracing::warn!(path, "write_if_absent: cannot determine parent directory; skipping")
                   return

    // 2. Best-effort parent create. Failure is warn-and-return.
    if create_dir_all(parent) is Err(e):
        tracing::warn!(path, error=e, "write_if_absent: failed to create parent directory; skipping")
        return

    // 3. ATOMIC no-clobber open. NO path.exists() precheck — O_EXCL IS the guard (NFR-04, R-03 #5).
    match OpenOptions::new().write(true).create_new(true).open(path):
        Ok(file) =>
            if file.write_all(content.as_bytes()) is Err(e):
                tracing::warn!(path, error=e, "write_if_absent: write_all failed")
            else:
                tracing::info!(path, "config seed written")
        Err(e) if e.kind() == ErrorKind::AlreadyExists =>
            // SKIP-IF-EXISTS: file already present — silent no-op, operator content survives (AC-05).
            // No log (matches existing behavior; avoids boot-time noise on every re-boot).
            ()
        Err(e) =>
            tracing::warn!(path, error=e, "write_if_absent: open failed")
    // NOTE: returns () — best-effort, NEVER an error. No `?`, no `.unwrap()`.
```

### `write_default_config_if_absent` (MODIFIED — delegate; signature UNCHANGED)

```
pub fn write_default_config_if_absent(path: &Path, force: bool):
    if force:
        // UNCHANGED intentional-overwrite path (handle_version --force). NOT a seed — keep fs::write.
        match fs::write(path, DEFAULT_CONFIG_TOML):
            Ok(())  => tracing::info!(path, "default config.toml written (force)")
            Err(e)  => tracing::warn!(path, error=e, "write_default_config_if_absent: write failed")
        // (parent create for the force branch stays as today, before the match)
    else:
        // DELEGATE the no-clobber seed write to the shared primitive.
        write_if_absent(path, DEFAULT_CONFIG_TOML)
```

Keep the parent-determination + `create_dir_all` for the `force` branch where it is today; `write_if_absent`
does its own parent-create for the no-force path. (Do not double-create; the force branch keeps its own
parent handling, the else branch defers entirely to `write_if_absent`.)

## State machine / lifecycle

None. Pure stateless write helpers.

## Initialization sequence

None — free functions, no construction.

## Data flow

- **Inputs:** `path: &Path` (absolute target), `content: &str` (seed body — `DEFAULT_CONFIG_TOML` for the
  global seed; C2's `render_per_slug_seed_toml()` output for the per-slug seed).
- **Output:** `()`. Side effect: file created at `path` iff absent.
- **Transformations:** none — bytes of `content` written verbatim.

## Error handling

- **Best-effort, infallible signature.** Every failure (parent undetermined, `create_dir_all` fails,
  `write_all` fails, non-`AlreadyExists` open error) is a `tracing::warn` then return `()` (C-09, R-10).
- **`AlreadyExists` is success, not error** — silent no-op (skip-if-exists, AC-05, R-03 #4).
- **No `?`, no `.unwrap()`, no panic** anywhere in the primitive (NFR-07).
- The caller (C3/C4) never observes a result — provisioning never gates the command/daemon.

## Key test scenarios (hints for the tester — see RISK-TEST-STRATEGY R-03, R-09, R-11)

- **R-09 regression (CRITICAL for this extraction):** the four existing `write_default_config_if_absent`
  tests at config.rs:11262–11346 (create-when-absent, no-overwrite-no-force, force-overwrite,
  silent-on-write-fail) STILL pass after delegation — same observable behavior.
- **R-03 #1/#2:** pre-place operator content at `path`; call `write_if_absent` → file byte-for-byte
  unchanged (skip-if-exists).
- **R-03 #4:** `AlreadyExists` is a silent no-op (no error, no overwrite).
- **R-03 #5:** no `path.exists()` precheck — the existence guard is the single `create_new` open
  (structural/grep assertion: no `.exists()` before the open).
- **R-10:** write into a non-writable parent → returns `()`, logs warn, no panic.
- **R-11:** call twice → second call no-ops regardless of order (idempotent via `create_new`).
- Parent dir absent → `write_if_absent` creates it, then writes.

## Open questions / gaps

None. The extraction is mechanical against verified existing code (config.rs:4836–4902).
```
