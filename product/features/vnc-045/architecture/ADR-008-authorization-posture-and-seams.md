## ADR-008: Authorization posture — `Capability::Write` gate IS the trust seam; `agent_id` audit-only; value-opacity is the hygiene seam

### Context
The security posture is human-LOCKED. ass-094 tested "is `Capability::Write` alone sufficient?" and returned NO for a general trusted-corpus model — but the human ruling is that in OSS the enforceable controls are audit + rate + lifecycle, NOT identity, because there is no client token and no per-agent trust resolution locally. `TrustLevel` (schema.rs:236) is stored but never consulted by the gate (registry.rs:92).

vnc-045 ships the `context_tag` MECHANISM only; the `protected_tags` value-hygiene policy is DEFERRED. This ADR therefore had to be corrected: the retrofit-hard authorization contract is **the gate LOCATION**, not a `min_trust_level` field. `min_trust_level` was a field on a `ProtectedTagsConfig` type that no longer ships in vnc-045, so it is removed here entirely — there is nothing to wire and no gap (its owning config does not exist).

### Decision
The op's authorization and integrity posture:

1. **Baseline gate: `Capability::Write` only** — reused verbatim from `context_correct` (tools.rs:1245 pattern; require_cap → registry.rs:92). Do NOT mint `Capability::Tag`; do NOT split add/remove/replace at the capability layer. Every tag value admits on bare `Write` — value-opacity (point 4). `context_tag` grants no new privilege and closes no hole: `context_correct` (unchanged) can already mutate any tag, so this is a fast path, not an access-control boundary (SD-10).

2. **The `Capability::Write` gate LOCATION is the enterprise trust seam** — this is the retrofit-hard contract, NOT a trust field. A future enterprise identity provider attaches a trust-level check **at this same gate**, where the principal is already resolved. There is exactly one forward-note in code and here: *"enterprise trust elevation attaches at the existing `Write` gate."* No `min_trust_level` field, no anti-self-attestation control, no inert config — with one client identity OSS cannot distinguish author from certifier, so such a control has nothing to bite on; it is absent by construction, not deferred plumbing.

3. **`agent_id` is audit-only, never an authorization input.** It is self-declared (`credential_type="none"`); recorded in the audit event for accountability, never gated on.

4. **Value-opacity is the write path's north star AND the hygiene seam** (SD-8). The handler writes any tag WITHOUT interpreting its value — no allow-list, no vocabulary; `delivery:proven`, `delivery:anythingelse`, and free-form `foo` all succeed on bare `Write`. There is exactly ONE marked pre-write interception point where a future `evaluate(tag) -> Allowed | Rejected` hygiene validator (the deferred `protected_tags` policy) drops in. vnc-045 ships that point as a marked seam ONLY — **NO stub, NO empty `ProtectedTagsConfig`, NO validator call.** The namespace derived from the tag prefix (before `:`) is recorded in audit but NEVER validated.

5. **Audit is the PRIMARY control** — every mutation emits the complete generic audit event (ADR-009): `operation="context_tag"`, `metadata={action, namespace, tag, prior_value, new_value}`; `prior_value` mandatory on remove/replace. `'context_tag'` is added to `audit_write_count_since` (audit.rs:84) as a latent signal (not a live throttle).

6. **Lifecycle guards live IN the op** (Goal 6, ass-094 B5): refuse tagging a **Quarantined** entry; **allow** tagging a **Deprecated** entry (the "refuse protected tag on deprecated" rule is a protected-tag concept and is deferred). These guards exist only on the `context_correct` path today (write_ext.rs:471-482) and are NOT inherited — the op enforces its own.

7. **Platform enforces NO evidence-binding** — the op checks nothing about value truth; `delivery:proven` requires no proof at the platform layer. Evidence-binding is the evaluating agent's responsibility (uni-capability skill).

### Consequences
- Easier: the op adds no new privilege and reuses the existing gate — no new authorization surface to get wrong.
- Easier: the enterprise trust seam is a LOCATION (the `Write` gate) that already exists, so nothing inert is carried; the future protected_tags feature attaches trust-elevation and value-hygiene at the two marked seams (this gate + the value-opacity pre-write point) with zero changes to the shipped mechanism.
- Harder/accepted: forensic value of the audit trail is capped until credentialed transport lands (attribution is declarative). Same known, accepted identity-model risk, not a new gap.
- Cross-references ADR-001 (direct write), ADR-004 (replace action), ADR-009 (audit contract), SD-8/SD-9/SD-10/SD-12.
