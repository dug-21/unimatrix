# Security Review: bugfix-473-security-reviewer

## Risk Level: low

## Summary

The fix replaces a shared `remaining_capacity` budget computation with an independent
`MAX_INFORMS_PER_TICK = 25` constant for Informs edge selection in the background graph
inference tick. All changes are confined to one internal, non-exposed service module.
No security concerns were found. The change is minimal, does not touch trust boundaries,
input parsing, access control, or serialization paths.

## Findings

### Finding 1: ADR-002 crt-037 Not Formally Superseded in Unimatrix
- **Severity**: low
- **Location**: Unimatrix entry #3955 (ADR-002 crt-037: Combined Cap with Informs Second-Priority Ordering)
- **Description**: The fix intentionally overturns the shared-cap design documented in ADR-002
  crt-037. The developer (473-agent-1) noted this ADR is "now superseded" but did not formally
  deprecate or correct entry #3955 in Unimatrix. Future agents running a briefing may receive
  stale knowledge suggesting the combined-cap design is still active.
- **Recommendation**: Deprecate or correct entry #3955 to point to the new independent-cap
  design. The fix-agent stored pattern entry #3969 which captures the anti-pattern, but the
  original ADR still describes the removed design as the current approach.
- **Blocking**: no

### Finding 2: Throughput Increase — Combined Tick Output Can Exceed Old Cap
- **Severity**: low
- **Location**: `nli_detection_tick.rs`, Phase 5 (lines 417-431 in post-fix file)
- **Description**: Under the old design, total edges written per tick was bounded by
  `max_graph_inference_per_tick` (a config field). Under the new design, total edges written
  per tick is bounded by `max_graph_inference_per_tick + MAX_INFORMS_PER_TICK`. In the worst
  case this is the configured Supports cap plus 25 Informs edges. This is an intentional
  design change, not a bug, but it represents a throughput increase that could affect
  write-pool load if the configured cap is large. No DoS risk is present because both
  bounds are statically constrained constants/config values with no external influence.
- **Recommendation**: Verify the `max_graph_inference_per_tick` default in `InferenceConfig`
  is set with awareness of the new combined throughput ceiling. No immediate action required.
- **Blocking**: no

## OWASP Concern Checklist

| Concern | Assessment |
|---------|------------|
| Injection (SQL/command/path) | Not applicable — no new query construction, no external input in changed code |
| Broken access control | Not applicable — Phase 5 is internal scheduling logic; no trust boundary crossed |
| Security misconfiguration | Not applicable — `MAX_INFORMS_PER_TICK` is a compile-time constant with documented rationale |
| Vulnerable components | Not applicable — `rand 0.9.2` (rand_chacha 0.9.0, ChaCha20) is a pre-existing dependency; no new dependencies introduced |
| Data integrity failures | Not applicable — shuffle + truncate preserves the existing DB-level dedup (Phase 4b seen_informs_pairs + existing_informs_pairs pre-filter) |
| Deserialization risks | Not applicable — no deserialization in changed code |
| Input validation gaps | Not applicable — no new external inputs; `MAX_INFORMS_PER_TICK` is an internal constant |
| Hardcoded secrets | None found — no credentials, tokens, or keys anywhere in the diff |

## Blast Radius Assessment

If `MAX_INFORMS_PER_TICK = 25` contains a subtle bug (e.g., set too high or too low):

- **Too high**: More Informs edges written per tick, increasing write-pool pressure. Failure
  mode is performance degradation, not data corruption or information disclosure. Safe.
- **Too low (including 0)**: Informs edges starved — equivalent to the original bug. Graph
  connectivity for Informs edges degrades silently over time. No crash, no panic, no data
  loss. Detectable via log field `informs_candidates_accepted`.
- **Shuffle RNG failure**: `rand::rng()` delegates to `rand_chacha` (ChaCha20). Failure here
  would panic; however, this is a pre-existing code path already used in `select_source_candidates`
  and tested across 2583 server tests. No new failure surface.

Worst case: a regression to Informs starvation equivalent to the original bug. This is
detectable via monitoring and has no security implications — it is a graph quality issue,
not an integrity or confidentiality issue.

## Regression Risk

- **Low**. The change is confined to Phase 5 of `run_graph_inference_tick`. The Supports path
  is unchanged (same sort, same truncate-to-config-cap). Phase 6 onward is unchanged. The
  Phase 4b dedup pre-filter is unchanged. Six old tests encoding the broken behavior were
  correctly removed and replaced with five tests asserting the new invariant.
- Pre-existing test suite (2583 tests, 22/22 smoke, 13/13 contradiction, 41/41 lifecycle)
  passed. No regressions observed.

## Dependency Safety

- No new dependencies introduced.
- `rand 0.9.2` is a pre-existing dependency. `rand_chacha 0.9.0` (ChaCha20 CSPRNG) is the
  backing RNG. No known CVEs apply. The shuffle is used for fairness across ticks, not for
  cryptographic purposes — CSPRNG is appropriate but not required here.

## Minimal Change Verification

- PASS. All 223 diff lines are confined to `nli_detection_tick.rs`.
- Five documentation/report files added under `product/features/bugfix-473/` (agents and
  reports dirs) — these are workflow artifacts, not code changes.
- No Cargo.toml changes, no schema changes, no other crates touched.

## PR Comments
- Posted 1 comment on PR #474
- Blocking findings: no

## Knowledge Stewardship

- Stored: nothing novel to store — the generalizable anti-pattern (never steal low-priority
  budget from high-priority fill in a shared tick cap) was already stored by 473-agent-1 as
  entry #3969. The stale-ADR observation (Finding 1) is PR-specific housekeeping, not a
  recurring anti-pattern warranting its own lesson entry.
