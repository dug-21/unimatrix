## ADR-003: --force Flag With Stderr Warning, No Interactive Confirmation

### Context

`--force` drops all existing data before import (AC-02, AC-27). Users who accidentally run `--force` on a production database lose data with no undo (SR-04). The only safety net is a prior export.

Three options considered:
1. **Interactive confirmation prompt** (`--force` triggers "Are you sure? [y/N]"): Standard for destructive CLI operations. But breaks CI/CD pipelines and scripted workflows where stdin is unavailable. Would require `--force --yes` for automation.
2. **Stderr warning with no prompt**: Logs a prominent warning including the entry count being dropped. No interactivity. Simple, predictable, scriptable.
3. **Require `--force --yes` double-opt-in**: Explicit safety without interactive prompts. But adds parameter complexity for a subcommand that inherently operates on a fresh database (the primary use case is restore, not production mutation).

### Decision

Use option 2: `--force` emits a prominent warning to stderr showing the number of entries being dropped, then proceeds without interactive confirmation. The warning includes the data directory path and entry count.

Format:
```
WARNING: --force specified. Dropping N existing entries and all associated data in {data_dir}.
```

Rationale:
- Import's primary use case is backup/restore into a fresh or known database. Users who specify `--force` have made an explicit decision.
- The scope excludes merge/append (non-goal). `--force` is the only path to import into a non-empty database.
- Interactive confirmation is hostile to CI/CD and scripted workflows, which are a primary use case for the Platform Hardening milestone.
- The export subcommand provides the safety net: users should export before importing with `--force`.

### Consequences

- **Easier**: `--force` works in CI/CD without stdin gymnastics. Simple, predictable.
- **Easier**: No parameter complexity (`--yes`, `--confirm`, etc.).
- **Harder**: A user who typos `--force` on a production database loses data with only a stderr warning. Mitigated by: (a) `--force` is a long flag, reducing accidental use, (b) the warning includes the entry count, giving the user a chance to notice in manual workflows, (c) export provides the recovery path.
