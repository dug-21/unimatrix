# Component 9 — Reserved Slugs (Rust)

**File:** `crates/unimatrix-server/src/infra/config.rs`
**ADR:** ADR-005 (#5084) · **AC:** FR-13, AC-13(slug-tie) · **Risk:** R-08

## Purpose

Re-derive the reserved-slug set from the POST-cutover grammar (`/v1/{slug}/...` only; no default alias). The VALUE is retained; the DERIVATION (rationale per literal) is re-documented so the set stays bound to the live route grammar. No registerable slug may shadow a route segment.

## Modified Constant (VALUE unchanged, doc-comment re-derived)

```
// config.rs:2483 — VALUE retained: ["v1","health","observe","tools"]
pub const RESERVED_SLUGS: [&str; 4] = ["v1", "health", "observe", "tools"];

// Re-derived doc-comment (ADR-005), per literal under the new grammar:
//   "v1"      — KEEP. Still the fixed first path segment; reserves the version namespace.
//   "health"  — KEEP. /health stays a top-level store-independent route; a "health" slug would
//               collide conceptually; cheap to keep.
//   "observe" — KEEP, NEW RATIONALE. /observe is NO LONGER top-level; observe is now
//               /v1/{slug}/observe (ADR-003). A slug "observe" would route /v1/observe/observe;
//               reserved now as a per-slug SUB-ROUTE segment, not a top-level route shadow.
//   "tools"   — KEEP, conservative. The /v1/tools->Default alias is DELETED (ADR-004); /v1/tools/...
//               now means "the project whose slug is tools" (unambiguous). Un-reserving is SAFE but
//               deferred to avoid surprising operators/docs during the #768 doc fast-follow window;
//               un-reserve is a one-line follow-up + test if a real project needs `tools` (OQ-3/OQ-C).
```

## `is_reserved_slug` (UNCHANGED)

```
fn is_reserved_slug(slug: &ProjectSlug) -> bool:
    RESERVED_SLUGS.contains(&slug.as_str())     // EXACT match only; no prefix/substring (toolsx, v1-prod NOT reserved)
```

## Call sites (UNCHANGED behavior)

- `validate_slug` (projects.rs:208, Component 8) — rejects a charset-valid slug equal to any reserved name.
- `validate_projects_config` (config.rs:2524) — rejects reserved slugs at config-load.
- The `validate_slug` error message at projects.rs:210-211 mentions "tools would shadow the default-project alias" — UPDATE that wording: the default alias no longer exists; `tools` is reserved conservatively (pending un-reserve), not because it shadows a default. Keep the message accurate to the new grammar.

## Data Flow

- IN: a candidate slug (operator at `register`, or a `[[projects]]` stanza at boot).
- OUT: accept / reject — a reserved name is rejected at the parse edge before any filesystem/route use.

## Error Handling

- Reserved slug at `register` → `ServerError::Config` (loud).
- Reserved slug in config → `ConfigError::ProjectSlugReserved` (loud, fails boot validation).

## Key Test Scenarios (hints)

1. R-08 sc.1 (rejection table): `register` against EVERY reserved name (`v1`, `health`, `observe`, `tools`) → each rejected.
2. R-08 sc.2 (grammar-coupling): assert `/v1/{slug}/observe` resolves for a registered slug WHILE slug `observe` is unregisterable — binds the reserved set to the live grammar.
3. R-08 sc.3 (`tools` decision pin): a test that LOCKS the chosen `tools`-reserved state so a silent flip is caught (OQ-3).
4. Exact-match only: `toolsx`, `v1-prod`, `healthcheck`, `observer` are NOT reserved (no over-broad rejection).
5. `validate_slug` error message no longer claims a "default-project alias" exists.
