# Test Plan: readme-rewrite (Component 1)

## Component Scope

File: `README.md` (root of worktree)
Change: Complete rewrite with 11 sections (FR-01a through FR-10).

Testing is entirely shell-based. Each test scenario is a verifiable assertion
expressed as a command with an expected result.

---

## R-01 / R-02: Factual Accuracy (Critical)

Every numeric or named fact must be sourced from the live codebase, not from
SCOPE.md estimates. All assertions below must PASS before Gate 3c.

### T-01: No redb references

```bash
grep -ri 'redb\|\.redb' README.md
# Expected: no output (exit code 1 = no match)
```

### T-02: Database file extension is .db not .redb

```bash
grep 'unimatrix\.db' README.md
# Expected: at least one match
grep 'unimatrix\.redb' README.md
# Expected: no output
```

### T-03: Crate count matches workspace

```bash
# Get verified count
CRATE_COUNT=$(ls /workspaces/unimatrix-nan-005/crates/ | wc -l | tr -d ' ')
echo "Crates in workspace: $CRATE_COUNT"
# Verify README lists the same count in architecture section
grep -i "$CRATE_COUNT.crate\|$CRATE_COUNT crate" README.md
# Expected: at least one match containing the verified number
```

### T-04: Schema version matches migration.rs

```bash
SCHEMA_VER=$(grep 'CURRENT_SCHEMA_VERSION' \
  /workspaces/unimatrix-nan-005/crates/unimatrix-store/src/migration.rs \
  | grep -o '[0-9]*' | head -1)
echo "Schema version: $SCHEMA_VER"
grep "schema v${SCHEMA_VER}\|version ${SCHEMA_VER}" README.md
# Expected: at least one match (architecture section data layout)
```

### T-05: SQLite table count — if stated explicitly, must match db.rs

```bash
TABLE_COUNT=$(grep -c 'CREATE TABLE IF NOT EXISTS' \
  /workspaces/unimatrix-nan-005/crates/unimatrix-store/src/db.rs)
echo "Tables in db.rs: $TABLE_COUNT"
# If README states a table count, it must equal TABLE_COUNT.
# If README omits table count (acceptable per NFR-05), this check is vacuous.
grep -i "${TABLE_COUNT}.table\|${TABLE_COUNT} table" README.md || echo "VACUOUS: table count not stated"
```

### T-06: Test count not understated

```bash
# README must not claim fewer tests than exist.
# Test counts change frequently; README should state "2000+" or similar qualified approximation.
# Verify no specific low number is claimed:
grep -i '1,500\|1500\|1,000\|1000 test' README.md
# Expected: no output
```

### T-07: Hook event names match hook.rs

```bash
for EVENT in UserPromptSubmit PreCompact PreToolUse PostToolUse Stop; do
  grep "$EVENT" README.md > /dev/null && echo "PASS: $EVENT" || echo "FAIL: $EVENT missing"
done
# Expected: PASS for all 5 events
```

### T-08: Rust version matches Cargo.toml

```bash
RUST_VER=$(grep 'rust-version' /workspaces/unimatrix-nan-005/Cargo.toml \
  | grep -o '[0-9]\.[0-9]*' | head -1)
echo "Rust version: $RUST_VER"
grep "$RUST_VER\|Rust $RUST_VER" README.md
# Expected: at least one match (getting started prerequisites)
```

### T-09: npm package name matches package.json

```bash
grep '@dug-21/unimatrix' README.md
# Expected: at least one match (getting started npm install command)
```

### T-10: Storage backend is SQLite not redb

```bash
grep -i 'SQLite' README.md
# Expected: at least one match
grep -i 'redb' README.md
# Expected: no output
```

---

## R-03: `maintain` Parameter (High)

### T-11: maintain documented as silently ignored

```bash
grep -i 'maintain' README.md
# Expected output must NOT contain language suggesting maintain=true triggers inline maintenance.
# Review manually for: "triggers", "runs maintenance", "invokes", "starts maintenance"
```

### T-12: No active maintain language

```bash
grep -i 'maintain=true.*trigger\|maintain.*trigger\|trigger.*maintain' README.md
# Expected: no output
```

---

## R-04: Tool Count (High)

### T-13: Tool table row count matches tools.rs

```bash
TOOL_COUNT=$(grep -c '#\[tool(' \
  /workspaces/unimatrix-nan-005/crates/unimatrix-server/src/mcp/tools.rs)
echo "Tools in tools.rs: $TOOL_COUNT"
# Count rows in README MCP Tool Reference table (lines starting with | context_)
README_TOOL_ROWS=$(grep -c '^| *context_' README.md)
echo "Tool rows in README: $README_TOOL_ROWS"
[ "$TOOL_COUNT" -eq "$README_TOOL_ROWS" ] && echo "PASS: counts match" || echo "FAIL: mismatch"
```

### T-14: No tool stated as 12 if codebase has 11 (or vice versa)

```bash
TOOL_COUNT=$(grep -c '#\[tool(' \
  /workspaces/unimatrix-nan-005/crates/unimatrix-server/src/mcp/tools.rs)
# If README states a count claim in prose, it must match TOOL_COUNT
grep -i "[0-9]* MCP tool\|[0-9]* tool" README.md | head -5
# Manual review: does stated count match TOOL_COUNT?
```

### T-15: All 11 expected tool names present

```bash
for TOOL in context_search context_lookup context_get context_store \
            context_correct context_deprecate context_quarantine \
            context_status context_briefing context_enroll context_cycle_review; do
  grep "$TOOL" README.md > /dev/null && echo "PASS: $TOOL" || echo "FAIL: $TOOL missing"
done
# Expected: PASS for all. If count from T-13 differs, adjust list accordingly.
```

---

## R-08: Aspirational Content (Medium)

### T-16: No forward-looking language about unimplemented features

```bash
grep -in 'will be\|coming soon\|planned\|roadmap\|future release\|not yet' README.md
# Expected: no output (or only legitimate uses unrelated to capabilities)
```

### T-17: No OAuth or HTTPS transport in security section

```bash
grep -i 'oauth\|https transport\|_meta.*identity\|_meta.*agent' README.md
# Expected: no output
```

### T-18: No Activity Intelligence or Graph Enablement as current features

```bash
grep -i 'activity intelligence\|graph enablement' README.md
# Expected: no output
```

---

## R-09: Terminology Consistency (Medium)

### T-19: Product name is "Unimatrix" not "UniMatrix"

```bash
grep 'UniMatrix' README.md
# Expected: no output
grep -c 'Unimatrix' README.md
# Expected: >= 1 (confirms correct form used)
```

### T-20: Tool names use underscore form

```bash
grep 'contextSearch\|contextStore\|contextLookup\|contextGet\|contextCorrect\|contextStatus' README.md
# Expected: no output (camelCase forms must not appear)
```

### T-21: Skill names use leading slash

```bash
grep -P 'query-patterns[^/]|store-adr[^/]|knowledge-search[^/]' README.md | grep -v '\.claude\|skills/' | head -5
# Expected: no matches (skill invocations without leading slash in prose)
```

### T-22: Storage backend consistently "SQLite"

```bash
grep -i 'sqlite' README.md | head -5
# Expected: at least one match
grep 'SQLITE\|SQlite' README.md
# Expected: no output (inconsistent casing)
```

---

## R-10: Security Section (Medium)

### T-23: Security section contains required elements

```bash
for TERM in "trust" "capabilities" "scanning\|content scan" "audit" "hash\|correction chain" "protected agent"; do
  grep -iE "$TERM" README.md > /dev/null && echo "PASS: $TERM" || echo "FAIL: $TERM missing from README"
done
```

### T-24: Security section does not describe unimplemented features

```bash
grep -i 'oauth\|https transport\|_meta' README.md
# Expected: no output
```

---

## R-11: Skills Table Completeness (Medium)

### T-25: Skills table row count matches filesystem

```bash
SKILL_COUNT=$(ls /workspaces/unimatrix-nan-005/.claude/skills/ | wc -l | tr -d ' ')
echo "Skills on filesystem: $SKILL_COUNT"
README_SKILL_ROWS=$(grep -c '^| */[a-z]' README.md)
echo "Skill rows in README: $README_SKILL_ROWS"
[ "$SKILL_COUNT" -eq "$README_SKILL_ROWS" ] && echo "PASS" || echo "FAIL: mismatch"
```

### T-26: No fabricated skill entries

```bash
# Each skill row in README must correspond to a real directory in .claude/skills/
while IFS= read -r skill_dir; do
  SKILL_NAME="/${skill_dir}"
  grep "$SKILL_NAME" README.md > /dev/null && echo "PRESENT: $SKILL_NAME" || echo "MISSING: $SKILL_NAME"
done < <(ls /workspaces/unimatrix-nan-005/.claude/skills/)
```

### T-27: /uni-git classification documented

```bash
grep -A2 'uni-git' README.md | head -5
# Expected: present with scope notation (developer/contributor context or similar)
```

---

## R-13: Acknowledgments (Low)

### T-28: Acknowledgments section preserved

```bash
grep -i 'claude-flow\|ruvnet\|acknowledgment\|credit' README.md
# Expected: at least one match
```

---

## Structural Completeness (AC-01)

### T-29: All 11 sections present

```bash
for SECTION in "Why Unimatrix\|Why Use" "Core Capabilities\|Capabilities" \
               "Getting Started" "Tips\|Maximum Value\|Operational" \
               "MCP Tool\|Tool Reference" "Skills Reference\|Skills" \
               "Knowledge Categories\|Categories" "CLI Reference\|CLI" \
               "Architecture" "Security"; do
  grep -i "$SECTION" README.md > /dev/null && echo "PASS: $SECTION" || echo "FAIL: $SECTION missing"
done
# Hero/intro is implicitly the top of file — verify first non-blank line is non-placeholder:
head -5 README.md
```

### T-30: No placeholder content

```bash
grep -i 'TODO\|TBD\|placeholder\|coming soon\|fill in\|to be written' README.md
# Expected: no output
```

### T-31: README line count within bounds

```bash
wc -l README.md
# Expected: 450-800 lines. Flag if under 450 (incomplete) or over 800 (ADR-001 split threshold).
```

---

## Getting Started Completeness (AC-09)

### T-32: npm install path present

```bash
grep 'npm install @dug-21/unimatrix' README.md
# Expected: exact command present
```

### T-33: Build from source path present

```bash
grep 'cargo build\|cargo install' README.md
# Expected: at least one match
```

### T-34: Configuration snippets present (MCP + hooks)

```bash
grep -c 'settings\.json\|mcpServers\|UserPromptSubmit' README.md
# Expected: >= 3 (MCP config + hooks config references)
```

---

## Knowledge Categories (AC-11)

### T-35: All 8 categories present by exact name

```bash
for CAT in outcome lesson-learned decision convention pattern procedure duties reference; do
  grep "$CAT" README.md > /dev/null && echo "PASS: $CAT" || echo "FAIL: $CAT missing"
done
```
