# nan-024-agent-8-trust-win-fix report

Unimatrix entries consulted: #5900 (ADR-002), #5906, briefing set (#5790 family); DECISIONS D-029/D-036/D-090.

## Report
Files: packages/unimatrix/lib/trust.js, test/trust.test.js, test/compat-matrix.test.js.
Fixes in trust.js (sibling ~/.unimatrix/bin/trust.js is a verbatim copy by install-store; still builtins-only):
1. icacls /save file is BOM-less UTF-16LE; decode on b[1]===0.
2. DACL-only output: owner from one PowerShell spawn (.NET GetAccessControl/GetOwner SID; Get-Acl fails with module-load error in the harness env). Owner check kept. Spawns only at init / identity mismatch.
3. SDDL alias LA allowed only when current SID ends -500 (runneradmin); OW allowed.
4. defaultSpawn takes extra env; DACL line matched by /^(O:|D:)/m.
Compat matrix: new-bridge UNIMATRIX_BUNDLE run now sets UNIMATRIX_PROJECT_ROOT=tmp. Ran locally: 6 pass, 0 fail, 1 skipped (pre-existing).
Tests: trust.test.js + trust-selfcontained 32 pass (added captured DACL-only output, BOM-less decode, foreign/missing owner, LA alias).

CI (dispatch run 45743786835): wiring-e2e windows Node 20 and 24 PASS; latency-gate windows FAIL (BREACH only: cloud FNF p95 141 vs 79, sync 164 vs 79, not-a-project 81 vs 74; latency issue is separate).
Q9 run 45744124919: init (cloud attach) PASS, MCP D-047 fallback forms PASS, hooks PASS; mcp-primary (cmd wrapper) rows FAIL (probe rows, unrelated to trust; not investigated).

## Issues
- Windows latency gate breaches (agent latency work).
- Q9 mcp-primary FAIL rows: confirm whether these are expected-negative proofs.

## Knowledge Stewardship
- Queried: context_briefing, context_get #5900/#5906
- Stored: "Windows trust: icacls /save is BOM-less UTF-16LE DACL-only; owner via .NET GetAccessControl, LA alias" via context_store
