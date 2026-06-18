## ADR-003: The out-of-tree credential store is keyed by `projectHash`, colocated at `~/.unimatrix/<projectHash>/remote.json` (OQ-6 / SR-08)

### Context

Scope B relocates the bearer credential out of the repo working tree (today `.claude/settings.local.json`, neither gitignored nor tracked — one `git add -A` commits a live secret) to a unimatrix-owned, out-of-tree, mode-0600 store. The credential is read by **two** consumers: the new MCP bridge (`mcp_url`/token/`fingerprint`) and the existing hook/observe client (`observe_url`/token/`fingerprint`). SR-08 is the keying hazard: the slug is server-authoritative (encoded in the bundle's `mcp_url`); `projectHash` is client-derived (`computeProjectHash(projectRoot)`, `config.js:123`, the existing key for `~/.unimatrix/<projectHash>/` socket + state). If the two consumers index the store by **different** keys, one silently fails to resolve its credential. SCOPE fixes "per-slug-keyed, out-of-tree" loosely; the architect must pick **one** key both consumers agree on and make it a constraint, not an open question.

### Decision

**Key the store by `projectHash`** = `computeProjectHash(projectRoot)`. Both consumers and the `init` writer index by it. This is a fixed constraint.

- The hook/observe client has **no slug at runtime** — it never decodes a bundle; it walks to the project root and hashes it (`config.js:257-260`). Keying by slug would force it to learn/derive slug: new machinery, new failure surface. Keying by `projectHash` requires **zero** new derivation on the read side.
- `projectHash` is already the codebase's per-project key (`~/.unimatrix/<projectHash>/` holds `unimatrix.sock` and `hook-client/` state). The credential file slots into the **same** directory, reusing the single derivation both consumers share — write-key and read-key are computed by one function (`computeProjectHash`) and cannot disagree.
- The slug is **not lost** — it is encoded inside `mcp_url`, which the store carries as payload. The bridge gets the slug for free by posting `mcp_url` verbatim (ADR-001 dumb-client). **Slug is payload, not key.**
- `init` writes the store using the same `computeProjectHash(detectProjectRoot(...))` it already computes; the bridge and hook client read using `computeProjectHash(walkToProjectRoot(...))`. One oracle, one key.

**Path / layout (OQ-6 ratified):** `~/.unimatrix/<projectHash>/remote.json`, mode 0600, written and re-chmod'd (the `writeRemoteSettingsLocal` 0600 pattern, `init.js:244-253`). The store is **colocated** in the existing per-project directory alongside the UDS socket and state dir (`unimatrix.sock`, `hook-client/`) — it is **not** a separate `credentials.json` and **not** a new XDG path. The filename `remote.json` names what it holds: the remote-attach credential + endpoint payload for this project. A **per-project file**, not a global slug→entry map: each project's hash directory holds exactly one `remote.json`, matching the existing per-project state layout. Idempotent re-`init` is a single-file rewrite; two attached projects yield two hash directories (AC-08b satisfied by directory separation, not in-file keying). No-homedir → null/terminal, mirroring `socketPathFor` (`config.js:189-200`).

**Not XDG `~/.config/unimatrix/`.** The precedent root is `~/.unimatrix/`; splitting credentials into `~/.config/` while state stays in `~/.unimatrix/` forks the per-project root and breaks the single-derivation invariant the hook client relies on (`socketPath` and `stateDir` share one root by construction). One root, one hash, one place.

### Consequences

**Easier:** both consumers resolve with the derivation they already compute (no slug plumbing into the hook client); the store co-locates with existing per-project state under one shared root; per-project files make 0600 scoping and idempotent re-init trivial; the keying ambiguity is closed before any code (SR-08 mitigated).

**Harder:** `projectHash` is a function of the realpath'd project root — a project moved/symlinked to a new path hashes to a new directory and would need re-`init` (this already true for the existing socket/state dir, so no new class of breakage). The slug↔projectHash relationship is implicit (slug lives in `mcp_url`); anyone debugging must know the slug is payload, not key.

Related: ADR-004 (this feature, the schema stored at this key), ADR-001 (this feature, bridge reads `mcp_url` from here), ADR-005 (this feature, scope independence — store lands first). Precedent: `config.js` `~/.unimatrix/<projectHash>/` per-project keying; entry #5107 (relocate-don't-gitignore; inventory all consumers; agree on the key).
