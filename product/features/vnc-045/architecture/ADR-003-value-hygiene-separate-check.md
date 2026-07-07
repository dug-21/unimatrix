> **DEFERRED — not part of vnc-045 delivery; carried to the future `protected_tags` feature.** (Deprecated in Unimatrix 2026-07-07 under the vnc-045 scope reduction; reasoning preserved here for the future feature. `protected_tags` value-hygiene ships nothing in vnc-045 — the mechanism only reserves the value-opacity pre-write seam; see ARCHITECTURE §"Future extension".)

## ADR-003: `protected_tags` value-hygiene is a dedicated check, code-separated from `validate_outcome_tags`

### Context
The `protected_tags` value-hygiene check (allow-list + single-value resolution) slots onto the same write-path interception point where `validate_outcome_tags` already runs (tools.rs:895-898). SR-07 (Med): these are two DISTINCT policies — (a) `validate_outcome_tags` (infra/outcome_tags.rs:39) enforces a hard-coded RESERVED-key vocabulary (`RECOGNIZED_KEYS`: type/gate/phase/result/agent/wave) for `category=="outcome"` entries; (b) `protected_tags` enforces a CONFIG-DRIVEN, per-slug, per-prefix value allow-list for `context_tag` writes on any entry. Conflating them at one site risks the outcome-tag reserved-key logic leaking into protected-prefix evaluation (or vice versa), and one is hard-coded while the other is data-driven and per-slug.

### Decision
Implement `protected_tags` hygiene as a **separate module** (`infra/protected_tags.rs`) with its own entry point `evaluate_protected_tag(&ProtectedTagsConfig, tag) -> TagDisposition` (`FreeForm | Allowed{prefix, single_value} | Rejected{reason}`). It is invoked by the `context_tag` handler only; it does NOT call, extend, or share code with `validate_outcome_tags`. Rules:
- A tag whose text matches no configured `prefix` is `FreeForm` → allowed on bare `Capability::Write`, no value check (AC-04).
- A tag matching a `prefix` must have `value ∈ rule.allowed_values`, else `Rejected` (data-hygiene: `delivery:provn` rejected). Value-set is config, never hard-coded (SD-10).
- `single_value` on the matched rule drives replace semantics (ADR-004).
- `min_trust_level` is read into the rule but NOT consulted in OSS (inert enterprise seam — ADR-008).

The two validators stay physically separate; `validate_outcome_tags` is untouched. `context_tag` does not route through the `category=="outcome"` path.

### Consequences
- Easier: reserved-outcome-vocabulary logic and config-driven protected-prefix logic evolve independently; neither pollutes the other.
- Easier: the policy is pure config data — adding/removing a protected prefix is an operator config edit, no code change (domain-agnostic, SD-10).
- Cost: a second validator module beside the existing one; the shared interception concept is deliberately NOT abstracted into one generic validator (that would re-couple the two policies SR-07 warns against).
- Cross-references ADR-005 (config type + per-slug threading), ADR-008 (min_trust_level inert), SR-07.
