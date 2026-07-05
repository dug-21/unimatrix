# Agent Report: vnc-044-agent-3-risk (architecture-risk mode)

**Deliverable:** `product/features/vnc-044/RISK-TEST-STRATEGY.md`

## Summary

14 risks identified against the concrete architecture (`resolve_graph_output`, `NodeSummary`/`GraphSummaryProjection`, `response/verbosity.rs` primitives, the `graph_read.rs:251` parse-and-drop seam). 4 Critical, 3 High, 4 Medium, 3 Low. Every risk maps to concrete test scenarios; every SR-01..SR-09 scope risk is traced.

## Top Risks (human attention)

- **R-01 (Critical) — UTF-8 preview flooring.** Naive `&content[..256]` panics on a multibyte codepoint straddling byte 256 — a request-triggered DoS on attacker-influenceable stored `content` (evidence #3706 shipped this exact panic; #4350: char-count cannot enforce a byte cap). Boundary table (empty/255/256/257/straddle) is the non-negotiable bar.
- **R-02 (Critical) — `content_truncated` byte-compare.** Must be `content.len() > 256`, not "did flooring move the index." The 257B-ASCII-floors-to-256 case is a silent false-negative that strands the agent without the `context_get` signal.
- **R-03 (Critical) — default-summary across ALL five node-bearing modes.** #913 cites only subgraph; the projection fans out to chain/current/inverse/filter, each with distinct envelope metadata. `current` is a single non-Vec node. Architect flagged this as OQ-3. Per-mode coverage with metadata-preservation assertions required.
- **R-04 (Critical) — `detail=full` byte-for-byte.** Golden-payload equality vs the pre-change binary (evidence #3426: serialization overhauls under-estimate ordering regression).
- **R-05 (High) — `format=markdown` uniform rejection.** Must reject on all seven modes (resolver runs pre-dispatch); risk is `neighbors`/`path` silently returning JSON.
- **R-06 (High) — shared-type guard (SR-06/07).** No `skip_serializing_if` on `EntryRecord`/`EdgeRecord`, no `ResponseFormat` variant; ~45-site blast radius (#4831). Code-review gate, not just tests.
- **R-11 (Med, doc gate) — lifecycle vs delivery status (SR-09).** Verified by tool-description + AC-06 review, NOT a functional test. Do not treat delivery-status absence as a defect.

## Coverage Requirements (minimum bar)

- UTF-8 preview boundary table: empty / <256 / exactly-256 / 257-ASCII / multibyte-straddle-256 — all valid UTF-8, no ellipsis.
- `content_truncated` byte-compare, both sides of 256, incl. the 257-floors-to-256 trap.
- Default + explicit summary per each of the five node-bearing modes, asserting envelope-metadata preservation.
- `detail=full` golden byte-equality (subgraph + one more mode).
- `format=markdown` rejected on all seven modes; error asserted by reason substring (not verbatim — #3337).
- Present-AND-absent key-set assertions for both node and edge summary shapes.
- `format=summary` alias equivalence + conflict-with-explicit-`detail` rejection.
- `detail` accept-and-ignore identical output on `neighbors`/`path`.

## Notes for the tester

- Assert error copy by **substring** (`"markdown"`, `"format=json"`), not the full sentence — ADR-001, ADR-002, and SPEC quote the message with minor wording differences (pattern #3337).
- Build multibyte adversarial content programmatically (`char::from_u32`), not bare source literals (pattern #4769).
- A fuzz/property test over random multibyte content (no panic, always valid UTF-8) is recommended for R-01 (cf. #4863).

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` (context_search) for UTF-8 truncation, wire-enum blast radius, and regression/golden patterns. Found and applied: #3706 + #4350 (byte-slice panic / byte-cap enforcement → R-01/R-02), #4831 (enum-variant blast radius → R-06), #3426 (formatter section-order regression, golden-output test → R-04), #3337 (architecture-string divergence → R-13), #4863 + #4769 (no-panic on untrusted input, adversarial-string construction → Security section, tester notes).
- Stored: entry #5511 "Projection/serialization change scoped to N sibling handlers is systematically under-tested when the motivating incident cites only one" via `/uni-store-pattern` (category: pattern). Cross-feature pattern (vnc-044 + #3426 col-026 + #3337 crt-028) generalizing the OQ-3 fan-out coverage gap. Feature-specific risks live in RISK-TEST-STRATEGY.md, not Unimatrix.
