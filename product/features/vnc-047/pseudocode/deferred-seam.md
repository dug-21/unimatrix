# C11 — Deferred mutation seam (comment-only)

**File:** `crates/unimatrix-server/src/mcp/tools.rs` `context_tag` handler (~:1558-1614)
**ADR:** ADR-006. **Scope:** NOT-in-scope #1. **Anti-stub:** comment ONLY — no stub, no `unimplemented!()`, no `todo!()`.

## Purpose

Reserve — in a comment only — the future home for cycle-tag mutation (add/remove/replace after start)
on the EXISTING `context_tag` tool via an additive, entry-defaulting `target`. NOT a new
`context_cycle_tag` tool, NOT built now. `context_tag` and `context_correct` behavior are UNCHANGED.

## What to add

A single comment block near the existing RETROFIT SEAM comments in the `context_tag` handler (the
handler already carries "RETROFIT SEAM #1/#2" comment markers at :1558-1614 — add beside them):

```
// ── vnc-047 DEFERRED SEAM (comment only, NOT built): cycle-tag mutation home ──
//   A future cycle-tag add/remove/replace verb attaches HERE via an additive,
//   entry-defaulting `target` on context_tag (e.g. target = entry (default) | cycle).
//   vnc-047 ships the SET-ONCE cycle-tag WRITE only (hook path, whole-set-once); there is
//   NO cycle-tag MUTATION. Do NOT add a `context_cycle_tag` tool. If a future cycle-tag
//   NAMESPACE query is added here, `like_escape` + ESCAPE clause become mandatory (the
//   cycle-tag write path in vnc-047 ships NO LIKE, so none is needed today).
```

## What NOT to do (guardrails)

- No new struct field, no new tool, no dispatch branch, no function stub.
- Do NOT touch `context_correct` or the entry `context_tag` write logic.
- No `unimplemented!()` / `todo!()` / placeholder (CLAUDE.md rule 2).

## Error handling

N/A — comment only, zero runtime behavior.

## Key test scenarios (hints)

None (comment-only). A diff-review confirms: exactly one comment added, no code path change, no new
tool in the handler registry (supports AC-06 "no new MCP tool").
