# Component 10 — First-Boot Token (CI-1)

**File:** `crates/unimatrix-server/src/http/token.rs`
**ADR:** ADR-008 (#5088) · **AC:** AC-11 · **Risk:** R-14 · **NFR:** NFR-06

## Purpose

Guarantee the first-boot bearer token is NEVER emitted to stdout or `tracing` logs. The validated `v:2` bundle is the SOLE token channel for the cloud/container HTTP surface (ADR-008). Local STDIO/UDS (no bundle, ADR-006) is functionally unchanged.

## IMPORTANT: this is a VERIFY-AND-GUARD component, not a remove-a-print component (OQ-B / CI-1)

The architecture cites a "token print at `token.rs:101`". The LIVE code does NOT print the token:
- The first-boot notice is `render_first_boot_notice()` (token.rs:205), a pure builder whose text is:
  `"[unimatrix] bearer token generated and stored (0600). Retrieve it with: unimatrix client-bundle"` — NO token, NO path, NO secret.
- It is emitted via `eprintln!(render_first_boot_notice())` (token.rs:171) — stderr, token-free.
- `load_existing_token` logs only `tracing::debug!("loaded existing bearer token from {path}")` (token.rs:248) — a PATH, not the token.

So CI-1 is largely ALREADY SATISFIED. This component's job is to (1) make the invariant a TESTED, enforced contract, and (2) add a regression guard so a future edit can't reintroduce a leak. The pseudocode below is the assertion surface, plus a defensive review of every emission site.

## Functions reviewed (assert token-free; do NOT introduce a removal of code that doesn't leak)

```
write_new_token(...):
    ... atomic temp+rename of the hex (token.rs:114-173) ...
    eprintln!(render_first_boot_notice())            // ASSERT: token-free (it is; keep pure builder)
    Ok(token_bytes)

render_first_boot_notice() -> String:
    // MUST NOT contain the token hex, the token-file path, or any secret (NFR-06).
    return "[unimatrix] bearer token generated and stored (0600). Retrieve it with: unimatrix client-bundle"

load_existing_token(path):
    tracing::debug!("loaded existing bearer token from {path}")   // ASSERT: logs the PATH only, never the bytes
```

## Defensive sweep (the actual delivery action)

```
1. grep token.rs (and the whole server crate's first-boot path) for any println!/eprintln!/print!/
   tracing::{info,debug,warn,error}!/format! that interpolates the token hex (the 64-hex value,
   `token_bytes`, `hex_string`). EXPECT: none reach an output sink. If any DO -> redact/gate.
2. If a token-emitting print is found AND it is reachable on BOTH cloud first-boot and the local
   STDIO/UDS path (ADR-006 / R-13 ∩ R-14): GATE it by deployment context (cloud-first-boot only),
   do NOT unconditionally remove it (a naive removal could regress local's token affordance, AC-10).
   The live code has no such shared print, so the expected outcome is: assert + guard, no gating needed.
3. Confirm the bundle (Component 1) carries the token (sole-channel positive: the token IS delivered,
   just only via the bundle).
```

## Data Flow

- IN: first-boot token generation (`write_new_token`) or load (`load_existing_token`).
- OUT (cloud): the token reaches the client ONLY inside the `v:2` bundle blob (Component 1).
- OUT (stdout/logs): a token-free notice (stderr) + a path-only debug log. No token substring anywhere.

## Error Handling

- Token file IO errors → `ServerError::ProjectInit` with the PATH (not the token) in the message (unchanged).

## Key Test Scenarios (hints)

1. AC-11 / R-14 sc.1: on the HTTP/cloud first-boot path, capture stdout AND `tracing` output; assert NO token substring appears anywhere.
2. R-14 sc.2 (sole channel): assert the emitted `v:2` bundle carries the token (positive delivery) and no parallel "also print it" path survives.
3. `render_first_boot_notice()` unit test: returned string contains no 64-hex run, no token-file path (a pure-builder assertion that doubles as the regression guard).
4. R-14 sc.3 (local non-regression): assert the local STDIO/UDS token affordance is unchanged; if any redaction/gating was added, it is scoped to cloud first-boot, not unconditional (cross-check R-13/AC-10).
