# Security Review: vnc-049-security-reviewer

## Risk Level: low

## Summary
The OpenCode observation harness (PR #989) is a defense-in-depth, fail-open addition. Command injection, SQL injection, path traversal, and C10 regression vectors are all closed. No blocking findings; one low-severity, non-blocking regression note on Mode B whole-document re-serialization.

## Findings

### F1 — Mode B re-serializes the entire opencode.json
- **Severity**: low
- **Location**: `packages/unimatrix/lib/opencode-install.js` `provisionPluginArray` / `writeJson`
- **Description**: When `opencode.json` exists, Mode B parses and re-serializes the whole document to append the `plugin[]` entry. Value-level preservation of `mcp.unimatrix`, the Ollama block, and non-Unimatrix keys is asserted (JSON.stringify equality per subtree, key order preserved by V8). True raw-byte preservation is not guaranteed for exotic strict-JSON formatting (numeric re-encoding, unicode escaping). JSONC/comment files are safely handled by the malformed→skip fail-safe; indentation is detected. Semantic content is preserved; idempotent after first run.
- **Recommendation**: None required. Optionally document that Mode B normalizes formatting on first run, or prefer Mode A (`.opencode/` dir) exclusively when both are viable.
- **Blocking**: no

### F2 — Ingest fail-loud canary is a debug_assert (compiled out in release)
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs` `extract_observation_fields`
- **Description**: The "opencode event with no source_domain stamp" canary is a `debug_assert!`, absent in release builds. No real risk: `derive_source_domain` is deterministic (`provider=="opencode"` → always `Some`), so the guarded condition is structurally unreachable.
- **Recommendation**: None.
- **Blocking**: no

## OWASP Assessment (per changed area)
- **Command injection (plugin shell-out)**: PASS. `emit.js` passes untrusted values as discrete Bun-Shell array args (auto-escaped) and stdin bytes — never string-built. Event name from fixed canonical map; provider is literal `"opencode"`; model charset-validated before inclusion.
- **SQL injection**: PASS. `source_domain`/`model_id` are parameterized binds (`?11`/`?12`) in single + batch INSERT; SELECTs use positional `row.get`. No data interpolation.
- **Input validation**: PASS. `model_id` `^[a-z0-9._/-]{1,128}$` enforced at three layers (JS shim, CLI, ingest), fail-open to NULL+warn. `source_domain` fixed literal, opencode-only stamp. Reject-to-NULL covers uppercase/space/control/`;`/over-length/empty.
- **Path traversal (installer)**: PASS. `isWithinProject` resolves+asserts every write target inside project root; plugin identity fixed; malformed config warned-and-skipped.
- **Deserialization**: PASS. New fields `#[serde(default)]` + `skip_serializing_if`; malformed bus payloads guarded and dropped (R-14). Frozen-fixture byte stability preserved.
- **Access control**: PASS. Subagent `agent_type` rides observe channel only, never lifted from spoofable tool args (R-07).
- **Secrets**: PASS. No hardcoded credentials. Content-free logger verified by tests (session id / path never logged). Only new dep is `@opencode-ai/plugin` peer dep pinned exact `1.18.31`; no known-CVE surface introduced.

## Blast Radius Assessment
Worst case is bounded to attribution correctness, not integrity/DoS/disclosure. The read-path prefer-stored fork only prefers a non-empty stored `source_domain`; NULL (every non-opencode + legacy row) falls through to the exact prior registry/DEFAULT resolution, so T-SEC-12/13 attribution for non-opencode rows is byte-for-byte unchanged. The ingest stamp is opencode-only and deterministic. Migration v31→v32 is additive nullable columns, idempotent (pragma pre-checks before either ALTER), transactional (rolls back to 31 on failure), no backfill. A subtle bug would misattribute opencode-vs-cloud model tagging at worst — no data corruption, no cross-provider contamination (proven by the mixed-row test).

## Regression Risk
Low. Installer is strictly additive and fail-safe; C10 sentinel (mcp.unimatrix + Ollama block) preserved at value level with passing tests. Non-opencode ingest and read paths untouched (opencode-only stamp + NULL-fallback fork). Fail-open posture throughout (plugin load, emit, every handler) means a harness fault degrades observation, never the OpenCode session or the existing pipeline.

## PR Comments
- Posted 1 review comment on PR #989 (comment, not request-changes)
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store — the patterns here (parameterized binds, three-layer charset validation, path-containment guard, fail-open shell-out via discrete args) are already established Unimatrix conventions correctly applied; no recurring cross-feature anti-pattern surfaced.
