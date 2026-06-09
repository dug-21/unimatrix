# ADR-006 (nan-018): Penalty Config Exposure Is Eval-Only — Deployment Defaults Stay Fixed

### Context

ADR-001 (nan-018) turns the crt-014 topology-penalty `const`s (`ORPHAN_PENALTY`, `CLEAN_REPLACEMENT_PENALTY`, `HOP_DECAY_FACTOR`, `PARTIAL_SUPERSESSION_PENALTY`, `DEAD_END_PENALTY`, `FALLBACK_PENALTY`, `MAX_TRAVERSAL_DEPTH`) into `GraphPenaltyConfig` fields threaded through `graph_penalty_with`. The point of that exposure is **measurement**: crt-053's steepness question is a sweep, and a sweep needs the levers reachable from a profile TOML.

A new, configurable surface is easy to mis-read. Once a deployed operator (or a future agent) sees `[graph_penalty]` fields in `UnimatrixConfig`, the natural — and wrong — inference is "these are deployment tuning knobs; set them in production to change ranking." That inference would:

- silently re-tune deployed retrieval behavior outside the formula authority, and
- breach nan-018's stated boundary (C-02): nan-018 is the *instrument*, it does **not** re-tune the deployed formula.

ASS-037 (#3984) is the penalty-formula authority. nan-018 deliberately does **not** change deployed defaults; AC-01/SR-08 require the defaults to reproduce current behavior **bit-for-bit**. This ADR records the boundary explicitly so the new config field cannot be mis-read as a license to tune deployment.

### Decision

**Penalty values remain fixed at their crt-014 v1 `const` defaults in deployment. The new `GraphPenaltyConfig` exposure is eval/measurement-only and is NOT license to re-tune deployed defaults.**

Concretely:

1. **Deployment runs at defaults.** A deployed Unimatrix server omits `[graph_penalty]` (or sets it equal to defaults); the dual-default discipline (#4064) guarantees omission resolves to the crt-014 consts bit-for-bit. Deployed ranking behavior is therefore identical to pre-nan-018.
2. **The config surface exists to feed eval profiles**, not production. Non-default `[graph_penalty]` values belong in eval profile TOMLs driving `unimatrix eval` sweeps against the fixture/snapshot corpora — never in a production deployment config as a tuning act.
3. **Formula authority is unchanged.** Any decision to *adopt* a swept value as a new deployed default is an ASS-037 (#3984) decision, made on the evidence nan-018 produces — it is out of nan-018's scope. nan-018 produces the evidence; it does not act on it.
4. This is the deployment-side complement to ADR-001's supersession note: ADR-001 partially supersedes crt-014 ADR-006 (#1606) **for the measurement path only**; the *deployed* "fixed for v1" behavior crt-014 ADR-006 describes still holds, and this ADR makes that explicit.

### Consequences

**Easier:** A future reader of `infra/config.rs` who finds the `[graph_penalty]` section has an unambiguous answer to "can I tune this in production?" — no, that is an ASS-037 decision; this field is for eval. The eval/deployment boundary is documented at the point of confusion, not buried in ADR-001's rationale.

**Harder:** The boundary is a documentation/convention guarantee, not a type-enforced one — nothing in the code prevents an operator from putting non-default penalties in a production config. Mitigation: the Band-2 config-knob reference states the eval-only intent prominently, and the dual-default discipline ensures the *default* (omitted) path is provably unchanged. Hard type-level prevention (e.g. gating the section behind an eval-only config layer) was considered out of scope for nan-018 and is left as a possible future hardening if mis-tuning is ever observed.
