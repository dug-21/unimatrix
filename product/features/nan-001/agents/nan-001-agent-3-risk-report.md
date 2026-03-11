# Agent Report: nan-001-agent-3-risk

## Agent
- **ID**: nan-001-agent-3-risk
- **Role**: Risk Strategist (Architecture-Risk mode)
- **Feature**: nan-001 (Knowledge Export)

## Artifacts Produced

- `/workspaces/unimatrix-nan-001/product/features/nan-001/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 4 |
| High | 4 |
| Medium | 6 |
| Low | 1 |
| **Total** | **15 risks, 37 test scenarios** |

## Top Risks for Human Attention

1. **R-01 (Critical)**: Hardcoded column list divergence from actual schema. The explicit column mapping (ADR-002) means any schema change silently breaks export completeness. Every column of every table must be verified in tests.

2. **R-03 (Critical)**: JSON-in-TEXT double encoding. Four columns across agent_registry and audit_log store JSON as TEXT. If the export code parses then re-serializes these values (instead of emitting the raw string), the round-trip corrupts agent capabilities and audit target_ids.

3. **R-05 (Critical)**: Transaction isolation. The entire export must run within a single BEGIN DEFERRED transaction. If the implementation accidentally releases the connection mutex or commits between table reads, concurrent server writes produce cross-table inconsistency.

4. **R-04 (Critical)**: NULL encoding. Five nullable columns across entries and agent_registry. If any are omitted from JSON (instead of serialized as `null`), nan-002 cannot distinguish "NULL" from "not exported."

## Open Questions

1. Does enabling `serde_json`'s `preserve_order` feature cause any existing `unimatrix-server` test to fail? ADR-003 identifies this as a risk but defers verification to implementation.
2. Should the export verify that the database is actually at schema v11 before proceeding, or trust that `Store::open()` migrated correctly?
3. If export encounters an entry_tags row with an entry_id not in the entries table (orphaned data), should it emit the row or skip it? Current architecture says emit — nan-002 handles validation. This should be confirmed.

## Scope Risk Traceability

All 9 scope risks (SR-01 through SR-09) traced. See the Scope Risk Traceability table in RISK-TEST-STRATEGY.md.

## Knowledge Stewardship

- Queried: No Unimatrix MCP tools available in this agent context. Historical intelligence queries could not be executed.
- Stored: Nothing novel to store -- first feature in the Nanoprobes phase; no cross-feature risk patterns visible yet.

## Self-Check

- [x] Every risk has a Risk ID (R-01 through R-15)
- [x] Every risk has at least one test scenario
- [x] Severity and likelihood assessed for each risk
- [x] Integration risks section present and non-empty
- [x] Edge cases section present and non-empty (10 cases)
- [x] Failure modes section describes expected behavior under failure
- [x] RISK-TEST-STRATEGY.md written to feature root
- [x] No placeholder risks -- each risk is specific to nan-001
- [x] Security Risks section present -- untrusted inputs and blast radius assessed
- [x] Scope Risk Traceability table present -- every SR-XX risk has a row
- [x] Knowledge Stewardship report block included
