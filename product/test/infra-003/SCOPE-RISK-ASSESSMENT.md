# Scope Risk Assessment: infra-003

> Test-only feature. Risks are framed as test-integrity traps (false-RED / false-GREEN /
> vacuous-pass), harness/dependency fragility, and scope-boundary creep — NOT production defects.
> Historical evidence: #5177 (vacuous-pass on load-bearing ACs), #2758 (named-but-absent test),
> #4473 (warn+continue masks failure-path), capability N3 #5161.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `sqlite3` / `vol` busybox sidecar absence yields a silent empty capture that empty-passes the negative control (false-GREEN). | High | Med | Spec MUST require hard-INFRA on absent sqlite3 (AC-08); architect: provision sqlite3 in the harness image like node, assert presence before any read. |
| SR-02 | WAL `-wal`/`-shm` sidecars not copied with the db → pre-checkpoint false-empty snapshot; positive control fails RED or negative reads stale-empty. | High | Med | Reuse the `cloud-bundle-lib.sh` `vol cat` idiom verbatim incl. WAL sidecars; never re-implement the copy. |
| SR-03 | Durability-barrier timing (#5321): a pre-barrier two-store read is a non-verdict but could be mistaken for one. | High | Med | Treat pre-barrier reads as INFRA, never a verdict (AC-06/AC-15); make the barrier an explicit, observable step in the harness flow. |
| SR-04 | Cert-pinned bearer / per-slug credential plumbing for two slugs (A + B) is more surface than the single-slug smoke; B-credential reuse of A's token would violate transport identity (#4950). | Med | Med | Read B on-disk via `vol` for the store-separation assertion (simpler, sufficient); only use B's own credential if reading over the wire (AC-07). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Overclaiming N3 (#5161) as proven/closed — the standing N5 (#788) regression gate is unwired; this is point-in-time only. | High | Med | Spec must state N3 stays `partial`; capability evidence wording = "advances, does not close" (Non-Goals, Tracking). |
| SR-06 | Reintroducing a parity shape (the removed D6, #845) — e.g. probe-for-probe symmetry with the UDS leg. | Med | Low | No parity-matrix entry, no UDS behavioral probe; ADR-006 `FORBIDDEN_IN_LOCAL` is *referenced as proof*, never re-run (AC-10). |
| SR-07 | Marker collision between observe and MCP-write surfaces → cross-attributed verdicts (can't tell which surface a marker proves). | High | Med | Spec must mandate two *distinct* markers; each surface's positive/negative verdict independently attributable (AC-15). |
| SR-08 | Slug allowlist constant drift (SR-09 / #4975) — re-typed regex/slug diverges from authoritative ADR-004 allowlist. | Med | Med | A reuses existing `arch-research`; B is an allowlist-valid literal; never re-type the ADR allowlist value (AC-09); ADR is authoritative. |
| SR-09 | Scope creep into H2 (pytest orchestrator dual-leg) or a new scaffold instead of cumulative shell extension. | Med | Low | Hold to H1: cumulative extension of infra-001 harness; defer H2; pick shell vs pytest in design (Q3) but no re-architecture. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-10 | The `debug_assert!` store↔adapter equality is compiled OUT of the release container — the whole motivation for the MCP probe; if the probe is weak/vacuous the shipped artifact keeps zero MCP-write isolation coverage. | High | Med | The MCP two-store content read is load-bearing, not optional — design it with the same rigor as observe; a `du` delta or success-RPC-only check is insufficient (must read content). |
| SR-11 | Single-restart registration ordering (#5079): routing is read once at boot; if B is registered after the restart its `ProjectEntry`/route won't exist → negative control reads a non-existent B store and may false-pass. | High | Med | Register BOTH A and B before the one restart (AC-01); assert both `/v1/A/observe` and `/v1/B/observe` route post-restart before any write. |
| SR-12 | Cumulative extension fragility: reusing infra-001/posture-smoke machinery risks coupling to its Gates 1–4 flow; a change there could silently alter this gate. | Med | Med | Extend, don't fork; keep the new isolation gate's assertions self-contained so an upstream smoke change surfaces as an explicit failure, not a skip. |

## Assumptions

- **(Goals 2–3, AC-03/AC-12, Q2 open)** The unique marker is queryable in a concrete table/column of the per-slug `unimatrix.db`. If no field round-trips into a queryable column, the positive control cannot be a content read — Q2 must be resolved in spec, not deferred to the tester (history #5177: deferred-because-unobservable → expose the seam, never downgrade the AC).
- **(Background §"Shared seam")** The MCP path's per-slug `McpAdapter` writes land in `entry.store` (construction invariant). The test exists *because* this is only a `debug_assert` in release — so it must be treated as unproven until the probe drives it, never assumed.
- **(Goal 1, AC-02)** The genuine funnel `parse_project_key → resolve_store → dispatch_request` is exercised by the real HTTP route, not a test shortcut; a `204`/success alone does not prove the write landed (must pair with content read).

## Design Recommendations

1. **Positive-gates-negative is the central integrity invariant** (AC-05/AC-14, SR-10): spec each surface so an absent A-marker fails RED before `landed_only_in_a` is ever reported. A "B unchanged" pass on a silently-failed A-write is the worst outcome — worse than no test (Problem Statement).
2. **Every verdict row must cite a check that would FAIL if the property broke** (#5177, #2758): no `du`-delta, no `other_count`/dir-count heuristic (replaced not extended, ass-084 OoS#2), no success-RPC-only MCP check — content reads only (SR-01, SR-02, SR-10).
3. **Resolve Q2 (marker field/table) and Q4 (slug B) in the spec** so the tester implements against a named column and literal slug — not anticipated names (SR-08, marker assumption).
4. **Provision + presence-assert the read dependencies first** (SR-01, SR-03): sqlite3 + `vol` sidecar + durability barrier are preconditions; their absence/mis-order is INFRA, never a verdict — warn+continue is forbidden (#4473).
