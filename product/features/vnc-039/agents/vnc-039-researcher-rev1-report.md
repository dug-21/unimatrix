# vnc-039 Researcher — Scope B revision (rev1)

Reshaped Scope B in SCOPE.md from "gitignore the in-tree creds file" to "relocate the credential out of the repo working tree into a unimatrix-owned, per-slug, out-of-tree store." Surgical edits only; Scope A, header, Problem Statement, Background Research, OQ-1/OQ-2/OQ-4 left untouched.

## Sections changed
- **Goal 5** — rewritten: relocate out-of-tree + stop squatting Claude Code's namespace; cleartext-at-rest accepted, cleartext-in-repo is the bug.
- **Non-Goal (at-rest encryption / keychain)** — reframed: at-rest cleartext accepted (recoverable at non-interactive spawn; encryption without keychain is theater); bounded risk is cleartext-in-repo + namespace-squatting.
- **Proposed Approach → Scope B** — retitled "Relocate the credential out of the repo working tree"; steps 5 (out-of-tree per-slug store), 6 (both consumers read from it), 7 (legacy migration); rationale updated.
- **AC-08** — rewritten (token only in out-of-tree store, nothing token-bearing in repo tree). **Added AC-08b** (per-slug entries in one global store, idempotent per slug) and **AC-08c** (both bridge and hook/observe resolve from the store).
- **AC-09** — token read from store at spawn, never on command line / in `.mcp.json`.
- **AC-11** — restated for relocation (validatable without a cloud).
- **Constraints** — added: out-of-tree/unimatrix-owned; per-slug keying single global store; both-consumers-one-store; and a PRE-EXISTING SCHEMA MISMATCH flag (see below).
- **OQ-3** — marked RESOLVED (bridge reads out-of-tree store, token never on CLI / in `.mcp.json`).
- **OQ-5** — COLLAPSED (no in-repo file to gitignore); residual legacy-migration note. **Added OQ-6** (store path/layout + slug-vs-projectHash keying).

## New constraint / migration concern discovered in code
**Pre-existing write/read schema mismatch** (independent of relocation, but in Scope B's blast radius):
- `writeRemoteSettingsLocal` writes `unimatrix.remote.{mcp_url, observe_url, token, fingerprint}` (`init.js:230-240`).
- The hook client reads `unimatrix.remote.{url, token, timeouts}` (`config.js:298-306`) and **never reads `fingerprint`** (so `config.pinnedFp` is unpopulated).
- Net effect today: the file-mode observe path silently fails its `nonEmpty(remote.url)` guard and falls back to local UDS; if it didn't, it would run unpinned.
- Implication for design: the relocation rewrites exactly this load/store pair, so it MUST land a single coherent schema (hook client must read `observe_url` + `fingerprint`) and reconcile the mismatch rather than port it forward. Flagged as latent bug, not a new goal.

**Legacy migration**: a prior `init` may have left `.claude/settings.local.json` with `unimatrix.remote`. Surfaced as a minor design question (Scope B step 7 / OQ-5 residual): migrate into the new store and/or clean the in-tree copy.

## Does the hook/observe path complicate the relocation?
**Yes — materially, and it is the load-bearing finding.** The credential is read by TWO consumers, not just the future MCP bridge:
1. The MCP bridge (future) — needs `mcp_url`/token/`fp`.
2. The EXISTING hook/observe telemetry client — `config.resolve()` file-mode branch reads token (+ intended `observe_url`/`fp`) from `.claude/settings.local.json` `unimatrix.remote` (`config.js:276-306`), invoked per hook event as `node lib/hook-client/index.js <EVENT>`, walking from cwd to project root.

So relocation cannot be bridge-only: the hook client's file-mode resolver must be repointed at the out-of-tree store too, while preserving its `UNIMATRIX_REMOTE_URL`/`_TOKEN` env override and UDS fall-through. No third-party / Claude-Code-native consumer reads `unimatrix.remote` (confirmed) — only our own code, so the blast radius is contained. A keying subtlety to resolve in design: the bundle's **slug** (server-authoritative) and the hook client's existing **projectHash** (client-derived, used for `~/.unimatrix/<projectHash>/`) are different keys; both consumers must index the store by the same one (OQ-6).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- no directly relevant entries (returned server-side identity/isolation ADRs, packaging procedures; nothing on edge-client credential storage). Corroborated all claims by reading code directly.
- Stored: entry #5107 "Secret-leak fixes: relocate out of the repo tree, not gitignore it; own your namespace" via context_store (generalizable security/edge-client pattern; feature-specific scope stays in SCOPE.md).
