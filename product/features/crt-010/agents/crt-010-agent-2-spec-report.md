# Agent Report: crt-010-agent-2-spec

## Agent
- **ID**: crt-010-agent-2-spec
- **Role**: Specification Writer
- **Feature**: crt-010 (Status-Aware Retrieval)

## Output
- **SPECIFICATION.md**: `product/features/crt-010/specification/SPECIFICATION.md`

## Summary
- **6 functional requirement groups** (FR-1 through FR-6) with 22 sub-requirements
- **4 non-functional requirement groups** covering latency, configuration, memory, compatibility
- **16 acceptance criteria** (AC-01 through AC-16)
- **11 domain model terms** defined
- **3 user workflows** documented (MCP flexible, UDS strict, compaction pruning)

## Key Decisions

1. **AC-IDs derived from scope goals**: SCOPE.md did not contain explicit AC-IDs. I assigned AC-01 through AC-16 mapping to each testable behavior: strict/flexible mode filtering, supersession injection mechanics, co-access exclusion, UDS hardening, compaction pruning, MCP asymmetry fix, and non-functional constraints.

2. **Empty results over wrong results (AC-10, FR-1.5)**: Per SCOPE goal 1 and SR-04, strict mode returns empty rather than falling back. This is the strongest stance and matches the principle "wrong information is worse than no information."

3. **Cosine similarity for injected successors (FR-2.4, AC-05)**: SCOPE listed this as a design decision needed. Specified option (a) — cosine from stored embedding — as the required approach. More accurate, and SR-01 latency constraint (15% p95 regression budget) provides the guardrail.

4. **Penalty bypass on explicit status filter (AC-14, FR-6.2)**: When an agent explicitly requests `status: Deprecated`, penalties are not applied. This preserves agent autonomy for deliberate historical queries.

5. **Engine crate decoupling (FR-3.3)**: Per SR-07, specified that co-access filtering uses `HashSet<u64>` or filter callback — no server-crate type imports into engine crate. Exact interface left to architect.

## Open Questions for Architect

1. **SearchService API shape**: FR-1.1 requires a retrieval mode parameter. Enum vs builder vs separate methods is an architectural choice. Recommend enum (`RetrievalMode::Strict | Flexible`) for clarity.

2. **Successor batch-fetch strategy**: FR-2.2 requires single store read. Architect should confirm whether `Store` supports multi-ID batch get or if this needs a new method.

3. **Penalty constant location**: NFR-2.1 places constants in `confidence.rs`. If engine crate restructuring is underway, architect may prefer a dedicated module.

## Self-Check

- [x] SPECIFICATION.md covers all scope components (1-6) and goals (1-5)
- [x] All 16 acceptance criteria are testable with described verification methods
- [x] Non-functional requirements include measurable targets (15% latency, specific penalty values)
- [x] Domain Models section defines 11 key terms
- [x] NOT in scope section explicit (9 exclusions)
- [x] Output in `product/features/crt-010/specification/` only
- [x] No placeholder or TBD sections
- [x] All 9 SR-XX risks addressed in constraints, requirements, or acceptance criteria
