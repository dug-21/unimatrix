## ADR-001 nan-022: Dimension-Keyed Parity Matrix Generalizing the nan-021 Single-Output Gate via One Dimension Registry

### Context
nan-021 (#836) proved exactly ONE C0 parity dimension (analytics/learning) by capturing one
output (`MetricVector`) from each transport leg and running one comparator
(`compare_metric_vectors`) modulo one closed exclusion set (`EXCLUDED`). #837 expands C0's
proof bar to SIX dimensions (retrieval, behavioral, analytics, proactive, precompact,
isolation). The constraint (AC-11/SR-04) is to EXTEND `infra-001` cumulatively — never a fork,
never a parallel scaffold. The nan-021 architecture (ADR-001 #5286 single-workload/one-identity/
one-token, pytest-as-orchestrator; ADR-003 #5293 closed-justified-exclusion comparator) already
solved the SR-05 drift hazard and the false-green hazard once; re-authoring re-introduces fixed
bugs. The risk is that six new parity surfaces are scaffolded as six near-duplicate ad-hoc
comparators that each re-list the dimension, drifting silently (#5302).

### Decision
Generalize the nan-021 single-output gate into a **dimension-keyed parity matrix** driven by ONE
authoritative dimension registry — same workload, same identity, same token, same orchestrator,
same closed-exclusion discipline; the only change is one → N.

(1) **`harness/parity_dimensions.py`** declares `DIMENSIONS: tuple[Dimension, ...]` — the SINGLE
authoritative enumeration of the six. Each `Dimension` (frozen dataclass) carries `id`,
`capture_key`, `wire_surface` (`mcp_bridge`|`hook_observe`), `comparator` (a
`DimensionComparator` subclass), `intra_transport_check: bool`, `blocks_c0_proof: bool`. EVERY
consumer — leg drivers, orchestrator, CI evidence table, forbidden-seed audit — iterates THIS
tuple; nothing else hand-lists the six (SR-05/#5302: single-source the contract, not just data).

(2) **One workload, more outputs.** The single `ParityWorkload` manifest + ONE stable session
identity + ONE run-correlation token (the SR-05/#832 root-cause defense) is preserved verbatim.
`drive_uds_leg` is extended to return a **dimension bundle** `{capture_key: capture, ...}`
instead of one `MetricVector`; the HTTPS smoke (`cloud_cycle_gates`) writes `{run_token,
dimension_bundle:{...}}` to `$HTTPS_VECTOR_OUT` instead of `{run_token, metric_vector}`. The
cross-process seam shape is unchanged; only the payload widens.

(3) **Pytest-as-orchestrator, extended.** The existing orchestrator drives both legs in ONE
invocation under one token, ingests both bundles (token-guarded `load_https_bundle`, never
empty), runs each dimension's comparator, and emits a per-dimension evidence table keyed by the
run token. A missing dimension ERRORS — never a vacuous pass (nan-021 R-03 carried to all six).

(4) **Analytics is CONSUMED, not re-proven** (AC-04 / Non-Goal): `MetricVectorComparator` wraps
`compare_metric_vectors`/`EXCLUDED` unchanged.

### Consequences
Easier: parity stays identical-by-construction (one manifest/identity/token); the registry makes
"the six" a single editable list so adding/auditing a dimension is one row; the extend-don't-fork
map gives spec a checklist to reject parallel scaffolding (AC-11); nan-021's solved hazards are
not re-litigated. Harder: `drive_uds_leg` and `cloud_cycle_gates` grow from one output to a
bundle (more wire surface, but each routed through the existing clients/bridge — ADR-005);
the bundle's on-disk schema becomes a real cross-language contract (Python ingest vs JS/shell
emit); a single registry edit changes behavior across all consumers (intended single-source).

Related: AC-01..AC-08, SR-04. Generalizes nan-021 ADR-001 (#5286), ADR-003 (#5293),
ADR-005 (#5290). Pairs with nan-022 ADR-002 (outcome classes), ADR-003 (comparator framework),
ADR-005 (two-surface routing), ADR-007 (augmented workload).
