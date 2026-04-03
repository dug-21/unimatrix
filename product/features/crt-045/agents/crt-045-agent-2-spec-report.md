# Agent Report: crt-045-agent-2-spec

## Task

Write SPECIFICATION.md for crt-045 — wire `TypedGraphState::rebuild()` into
`EvalServiceLayer::from_profile()` to eliminate the cold-start empty graph during eval runs,
fix `ppr-expander-enabled.toml`, and define a verification integration test.

## Output

`product/features/crt-045/specification/SPECIFICATION.md`

## Key Decisions

1. **Write-after-construction (option b) is the specified approach** — `rebuild()` is called
   before `with_rate_config()`, result is written into the handle post-construction. This avoids
   any signature change to `with_rate_config()`. SR-01 verification (Arc sharing model) is flagged
   as an open question for the architect to resolve before committing to this approach.

2. **`typed_graph_handle()` scoped to `pub(crate)`** — per SR-03. Delegated to
   `self.inner.typed_graph_handle()`. May use `#[cfg(test)]` guard if test-only.

3. **TOML schema fully specified** — `distribution_change = false`, `mrr_floor = 0.2651`,
   `p_at_5_min = 0.1083`. A comment explaining the intentional `false` is required to prevent
   future silent breakage (SR-04 mitigation).

4. **AC-06 test must invoke a live search call** — not merely inspect the handle (SR-05 mitigation,
   wired-but-unused anti-pattern from entry #1495). Specified as an explicit requirement in FR-07.

5. **Test snapshot must use Active entries with real edges** — not Quarantined/Deprecated entries
   (SR-06 mitigation, ADR-004 addendum entry #3768). Codified in C-09.

6. **Rebuild failure degrades gracefully** — cycle or I/O error logs `warn!` and leaves
   `use_fallback = true`. Returns `Ok(layer)`. AC-05 verification test specified.

## Open Questions (for Architect)

- **SR-01:** Verify `SearchService` holds `Arc::clone()` of `TypedGraphStateHandle`, not a value
  copy, before committing to write-after-construction. Read `services/mod.rs` lines ~432–434 and
  the `SearchService` constructor. If it is a value copy, option (a) is required and this
  specification must be revised with a constraint change flag.

## Self-Check

- [x] SPECIFICATION.md covers all AC-01 through AC-08 from SCOPE.md
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (5s rebuild, zero new test failures)
- [x] Domain Models section defines all key terms
- [x] NOT in scope section is explicit
- [x] Output file is in `product/features/crt-045/specification/` only
- [x] No TBD sections — all open questions identified and attributed
- [x] Knowledge Stewardship report block included in SPECIFICATION.md

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entry #4096 (cold-start pattern, directly
  applicable); entry #3768 (ADR-004 Quarantined filter, applied in C-09/FR-07); entry #3526
  (dual-type JSON boundary, applied in C-07/NFR-05). All three incorporated.
