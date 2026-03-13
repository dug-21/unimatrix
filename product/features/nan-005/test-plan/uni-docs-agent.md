# Test Plan: uni-docs-agent (Component 2)

## Component Scope

File: `.claude/agents/uni/uni-docs.md` (new file)
Change: New agent definition created by nan-005.

Testing verifies file existence, required frontmatter, and all behavioral
constraints mandated by FR-11 and the risk scenarios for R-05 and R-12.

---

## File Existence

### T-01: Agent definition file exists

```bash
ls /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: file listed (exit code 0)
```

### T-02: File is non-empty and not a stub

```bash
wc -l /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: >= 30 lines (agent defs are 50-150 lines by pattern)
```

---

## Frontmatter (Agent Pattern Compliance)

Existing agents (uni-vision-guardian.md, uni-architect.md) use YAML frontmatter
with `name`, `type`, `scope`, `description`, `capabilities` fields.

### T-03: Frontmatter block present and valid

```bash
head -10 /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: starts with --- (YAML frontmatter delimiters)
```

### T-04: Required frontmatter fields present

```bash
for FIELD in 'name:' 'type:' 'description:'; do
  grep "^$FIELD" /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md > /dev/null \
    && echo "PASS: $FIELD" || echo "FAIL: $FIELD missing"
done
```

---

## R-05: Behavioral Gaps (High)

### T-05: Artifact reading instructions present — SCOPE.md

```bash
grep 'SCOPE\.md' /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match (agent reads SCOPE.md as primary artifact)
```

### T-06: Artifact reading instructions present — SPECIFICATION.md

```bash
grep 'SPECIFICATION\.md' /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match
```

### T-07: Fallback chain documented — SPECIFICATION.md missing

```bash
grep -i 'fallback\|missing\|absent\|not found\|fall back' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match describing fallback behavior
```

### T-08: Fallback to SCOPE.md when SPECIFICATION.md absent

```bash
# The fallback direction must be: SPECIFICATION.md missing → use SCOPE.md only
grep -i 'scope.*only\|fall.*scope\|SCOPE.md.*fallback\|fallback.*SCOPE' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match
```

### T-09: Skip condition when SCOPE.md missing

```bash
grep -i 'skip\|cannot proceed\|SCOPE.md.*missing\|no scope' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: match indicating skip/abort when SCOPE.md is also absent
```

### T-10: Scope boundary — README.md only

```bash
grep -i 'README\.md only\|only.*README\|README.*only' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match
```

### T-11: Scope boundary — does not modify .claude/ files

```bash
grep -i '\.claude\|protocol.*file\|agent.*definition\|per.feature' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md | head -5
# Expected: a negative constraint statement — something like "does NOT modify .claude/"
# Review output manually: confirm the agent is explicitly told NOT to edit these paths
```

---

## R-12: No Source Code Reading (Low)

### T-12: Explicit no-source-code constraint

```bash
grep -i 'source code\|rust.*file\|\.rs.*file\|grep.*crate\|not.*read.*code\|artifacts.*not.*source' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match stating the agent reads artifacts, not source code
```

---

## Behavioral Rules Completeness (FR-11b)

### T-13: Reads README.md to identify affected sections

```bash
grep -i 'current.*README\|README.*read\|identify.*section\|affected.*section' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match
```

### T-14: Targeted edits — not full rewrite

```bash
grep -i 'targeted\|specific.*section\|not.*rewrite\|incremental\|partial' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match
```

### T-15: Commits with docs: prefix

```bash
grep -i 'docs:\|commit.*docs\|docs.*prefix' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match
```

---

## Self-Check Block

Existing agents include a self-check section (checklist before returning results).

### T-16: Self-check or output verification section present

```bash
grep -i 'self.check\|before.*return\|verify\|checklist' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match
```

---

## Protocol Spawn Compatibility

The delivery protocol will spawn uni-docs with: feature ID, SCOPE.md path,
SPECIFICATION.md path, README.md path. The agent definition must accept these.

### T-17: Agent accepts feature ID input

```bash
grep -i 'feature.*id\|feature_id\|feature ID' \
  /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md
# Expected: at least one match in inputs section
```

### T-18: Agent accepts README.md path input

```bash
grep 'README\.md' /workspaces/unimatrix-nan-005/.claude/agents/uni/uni-docs.md | head -5
# Expected: README.md appears as an input
```
