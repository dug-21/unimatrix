# ass-094 — Authorization & anti-poison controls for `context_tag`

## Question

Assuming we implement `context_tag` (the in-place tag-mutate op recommended by ass-093), two
questions:

1. **What controls/capabilities around tag management should agents have?** What is the correct
   authorization model — capability granularity, mutation scope, protected namespaces, and any
   add-vs-remove asymmetry?
2. **What security controls beyond the Write capability are needed to keep data in the repository
   trusted?** Tags steer retrieval; an authorized-but-hostile or careless agent that can freely
   mutate any tag can bury trusted entries, boost poisoned ones, or forge status. What controls
   (poison-resistance, vocabulary, attribution, rate/budget, trust tiers) close that gap?

The forcing case remains capability delivery status (`missing | partial | proven | claimed`)
carried as a reserved-namespace tag: a `proven` flag is a trust-bearing claim, and whoever can
write it can assert a capability is delivered when it is not.

## Why it matters to the vision

- **Trusted corpus** — the self-learning engine ranks and serves on tags. If tag mutation is
  gated only by `Capability::Write`, retrieval integrity reduces to "any writer can steer
  retrieval." Trusted data requires controls on *who* may assert *which* tags on *which* entries.
- **Domain-agnostic** — the authorization model must be generic (capability + namespace + scope),
  not a bespoke status-permission. It has to generalize to any reserved namespace, not just
  capability status.
- **Poison-resistance (SLN1)** — SLN1 currently budgets `context_store` + `context_correct`
  writes. A new mutate op is invisible to that budget unless deliberately folded in. Tag mutation
  is a *cheaper* poison vector than content injection (no re-embed, immediate retrieval effect),
  so it needs at least as much scrutiny.

## Breadth

**code+ecosystem — dual-track (Case 3).**
- **Internal track** — our `Capability` enum/registry, SLN1 write-budget, `trust_source`,
  quarantine/deprecation model, audit attribution, 1-client:1-project scoping, the `entry_tags`
  lane.
- **External track** — industry practice on metadata/tag poisoning in RAG and knowledge systems,
  controlled-vocabulary governance, and authorization models for annotation/labeling.

## Approach & confidence

- **Approach**: investigation + evaluation.
- **Confidence required**: directional (a recommended authorization model + control set; no PoC).

## Goal questions

### Part A — tag-management controls/capabilities for agents (internal-anchored)

- **A1 — Capability granularity.** Is `Capability::Write` the correct gate for tag mutation, or is
  a distinct/finer capability warranted (e.g. `Capability::Tag`, or separate add vs remove)? What
  does the `Capability` enum/registry currently support, and how are capabilities assigned to
  agents/credentials?
- **A2 — Mutation scope.** Can an agent tag/untag *any* entry, or only entries it authored /
  within its project? What ownership or project scoping applies (1-client:1-project, vnc-034)?
- **A3 — Protected namespaces.** Should reserved tag namespaces (e.g. `status:*`,
  capability-delivery tags) require higher/dedicated authorization than free-form descriptive
  tags? Who may write a trust-bearing tag like `proven`? What in the current model could enforce a
  namespace boundary?
- **A4 — Add vs remove asymmetry.** Is removing a tag (burying trusted data, erasing a status)
  more dangerous than adding one? Should removal carry distinct controls?

### Part B — security controls beyond Write for trusted data

- **B3 — Attribution/provenance (internal).** What must every tag mutation record (agent,
  capability_used, trust_source, credential_type, old/new value) to stay forensically trustworthy?
  Ties to the ass-093 audit-metadata recommendation.
- **B4 — Rate/budget & abuse (internal).** How should tag mutation fold into the SLN1 write-budget
  (`audit_write_count_since`, audit.rs)? What per-agent / per-entry rate controls prevent tag
  flooding?
- **B5 — Trust tiers / quarantine (internal).** Does `trust_source` gate who may set authoritative
  tags? Can quarantined/deprecated entries be tagged? Does tagging interact with trust/lifecycle
  state?

### External track — informs A2/A3/A4/B1/B2

- **E1 — Metadata/tag poisoning patterns.** How is mutable retrieval-steering metadata abused in
  RAG / knowledge systems, and what controls beyond write-permission do mature systems use to
  resist it? (informs B1 — retrieval-steering/poison)
- **E2 — Controlled-vocabulary governance.** When and how do knowledge systems enforce tag
  allow-lists / reserved namespaces vs. free-form tagging, and the cost/benefit? (informs B2, A3)
- **E3 — Annotation/labeling authorization models.** How do multi-agent / multi-user knowledge
  bases scope who may apply which labels — ownership scoping, protected labels, add-vs-remove
  asymmetry? (informs A2, A3, A4)

## Out of scope

- Implementation of `context_tag`.
- Adding an entry-level `metadata` key/value column (schema change; deferred until a typed-value
  use case forces it — settled with the human).
- Re-opening the ass-093 mechanism decision (tags are the lane; in-place mutate is chosen).
- A domain-specific first-class `status` field on `EntryRecord`.

## Known constraints / prior art

- **ass-093 FINDINGS** (`product/research/ass-093/FINDINGS.md`) — tags outside the hash;
  `context_tag` gated on `Capability::Write`, audited op with old/new tag in metadata; fold into
  SLN1 write-budget; reserved status-tag namespace flagged as an open control; write `entry_tags`
  directly.
- **Hypothesis (challengeable)** — *"`Capability::Write` alone is sufficient."* This is the
  position under test; treat it as challengeable, not given.
- **Hard constraints** — domain-agnostic (generic capability/namespace/scope, no bespoke
  status-permission); tags are the lane; no schema change; must not weaken Architectural
  Principles 1 (hash chain), 2 (append-only audit), 3 (service-layer capability checks), 7
  (in-memory hot path), or SLN1 (poison-resistance).
- `Capability` enum / registry (`registry.rs`); `require_cap` gating (`tools.rs`).
- SLN1 write-budget (`audit.rs:79-92`, `audit_write_count_since`).
- `trust_source` field; quarantine/deprecation (vnc-010); 1-client:1-project (vnc-034, #4946).
- #5505 — two-status trap; ADR-006/#360 — `entry_tags` junction table.

## Expected output (FINDINGS.md)

1. A recommended **authorization model** for `context_tag`: capability granularity, mutation
   scope, protected-namespace handling, add-vs-remove treatment — with evidence from the current
   capability/registry code.
2. A **security control set** beyond Write required to keep the corpus trusted, each control
   scored/justified, mapped to the poison vectors it closes.
3. External best-practice grounding (poison patterns, vocabulary governance, labeling authz) with
   an explicit statement of what transfers to our model and what does not.
4. Residual poison/trust risks that would gate implementation, with mitigations.
5. A clear verdict on the challengeable hypothesis (is `Capability::Write` alone sufficient — yes/no
   and why).
