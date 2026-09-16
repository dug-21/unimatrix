# C7 — OpenCode domain pack

**Location:** `crates/unimatrix-observe/src/domain/mod.rs` (106 code-lines, under cap).
**ADRs:** ADR-001 (pack is legacy-row fallback + category registration, NOT the AC-03 mechanism).
**Risks:** R-04.2 (legacy fallback). **Wave:** 1 (independent).

## Purpose

Register an `opencode` `DomainPack` so that (a) opencode categories are added to `CategoryAllowlist` at
startup, and (b) legacy NULL rows whose events map to the opencode pack resolve correctly via the read
path. **This pack is explicitly NOT how AC-03 is achieved** — persistence (C5) is. It exists for
category registration and the legacy-row fallback branch (C6).

## Edit — add builtin (or config) opencode pack

Mirror `builtin_claude_code_pack()` (`:45`):
```
fn builtin_opencode_pack() -> DomainPack {
    DomainPack {
        source_domain: "opencode".to_string(),   // matches ^[a-z0-9_-]{1,64}$ (validated at registry::new)
        event_types: vec![ /* see decision below */ ],
        categories: vec![ /* same 7 INITIAL_CATEGORIES as claude-code */ ],
        rules: vec![],                             // built-in Rust rules, no DSL descriptors
    }
}
```
Register it in `DomainPackRegistry::new` / the builtin seeding path alongside claude-code, and in
`with_builtin_claude_code` if opencode should be present zero-config (delivery decision).

### event_types decision (EC-07 collision — critical)

`resolve_source_domain` (`:180`) returns a **non-deterministic** domain when two packs share an
`event_type` (HashMap iteration). OpenCode emits the SAME canonical names as claude-code. Therefore:

- **DO NOT** give the opencode pack overlapping canonical event_types (`PreToolUse`, etc.) — that
  reintroduces the exact collision ADR-001 was written to avoid, making legacy resolution
  non-deterministic between claude-code and opencode.
- The opencode pack's `event_types` should be **empty of the shared canonical names**. Since the
  AC-03 mechanism is the persisted column (C5), the pack does not need to claim canonical events at all.
- **Flagged decision for delivery:** either (i) register opencode with `event_types: vec![]`-equivalent
  that claims NOTHING shared (preferred — pack exists only for `categories` + `lookup("opencode")`), or
  (ii) if any opencode-unique event names exist, list only those. An empty `event_types` means "claims
  ALL" per current semantics (`:183`) — that is WRONG here and would hijack resolution; delivery must
  NOT use empty-claims-all. Give it a sentinel/opencode-only event list or guard the collision.

This is the single subtle correctness point in C7. Resolve it explicitly; do not leave it to chance.

## Data flow

Used at: server startup (`iter_packs` → CategoryAllowlist registration, `:194`) and read-path legacy
fallback (`resolve_source_domain`, invoked by C6 only when the stored column is NULL).

## Error handling

- `DomainPackRegistry::new` validates `source_domain` format (`:107`) and rejects `"unknown"` — `"opencode"`
  passes. Invalid pack → server refuses to start (existing FM-01 behavior), acceptable.

## Key test scenarios (hints for tester)

1. `lookup("opencode")` returns the pack; `source_domain` format valid.
2. **No collision** (EC-07): registering opencode does not make `resolve_source_domain("PreToolUse")`
   non-deterministic — claude-code still resolves deterministically for legacy rows (R-05/R-04.2).
3. opencode categories present in `CategoryAllowlist` after startup.
4. Legacy NULL opencode-era row (if any) resolves via fallback without regressing claude-code rows.
</content>
