# Pseudocode: jsonl-removal

## Shell Hooks

### File: .claude/hooks/observe-pre-tool.sh

Remove lines:
```bash
OBS_DIR="${HOME}/.unimatrix/observation"
mkdir -p "$OBS_DIR"
echo "$RECORD" >> "${OBS_DIR}/${SESSION_ID}.jsonl"
```

Keep: INPUT reading, SESSION_ID extraction, RECORD construction, exit 0.
The hook still reads stdin and exits 0. Currently it only writes to JSONL -- after col-012 the hook becomes a no-op stub (events go through UDS RecordEvent from the CLI tool, not from these hooks directly).

Actually looking at the hooks more carefully -- these hooks ONLY write JSONL. They do NOT forward to UDS. The UDS forwarding happens in the `unimatrix-server hook` CLI path. So after removing JSONL writes, these hooks become empty stubs.

### Revised approach for all 4 hook scripts:

Each hook becomes a minimal stub:
```bash
#!/usr/bin/env bash
# {HookType} hook: events persisted via UDS RecordEvent handler.
# JSONL write path removed (col-012).
# Exits 0 unconditionally (FR-01.4).
exit 0
```

## File: crates/unimatrix-observe/src/parser.rs

### Option A: Remove entirely
If `parse_timestamp` is not used outside parser.rs tests.

### Option B: Retain parse_timestamp only
Check if parse_timestamp is used by other modules. If yes, move it to a utility module or keep parser.rs with only parse_timestamp.

Grep for `parse_timestamp` usage outside parser.rs:
- lib.rs re-exports it
- Check if any other crate uses it

Decision: Check at implementation time. If only re-exported but not consumed, remove entirely.

Remove:
- `RawRecord` struct
- `parse_line` function
- `parse_session_file` function
- All tests except parse_timestamp tests (if retained)

## File: crates/unimatrix-observe/src/files.rs

### Remove entirely

Functions to remove:
- `observation_dir()`
- `discover_sessions()`
- `identify_expired()`
- `scan_observation_stats()`
- `DEFAULT_OBSERVATION_DIR` constant
- All tests

## File: crates/unimatrix-observe/src/lib.rs

### Update re-exports

Remove:
```rust
pub use files::{discover_sessions, identify_expired, observation_dir, scan_observation_stats};
pub use parser::{parse_session_file, parse_timestamp};  // or just parse_timestamp if retained
pub use types::{SessionFile, ParsedSession};  // if no longer needed
```

Keep:
```rust
pub mod source;
pub use source::ObservationSource;
```

Remove module declarations:
```rust
// pub mod files;   -- REMOVED
// pub mod parser;  -- REMOVED (or retained for parse_timestamp)
```

## File: crates/unimatrix-observe/src/types.rs

### Remove SessionFile type (if no longer referenced)

Check if SessionFile is used anywhere after files.rs removal. If not, remove the struct.

### Remove ParsedSession type (if no longer referenced)

Check if ParsedSession is used by attribution.rs or elsewhere. If attribution.rs is retained as utility, ParsedSession may still be needed.

## File: crates/unimatrix-observe/src/attribution.rs

### Retain as utility module

Keep `extract_feature_signal` and helper functions -- they may be useful for features that predate session registration. But remove from primary pipeline path.

`attribute_sessions` function stays but is no longer called from context_retrospective.

## Consumers to update

After removing files.rs re-exports, check for broken imports:
- `crates/unimatrix-server/src/services/status.rs` uses `unimatrix_observe::observation_dir()` and `unimatrix_observe::scan_observation_stats()` -- these are replaced by SqlObservationSource in retrospective-migration component
- `crates/unimatrix-server/src/mcp/tools.rs` uses `unimatrix_observe::discover_sessions()`, `parse_session_file()`, `attribute_sessions()` -- replaced in retrospective-migration component

## Notes

- R-08: Hook scripts must still exit 0 unconditionally
- AC-08: Verify no JSONL writes remain in hooks (grep verification)
- AC-09: Verify compilation succeeds without parser.rs/files.rs
- AC-13: Net code reduction -- removing ~360 lines of JSONL infrastructure
