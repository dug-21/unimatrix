# Agent Report — vnc-045-agent-2-spec

**Role:** Specification writer
**Artifact:** `product/features/vnc-045/specification/SPECIFICATION.md`

## Summary
Produced SPECIFICATION.md from SCOPE.md (14 SDs binding), ass-093 (mechanism) and ass-094 (authorization/anti-poison) FINDINGS, and the SCOPE-RISK-ASSESSMENT. All 9 acceptance criteria (AC-01..AC-09) expanded into testable form with explicit verification methods; 14 functional + 8 non-functional requirements; `context_tag` op input model, `protected_tags` config domain model, and audit-record model defined; ubiquitous language section fixes fast-path/lockdown/value-hygiene/audit-as-primary/enterprise-seam vocabulary.

## Key decisions / interpretations
- Modeled the op with exactly two actions (`add`/`remove`); single-value **replace** is `add`-on-a-`single_value:true`-prefix, not a third action — keeps the capability layer un-split (SD-6).
- Value-hygiene boundary stated precisely per instruction: free-form bypasses policy; protected-prefix rejected ONLY on value ∉ allowed_values; authorization is `Capability::Write` in ALL cases; no elevated-trust gate.
- Honored all four non-goals explicitly: no evidence enforcement, no anti-self-attestation, no agent_id/identity authorization, no content mutation. `min_trust_level` retained as inert enterprise seam only.
- Folded SR-01..SR-10 into NFRs, constraints, and open questions (derived-state blast radius, replace atomicity, five-site threading, daemon-path decision, merge replace-vs-merge, outcome-tag non-conflation).
- Pulled tool-description-honesty (FR-14/AC-09) directly from lesson #4303 and `agent_id: Option<String>` requirement from lesson #1301.

## Ambiguities found (routed to architect as design-time, not open scope)
1. Cadence-guard state model unspecified (in-memory vs persisted, per-slug scoping) — SR-08.
2. `merge_configs` replace-vs-merge for the policy list has a security edge (silent allow-list inheritance) — SR-09; spec recommends default-replace + ADR.
3. Daemon-path (`main.rs:~980`) reads global config directly while only the per-slug HTTP loop calls `resolve_slug_config` — `protected_tags` may be silently inert on the daemon path; requires a deliberate documented decision — SR-06/A2. Highest-likelihood silent miss.

None block the spec; all are architect design decisions with defaults noted.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` -- surfaced #4303 (bounded-capability tool descriptions must not overclaim), #1301 (Params struct needs `agent_id`), #317/#275 (MCP handler context ceremony), #360 (`entry_tags` FK CASCADE), #4451 (trust-level gating precedent, not activated per SD-7). Applied as cited in the spec.
- Stored: nothing (read-only tier; spec decisions are feature-specific, no generalizable pattern not already captured).
