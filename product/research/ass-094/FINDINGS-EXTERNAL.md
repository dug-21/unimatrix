# FINDINGS (EXTERNAL track): Authorization & anti-poison controls for `context_tag`

**Spike**: ass-094 · **Date**: 2026-07-06 · **Approach**: investigation + evaluation (external survey) · **Confidence**: directional · **Track**: EXTERNAL — answers E1/E2/E3 only.

Scope note: the codebase was not read. "Transfers / does not transfer" is reasoned against the SCOPE.md description (MCP knowledge engine; tags drive retrieval ranking + filtering; a reserved-namespace tag carries trust-bearing status like `proven`; capability-based authz; append-only audit log; domain-agnostic goal).

---

## E1 — Metadata/tag poisoning patterns

**Answer**: The RAG literature documents four abuse patterns that apply directly to a mutable retrieval-steering tag. They are cheaper and stealthier than content injection, and write-permission stops none of them.

**Attack patterns:**
1. **Retrieval boosting / injection** — PoisonedRAG (Zou, Geng, Wang, Jia; USENIX Security 2025): five malicious texts in a base of millions → ~90% attack success. A tag that boosts ranking is a strictly cheaper lever than PoisonedRAG's crafted-text optimization — the attacker gets the ranking effect with no adversarial-text crafting and no re-embed. (arXiv:2402.07867)
2. **Retrieval burying / suppression (jamming)** — "Machine Against the RAG" (arXiv:2406.05870): *preventing* a correct answer from surfacing is a valid, stealthier attack than substituting a wrong one ("refusals aren't amenable to fact-checking, aren't anomalous"). Removing/altering a tag a legitimate entry relies on to rank is the metadata-lane blocker document. This is the direct evidence that **tag removal is an attack** (feeds A4).
3. **Fake authority via metadata / "context poisoning"** — injecting admin directives, urgent headers, fake authority markers so the model prioritizes the item (Promptfoo; Christian Schneider). A `proven` tag settable by any writer **is** exactly this pattern, formalized into a first-class flag.
4. **Metadata-filter bypass** — access control implemented as a query-time metadata filter while the store is otherwise reachable makes the filter advisory, not enforced (drainpipe.io; BeyondScale). Lesson: a tag is a *hint to the ranker*; never treat "the tag says trusted" as an enforced decision.

**Controls beyond a write bit** (OWASP LLM04:2025): per-record provenance (source/author/classification verified at retrieval time); cryptographic integrity at ingest (hash + verify before serve); provenance/attestation frameworks for trust claims (SLSA/in-toto — a trust assertion is a signed attributed statement, not a mutable flag); behavioral anomaly detection (RevPRAG, ~98% TPR / ~1% FP, arXiv:2411.18948).

**Recommendation**: Treat `context_tag` as a poison vector at least as severe as content injection (cheaper, immediate ranking effect, both boost *and* bury proven effective). Strongest transferable control = **bind every mutation to attributed provenance** (who, capability, old→new, timestamp) in the existing append-only log — the lightweight analog of OWASP's per-record-provenance/attestation. Never let a tag's *presence* substitute for an *enforced* authorization decision.

**Transfers**: the four patterns map one-to-one onto a retrieval-steering tag; provenance/audit-attribution controls transfer directly. **Doesn't transfer**: PoisonedRAG's optimization method and RevPRAG's activation-analysis detector target content/embedding/generation, not a tag-mutate op gate; per-record hashing is largely subsumed by the existing hash chain + audit log; full DSSE/Sigstore signing is heavier than a single-project engine needs — the audit record is the right-sized attestation.

---

## E2 — Controlled-vocabulary governance

**Answer**: The mature pattern is **hybrid**: an open folksonomy for descriptive tags plus a small **reserved/controlled namespace** for system-meaning tags, gated by a *distinct* mechanism from ordinary writes. Engineering systems converge on **reserved-prefix convention + enforcement at the write boundary**.

**Tradeoff**: Folksonomy scales, needs no expertise, lets vocabulary emerge, but has no authoritative term and no synonym/polysemy control (Guy & Tonkin, Webology 2007; ScienceDirect). Controlled vocabulary gives authority + hierarchy at curation cost. Stable vocabularies do emerge in open tagging but slowly and are driven by a few "supertaggers" (arXiv:1502.02777) — so you cannot rely on emergent consensus to protect a trust-bearing tag.

**Reserved-namespace exemplars (transfer best):**
- **Kubernetes** — `kubernetes.io/` and `k8s.io/` prefixes are **reserved for core components**; automated writers **must** carry a prefix, unprefixed keys are user-private; the **NodeRestriction** admission controller stops kubelets self-applying reserved labels; `kubernetes.io/metadata.name` is set **immutably** by the control plane. Cleanest model of "reserved namespace requiring dedicated authority, enforced at the mutation boundary."
- **Prometheus** — label names beginning `__` are **reserved for internal use**; `__meta_`/`__tmp` conventions. A system-owned reserved namespace.
- **GitLab scoped labels** — `key::value` with **mutual exclusion**: an item can't hold two scoped labels sharing a key; a new value replaces the prior. Models status as a single-valued key.
- **Stack Overflow** — tag *creation* is itself gated by a capability tier (see E3).

**Recommendation**: Adopt the hybrid model — descriptive tags free-form (low cost, high recall), a **reserved namespace** (e.g. `status:`/reserved-prefix) for trust tags gated distinctly. Domain-agnostic: the rule is "writes to the reserved prefix require capability X," parameterized by prefix, not a bespoke `proven` permission. Model delivery status as a **mutually-exclusive scoped key** (`status::proven` replaces `status::partial`) so a conflicting second status is unrepresentable. Enforce at the **write op**, not via later curation (supertagger dynamics are too slow to protect a trust flag).

**Transfers**: reserved-prefix + boundary enforcement (k8s), system-owned reserved namespace (Prometheus `__`), single-valued scoped keys (GitLab) — all domain-agnostic. **Cost**: one prefix-validation check on the write path buys forgery resistance; keeping descriptive tags free-form avoids taxonomy-curation overhead. **Doesn't transfer**: full taxonomy/ontology curation is unwarranted (the need is a protected flag, not a governed subject vocabulary); Prometheus "strip on read" doesn't apply (status must persist and be queryable).

---

## E3 — Annotation/labeling authorization

**Answer**: Mature systems converge on four mechanisms, all relevant: (a) **graduated capability tiers** separating stronger label ops from weaker; (b) **separation of duties / anti-self-attestation**; (c) **role/ownership scoping** for create vs. apply; (d) explicit **add-vs-remove (upgrade-vs-downgrade) asymmetry** on trust labels.

**Evidence:**
- **Stack Overflow** — distinct thresholds gate distinct ops: create a tag, retag, *suggest* a synonym, *approve* a synonym, vote-delete. Minting/approving vocabulary and destructive ops sit **above** merely applying an existing label; *approve* > *suggest*. Capability-tiered tag ops are standard.
- **Kubernetes NodeRestriction** — a kubelet may not apply reserved labels to *itself*, precisely to stop self-asserting privileged status. Most direct analog to the forcing case: **an agent must not be able to stamp `proven` on an entry it authored** — self-attestation is the vulnerability.
- **GitLab** — creating a label needs a minimum role; applying is broader. "Protected labels" is an open, *unshipped* request (gitlab-org/gitlab#293424) — role-gating which labels a member may apply is a recognized gap even mature platforms haven't fully solved, so a purpose-built gate here is defensible.
- **Microsoft Purview sensitivity labels** — **downgrading/removing** a protective label is higher-friction (justification + audit) than applying one. With the E1 jamming result (removal = burying), this establishes **removal/downgrade of a trust tag must be gated at least as strictly as addition** — often more, always audited.
- **in-toto / SLSA / OWASP** — a *trust* claim is modeled as an authenticated, attributed statement by an authority **distinct from the artifact producer**. `proven` is semantically an attestation, so it should carry who asserted it under which capability, ideally settable only by a verifying authority.

**Recommendation**: Separate "apply a trust tag" from "apply a descriptive tag" as distinct capability tiers (SO model) — reject `Capability::Write` sufficing for the reserved trust namespace on this evidence. **Bar self-attestation** of trust tags (NodeRestriction / separation-of-duties) — the highest-leverage control for the `proven` forcing case, and domain-agnostic (a rule about reserved-namespace writes on entries you authored). Make removal/downgrade **asymmetrically gated** and always audited (Purview + jamming). Attribute every trust-tag mutation so the append-only log functions as the attestation record.

**Transfers**: capability tiering, anti-self-attestation, add-vs-remove asymmetry, attributed-attestation modeling — all directly, all expressible over "reserved-namespace tag writes." **Doesn't transfer**: reputation-*score* mechanics (presume a large human community — inapplicable to a few agents holding explicit capabilities; transfer the *tiering*, not the currency); full cryptographic attestation (DSSE/Sigstore) is heavier than warranted given the existing audit log; human-in-the-loop review dialogs don't map to autonomous agents — the enforceable analog is a hard capability gate.

---

## Unanswered Questions
- Exact capability granularity / enforcement point (new `Capability::Tag`? add/remove split onto capabilities?) is internal-track work — external practice says "tier and separate," not which enum variant (A1/A4).
- Reserved-prefix (`status:`) vs. scoped `key::value` is a schema/ergonomics call — evidence supports either, mild preference for scoped single-valued key.
- No external source benchmarks the *cost* of a reserved-namespace gate in an MCP/agent setting — cost claims are reasoned from k8s/Prometheus precedent.

## Out-of-Scope Discoveries
- **Metadata-filter-bypass as tenant-isolation risk** (drainpipe.io): if tags are ever used as a *cross-project access-control* filter (not just a ranking hint), the "advisory filter over shared store" failure applies. Out of scope under 1-client:1-project (vnc-034); flag for a spike if cross-project retrieval is introduced.
- **Anomaly detection of poisoned retrieval** (RevPRAG, arXiv:2411.18948): complementary defense-in-depth layer, not a tag-authz control; possible future spike.

## Recommendations Summary
- **E1**: Treat `context_tag` as a poison vector ≥ content injection; four documented patterns (boost, bury/jam, fake-authority, filter-bypass) all apply. Bind every mutation to attributed provenance in the append-only log. Write-permission alone stops none.
- **E2**: Hybrid model — free-form descriptive tags + distinctly-gated reserved namespace for trust tags (k8s prefix + NodeRestriction; Prometheus `__`; GitLab scoped single-valued keys); enforce at the write boundary; status = mutually-exclusive scoped key. Domain-agnostic.
- **E3**: Separate trust-tag application into its own capability tier; **bar self-attestation** (highest-leverage control for `proven`); make removal/downgrade asymmetrically gated + audited; attribute every mutation.
- **Verdict input**: External practice does not support "`Capability::Write` alone is sufficient." Everywhere trust-bearing labels are governed they are separated from ordinary metadata writes by a reserved namespace, a distinct/tiered capability, an anti-self-attestation rule, and asymmetric removal controls. Final verdict is the internal track's against the actual capability model.

## Sources
- PoisonedRAG — arXiv:2402.07867 (USENIX Security 2025) · https://arxiv.org/abs/2402.07867
- Machine Against the RAG (jamming/blocker docs) — arXiv:2406.05870 · https://arxiv.org/pdf/2406.05870
- RevPRAG — arXiv:2411.18948 · https://arxiv.org/pdf/2411.18948
- OWASP LLM04:2025 Data & Model Poisoning (via Indusface) · https://www.indusface.com/learning/owasp-llm-data-and-model-poisoning/
- Promptfoo RAG poisoning · https://www.promptfoo.dev/blog/rag-poisoning/
- Christian Schneider, RAG forgotten attack surface · https://christian-schneider.net/blog/rag-security-forgotten-attack-surface/
- codesecai RAG poisoning prevention · https://codesecai.com/rag-poisoning-prevention-guide/
- drainpipe.io retrieval governance · https://drainpipe.io/knowledge-base/what-is-retrieval-governance-for-rag-and-how-do-teams-prevent-sensitive-documents-from-being-retrieved-in-the-first-place/
- Kubernetes Labels & Selectors · https://kubernetes.io/docs/concepts/overview/working-with-objects/labels/
- Prometheus data model · https://prometheus.io/docs/concepts/data_model/
- GitLab Labels / #293424 · https://docs.gitlab.com/user/project/labels/
- Guy & Tonkin, Folksonomies — Webology 4(2) 2007 · https://www.webology.org/2007/v4n2/editorial12.html
- Folksonomies overview, ScienceDirect · https://www.sciencedirect.com/topics/computer-science/folksonomies
- Supertaggers — arXiv:1502.02777 · https://arxiv.org/pdf/1502.02777
- Microsoft Purview sensitivity labels · https://learn.microsoft.com/en-us/purview/sensitivity-labels
- in-toto attestation · https://github.com/in-toto/attestation
- SLSA attestation model · https://slsa.dev/attestation-model
