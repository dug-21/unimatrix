# Test Plan: query-skills
## Component: `.claude/skills/uni-knowledge-search/SKILL.md` and `.claude/skills/uni-knowledge-lookup/SKILL.md`

### Risk Coverage

| Risk | Severity | Test(s) |
|------|----------|---------|
| AC-09 | Acceptance | Manual review of skill file diffs |

---

## Overview

Component 6 is documentation-only. No Rust code changes, no new functions, no MCP schema
changes. The two skill files receive prose updates that:

1. Add `helpful: true` to primary example invocations.
2. Add guidance on when to pass `helpful: false`.

Because this is documentation, all verification is manual review (no automated tests).

---

## Manual Review Checklist (AC-09)

Run at Stage 3c as part of RISK-COVERAGE-REPORT.md:

### `uni-knowledge-search/SKILL.md`

```bash
git diff HEAD crates/.claude/skills/uni-knowledge-search/SKILL.md
# Or:
git diff HEAD .claude/skills/uni-knowledge-search/SKILL.md
```

Assert:
- [ ] At least one example invocation includes `helpful: true`
- [ ] Guidance text explains when to pass `helpful: false` (entry was retrieved but not applicable)
- [ ] No functional behavior changes (parameter schemas unchanged)
- [ ] The `helpful: true` example appears in the primary/recommended usage section, not as a footnote

### `uni-knowledge-lookup/SKILL.md`

```bash
git diff HEAD .claude/skills/uni-knowledge-lookup/SKILL.md
```

Assert:
- [ ] At least one example invocation includes `helpful: true`
- [ ] Guidance text explains when to pass `helpful: false`
- [ ] No functional behavior changes

### Negative Assertions

- [ ] `LookupParams` or `SearchParams` Rust structs did NOT gain a required `helpful` field
  (it already exists as `Option<bool>`)
- [ ] No new MCP parameter schema entries for `access_weight` (this is server-internal)

---

## Integration Expectations

No integration tests needed for documentation-only changes. The behavior wired by these
skill updates (implicit helpful votes via `context_get`, doubled access via `context_lookup`)
is tested in the `deliberate-retrieval-signal` component tests.

The skill changes are a signal change: they encourage agents to explicitly pass `helpful: true`
in `context_search` and `context_lookup` calls, which routes through the existing `helpful`
parameter to the existing vote infrastructure — no new code path is created.

---

## Scope Note

FR-08 explicitly limits skill changes to `context_search` and `context_lookup`. The
`context_get` skill (if it exists) is NOT modified here — the implicit helpful vote for
`context_get` is implemented at the server handler level (Component 5), not in the skill file.
Agents do not need to pass `helpful: true` for `context_get` — it is automatic when the
parameter is omitted.
