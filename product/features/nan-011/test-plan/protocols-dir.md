# Test Plan: protocols/ Directory

## Component Scope

Files under test:
- `protocols/uni-design-protocol.md` (new file, copy of .claude/protocols/uni/)
- `protocols/uni-delivery-protocol.md` (new file, copy)
- `protocols/uni-bugfix-protocol.md` (new file, copy)
- `protocols/uni-agent-routing.md` (new file, copy)
- `protocols/README.md` (new file, no .claude/ source equivalent)
- `.claude/protocols/uni/uni-design-protocol.md` (source, corrected)
- `.claude/protocols/uni/uni-delivery-protocol.md` (source, corrected)
- `.claude/protocols/uni/uni-bugfix-protocol.md` (source, corrected)
- `.claude/protocols/uni/uni-agent-routing.md` (source, corrected)

Acceptance criteria covered: AC-14, AC-15
Risks covered: R-04 (Critical), R-09 (Med), R-11 (Med, protocol subcategory)

---

## AC-14: protocols/ Directory Structure and context_cycle Example

**Risk**: R-09 (Med/Med) — missing or stale context_cycle example defeats distribution purpose

### Step 1 — Directory existence and file count

```bash
ls /workspaces/unimatrix/protocols/
# Expected output contains: uni-design-protocol.md, uni-delivery-protocol.md,
# uni-bugfix-protocol.md, uni-agent-routing.md, README.md
# (exactly 5 files)
```

Assert: all 5 files exist. No extra files. No subdirectories.

### Step 2 — All files are regular files (not symlinks)

```bash
ls -la /workspaces/unimatrix/protocols/
# Check file type indicator in leftmost column: must be '-' (regular file), not 'l' (symlink)
```

Assert: every file listed shows `-` as the first character in the permissions column.
Symlinks do not survive `npm pack` — any symlink is a packaging defect (ADR-003, constraint 6).

### Step 3 — context_cycle example in protocols/README.md

```bash
grep -n "context_cycle" /workspaces/unimatrix/protocols/README.md
# Expected: at least three matches (one per call type)
```

Assert the output includes lines showing all three call types:
- `type: "start"` (or `"type": "start"`)
- `type: "phase"` (or `"type": "phase"`)
- `type: "stop"` (or `"type": "stop"`)

```bash
grep -n '"start"\|"phase"\|"stop"' /workspaces/unimatrix/protocols/README.md
# Expected: at least one match per call type (3 total)
```

### Step 4 — Correct parameter names in context_cycle example

Assert: the example does NOT use deprecated parameter names such as `phase_id`, `phase_type`,
or any name that differs from the current MCP tool signature. The correct call signature is:

```
mcp__unimatrix__context_cycle({ "feature": "<id>", "type": "start" })
mcp__unimatrix__context_cycle({ "feature": "<id>", "type": "phase", "phase": "<name>" })
mcp__unimatrix__context_cycle({ "feature": "<id>", "type": "stop" })
```

```bash
grep -n "phase_id\|phase_type" /workspaces/unimatrix/protocols/README.md
# Expected: zero matches (deprecated parameter names must not appear)
```

### Step 5 — Generalizability note present

Read `protocols/README.md`. Assert: at least one sentence clarifies that the protocol
files and context_cycle pattern are not Claude Code-specific — they apply to any
agentic workflow tool.

**Pass criteria**: All 5 files exist as regular files. context_cycle example shows all
3 call types with correct parameter names. No deprecated parameter names. Generalizability
note present.

---

## AC-15: Protocol Files Clean + Dual-Copy Drift = Zero

**Risk**: R-04 (Critical/High) — distributed protocols diverge from internal; R-11 (stale references)

### Step 1 — Stale references in protocols/ (distributed copies)

```bash
grep -rn "NLI\|MicroLoRA\|unimatrix-server\|HookType" /workspaces/unimatrix/protocols/
# Expected: zero matches
```

If any match appears: identify which file contains it. If it was introduced during the
copy process (i.e., the source is clean), the copy failed. If the source also has it,
the source correction was missed.

### Step 2 — Stale references in .claude/protocols/uni/ (source files)

```bash
grep -rn "NLI\|MicroLoRA\|unimatrix-server\|HookType" /workspaces/unimatrix/.claude/protocols/uni/
# Expected: zero matches
```

This must pass before Step 3 is meaningful — if the source is not clean, the copies
cannot be clean either.

### Step 3 — Diff verification: source ↔ copy for all 4 protocol files

Run each diff command. Each MUST produce zero output (empty means identical):

```bash
diff /workspaces/unimatrix/.claude/protocols/uni/uni-design-protocol.md \
     /workspaces/unimatrix/protocols/uni-design-protocol.md
# Expected: no output (exit code 0, zero lines of diff)

diff /workspaces/unimatrix/.claude/protocols/uni/uni-delivery-protocol.md \
     /workspaces/unimatrix/protocols/uni-delivery-protocol.md
# Expected: no output

diff /workspaces/unimatrix/.claude/protocols/uni/uni-bugfix-protocol.md \
     /workspaces/unimatrix/protocols/uni-bugfix-protocol.md
# Expected: no output

diff /workspaces/unimatrix/.claude/protocols/uni/uni-agent-routing.md \
     /workspaces/unimatrix/protocols/uni-agent-routing.md
# Expected: no output
```

If any diff produces output: the copy was made before the source was corrected (violating
the source-before-copy constraint), or the copy was modified independently. Either way is
a defect.

### Step 4 — protocols/README.md has no .claude/ source (it is authored independently)

```bash
ls /workspaces/unimatrix/.claude/protocols/uni/README.md 2>&1
# Expected: "No such file or directory" — there is no source equivalent to diff against
```

This is expected behavior per ADR-003: `protocols/README.md` is authored once in
`protocols/` and has no `.claude/` counterpart.

### Step 5 — context_cycle signatures in source protocol files

The corrections applied to `.claude/protocols/uni/` include verifying context_cycle call
signatures. Assert none of the 4 source protocol files use deprecated parameter names:

```bash
grep -rn "phase_id\|phase_type" /workspaces/unimatrix/.claude/protocols/uni/
# Expected: zero matches
```

**Pass criteria**: Steps 1 and 2 return zero stale references. All four diffs (Step 3)
produce zero output. protocols/README.md has no .claude/ source (expected). Step 5
returns zero deprecated parameter names.
