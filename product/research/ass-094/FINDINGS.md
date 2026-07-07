# FINDINGS: Authorization & anti-poison controls for `context_tag`

**Spike**: ass-094
**Date**: 2026-07-06
**Approach**: investigation + evaluation (synthesis of internal code track + external best-practice track)
**Confidence**: directional (internal answers are code-anchored with file:line evidence; external answers are literature-grounded with source citations; no PoC)

This document synthesizes the internal track (`FINDINGS-INTERNAL.md`, answering A1/A2/A3/A4/B3/B4/B5 against the codebase) and the external track (`FINDINGS-EXTERNAL.md`, answering E1/E2/E3 from RAG-poisoning, controlled-vocabulary, and labeling-authorization literature). The tracks were investigated independently and **converge**; where they appear to diverge, the divergence is reconciled explicitly in the boxed notes below.

---

## Verdict on the challengeable hypothesis (lead)

**Hypothesis under test: "`Capability::Write` alone is sufficient" — the verdict is NO.**

Both tracks reach this independently.

`Capability::Write` is the correct **baseline** gate — reuse it — but it is *necessary, not sufficient*. The internal code evidence:

1. **No discrimination.** `Write` is a global flat capability; `require_capability` ignores `trust_level` entirely (`registry.rs:85`) and no ownership check exists anywhere in the mutation paths (A2). Any Write-capable agent can forge or erase a `status:proven` claim on **any** entry in the project.
2. **Guard bypass.** A direct `context_tag` op sidesteps the lifecycle guards that today protect quarantined/deprecated entries — those live only on the `context_correct` path (`write_ext.rs:471-482`) (B5).
3. **Budget bypass.** `Write` does not wire the op into the live rate limiter (`gateway.rs`) or the persistent counter (`audit.rs:83`); both need explicit extension or tag mutation is an unbudgeted poison vector (B4).
4. **No per-entry control** exists, so single-entry status oscillation / burial is uncaught (B4).

The external practice backs this without exception: **everywhere trust-bearing labels are governed, they are separated from ordinary metadata writes** — by a reserved namespace (Kubernetes `kubernetes.io/` + NodeRestriction; Prometheus `__`), a distinct/tiered capability (Stack Overflow retag/approve tiers above apply), an anti-self-attestation rule (NodeRestriction bars a kubelet self-applying reserved labels), and asymmetric removal controls (Microsoft Purview downgrade friction). A write bit stops none of the four documented tag-poisoning patterns (E1). A tag that boosts ranking is a *strictly cheaper* attacker lever than PoisonedRAG's crafted-text optimization (arXiv:2402.07867) — same ranking effect, no adversarial-text crafting, no re-embed.

**Minimum sufficient set** (all reuse existing primitives; none requires a schema change):
`Write` (baseline) **+** reserved-namespace validation with an elevated bar bound to the existing-but-unused `TrustLevel` (A3) **+** anti-self-attestation on reserved trust tags (E3, reconciled below) **+** lifecycle-state guard enforced *in the op* (B5) **+** `check_write_rate` wiring and `audit_write_count_since` inclusion (B4) **+** mandatory prior/new value in audit metadata (B3/A4) **+** a per-`(entry, reserved-namespace)` cadence guard (B4).

---

## Model as it exists today (shared basis)

From the internal track (code evidence):

- **Capability enum** (`crates/unimatrix-store/src/schema.rs:262-276`): five flat variants — `Read=0, Write=1, Search=2, Admin=3, SessionWrite=4`. Exhaustive `as_audit_str` (schema.rs:283-292) and `TryFrom<u8>` (schema.rs:294-307); adding a variant is a compile-checked extension.
- **Assignment** is a flat `capabilities: Vec<Capability>` per agent (`AgentRecord`, schema.rs:317). Permissive auto-enroll → `[Read, Write, Search]`; strict → `[Read, Search]`; `session_caps` can override (`infra/registry.rs:39-52, 68-78`). `system` → all four incl. Admin (registry.rs:187-194); `human` → `Privileged`.
- **Trust hierarchy exists but is unused by the gate**: `TrustLevel` (System=0, Privileged=1, Internal=2, Restricted=3, schema.rs:234-246) is stored on every agent but `require_capability` checks only `record.capabilities.contains(&cap)` (registry.rs:81-100). Trust level is recorded, never gated on.
- **Gating** (`require_cap`, server.rs:631-644 → registry.rs:92): every content-mutating MCP op gates on the same `Capability::Write` (tools.rs:884 store, 1245 correct, ...); Admin gates enrollment/status-admin (tools.rs:2015, 2139).
- **Two independent write-rate mechanisms**: (1) **Live** per-caller sliding-window limiter `SecurityGateway::check_write_rate` (`services/gateway.rs:35-101, 166-169`), default 60/hr, keyed by `CallerId`, in-memory, resets on restart, `UdsSession` exempt (gateway.rs:60), enforced at `store_ops.rs:114` and `store_correct.rs:29`; (2) **Persistent** counter `audit_write_count_since` (`audit.rs:79-92`), SQL `COUNT` over `audit_log` for `operation IN ('context_store','context_correct')` — **no live non-test consumer exists** in the current tree; it is a latent SLN1 budget signal.
- **Provenance fields are largely placeholders**: `credential_type` hardcoded `"none"` at every MCP write site (tools.rs:826, 1185, ...); `trust_source` records write **origin** not credential, hardcoded `"agent"` for store/correct (tools.rs:986, 1319); `agent_id` is self-declared. Today's attribution is **declarative, not authenticated**.
- **A controlled-vocabulary validator already exists**: `infra/outcome_tags.rs:9-80` parses structured `key:value` tags for `category == "outcome"`, rejects keys outside a `RECOGNIZED_KEYS` allow-list, requires `type`, forbids duplicates, validates per-key value sets — already invoked in the write path (tools.rs:891-900). This is the reusable substrate for reserved namespaces.

---

## Expected Output 1 — Recommended authorization model for `context_tag`

### A1 — Capability granularity

**Q (SCOPE):** Is `Capability::Write` the correct gate for tag mutation, or is a distinct/finer capability warranted (`Capability::Tag`, or add-vs-remove split)? What does the enum/registry support and how are caps assigned?

**Answer:** The `Capability` enum is deliberately coarse (5 flat variants, schema.rs:262-276); assignment is a flat per-agent `Vec<Capability>` (schema.rs:317). Today all content mutation — store, correct, and the only existing tag-write path (`update()`'s DELETE-all/re-INSERT) — is gated by a single global `Capability::Write`. There is no finer axis: no `Tag` cap, no add/remove split, no per-namespace or per-trust-tier discrimination, and `require_capability` ignores `trust_level` (registry.rs:85). For tags, "Write alone" is literally the current state.

**Evidence:** schema.rs:262-276, 317; registry.rs:81-100; tools.rs:884, 1245.

**External grounding (E3):** Mature systems tier label operations — Stack Overflow gates *create/retag/suggest-synonym/approve-synonym/vote-delete* at distinct thresholds, with destructive and vocabulary-minting ops **above** merely applying an existing label. External practice says "tier and separate," but does **not** prescribe which enum variant — it is agnostic to whether the tier is realized as a new capability or a trust-level bar (E3 Unanswered).

**Recommendation:** Reuse `Capability::Write` as the baseline gate for `context_tag`. Do **not** mint `Capability::Tag` and do **not** split add-vs-remove at the capability layer. A proliferation of coarse capabilities does not express the real requirement, which is *namespace × trust-tier*, not a new global verb. Keep free-form tags on plain `Write`; gate reserved/trust-bearing namespaces on an elevated bar (A3).

> **Reconciled divergence (enforcement point — make this unambiguous):** The external track calls for "a distinct/tiered capability" for trust tags (SO tiering). The internal track recommends **not** minting `Capability::Tag` and instead realizing the tier through the **existing-but-unused `TrustLevel`**. These are the same recommendation at different altitudes: external prescribes *separation* ("trust-tag application must not be authorized by the same mechanism as ordinary writes"); internal prescribes the *mechanism* that satisfies it without a new enum variant. **The enforcement point is a reserved-namespace policy check at the `context_tag` service write site — exactly where `validate_outcome_tags` already runs (tools.rs:891-900) — that (a) validates the tag value against an allow-list and (b) requires the caller's `TrustLevel` to meet a per-prefix minimum.** The "tier" is `TrustLevel`, not a new `Capability`. `Capability::Write` remains the coarse admission gate; `TrustLevel` becomes the discriminator that `require_capability` does not currently consult. This activates a dormant schema dimension rather than adding one, and satisfies the domain-agnostic constraint (a rule about reserved prefixes, not a bespoke `proven` permission).

### A2 — Mutation scope

**Q (SCOPE):** Can an agent tag any entry, or only entries it authored / within its project? What ownership/project scoping applies (1-client:1-project, vnc-034)?

**Answer:** Two boundaries, only one enforced.
- **Cross-project** is enforced structurally by 1-client:1-project / per-slug database (vnc-034, `projects.rs:109` — each slug owns its own `{base}/{slug}/` DB tree). An agent bound to project X's DB physically cannot reach project Y's entries; no per-entry check is needed.
- **Within a project** there is **no authorship/ownership scoping.** `entries` carries `created_by`/`modified_by` columns (`db.rs:558-559`), populated, but no code path compares caller to `created_by` before a mutation. The gate is global `Write` (registry.rs:85). Any Write-capable agent can correct/deprecate/quarantine — and would be able to tag — any entry regardless of author.

**Evidence:** projects.rs:109; db.rs:558-559; registry.rs:81-100; absence of any `created_by ==` comparison in mutation paths.

**External grounding (E3):** Kubernetes NodeRestriction bars a kubelet from applying reserved labels **to itself** — the most direct analog to the forcing case: an agent must not stamp `proven` on an entry it authored. Self-attestation is the vulnerability. in-toto/SLSA model a trust claim as an authenticated statement by an authority **distinct from the artifact producer**.

**Recommendation:** For **free-form tags**, do not add authorship scoping — a shared, cross-annotated corpus is the point of the retrieval model; agents legitimately tag each other's entries. Treat `created_by`/`modified_by` as the provenance substrate (accountability via audit), not an auth gate, and record `modified_by` on every tag mutation.

> **Reconciled divergence (anti-self-attestation):** The internal track (A2) concludes "no authorship scoping"; the external track (E3) makes anti-self-attestation the **highest-leverage control** for `proven`. These reconcile by **scope of application**: authorship is *not* gated for free-form descriptive tags (internal is right — keep them collaborative), but for the **reserved trust-bearing namespace only**, add an anti-self-attestation rule — an agent may not set a reserved trust tag (e.g. `status:proven`) on an entry where it is the `created_by`/`modified_by`. This is domain-agnostic (a rule about reserved-prefix writes on entries you authored), reuses the already-populated `created_by` column as the check input, and closes the fake-authority self-boost vector without imposing authorship gating on the collaborative common case. **This is the one control the internal track did not surface; the external track supplies it and it belongs in the recommended model.**

### A3 — Protected namespaces

**Q (SCOPE):** Should reserved tag namespaces (`status:*`, capability-delivery tags) require higher authorization than free-form tags? Who may write a trust-bearing tag like `proven`? What could enforce a namespace boundary?

**Answer:** The substrate already exists. `infra/outcome_tags.rs` is a working controlled-vocabulary/reserved-namespace validator (RECOGNIZED_KEYS allow-list, required `type`, no duplicates, per-key value sets), already invoked in the write path (tools.rs:895-900). The category registry (`self.categories.validate`, tools.rs:891-893) is a second allow-list precedent. So the model already has (1) a write-time interception point, (2) a `key:value` structured-tag convention, and (3) allow-list value validation. A `status:*` reserved namespace slots directly onto this. **Who may write `proven` today: nobody is specially gated — any `Write` agent can. That is the gap.**

**Evidence:** infra/outcome_tags.rs:9-80; tools.rs:891-900.

**External grounding (E2):** The mature pattern is **hybrid** — open folksonomy for descriptive tags plus a small reserved/controlled namespace for system-meaning tags, gated by a mechanism *distinct* from ordinary writes, enforced **at the write boundary** (not via later curation — supertagger consensus is too slow to protect a trust flag; arXiv:1502.02777). Exemplars that transfer: Kubernetes reserved prefixes + NodeRestriction enforcement + immutable control-plane-set `kubernetes.io/metadata.name`; Prometheus `__`-reserved internal labels; **GitLab scoped labels** (`key::value` with mutual exclusion — a new value replaces the prior, so a conflicting second status is unrepresentable).

**Recommendation:** Introduce a **reserved-prefix policy table** (domain-agnostic: a set of prefixes such as `status:` plus a minimum `TrustLevel` per prefix), validated at the `context_tag` write site exactly where `validate_outcome_tags` already runs. Two rules: (1) **value allow-list** — a `status:` value must be in `{missing, partial, proven, claimed}` (reuse the outcome_tags pattern); (2) **elevated authorization** — a reserved-namespace write requires more than bare `Write`, bound to the existing `TrustLevel` (require System/Privileged/Internal). Free-form (non-prefixed) tags stay on plain `Write`. Consider modeling delivery status as a **mutually-exclusive scoped key** (GitLab pattern: `status::proven` replaces `status::partial`) so an entry cannot hold two conflicting statuses — this is a schema/ergonomics call (see Unanswered).

### A4 — Add vs remove asymmetry

**Q (SCOPE):** Is removing a tag (burying trusted data, erasing a status) more dangerous than adding one? Should removal carry distinct controls?

**Answer:** Yes, removal is more dangerous for the trust-bearing case. Adding a bad tag is additive and visible — the entry still surfaces, the noise is detectable. Removal is a **silent erasure with immediate effect**: tag-filtered reads load tags live from `entry_tags` on every query (ass-093 Q2; graph_read_filter.rs:186-209), so removing a discriminating tag buries an entry out of the next tag-filtered result set instantly, and removing `status:proven` erases a delivery claim. There is **no tag version history** — the append-only audit log is the *only* record a removed tag ever existed (ass-093 Q5).

**Evidence:** graph_read_filter.rs:186-209 (live tag reads); audit.rs:46-64 (append-only log, no tag history); precedent `emit_edge_cleanup_audit` (server.rs:659-690) preserves removed-edge tuples in a distinct audit record precisely because an eager delete is irreversible.

**External grounding (E1, E3):** "Machine Against the RAG" (arXiv:2406.05870) establishes that **jamming — preventing a correct answer from surfacing — is a valid, stealthier attack** than substituting a wrong one ("refusals aren't amenable to fact-checking, aren't anomalous"). This is direct evidence that **tag removal is an attack**. Microsoft Purview makes **downgrading/removing** a protective label higher-friction (justification + audit) than applying one. Combined: removal/downgrade of a trust tag must be gated **at least as strictly as addition** — often more, always audited.

**Recommendation:** Asymmetric controls at the op/audit layer, **not** at the capability layer:
1. Audit both add and remove; for remove/replace, recording the **prior value** in audit metadata is a hard requirement (mirror `emit_edge_cleanup_audit` — a reconstructable record for an irreversible delete).
2. For reserved namespaces, **removal/overwrite requires the same elevated bar as setting** (A3) — you cannot erase a `proven` claim with less authority than it took to assert it.
3. Model a status change as an atomic **replace** (old + new logged together), not remove-then-add, so oscillation and demotion are legible in one audit record.
No add/remove capability split is warranted.

---

## Expected Output 2 — Security control set beyond Write (each mapped to the poison vector it closes)

Every control below reuses an existing primitive; none requires a schema change. The poison vectors are the four documented in E1 (boost, bury/jam, fake-authority, filter-bypass) plus the internal-specific budget/lifecycle bypasses.

| # | Control | Reuses | Closes poison vector |
|---|---------|--------|----------------------|
| C1 | **Reserved-prefix value allow-list** — `status:` value ∈ `{missing,partial,proven,claimed}`, validated at write site | `outcome_tags` validator (outcome_tags.rs:9-80; tools.rs:891-900) | Fake-authority (E1-3): forging arbitrary status strings |
| C2 | **Elevated `TrustLevel` bar for reserved prefixes** — reserved-namespace write requires System/Privileged/Internal | `TrustLevel` (schema.rs:234-246), currently unused by gate | Fake-authority / retrieval-boost (E1-1,3): any `Write` agent asserting `proven` |
| C3 | **Anti-self-attestation on reserved trust tags** — caller may not set a reserved trust tag on an entry it authored (`created_by`/`modified_by`) | `created_by`/`modified_by` (db.rs:558-559), already populated | Self-boost fake-authority (E1-3, E3 NodeRestriction) — the forcing-case vulnerability |
| C4 | **Mandatory prior-value in audit on remove/replace** + atomic replace for status | `AuditEvent.metadata` (schema.rs:358-374); `emit_edge_cleanup_audit` precedent (server.rs:659-690) | Retrieval-burying / jamming (E1-2): silent erasure with audit as sole recovery |
| C5 | **Symmetric bar on reserved-namespace removal/downgrade** (= setting bar, always audited) | C2 bar + audit | Jamming / status erasure (E1-2, E3 Purview) |
| C6 | **Wire `context_tag` into `check_write_rate`** (live per-caller throttle) | `SecurityGateway::check_write_rate` (gateway.rs:166-169) | Tag flooding / dilution (E1-1); closes the unthrottled-write bypass |
| C7 | **Add `'context_tag'` to `audit_write_count_since` op list** | audit.rs:83 | SLN1 budget-invisibility bypass (B4) |
| C8 | **Per-`(entry, reserved-namespace)` cadence guard** — reject rapid status flips / >N reserved mutations per entry per window | new guard (no per-entry control exists; gateway keyed by CallerId only, gateway.rs:56-101) | Status oscillation / single-entry burial that slips under the per-caller cap |
| C9 | **Lifecycle guard in the op** — forbid tagging quarantined entries; refuse reserved tags on deprecated entries | mirror `context_correct` refusals (write_ext.rs:471-482; tools.rs:938-940) | Re-steering retrieval toward isolated content / forging `proven` on quarantined or superseded data |
| C10 | **Attributed provenance on every mutation** — `operation="context_tag"`, `capability_used`, `agent_id`, `session_id`, metadata `{action, namespace, tag, prior_value, new_value}` (+ writer trust tier for reserved) | `AuditEvent` (schema.rs:358-374; audit.rs:46-67) | Forensic un-traceability across all vectors; the append-only log becomes the attestation record (E1 OWASP, E3 in-toto) |

**Load-bearing subset:** C2+C3 (who may assert a trust tag), C6 (the only *live* write throttle), C9 (guard the new op does not inherit), and C10 (the attestation record) are the controls without which the op is a net-new poison vector.

---

## Expected Output 3 — External best-practice grounding: what transfers / what doesn't

### E1 — Metadata/tag poisoning patterns

Four RAG abuse patterns map one-to-one onto a mutable retrieval-steering tag, all cheaper and stealthier than content injection, none stopped by write-permission:
1. **Retrieval boosting** — PoisonedRAG (arXiv:2402.07867, USENIX Security 2025): 5 malicious texts in millions → ~90% success; a ranking-boosting tag is a strictly cheaper lever (no text crafting, no re-embed).
2. **Retrieval burying / jamming** — arXiv:2406.05870: suppression is a valid, stealthier attack; direct evidence that **tag removal is an attack** (feeds A4).
3. **Fake authority via metadata / context poisoning** — Promptfoo; Christian Schneider: a `proven` tag settable by any writer *is* this pattern, formalized into a first-class flag.
4. **Metadata-filter bypass** — drainpipe.io; BeyondScale: a tag is a *hint to the ranker*; never treat "the tag says trusted" as an enforced decision.

**Transfers:** the four patterns and provenance/audit-attribution controls (OWASP LLM04:2025 — per-record provenance verified at retrieval time; SLSA/in-toto attestation modeling) transfer directly. **Doesn't transfer:** PoisonedRAG's optimization method and RevPRAG's activation-analysis detector (arXiv:2411.18948, ~98% TPR/~1% FP) target content/embedding/generation, not a tag-mutate gate; per-record hashing is largely subsumed by the existing hash chain + audit log; full DSSE/Sigstore signing is heavier than a single-project engine needs — **the append-only audit record is the right-sized attestation.**

### E2 — Controlled-vocabulary governance

Mature pattern is **hybrid**: open folksonomy for descriptive tags + a small reserved namespace for system-meaning tags, gated distinctly, enforced at the write boundary. **Transfers:** reserved-prefix + boundary enforcement (Kubernetes), system-owned reserved namespace (Prometheus `__`), single-valued scoped keys (GitLab `key::value` mutual exclusion). Cost is one prefix-validation check on the write path; keeping descriptive tags free-form avoids taxonomy-curation overhead. **Doesn't transfer:** full taxonomy/ontology curation (the need is a protected flag, not a governed subject vocabulary); Prometheus "strip on read" (status must persist and be queryable). Emergent-consensus governance is too slow to protect a trust flag (supertagger dynamics, arXiv:1502.02777) — enforce at write, not curation.

### E3 — Annotation/labeling authorization

Four mechanisms converge: graduated capability tiers (Stack Overflow), separation-of-duties / anti-self-attestation (Kubernetes NodeRestriction), role/ownership scoping for create-vs-apply (GitLab), and add-vs-remove asymmetry on trust labels (Microsoft Purview). **Transfers:** capability tiering, anti-self-attestation, add-vs-remove asymmetry, attributed-attestation modeling — all expressible over "reserved-namespace tag writes." **Doesn't transfer:** reputation-*score* mechanics (presume a large human community — transfer the tiering, not the currency); full cryptographic attestation (DSSE/Sigstore); human-in-the-loop review dialogs (the enforceable analog for autonomous agents is a hard capability/trust gate). GitLab "protected labels" being an *unshipped* request (gitlab-org/gitlab#293424) shows even mature platforms haven't fully solved role-gating which labels a member may apply — a purpose-built gate here is defensible.

**Net transfer statement:** External practice confirms the internal recommendation shape — *reuse a coarse write gate as admission, discriminate on reserved namespace × elevated trust tier, add anti-self-attestation, gate removal symmetrically, attribute every mutation.* What the codebase should **not** import: cryptographic signing, taxonomy curation, reputation currency, and human-review dialogs — all heavier than a single-project MCP engine with an existing hash chain + append-only audit warrants.

---

## Expected Output 4 — Residual poison/trust risks gating implementation, with mitigations

| Risk | Evidence | Mitigation |
|------|----------|------------|
| **Attribution is declarative, not authenticated.** `credential_type` hardcoded `"none"` (tools.rs:826…), `trust_source` records origin not credential and is hardcoded `"agent"` (tools.rs:986/1319), `agent_id` self-declared. Every trust argument leaning on the audit trail (C3, C10) is only as strong as the declared identity. | Internal B3; External E1 (OWASP per-record provenance presumes *verified* source). | Document the bound explicitly — do not silently assume it away. The controls are correct in structure but their forensic value is capped until credentialed transport lands (`credential_type` real). Gate the *strength claim*, not the implementation. |
| **`TrustLevel` is dormant.** Stored on every agent, never consulted by `require_capability` (registry.rs:85). C2/C5 depend on activating it. | Internal A3/B5. | Activating it for reserved-prefix writes is the intended home and requires no schema change — but it is net-new gate logic that must be tested (latency + correctness). |
| **`audit_write_count_since` has no live enforcement consumer.** Only a wrapper + crt-001 tests reference it; the persistent SLN1 budget is latent. C7 wires `context_tag` into a counter nothing currently reads. | Internal Out-of-Scope discovery. | C7 is still correct (future-proofs the signal) but do **not** assume it provides live throttling — C6 (`check_write_rate`) is the only *enforced* limit, and it resets on restart and exempts `UdsSession` (gateway.rs:60). |
| **No per-entry control today.** Sliding window keyed by `CallerId` only (gateway.rs:56-101). Status-oscillation/burial on a single high-value entry needs few calls and slips under the per-caller cap. | Internal B4. | C8 (per-`(entry, reserved-namespace)` cadence guard) is a *new* control, not a wiring change — larger implementation surface; threshold requires operational tuning. |
| **Filter-bypass if tags ever become access control.** A tag is a ranking hint; treating "tag says trusted" as an enforced decision is the advisory-filter failure. | External E1-4, drainpipe.io. | Never let a tag's *presence* substitute for an *enforced* authorization decision. Out of scope under 1-client:1-project today; flag if cross-project retrieval is introduced (see Out-of-Scope). |

---

## Expected Output 5 — Hypothesis verdict

Stated in full at the top. In one line: **`Capability::Write` alone is NOT sufficient** — it is the correct baseline admission gate, but the trusted-corpus requirement is met only by adding reserved-namespace validation + an elevated `TrustLevel` bar + anti-self-attestation + a lifecycle guard in the op + rate/budget wiring + mandatory audit prior/new value + a per-entry cadence guard, all reusing existing primitives with no schema change. Both tracks converge on this; external practice supplies no counter-example where trust-bearing labels are governed by an undifferentiated write bit.

---

## Unanswered Questions (merged, deduped)

- **Exact min-`TrustLevel` per reserved prefix, and whether the prefix set is config-driven** — implementation design choice, depends on operational tuning (internal). External practice says "tier and separate," not which enum variant or which tier value (external A1/A4).
- **Reserved-prefix (`status:`) vs. scoped `key::value` single-valued key** — a schema/ergonomics call; evidence supports either, mild external preference for the GitLab-style scoped single-valued key (so a conflicting second status is unrepresentable).
- **Numeric per-`(entry, reserved-namespace)` cadence threshold** (C8) — operational tuning.
- **Cost of a reserved-namespace gate in an MCP/agent setting** — no external source benchmarks this; cost claims are reasoned from Kubernetes/Prometheus precedent (one prefix-validation check on the write path).

None of these block the recommendation; all are implementation-tuning decisions.

## Out-of-Scope Discoveries (merged, deduped — carry-forwards, not pursued)

- **`audit_write_count_since` dormancy** — the persistent SLN1 write-budget has no live enforcement consumer in the current tree (only wrapper + crt-001 tests); the only live throttle is the in-memory gateway limiter (resets on restart, exempts `UdsSession`). Worth a separate look at whether SLN1's persistent budget is wired anywhere. *Why it matters:* a control assumed active in planning may be dormant.
- **`credential_type` hardcoded `"none"` + `agent_id` self-declared** across all MCP write sites — attribution is declarative, not authenticated, until credentialed transport lands. Bounds the forensic value of *every* audited op, not just tags.
- **`TrustLevel` carried but never gated on** (registry.rs:85) — a whole authorization dimension in the schema is ignored; C2/C5/B5 could activate it. *Why it matters:* latent authorization capacity already present.
- **Metadata-filter-bypass as a tenant-isolation risk** (drainpipe.io) — if tags are ever used as a *cross-project access-control* filter rather than a ranking hint, the "advisory filter over shared store" failure applies. Out of scope under 1-client:1-project (vnc-034); **flag for a spike if cross-project retrieval is introduced.**
- **Behavioral anomaly detection of poisoned retrieval** (RevPRAG, arXiv:2411.18948, ~98% TPR/~1% FP) — a complementary defense-in-depth layer, not a tag-authz control; possible future spike.

## Recommendations Summary

- **A1 (granularity):** Reuse `Capability::Write` as baseline; do not mint `Capability::Tag` or split add/remove at the cap layer. The real axis is namespace × trust-tier, realized via the existing `TrustLevel`, enforced at the `context_tag` write site (where `validate_outcome_tags` already runs).
- **A2 (scope):** Cross-project isolation is structural (per-slug DB, vnc-034); in-project there is zero authorship scoping. Keep free-form tags collaborative (`created_by`/`modified_by` + audit for accountability, not gating) — but add anti-self-attestation on **reserved trust tags only** (an agent may not set `status:proven` on an entry it authored).
- **A3 (namespaces):** Build on the existing `outcome_tags` controlled-vocabulary validator; add a reserved-prefix policy (`status:*`) with a value allow-list and an elevated authorization bar bound to the existing (unused) `TrustLevel`. Enforce at the write boundary, not via curation. Consider GitLab-style mutually-exclusive scoped keys for status.
- **A4 (add vs remove):** Removal is more dangerous (silent burial + status erasure, audit is sole recovery; jamming is a documented attack). Require prior-value in audit, gate reserved-namespace removal at the same bar as setting (always audited), model status changes as atomic replace. No capability split.
- **B3 (provenance):** Mandate `operation="context_tag"`, `capability_used`, and metadata `{action, namespace, tag, prior_value, new_value}` (+ writer trust tier for reserved namespaces); prior value mandatory on remove/replace. The append-only log is the right-sized attestation record — but flag that attribution is declarative until credentialed transport.
- **B4 (rate/budget):** MUST wire `context_tag` into `gateway.check_write_rate` (the only live throttle) and MUST add it to `audit_write_count_since`'s op list; add a NEW per-`(entry, reserved-namespace)` cadence guard against oscillation/burial (no per-entry control exists today).
- **B5 (trust/lifecycle):** Neither `trust_source` nor `TrustLevel` gates tagging today. The new op must refuse quarantined entries, refuse reserved tags on deprecated entries, and require an elevated `TrustLevel` for authoritative tags — enforced *in the op*, not inherited from the correct-path.
- **E1/E2/E3 (external):** `context_tag` is a poison vector ≥ content injection; four documented patterns (boost, bury/jam, fake-authority, filter-bypass) all apply and write-permission stops none. Adopt the hybrid vocabulary model (free-form + distinctly-gated reserved namespace), tier trust-tag application above ordinary writes, bar self-attestation (highest-leverage control for `proven`), gate removal asymmetrically, attribute every mutation. Transfer the tiering/anti-self-attestation/reserved-prefix patterns; do not import cryptographic signing, taxonomy curation, reputation currency, or human-review dialogs.
- **Verdict:** `Capability::Write` alone is **NOT sufficient** — right baseline, plus a required control set (reserved-namespace validation + trust bar + anti-self-attestation + lifecycle guard + rate/budget wiring + audit prior/new value + per-entry cadence), all reusing existing primitives with no schema change. Both tracks converge; external practice offers no counter-example.

---

## Design refinements (post-synthesis, human-directed)

Two rulings from the design review that sharpen A2/C3 and the residual-risk framing. They **correct** the synthesis where it overstated anti-self-attestation.

### R1 — Anti-self-attestation is per-prefix config, default off; evidence-binding is the real `proven` gate

The synthesis (A2 reconciled note, C3) recommended anti-self-attestation as a standing control. That is **separation of duties** — it assumes author and certifier are meant to be distinct principals. The forcing-case design deliberately **unifies** them: the capability authority (uni-zero) both authors goals and owns their delivery status. A blanket "may not tag your own entry" blocks the intended path.

Resolution: C2 (trust/role gate) and C3 (anti-self-attestation) are **alternative** controls for different threat models, chosen **per protected prefix**, not stacked. The reserved-prefix policy row carries the choice:

```
delivery:
  accepted_values:        [missing, partial, proven, claimed]   # C1 value allow-list
  required_writer:        <authority predicate>                 # C2 — see R2
  require_evidence_for:   [proven]                              # the real anti-forgery gate
  require_separate_attestor: false                              # C3 — off: unified duty by design
  removal_bar:            = set bar                             # C5
```

What actually keeps `delivery:proven` honest in this model is **not** author≠certifier; it is (1) only the authority role may write the prefix (C2), (2) `proven` requires **attached behavioral evidence** per the uni-capability skill — a protocol/validation gate, not an identity check — and (3) audit attribution of every write. C3 only buys security when an independent second principal exists to be the attestor; where duty is unified, it adds friction with no protection and is set off. It remains available per-prefix (e.g. a future `released:` tag requiring a separate signer) by flipping the flag.

### R2 — Identity: architect for enterprise, build for OSS

The synthesis flagged two residual risks — `required_writer` binds to a `TrustLevel` **tier, not an identity** (any agent at that tier could write `delivery:*`), and **attribution is declarative** (`agent_id` self-declared, `credential_type="none"`). These are **not new gaps**; they are the **same known, accepted risk** already on the identity-model evolution path (per-call agent_id → auto-enroll → Gen 3 client token). The resolution is the same seam:

- **`required_writer` is architected as an authorization predicate over a principal** so an enterprise identity provider can pin `delivery:*` to the uni-zero role specifically. The **OSS build resolves that predicate against what exists** — the client-level token (Gen 3) + `TrustLevel`. A single developer's agents share one client identity and one tier, so "only uni-zero" collapses to "only this trusted client" — the **accepted single-developer-trusts-his-agents risk**, documented, not closed in OSS.
- **Anti-self-attestation (R1) is genuinely inert in the OSS build**: with one client identity, OSS cannot distinguish author from certifier — every agent is the same principal — so the control has nothing to bite on. It becomes a real lever only at the enterprise seam where agents carry distinct principals. Default-off in OSS is therefore correct by construction, not merely by preference.
- **In OSS, the enforceable anti-forgery gate is evidence-binding (R1), not identity.** Identity tightening (who may assert `proven`) rides in on the enterprise seam later; the evidence requirement + audit trail hold today.

**Principle for implementation:** the protected-tag policy **schema is identity-shaped**; the **OSS enforcement is client-token-shaped**; the gap between them is the documented, accepted risk. Same posture as real identity — architect the seam, build the coarse resolution.
