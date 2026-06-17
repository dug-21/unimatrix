# Component 11 — Local STDIO/UDS Direct-Binding Guard (C-13)

**File:** `crates/unimatrix-server/src/main.rs` (`:859` UDS, `:1158` STDIO) — NO production change
**ADR:** ADR-006 (#5087, corrected) · **AC:** AC-10 · **Risk:** R-13 (Critical, the GATE-2 guard)

## Purpose

This is a NEGATIVE / GUARD component. It changes NO production code. It documents the boundary the rest of vnc-038 must not cross, and specifies the structural assertions that fail the instant delivery routes local through the unified resolver or makes local a resolver key. Local keeps its DIRECT path-hash store binding (ADR-006 tightening). This is the load-bearing GATE-2 confirmation guard.

## The Invariant (what must remain TRUE — do not modify)

```
LOCAL UDS boot   (main.rs:859):   open ~/.unimatrix/{hash}/unimatrix.db DIRECTLY -> Arc<Store> -> handler
LOCAL STDIO boot (main.rs:1158):  open ~/.unimatrix/{hash}/unimatrix.db DIRECTLY -> Arc<Store> -> handler

These paths MUST:
  - NEVER call parse_project_key
  - NEVER construct the HTTP resolver (DefaultResolver / MultiProjectRouter)
  - NEVER reference ProjectKey::Default (which is being deleted anyway)
  - NEVER use a bundle
  - NEVER be self-registered as a resolver key (no derived path-hash key in the slug map)
  - require NO manual slug (path-hash derived automatically at boot, as today)
```

Two independent store-binding mechanisms by transport (ADR-006):
- cloud/container HTTP: slug from URL → `parse_project_key` → `ProjectKey::Slug` → unified resolver → `Arc<Store>`
- local STDIO/UDS: path-hash from install path at boot → direct `Store::open` → `Arc<Store>` threaded to handler

They are NOT two keys in one map; they are two distinct boot-time wiring paths.

## Delivery Constraint (HARD BOUNDARY — restate to implementers)

```
The ADR-004 deletions (DefaultResolver, /v1/tools->Default arm, _ => Default fallback, Default
resolver/dispatch arms) are HTTP-cloud/container ONLY. They MUST NOT reach the local boot paths.
Routing local through the resolver, or making local a resolver-map key, REGRESSES AC-10 and is a
scope violation (it creates a cross-store path that does not exist today).
```

## Guard Assertions (the deliverable — added as tests, not production code)

```
G1 — Direct-binding assertion (load-bearing):
     Local STDIO (main.rs:1158) and UDS (main.rs:859) still open ~/.unimatrix/{hash}/unimatrix.db
     directly at boot and thread Arc<Store> straight to their handlers, with NO slug supplied —
     behavior unchanged from ADR-004.

G2 — Resolver-bypass assertion (structure/grep guard that FAILS on regression):
     The local boot paths never invoke parse_project_key, never construct DefaultResolver/
     MultiProjectRouter, never reference ProjectKey::Default, never touch a bundle. A guard that
     fails the instant a future edit threads local through the resolver or adds a local resolver-map key.

G3 — No-resolver-key assertion (ADR-006 tightening):
     Local is NOT self-registered as a resolver key; the unified resolver's key space is
     ProjectKey::Slug only; there is no derived path-hash key in the slug map.

G4 — HTTP-only-deletion cross-check (with Component 5 / R-07):
     The ADR-004 deletions are confined to HTTP code and do not reach the local STDIO/UDS boot paths.
```

## Edge Case (must NOT trigger loud-first-boot)

```
Local STDIO/UDS boot with NO [[projects]] and NO slug -> resolves its path-hash store DIRECTLY,
NOT a loud-first-boot failure. The AC-09 "register a project to begin" rule is CLOUD-ONLY; local
must not be caught by the empty-config failure (R-13 edge / boot-wiring Component 7 must gate the
empty-projects loud message to the served/HTTP path, not the local transport).
```

## Key Test Scenarios (hints)

1. R-13 sc.1 (G1): local-UDS/STDIO fixture — path-hash store resolves directly over UDS/STDIO with no slug, NOT through the resolver; behavior unchanged from ADR-004.
2. R-13 sc.2 (G2): structure/grep guard — local boot paths contain no `parse_project_key`, no resolver construction, no `ProjectKey::Default`, no bundle reference. FAILS if a future edit wires local through the resolver.
3. R-13 sc.3 (G3): assert the resolver key space is `ProjectKey::Slug` only; no path-hash key in the slug map.
4. R-13 sc.4 (G4): the ADR-004 deletions don't reach `main.rs:859`/`:1158` (diff-scope check).
5. Edge: local boot with empty `[[projects]]` does NOT emit the cloud loud-first-boot failure; it serves its direct store.
