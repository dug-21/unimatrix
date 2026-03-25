# Test Plan: hook-registration (.claude/settings.json)

**File:** `.claude/settings.json`
**Risks covered:** R-14 (wrong command pattern or casing)

---

## Test Strategy

`.claude/settings.json` is a JSON configuration file, not source code. There is no `cargo test`
for it. Tests are structural JSON inspections run as shell assertions during Stage 3c.

---

## Structural Inspection Tests (Stage 3c shell assertions)

### T-HR-01: Key exists with correct casing (AC-01)
**AC:** AC-01
**Risk:** R-14

```bash
grep -c '"PostToolUseFailure"' .claude/settings.json
```
Assert output is `1` (exactly one occurrence). Casing is load-bearing — Claude Code uses the exact
key name to match the hook event. `postToolUseFailure` or `POST_TOOL_USE_FAILURE` would silently
never fire.

---

### T-HR-02: matcher is wildcard (AC-01)
**AC:** AC-01
**Risk:** R-14

```bash
grep -A3 '"PostToolUseFailure"' .claude/settings.json
```
Assert the output contains `"matcher": "*"`. A non-wildcard matcher would cause the hook to fire
only for matching tool names, missing all others.

---

### T-HR-03: command contains `unimatrix hook PostToolUseFailure` (AC-01)
**AC:** AC-01
**Risk:** R-14

Assert the command string in the `PostToolUseFailure` entry contains:
- The substring `unimatrix hook PostToolUseFailure`
- The same binary path pattern as the `PreToolUse` and `PostToolUse` entries

```bash
# Extract command fields for all three hook events; they must share the same binary path prefix
python3 -c "
import json, sys
with open('.claude/settings.json') as f:
    cfg = json.load(f)
hooks = cfg.get('hooks', {})
pre_cmd   = hooks.get('PreToolUse',         [{}])[0].get('hooks', [{}])[0].get('command', '')
post_cmd  = hooks.get('PostToolUse',         [{}])[0].get('hooks', [{}])[0].get('command', '')
fail_cmd  = hooks.get('PostToolUseFailure',  [{}])[0].get('hooks', [{}])[0].get('command', '')
# All must contain 'unimatrix hook'
assert 'unimatrix hook' in pre_cmd
assert 'unimatrix hook' in post_cmd
assert 'unimatrix hook PostToolUseFailure' in fail_cmd
print('PASS')
"
```

The exact JSON schema of settings.json for hooks may differ; the assertion above is representative.
During Stage 3c, adjust the field path to match actual schema if needed.

---

### T-HR-04: Consistent binary path format (AC-01)
**AC:** AC-01
**Risk:** R-14

The binary path component of the `PostToolUseFailure` command must use the same absolute path
format as `PreToolUse` and `PostToolUse`. A relative path where others use absolute (or vice versa)
would fail in certain Claude Code execution environments.

**Assertion:** Extract the path prefix from all three command strings; assert they are identical
up to the event-name suffix.

---

## Integration Test Expectations

No infra-001 integration test covers this. The hook fires from Claude Code, not from the MCP
JSON-RPC interface. AC-12 (binary exit-code) is covered in the `hook-dispatcher` test plan.

---

## Edge Cases

- The JSON file must remain valid JSON after the addition. Run `python3 -m json.tool .claude/settings.json` and assert exit code 0.
- The `PostToolUseFailure` entry must not duplicate the `PostToolUse` entry (different event
  names, different semantics). They are siblings in the `hooks` object, not nested.
