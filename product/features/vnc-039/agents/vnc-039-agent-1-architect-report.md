# Agent Report — vnc-039-agent-1-architect

## Task
Architecture for vnc-039: pure-JS stdio->HTTPS MCP bridge restoring context_* over the cloud (Scope A), plus relocating the bearer credential out of the repo working tree to a unimatrix-owned per-key store (Scope B).

## Artifacts
- ARCHITECTURE.md — product/features/vnc-039/architecture/ARCHITECTURE.md
- ADR-001 — architecture/ADR-001-bridge-translation-and-pinned-flush.md (Unimatrix #5108)
- ADR-002 — architecture/ADR-002-bridge-entrypoint-subcommand.md (Unimatrix #5109)
- ADR-003 — architecture/ADR-003-credential-store-keying-and-path.md (Unimatrix #5110)
- ADR-004 — architecture/ADR-004-canonical-store-schema-reconcile-mismatch.md (Unimatrix #5111)
- ADR-005 — architecture/ADR-005-bundle-only-boundary-and-scope-sequencing.md (Unimatrix #5112)

## Key Decisions
- **Bridge (ADR-001):** new pure-Node-stdlib lib/hook-client/mcp-bridge.js, single-session, five testable units; reuses cert-pin.js + transport-http.js pinned-flush verbatim. Fail-LOUD trust contract: token flushed only after fingerprint matches, every new TLS socket re-pinned (persistent-connection divergence from the single-shot observe path — SR-02/A3). Live self-signed-handshake test is an AC; fresh-context security review even on green gates.
- **OQ-4 (ADR-002):** `unimatrix mcp-bridge` subcommand routed to JS via an early branch in bin/unimatrix.js (returns before the Rust exec block). .mcp.json targets the resolved module path directly for a lean spawn; the subcommand is the human/debug surface.
- **OQ-6/SR-08 (ADR-003):** store KEYED BY projectHash (not slug). Path ~/.unimatrix/<projectHash>/credentials.json, per-project file, mode 0600, reusing the existing per-project root. Slug is payload (inside mcp_url), not key — the hook client has no slug at runtime.
- **SR-07 (ADR-004):** one canonical schema {schema_version, mcp_url, observe_url, token, fingerprint, timeouts?}. observe_url replaces the broken `url` key; both consumers read fingerprint. Fixes the latent unpinned/UDS-fallback bug (hook client newly populates pinnedFp -> observe POST becomes actually pinned on bundle creds). credstore.js is the sole store accessor. .mcp.json is token-free (only node + bridge path + projectHash).
- **Bundle-only + sequencing (ADR-005):** cloud MCP is bundle-only; legacy path emits a loud deterministic unsupported message. Scope B is independent (no #774 dep) and LANDS FIRST; Scope A's live validation trails #774.

## Open Questions for the Human
1. .mcp.json invocation: resolved module path (chosen, lean) vs literal `unimatrix mcp-bridge` string (legible, extra hop). One-line flip.
2. Legacy in-tree credential: delete stale unimatrix.remote subtree on next bundle init (chosen) vs leave-but-stop-writing. SCOPE leans toward deletion.
3. Confirm #774 sequencing: B merges independently; A merges with a not-validated-live caveat until #774 lands.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get -- #5105 (stdio-bridge-not-native-http, grounds ADR-001), #5081 (v:2 bundle dumb-client, grounds the verbatim-mcp_url invariant), #5107 (relocate-don't-gitignore + the documented schema mismatch, grounds ADR-003/004), #4708 (Mcp-Session-Id capture/replay semantics), #4970 (vnc-034 F1 dead-pin false-green, elevates the SR-02 fail-loud/live-test contract). Read client code: init.js, bin/unimatrix.js, bundle.js, cert-pin.js, transport-http.js, config.js resolve().
- Stored: ADRs #5108–#5112 via context_store (category decision, topic vnc-039). One typed edge: #5108 Supports #4970 (the dead-pin lesson directly validates ADR-001's fail-loud, test-the-pin-live acceptance — traversal-necessary for any agent revisiting the bridge trust contract). No supersession (vnc-038 ADRs are built upon, not replaced).
