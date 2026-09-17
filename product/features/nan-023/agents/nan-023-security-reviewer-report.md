# Security Review: nan-023-security-reviewer

## Risk Level: low

## Summary
Fresh-context review of PR #996 (per-harness config wiring for claude-code / opencode / codex-cli, surgical TOML writer, hook-client `--provider` hint). No secrets are written to any config, all command/argv construction is properly escaped or uses argv arrays, path containment and fail-safe posture hold, and zero new dependencies are introduced. No blocking findings.

## Findings

### F-1: Internal-path double-quote not escaped in hook command string
- **Severity**: low (informational)
- **Location**: `packages/unimatrix/lib/merge-settings.js:98` (`buildHookClientCommand`) and `:126` (legacy `LD_LIBRARY_PATH` form)
- **Description**: The command string quotes a whitespace-containing path but does not escape an embedded `"`. A literal double-quote would break shell parsing. The interpolated values are the **package install path** (`require.resolve` of `hook-client/index.js`) and the resolved binary dir — never the project root or external input — so a project path containing metacharacters (the R-10 scenario) does not reach this. Pre-existing 2-arg behavior, unchanged by this PR.
- **Recommendation**: None required. If ever hardened, escape embedded quotes; not warranted at current likelihood.
- **Blocking**: no

### F-2: Lexical (non-realpath) containment check
- **Severity**: low (informational)
- **Location**: `packages/unimatrix/lib/opencode-install.js:85` (`isWithinProject`), reused by codex writers
- **Description**: Containment uses lexical `path.resolve` + `startsWith(root + sep)`; it does not resolve symlinks. Write targets are fixed constants joined to `dir` (no `..`), so the guard is defense-in-depth and always passes for these paths. A symlinked `.codex`/`.opencode` planted in the tree could be followed by `writeFileSync`, but that requires pre-existing control of the checkout — the same exposure as any in-project writer.
- **Recommendation**: Accept for this threat model (developer running `init`/`wire` in their own repo). Matches existing `init.js` behavior.
- **Blocking**: no

## Positive Confirmations
- **Principle 8 / R-03 (no secret in config)**: No token/url/bearer written anywhere. Codex + opencode cloud both emit token-free stdio bridge `command="node", args=[bridge, hash]`. In-memory token in `bin/unimatrix.js resolveWireTransport` is discarded by `wire.js resolveTransport` (presence-only signal). Bare `url` refused as a write source.
- **R-10 (command/argv injection)**: All codex TOML values routed through `tomlString()`; re-parse round-trip proven in `codex-toml-surgical.test.js`. Opencode MCP `command` is an argv array (no shell). Codex hooks command JSON-serialized.
- **Path traversal**: shipped-skill filename `..` throw retained; containment guard on every write.
- **Fail-safe/non-clobber (R-08/R-11)**: opencode/codex warn-and-skip, never throw; malformed input byte-preserved; claude loud-checkpoint preserved (SR-07).
- **Intent gate (R-12)**: single self-gating site; malformed check precedes gate.
- **Zero new dependencies**: package.json/lockfiles unchanged; all new requires relative or node builtins.

## Blast Radius Assessment
Worst case of a subtle writer bug is bounded to a single project-local config file (`.mcp.json`, `.claude/settings.json`, `opencode.json`, `.codex/config.toml`, `.codex/hooks.json`). Fail-safe posture (preserve + skip) means the failure mode is a non-write / visible skipped leg, not silent corruption, data disclosure, or privilege escalation. No network write path, no deserialization of untrusted remote data, no credential persisted to disk.

## Regression Risk
Low. claude-code writers are reused unchanged behind `wire.js`; backward-compat golden fixtures (`test/fixtures/wire/golden/`) present and `buildHookClientCommand` 2-arg call site returns byte-identical output. The `--provider` hint path is additive and fail-open (unknown hint ignored). `--force` is definitions-only and never forwarded to the wire layer.

## PR Comments
- Posted 1 comment on PR #996 (https://github.com/dug-21/unimatrix/pull/996#issuecomment-5706796935)
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store — findings are PR-specific and non-blocking; the token-free-bridge and tomlString-escaping patterns are already documented in the feature ADRs (ADR-002 / Principle 8). No recurring cross-feature anti-pattern surfaced.
