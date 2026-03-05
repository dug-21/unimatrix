# Test Plan: jsonl-removal

## Risk Coverage

- R-08: Hook script breakage after JSONL removal (Low)

## Test Scenarios

### T-JR-01: Compilation succeeds without parser.rs and files.rs
**Type**: Build verification
**AC**: AC-09, AC-12

Action: `cargo build --workspace`
Assert: No compilation errors referencing removed modules or functions

### T-JR-02: No JSONL write references in hook scripts
**Type**: Manual verification (automated grep)
**AC**: AC-08

Action: `grep -r 'jsonl\|OBS_DIR\|observation' .claude/hooks/observe-*.sh`
Assert: No references to JSONL writes, observation directory, or .jsonl files

### T-JR-03: Net code reduction
**Type**: Manual verification
**AC**: AC-13

Action: `git diff --stat` after all changes
Assert: Net negative line count

## Implementation Notes

- T-JR-01 is verified automatically by cargo build in Gate 3b
- T-JR-02 is a grep check
- T-JR-03 is a diff stat check at the end
- No dedicated test code needed for this component
