# Agent Report: nan-024-agent-1-pseudocode (uni-pseudocode, Stage 3a)

Unimatrix entries consulted: #5888 (resolved to #5889 ADR-001), #5875 (#5890 ADR-002), #5887 (#5891 ADR-003), #5886 (#5892 ADR-004), #5878 (#5893 ADR-005), #5882 (#5894 ADR-006), #5763, #5774, #5770, #5765, #5737 (briefing/pattern search)

## Report
Files (all under /workspaces/unimatrix/product/features/nan-024/pseudocode/): OVERVIEW.md, shim.md, shim-entry.md, install-store.md, resolve-runtime.md, trust.md, project-state.md, launcher.md, hook-ownership.md, merge-settings.md, wire-init.md, codex-install.md, opencode-install.md, hook-client.md, ci-and-docs.md.
Headless contract (D-053) is ci-and-docs Part B (named explicitly). Q9 proof not redone (exists; D-072 fallback applied in `lib/mcp-entry.js` for every Windows install).
Language flags: no crates/ + packages/ span. Mixed: shim (sh + cmd), ci-and-docs (yml/md plus JS in Part B). New helper files added within components: `lib/hook-route.js`, `lib/status.js` (launcher), `lib/claude-json.js`, `lib/mcp-entry.js` (merge-settings).
Wave plan: in OVERVIEW.md.

## Open questions / gaps
1. OQ-1 shim-entry trust bootstrap: ADR says shim-entry runs the trust check but `lib/trust.js` lives in the package it is vetting. Resolved in pseudocode by installing a sibling `~/.unimatrix/bin/trust.js` copy. Needs architect confirmation.
2. OQ-2 D-057/D-066 "init/upgrade preflight exit 1": init re-records node, so the preflight is vacuous for init; specified as effective for `wire` and upgrade-without-re-record only.
3. OQ-3 `config.js`/`remote-resolve.js` now require `../project-state.js`; confirm `test/check-zero-deps.js` and `check-hook-client-size.js` allow it.
4. OQ-4 `.gemini/settings.json` / README replacement text for Rust `unimatrix hook` (D-074 says planned, replacement form unspecified); AC-11 grep rule defined by me (predicate unusable because abs shim is "owned").
5. OQ-5 Codex detection: hooks to `~/.codex/hooks.json` are only written when project `.codex/` exists or `--harness codex-cli`; Codex-only users without project `.codex/` get no hooks. Also `CODEX_HOME`/`XDG_CONFIG_HOME` not honoured (spec literal paths).
6. ADR-005 gap: pre-Node root-marker check misses git worktrees (Node maps worktree cwd to the main repo root, which may not contain cwd). Added clause (d) (ancestor `.git` file -> start Node). Also headless clause made looser (env set + PROJECT_ROOT non-empty -> Node) to stay a lossless superset. Windows junction cwd remains a residual.
7. Stable copy lacks the Rust binary: `pkg/<version>/` must also carry the platform binary package (Linux) or resolve-runtime's "installed binary" fails at fire time; install-store copies it into `pkg/<version>/node_modules`. Confirm size/cost acceptable.
8. Signature extensions (additive): `resolveRuntime.allowSelf`, `buildShimCommand(..., home)`, `isInstalled(..., {env, home})`, `effectiveEnv`. `effectiveEnv` added because a store credential in another repo would otherwise let exported bundle env win (leak).
9. `mcp-stdio` local: Node cannot `exec`; launcher spawns with inherited stdio (two processes), contradicting "exec"/"one process after exec" wording in R-07.
10. R-38: exact `~/.claude.json` project-key spelling (symlink/realpath, Windows `C:\` vs `c:/`) unverified; pseudocode reuses an existing matching key and otherwise writes realpath; CI check specified.
11. Trust check refuses symlinked redirect/install files (conservative); may affect #1025 dogfood workflows.
12. Hook-client exit code: client currently never sets nonzero for connection errors; D-042 "keep current codes" read as "unchanged".

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing, context_search (pattern: hook client shim wiring; decision: nan-024), context_get on ADR-001..006 -- ADRs consulted and applied; nan-023 patterns #5770 (mapToCanonical for --provider) and #5774 (codex-install fail-safe legs) respected.
- Deviations from established patterns: none (ADR deviations flagged above as open questions, not applied silently).

## Rework (agent nan-024-agent-1-pseudocode-rework)

Unimatrix entries consulted: #5896 (ADR-001), #5897 (ADR-003), #5899 (ADR-004), #5898 (ADR-005), #5890 (ADR-002), #5894 (ADR-006) via DECISIONS.md/ARCHITECTURE.md/ADR-INDEX.md; briefing for nan-024 (#3611, #3612 stale-surface lessons applied: every signature change propagated to all files).

Files changed (pseudocode/): OVERVIEW.md, shim.md, shim-entry.md, install-store.md, trust.md, resolve-runtime.md, project-state.md, launcher.md, hook-client.md, merge-settings.md, wire-init.md, codex-install.md, ci-and-docs.md.
Applied: D-077 (launcher mcp-stdio spawn, exit code + signal mirrored, no stdout before spawn); D-078 (install-store copies Linux platform binary package into pkg/<version>/node_modules; size in 3c); D-079 (pre-check-only clauses; Node predicate unchanged; superset test specified in shim.md); D-080 (allowSelf/home/{env,home} accepted; effectiveEnv isolated and flagged PENDING H1 with call sites and removal recipe); D-081 (detectsCodex, codexHome, $CODEX_HOME destinations; DEFAULT flagged cases); D-082 (NodeNotRecordableError for init; separate path-exists preflight for wire/upgrade); D-083 (architect forms and AC-11 grep verbatim); ADR-002 trust bootstrap marked PENDING H1 in shim-entry, install-store, trust; ci-and-docs split Part B -> uni-js-dev, Parts A/C -> CI/docs agent, wave plan updated.

Remaining open: pending H1 (trust bootstrap; effectiveEnv); DEFAULT flagged (relative/invalid CODEX_HOME; Codex MCP follows hooks detection, opt-in gate dropped); OQ-3 (zero-deps/size checks vs project-state require); R-38 key spelling; symlinked redirect refusal vs #1025; Windows pre-check junction residual; Gemini Windows shell semantics.

## Knowledge Stewardship (rework)
- Queried: mcp__unimatrix__context_briefing -- ADRs and stale-surface lessons; deviations from established patterns: none.
