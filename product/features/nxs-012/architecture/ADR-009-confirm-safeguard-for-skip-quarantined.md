## ADR-009: --confirm Safeguard for --skip-quarantined Export

### Context

`--skip-quarantined` produces a non-exact export: the output file intentionally omits entries and their dependents. A user who runs this flag without understanding the consequences may produce an export they later expect to be a complete backup, leading to data loss when the original database is no longer available.

The import CLI established a precedent with `--force` (nan-002 ADR-003, Unimatrix #1145): destructive operations use a CLI flag (not interactive confirmation) and emit a stderr warning. Interactive prompts were explicitly rejected because they break CI/CD pipelines and scripted backup workflows.

The same concern applies here: `--skip-quarantined` may be used in automated export scripts (cron backups, pre-migration snapshots). An interactive "are you sure?" prompt would break these workflows.

Three alternatives were considered:

**Option A: Require `--confirm` flag alongside `--skip-quarantined`.** If `--skip-quarantined` is present without `--confirm`, abort with a clear message. No interactive prompt. CI scripts add `--confirm` explicitly.

**Option B: Interactive stdin prompt.** Breaks scripted/automated usage (SR-09). Inconsistent with nan-002 ADR-003 precedent.

**Option C: Stderr warning only (no confirmation).** Matches `--force` behavior exactly. However, `--force` is a single-flag operation with a well-understood meaning ("drop data"). `--skip-quarantined` is more subtle -- users may not realize the export is incomplete. A slightly higher bar is appropriate.

### Decision

Option A. When `--skip-quarantined` is passed without `--confirm`, the export aborts immediately with:

```
Error: --skip-quarantined produces a filtered export (quarantined entries and their
dependents are excluded). The export file will NOT be an exact copy of the database.
Add --confirm to acknowledge this and proceed.
```

When both `--skip-quarantined` and `--confirm` are present:
1. Proceed with the filtered export.
2. Emit a stderr summary after export: "Skipped N quarantined entries and M dependent rows."

When `--confirm` is present without `--skip-quarantined`, it is silently ignored (no error). This avoids breaking scripts that unconditionally pass `--confirm`.

**Implementation location**: The check happens in `run_export_inner` before opening the database, immediately after CLI argument parsing. No database access is needed for this validation.

```rust
// In run_export_inner, after argument parsing:
if skip_quarantined && !confirm {
    return Err("--skip-quarantined produces a filtered export ...".into());
}
```

**CLI threading**: `confirm: bool` threads through `run_export` -> `run_export_inner` alongside `skip_quarantined: bool`. Both are `clap` boolean flags on the export subcommand.

### Consequences

- Automation-compatible: scripts add `--confirm` explicitly, no stdin interaction
- Consistent with nan-002 ADR-003 precedent (CLI flags, not interactive prompts)
- The error message explains what `--skip-quarantined` does and what to do about it -- users learn the semantics before proceeding
- `--confirm` without `--skip-quarantined` is a no-op, preventing script breakage
- The check is a pure argument validation -- no database access, no file I/O, instant failure path
- Slightly higher confirmation bar than `--force` (two flags vs one) is appropriate given the subtlety of the operation: `--force` is obviously destructive, `--skip-quarantined` is a non-obvious data reduction
