> **DEFERRED — not part of vnc-045 delivery; carried to the future `protected_tags` feature.** (Deprecated in Unimatrix 2026-07-07 under the vnc-045 scope reduction; reasoning preserved here for the future feature. The per-`(entry, namespace)` cadence guard is a namespace-scoped `protected_tags` concept; vnc-045 ships `check_write_rate` as the only live throttle.)

## ADR-007: Cadence guard state model — in-memory, restart-reset, per-`(slug, entry, prefix)`

### Context
SD-9 requires a NEW per-`(entry, protected-namespace)` cadence guard against single-entry status oscillation/burial — a threat the per-caller `check_write_rate` cannot catch, because oscillating one high-value entry needs few calls and slips under the per-caller cap (ass-094 B4: no per-entry control exists today; the sliding window is keyed by `CallerId` only, gateway.rs:56-101). SR-08 (Med): this is a net-new stateful anti-abuse primitive with no precedent; state-model choices (in-memory vs persisted, global vs per-slug, restart behavior) are unspecified and easy to get wrong. It must align with the one live throttle's semantics, not invent a divergent model.

### Decision
The cadence guard is a new in-memory sliding-window primitive on `SecurityGateway`, deliberately mirroring `check_write_rate`'s model (gateway.rs:42-101):
- **Key: `(slug, entry_id, prefix)`.** Scoped to the protected prefix on a specific entry, within a slug (so two projects on one instance never share a counter). Free-form (non-protected) tags are NOT cadence-limited — the guard fires only for protected-prefix mutations (AC-06).
- **In-memory, resets on restart.** Same durability class as `check_write_rate` (in-memory `Mutex<HashMap>`, gateway.rs:45). No persistence, no schema change — consistent with the live limiter and SD-3.
- **Sliding window, threshold N per window.** Reject more than N protected mutations on one `(entry, prefix)` per window. Default: reuse the write window (3600s); N small (operational tuning, Open Question — not fixed here).
- **A `single_value` replace counts as ONE mutation** (ADR-004), not two, so a legitimate flip is not mistaken for oscillation and a corrective re-write is not starved (SR-02).
- **Ordering: after `check_write_rate`, before the store write** (see ARCHITECTURE §1 flow). Emits a distinct `ServiceError` (cadence variant) mapped to `invalid_params`/throttle at the handler.
- `UdsSession` exemption is NOT inherited: unlike the per-caller limiter (which exempts local UDS), the cadence guard protects a specific entry from oscillation regardless of transport, so it applies to all callers. (Rationale: the threat is entry burial, not caller flooding.)

### Consequences
- Easier: single-entry oscillation/burial is caught with a model an implementer already understands (it is `check_write_rate` re-keyed), minimizing the net-new surface SR-08 warns about.
- Bounded: in-memory + restart-reset means the guard is a live throttle, not a persistent budget — the same accepted limitation as `check_write_rate` (ass-094: resets on restart). It is anti-abuse, not an audit record; the audit log remains the durable transition history (SD-8).
- Cost: per-`(slug, entry, prefix)` keying grows the map with active entries; entries are evicted lazily by window expiry (same as the caller window, gateway.rs:76-82).
- Open: threshold N and window are operational tuning, not pinned here.
- Cross-references ADR-004 (replace = one event), SD-9, SR-08.
