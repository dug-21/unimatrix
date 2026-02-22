# ASS-007: MCP Interface Specification (Track 3)

**Phase**: Assimilate (Research Spike)
**Source**: Pre-Roadmap Spike, Track 3
**Date**: 2026-02-20
**Status**: In Progress
**Depends On**: ASS-001 (D1), ASS-002 (D4), ASS-003 (D2), ASS-004 (D5/D5b/D5c), ASS-005 (D3), ASS-006 (D6a/D6b/D6c)

---

## Objective

Using the outputs of all prior tracks (D1-D6), write the complete MCP tool specification — the contract that everything depends on. This is D7: the interface that the roadmap is built to deliver.

## Inputs

| Deliverable | Source | Key Decisions |
|-------------|--------|---------------|
| D1: hnsw_rs Capability Matrix | ASS-001 | DistDot, FilterT closures, no deletion, non-atomic persistence |
| D2: redb Storage Pattern Guide | ASS-003 | Single DB, multi-table, MVCC, compound keys, bincode metadata |
| D3: Learning Model Comparison | ASS-005 | Metadata lifecycle wins (~95% value at ~10% complexity) |
| D4: MCP Integration Guide | ASS-002 | rmcp 0.16, stdio, instructions field, tool annotations |
| D5/D5b/D5c: Context Injection | ASS-004 | Three hard constraints, generic query model, dual retrieval |
| D6a/D6b/D6c: Config Surface | ASS-006 | Three-tier config, orchestrator-passes-context, user-scoped MCP |

## Deliverable

**D7: Complete MCP Tool Specification** — for every tool: exact description text, parameter schema, response format, annotations, behavioral notes, and version gate.

## Pre-Specification Research

- `research/TRACK-SYNTHESIS.md` — synthesis of all D1-D6 findings
- `research/SCENARIO-ANALYSIS.md` — scenario-driven design exploration (7 scenarios)

## Tracking

Specification document: `D7-MCP-INTERFACE-SPECIFICATION.md`
