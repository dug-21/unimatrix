# Agent Report: crt-057-agent-2-spec

**Role:** Specification writer
**Feature:** crt-057
**Output:** `product/features/crt-057/specification/SPECIFICATION.md`

## Summary

Produced SPECIFICATION.md to the LOCKED three-axis contract. No conflicts found against the contract.

- 23 functional requirements (FR-1..FR-23): each of the three axes + all compositions (force+flag, memo-hit parity, degraded/Reconstructed path), capture advisory (FR-17, OQ-3=yes), WARN-and-proceed incompleteness (FR-18, OQ-2), four consumer-reconciliation sites (FR-19..FR-22), ADR amendment (FR-23).
- 8 non-functional requirements (NFR-1..NFR-8): restored AC#10 ≥80% token reduction (measurable), no new persistence path incl. reclamation-without-extraction, bounded residency envelope, hot-path safety, secrets posture, lockstep integrity, exhaustive retention match, lint hygiene.
- 16 acceptance criteria (AC-01..AC-16), each with a verification method, mirroring/refining SCOPE ACs.
- Domain model naming the three axes, non-extractive vs extractive review states, report-source = durable observations, candidates-source = in-memory buffer one-shot, provenance/degradation and reclamation vocabulary.
- Constraints CON-1..CON-8 folding SR-01..SR-12; CON-1 = consumer-reconciliation atomic unit; CON-2 = four-site lockstep.
- Out of scope: NG-6 in-summary distillation → ass-090 (#896).

## Open questions surfaced for architect

- OQ-A: `"summary"` alias drop-vs-fold (CON-5 / SR-06).
- OQ-B: OQ-2 refuse-vs-warn final call (FR-18 / AC-13).
- OQ-C: capture-advisory wording/placement (FR-17).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` (crt-057) — #4750, #4850, #5037/#5051/#5042, #4848/#4856, #3800, #5021. Confirmed report is observation-derived; candidates response-transient/assembly-level.
- Stored: Nothing (read-only tier; feature-specific; D-3 ADR belongs to architect).
- Declined: Storing spec interpretations as patterns/ADRs — not generalizable; #4750 already exists.
