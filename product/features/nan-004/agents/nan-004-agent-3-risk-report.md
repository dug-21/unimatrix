# Agent Report: nan-004-agent-3-risk

## Mode
Architecture-Risk

## Artifact Produced
`product/features/nan-004/RISK-TEST-STRATEGY.md`

## Risk Summary
- **Critical**: 2 risks (R-01 settings.json merge corruption, R-02 absolute path invalidation)
- **High**: 5 risks (R-03 binary portability, R-04 idempotency, R-07 CI toolchain, R-09 .mcp.json merge, R-15 publish order)
- **Medium**: 6 risks (R-05 shim routing, R-06 version drift, R-08 postinstall failure, R-11 project root divergence, R-13 require.resolve, R-14 malformed JSON)
- **Low**: 2 risks (R-10 skill overwrite, R-12 binary rename)
- **Total**: 15 risks, 51 test scenarios

## Top Risks for Attention

1. **R-01 (Critical)**: The `merge-settings.js` module is the highest-risk component. It manipulates user configuration with nested JSON structures, must handle pre-rename and post-rename commands, and must never drop data. ADR-004's prefix-match approach is sound but the implementation needs exhaustive edge-case testing. Historical entry #320 (backward-compatible nested JSON patterns) confirms this class of problem requires careful intermediate-representation design.

2. **R-02 (Critical)**: Absolute paths (ADR-001) solve the PATH resolution problem but create a fragility: every `node_modules` reinstall or project move breaks all hooks and the MCP server. The mitigation (re-run `npx unimatrix init`) is acceptable but the failure mode must produce clear diagnostics, not cryptic "file not found" errors.

3. **R-15 (High)**: npm publish ordering in CI is easy to get wrong. If the root package publishes before the platform package, early installers get a broken package. The workflow must enforce platform-first ordering with failure gating.

## Scope Risk Traceability
All 10 SR-XX risks traced. SR-01/02/03/04/08/09/10 map to architecture risks. SR-05/06/07 resolved by UX or delivery phasing (not testable risks).

## Open Questions
1. Is the ONNX runtime statically linked in release builds? Architecture open question #1 must be answered before CI can be finalized. R-03 severity depends on this.
2. Should `merge-settings.js` acquire a file lock? Concurrent init runs are unlikely but could corrupt settings.json.
3. How should init handle paths with spaces in hook command strings? JSON handles it, but the shell `command` field in settings.json executes as a shell string.

## Knowledge Stewardship
- Queried: /knowledge-search for "lesson-learned failures gate rejection" -- found #1105, #1045 (pass outcomes, not directly relevant)
- Queried: /knowledge-search for "risk pattern" category:pattern -- found #1009, #550 (agent classification, workflow-only scope; not directly relevant)
- Queried: /knowledge-search for "npm packaging binary distribution" -- found #349 (Cargo features lesson), #277 (sequential migration ADR); #349 informs R-07
- Queried: /knowledge-search for "settings.json merge JSON configuration" -- found #320 (intermediate serialization for backward-compatible nested JSON); directly informs R-01
- Stored: nothing novel to store -- first packaging/distribution feature, no cross-feature pattern visible yet
