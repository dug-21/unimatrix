# FINDINGS: ass-111 S3 — Shared scope semantics

**Spike**: ass-111 S3 (campaign ass-111, GH #1050)
**Date**: 2026-11-26
**Approach**: evaluation (code + ecosystem)
**Confidence**: directional. Code facts below are verified against the working tree (`file:line`). No PoC was run and no fan-out latency was measured: fan-out code does not exist. Every cost figure here is arithmetic on S2's [M] numbers and is tagged [E].
**Agent**: researcher-ass-111-s3

Unimatrix entries consulted: #5836 (personal-cloud goal, 1:1 integrity boundary clause), #5542 (KI-CHAIN), #5558 (KI-CHAIN-XV), #5563 (KI-CHAIN-XV-STRONG), #5552 (SL1), #5556 (SL2), #5560 (SL4), #5546 / #5551 / #5795 (summaries only). Prior findings built on: S1 (final), S2 plus the architect review (all nine [S3] constraints), S4, S5, and ass-060 FINDINGS Q5/Q6 (owner store). No Unimatrix writes.

---

## Ground truth that shapes the design (code-verified)

| # | Fact | Where | Consequence for shared scope |
|---|---|---|---|
| F1 | The "hash chain" is a **per-lineage correction chain**, not one linear chain per store. A correction sets `previous_hash = predecessor.content_hash` and `version + 1`, inside the store's own transaction. | `unimatrix-store/src/write_ext.rs:487-603`; `chain_verify.rs` (looks predecessors up by `supersedes` id *within one corpus*, and reports `MissingPredecessor` otherwise) | A correction whose predecessor sits in another store cannot be verified. It would show up as `MissingPredecessor`. So "reader corrects a shared entry" cannot exist as a chain operation (constraint 2). |
| F2 | `content_hash` = SHA-256 over `title: content` only. | `unimatrix-store/src/hash.rs:7` | Status, category, tags and `created_by` are outside the hash. A raw-DB status flip on a shared entry (for example un-quarantining it) is invisible to `chain_verify`. See Out-of-Scope Discoveries. |
| F3 | Learning counters live **on the entry row**: `access_count`, `helpful_count`, `unhelpful_count`, `confidence`, `last_accessed_at` are updated in place by `UPDATE entries …`. | `db.rs:707-735`; `write_ext.rs:47-110` (`record_usage_with_confidence`) | Recording a reader's usage of a shared entry "normally" would write into the shared store. Any reader-side learning about shared entries therefore needs a reader-owned overlay table. |
| F4 | Entry ids are a per-store `INTEGER PRIMARY KEY` allocated by `counters::next_entry_id`. Every learning and reference table keys on a bare integer: `co_access(entry_id_a, entry_id_b)`, `injection_log.entry_id`, `feature_entries.entry_id`, `query_log.result_entry_ids` (JSON text), `audit_log.target_ids` (JSON text), `graph_edges(source_id, target_id)`, `goal_clusters.entry_ids_json`. | `db.rs:812-1250` | Constraint 1 is not hypothetical. Every one of these tables would silently alias a shared id onto a home id. |
| F5 | Search, briefing, hook injection and compaction restore all run through **one function**, `SearchService::search`. | `mcp/tools.rs:801`, `services/index_briefing.rs:204`, `uds/listener.rs:1677` | Federation can be one layer wrapped around that funnel. No retrieval surface needs its own fan-out code. |
| F6 | The fused score is a weighted sum of terms in [0,1]: sim, nli, conf, coac, util, prov, plus two additive phase terms. Several of those terms are **store-relative**. `util` comes from the store's effectiveness (its own `injection_log`). `prov` comes from the project config's `boosted_categories`. `phase_explicit` looks up the reader's `PhaseFreqTable` by bare entry id. PPR and Contradicts suppression run over the store's own `TypedGraph`. The query embedding is adapted by the store's `AdaptationService` (`search.rs:677-680`). | `services/search.rs:98-122, 237-249, 1226-1340, 1366-1415` | A cross-store merge needs an explicit rule (constraint 4). Concatenating per-store `final_score` values is not sound. |
| F7 | All `id` tool params use a strict int-or-numeric-string deserializer. `"42"` parses to 42, and `"decisions:42"` is **rejected**. | `mcp/serde_util.rs:19-50`; `GetParams`, `CorrectParams`, `DeprecateParams`, `QuarantineParams`, `TagParams` in `mcp/tools.rs:285-410` | A non-numeric qualified id (`alias:id`) is rejected by every mutation tool today, before any handler runs. Mis-targeting is unrepresentable as long as a qualified id is **never** rendered as a bare number or a numeric string. |
| F8 | `ATTACH` appears nowhere in the Rust code. | grep of `crates/` | The S2 one-way trap has not been entered. Keep it that way with a gate (Q1 recommendation). |
| F9 | `Status::Proposed = 2` exists, but no write path uses it. | `unimatrix-store/src/schema.rs:13`; S5 | It is available for a post-launch proposal path into shared spaces. |

---

## The shared-scope model (summary)

```
Tenant T (owner)
 ├─ Project P1 (home store, kind=project)  ──read fan-out──┐
 ├─ Project P2 (home store, kind=project)  ──read fan-out──┤   via enumerated grants,
 └─ Shared space S (kind=shared, own SQLite + HNSW,  ◄─────┘   keyed on the HOME project
      own chain, own audit, own PCD, learning_mode=curated)
           ▲ writes only via S's own URL /v1/T/S by principals holding Writer on S
```

- A **shared space** is a project-shaped store (S1 hierarchy; ass-060 "owner store = distinguished slug") with `kind = shared` on the control-plane project registry. It is never an ordinary project that has been flagged "visible".
- A **grant** is a control-plane row: `(grant_id, shared_project_id, consumer_tenant_id, consumer_project_id, alias, exposed_classes, max_results, created_by_principal, created_at, revoked_at)`. It is enumerated per consumer project, with no wildcard (S1 R8, Hard). It is audited in the control-plane audit (S1 G11).
- **Read** = server-side fan-out from the home store to the granted shared stores, through `resolve_store`-issued read-only views. **Write** = an ordinary write addressed to the shared space's own URL. **Learning** = one-way, into the reader's store only.
- Depth is 1. A shared space has no grants of its own, so there is no transitive sharing.

---

## Findings

### Q: "What may a tenant/project read from a shared scope, and how do shared entries merge into its retrieval, briefing, and injection ranking?"

**Answer**: A consumer project reads **active, non-superseded knowledge entries** of the shared spaces it holds grants for, through the ranked surfaces (search, briefing, hook injection, compaction restore) and through qualified `context_get`. Nothing else in the shared store is readable. Each store contributes candidates ranked by its own pipeline (HNSW, PPR, supersession injection and Contradicts suppression, all within that store). The reader then **re-fuses the union in its own frame**, with its own weights. Store-relative terms are either recomputed for foreign entries or set to a defined neutral value, so that home-only behaviour is byte-identical when no grants exist.

**Read surface (rules SS-R1…SS-R8)**

| Rule | Statement |
|---|---|
| SS-R1 Readable | The shared store's `entries` (+ `entry_tags`). Ranked surfaces return only `Active`, non-superseded entries, whatever the reader's `RetrievalMode`. Foreign entries always use `Strict` semantics, because the reader cannot correct them. Qualified `context_get` follows supersession to the active terminal by default (vnc-042 semantics) and returns the stored version with `follow_supersessions=false`. Quarantined and `Proposed` entries are never returned to a consumer. |
| SS-R2 Not readable | The shared store's `audit_log`, `query_log`, `observations`, `sessions`, `cycle_events`, `injection_log`, `co_access`, `graph_edges` (no `context_graph` / `context_edge` across stores in V2 initial), status report, cycle review, agent registry, and PCD (except category names, which are carried on the entry). |
| SS-R3 Qualified ids on the wire | A foreign entry is rendered as the string `"{alias}:{id}"` (for example `"org-decisions:42"`), never as a number or a numeric string. The alias is the grant alias. It is unique per consumer project, never all digits, **immutable**, and tombstoned on revoke (the S1 R7 discipline). Home ids stay integers. Briefing and index tables (`CONTEXT_GET_INSTRUCTION`) render the qualified form. Under F7, any mutation tool given a qualified id rejects it before the handler runs. Add an explicit error message ("foreign entry is read-only; fork it") in place of the generic parse error. |
| SS-R4 Qualified ids at rest (constraint 1) | Reader-owned rows that reference a foreign entry carry `(store_id = immutable project_id, entry_id)`. `injection_log` and `feature_entries` get a nullable `store_id` column (NULL = home). `query_log.result_entry_ids` and `audit_log.target_ids` use qualified JSON objects `{"s":<project_id>,"e":<id>}` for foreign ids. `co_access`, `graph_edges` and `goal_clusters` **never** receive foreign ids in V2 initial (SS-L5). This is additive schema work (ass-060 invariant 4). |
| SS-R5 Surfaces | Search, briefing, hook injection and PreCompact restore are federated **by default** when grants exist. That is the value: shared decisions arrive before the agent asks. Each call accepts an optional narrowing `scope: "home" \| "all" \| ["alias", …]`. `context_lookup` (exact filters) is **home-only by default** and federates only when `scope` is given. The payload may *narrow* within the server-side grant set. It can never add a store (see Q4, refinement of S1 R1). |
| SS-R6 Class exposure | The grant carries `exposed_classes`, defaulting to {K1, K2, K3}. K4 ingestion and K5 public-chat principals of the consumer project **do not fan out** unless the Owner adds them. Without this rule, an internal decisions space granted to Med Search's project would be served to anonymous web users through the K5 backend token. |
| SS-R7 Status freshness | Fan-out is live. Status and content come from the shared store at query time, the same way home entries do today (~33 point reads per search, S2 M12). So a correction, deprecation or quarantine in the shared space is visible to every consumer on its next query. Derived state (shared `TypedGraph`) lags by its debounce, as at home (S2 Q2). |
| SS-R8 Degradation (P5) | If a granted shared store is not resident and its cold-start exceeds the fan-out budget, or a cross-cell call times out, the request serves **home-only**. The response carries `shared_degraded: [alias…]`. A shared-store failure never blocks or fails a home request. |

**Merge rule (constraint 4): home-frame re-fusion**

1. **Embed once.** Compute the raw base-model query embedding once (constraint 8: same model and dimension; checked at grant creation). The home store applies its own adapter. Shared stores run with **identity adaptation** (SS-L6), so their similarity sits in the base-model frame.
2. **Per-store candidate generation.** Each store `s ∈ {home} ∪ grants` (at most `G_max`) runs the stages that depend only on its own state: HNSW top-`k_s`, supersession injection, PPR expansion and blend over `s`'s own graph, status filter (SS-R1 for foreign), and Contradicts suppression within `s`. It returns candidates with their term vector `(sim_s, conf_s, category)`. No candidate ever refers to another store's id. PPR never crosses stores.
3. **Union scoring in the reader's frame**, using the reader's `FusionWeights` (reader PCD):

| Term | Home candidate | Foreign candidate |
|---|---|---|
| `sim` | as today (PPR-blended) | as returned by `s` (PPR-blended within `s`) |
| `nli` | computed once over the **union** (query-candidate cross-encoder, store-independent) | same |
| `conf` | home `entry.confidence` | owning store's `entry.confidence`. Curated, so it is a pure function of the shared corpus (SS-L3) |
| `util` | home effectiveness | **neutral** (`utility_delta(None)`) in V2 initial. Later from the reader's own qualified `injection_log` (SS-L2) |
| `prov` | reader `boosted_categories` | same, applied by category name |
| `phase_histogram` | reader session histogram by category | same (category-name keyed, so valid cross-store) |
| `phase_explicit` | reader `PhaseFreqTable` | the value a home entry **absent from the bucket** receives today (`search.rs:1316-1330`), so foreign entries are neither boosted nor penalised relative to unseen home entries |
| status penalty | as today | 1.0 (only Active foreign entries reach this stage) |

4. **Caps and total order.** Each grant contributes at most `max_results` (default ⌈k/2⌉) after scoring. The merged list is sorted by `(final_score desc, home before foreign, store_id asc, entry_id asc)` and truncated to `k`. The order is total, so it serves determinism (constraint 5). The reader's similarity and confidence floors apply to the union.
5. **Reader-local suppression.** A foreign entry is dropped from the reader's results when the reader has **forked** it (Q2) or holds a reader-owned **override** row `(home_entry_id → store_id, entry_id)`. This is the layered-configuration pattern (org default, project override). Contradictions *between* stores are not detected in V2 initial, because no cross-store edges exist. The override is the explicit, reader-owned way to say "our project decided otherwise".

**Why re-fusion and not RRF or raw score concatenation.**
- Concatenating per-store `final_score` values is unsound under F6. `util` and `prov` are store-relative, and a store whose NLI is absent re-normalises its weights (`FusionWeights::effective`), so equal scores would mean different things.
- Reciprocal Rank Fusion is the industry default for heterogeneous engines because it discards score magnitudes ([Elastic RRF](https://elastic.co/guide/en/elasticsearch/reference/8.x/rrf.html)). Here it would force-interleave weak shared hits with strong home hits.
- Unimatrix controls every store's model, formula and term ranges, so a common frame is available. Keep RRF as the fallback if the calibration test (below) fails.

**Proving it** (V2 acceptance):
- **Parity.** With zero grants, every surface returns byte-identical results to home-only. This extends PDN1/W2 additivity (S5).
- **Twin test.** Store an identical entry in home and in a shared space. The foreign copy's `sim` and `nli` equal the home copy's, and `|Δfinal| ≤ w_conf·|Δconf| + w_util·|Δutil|`.
- **Isolation gate.** A non-granted project cannot obtain any foreign entry through any surface, a granted project cannot write or record learning into the shared store, and the gate is tested in both directions (lesson #5348).

**Cost [E]**: each fan-out store adds about 33 synchronous point reads per search (S2 M12) plus one HNSW search. With `G_max = 3` that is ≤ ~132 reads, run in parallel per store on the same cell. Against S2's single-store p50 of 12 ms [M10], fan-out is expected to stay within the same order of magnitude, but **it is unmeasured**. NLI over the union grows with `Σ k_s`. NLI is off by default (`w_nli` path) and is the only term whose cost scales with fan-out.

**Recommendation**:
- Adopt SS-R1…SS-R8 and home-frame re-fusion.
- Set `G_max = 3` grants per consumer project and `max_results = ⌈k/2⌉` per grant at launch (constraint 7).
- Ship the parity, twin and bidirectional-isolation tests as the definition of done.
- Implement federation as one `FederatedSearch` layer wrapped around `SearchService::search` (F5), using `resolve_store` read views.
- Add a CI gate that rejects `ATTACH` and any SQL naming a second database (F8, S2 reversibility, architect C6).

---

### Q: "Who may write to a shared scope, and whose hash chain and audit log does a shared write enter? How are corrections/deprecations of shared entries propagated?"

**Answer**: Only principals that hold a **Writer or Owner role on the shared space itself**, with the request addressed to the shared space's own URL. In V2 initial those principals must be **members of the owning tenant**. A consumer-tenant user who needs to contribute is invited into the owning tenant. Every shared write enters **only the shared store's chain and audit log**. Corrections and deprecations are not "propagated": consumers see them on their next query because fan-out is live. Reader-side residues (forks, overrides, overlay counters, qualified references) are reconciled by rule, never by sync.

**Write rules (SS-W1…SS-W7)**

| Rule | Statement |
|---|---|
| SS-W1 Addressing | A shared write is an ordinary `context_store` / `correct` / `deprecate` / `edge` / `quarantine` call on `/v1/{owner_tenant}/{shared}`. It is authorised by S1's per-request grant check on exactly that project (S1 R3). A consumer binding has **no** write path into a shared store. Its granted stores are read-only views (Q4). |
| SS-W2 Who | Owner: everything, including quarantine (Admin stays human-Owner-only, S1). Writer: store, correct, deprecate, edge. Service and workload principals may be granted Writer on a shared space for **decisions-as-code**: a K3 CI job syncing an `org-decisions` repo into the space. Owners may require tier T2+ for writes to `decision`-class categories (S1 R5 tier-gate). Shared content is injected into many projects' agents, which makes shared write access a prompt-injection amplifier. Content scanning applies as for `context_store`. |
| SS-W3 Chain and audit | Shared writes produce entries, lineage links (`previous_hash`/`version`, F1) and `audit_log` rows **in the shared store only**. Each row carries `principal_id`, home `tenant_id`, `credential_type`, `attestation_tier` (S1 G9) and the self-asserted `agent_id` as annotation. Nothing is written to any consumer store at write time. |
| SS-W4 Publish (home → shared) | "Promote this project lesson to the org space" is a write **addressed to the shared URL**. The server resolves the cited source `(project_id, entry_id)` through `resolve_store` under the same principal's read grant (never cross-DB SQL) and copies title and content. It records provenance metadata `published_from = {project_id, entry_id, content_hash}`. Provenance is metadata, **not** `previous_hash`: the copy is a genesis entry (`version` 1) in the shared chain. The two entries are never synced. |
| SS-W5 Fork (shared → home) | A consumer that needs to deviate forks: a home `context_store` that cites the foreign qualified id. It creates a reader-owned entry with `forked_from = {project_id, entry_id, content_hash}` and a reader-local suppression of the original (merge step 5). This is the only "correction" a consumer can make. It is the copy-in that constraint 2 permits: a new entry in the reader's chain, never synced, never kept in step. |
| SS-W6 Cross-tenant writes | **No** in V2 initial. There are no cross-tenant write grants and no consumer proposal path. Post-launch candidate: a `propose` grant that writes `Status::Proposed` (F9) into the shared store, excluded from all retrieval until a shared-space Writer promotes it. That is a cross-tenant write into another tenant's chain, so it needs its own isolation proof. |
| SS-W7 Grant admin | The shared space's Owner creates and revokes grants, and the **consumer project's Owner must accept** each one. Accepted shared content flows into the consumer's proactive injection, so an unaccepted grant would let one party push content into another's agents. Intra-tenant, this is the same Owner. Grant changes go to the control-plane audit (S1 G11). |

**Propagation of corrections, deprecations and quarantines**

| Event in shared space | What a consumer sees | Reader-side residue and its rule |
|---|---|---|
| Correction (A → A′) | The next query serves A′. Supersession injection runs inside the shared pipeline. Qualified `get("alias:A")` follows to A′. | Overlay counters keyed on A are **not** migrated, matching today's semantics where a correction starts at `access_count = 0`. Forks of A are flagged `upstream_changed` when the fork's stored `content_hash` ≠ the current terminal's hash. This is reported in the reader's status and briefing, never auto-applied. |
| Deprecation | Dropped from ranked surfaces (SS-R1). Reachable by qualified get. | Forks are flagged `upstream_deprecated`. |
| Quarantine | Excluded everywhere, immediately. | Same as deprecation. Reader rows keep only ids, never content. |
| Grant revoked | Fan-out stops within the authorization-cache revocation bound (architect C9). | Qualified references in reader logs remain as history and resolve to "unavailable". **Forks remain reader-owned** (escalation E3). |
| Shared space deleted | Same as revoke. | The `project_id` is immutable and never reissued (S1 R7), so a dangling reference can never resolve to a different store. |

**Integrity strengthening specific to sharing**
- **Reader-side observation of what was served.** The reader's audit row for a federated read records `{s, e, content_hash}` for each foreign entry served. This is not a chain link: `chain_verify` never walks it. It gives the consumer tenant independent evidence of the exact shared content its agents received. If shared content is later rewritten by a raw-DB writer (the owner's operator or the platform operator), the consumer's own audit contradicts it. This is the cheapest cross-tenant step toward KI-CHAIN-XV-STRONG (#5563).
- **Hash on the wire.** Foreign entries are returned with `content_hash`, so a client can recompute `SHA-256(title: content)` (F2).
- **Store-level head anchoring** (post-launch, under KI-CHAIN-XV-STRONG): each store maintains an append-only head over every mutation (id, content_hash, status), returned as `(store_id, corpus_epoch, head)` and recorded in consumer audit. Sharing amplifies the case S2 made. One silent edit of a shared decision reaches N projects, and today's chain does not cover status changes (F2).

**Recommendation**:
- V2 initial: writes go only via the shared space's own URL, by members of the owning tenant with Writer or Owner on the space.
- Shared writes enter only the shared chain and audit, with full S1 G9 attribution.
- Build publish (home → shared) and fork (shared → home) as **copies with hash-pinned provenance metadata**, never `previous_hash` links.
- Corrections propagate by live fan-out. Forks get `upstream_changed` detection.
- Require consumer-side acceptance of grants.
- Record `content_hash` of every served foreign entry in the reader's audit.

---

### Q: "Does shared-scope usage feed learning — the shared scope's, the reader's, both, neither — without cross-tenant leakage?"

**Answer**: **The reader's, one way. The shared scope's, none.** In V2 initial a shared space records **no traffic-derived learning from anyone**, including its own owner-tenant writers. A consumer's usage of shared entries lands only in the consumer's store, keyed by qualified id, under the consumer's own S4 predicate. Its effect is limited to that consumer's ranking.

**Learning rules (SS-L1…SS-L7)**

| Rule | Statement | Why |
|---|---|---|
| SS-L1 Reader traffic never writes into a shared store | No access counts, votes, `co_access`, `query_log`, `injection_log`, observations, confidence recompute or feature attribution go into the shared store from consumer traffic. Enforced by type: a consumer request holds only a read view of the shared store, with no write or learning methods (Q4). | F3: the counters live on the shared entry row, so any "normal" usage path would mutate the shared store (constraint 3; S4 LI-8). |
| SS-L2 Reader-side overlay | Consumer usage of foreign entries is recorded in a reader-owned `foreign_usage(store_id, entry_id, access_count, helpful, unhelpful, last_accessed_at, principal_id)` plus qualified `injection_log` / `query_log` rows. It is gated by the reader's LI-1 predicate (read token, `none` contribution or deterministic project ⇒ nothing recorded). V2 initial **records** this overlay. Applying it in ranking (a reader-local util and helpfulness term for foreign entries) waits for the twin test to show it is calibrated. | One-way learning toward the reader is exactly what constraint 3 permits. It also gives SL1/SL2 reach: "which org decisions did this cycle use, and were they helpful?" in the consumer's own retro. |
| SS-L3 Shared space is curated | A shared space has fixed `learning_mode = curated`: every request-time sink is off for every principal (S4 D1/D2), and confidence uses the deterministic profile (S4 D3: base, correction and trust terms only, recomputed on write). Auto-quarantine and extraction are off (D5). Corpus-derived graph inference (NLI/cosine edges) is **off by default**, and the Owner may enable it; any edge write bumps the corpus epoch. | (a) Shared ranking state changes only when its corpus changes, which makes constraint 5 solvable. (b) The only influence path into consumers is the owner's explicit curation, the thing the grant authorises. (c) Usage-driven confidence in a many-reader space is a poisoning surface (SLN1 #5528). (d) There is nothing to merge: no consumer signal has a home in the shared store. |
| SS-L4 No reader-to-reader influence | Reader A's overlay is in A's store, and B cannot read it. Structural. | The Hard constraint: no tenant or project influences another's learning except through the grant. Consumers are not party to each other's grants. |
| SS-L5 No foreign ids in learned graph state | `co_access`, `graph_edges` (CoAccess / Informs / Supports), `goal_clusters` and `PhaseFreqTable` never key on foreign entries in V2 initial. | These structures key on bare u64 ids (F4) and feed PPR. Qualifying them is a larger change with a measured-benefit bar. |
| SS-L6 Models | Shared stores use identity MicroLoRA adaptation and never train one. No adapter is trained across projects (architect C13). | It keeps the similarity frame common for re-fusion and removes a determinism hazard (S4 D6). |
| SS-L7 Audit is never a learning source | Reader audit rows that name foreign entries (Q2) are never read by a learning job. | S4 LI-3, applied to the new rows. |

**Explicit aggregated feedback channel (post-launch, constraint 3's "explicit, granted, aggregated input")**
- A grant flag `feedback: on` lets consumers' **explicit** votes on foreign entries be aggregated by the platform into a shared-store-owned `consumer_feedback(entry_id, helpful, unhelpful, distinct_consumers, window)`.
- Only consumers that opted in contribute. A minimum distinct-consumer threshold (for example ≥3) applies before any row is visible, and rows carry no consumer ids, sessions or query text.
- It feeds **owner review only** (the shared space's status and curation queue), never ranking.
- This keeps SS-L3 intact while giving owners a signal about stale shared decisions.

**Determinism (constraint 5)**
- A deterministic reader (S4 D-mode) **includes** shared results.
- The S4 fingerprint is extended with `grant_set_version` and `[(store_id, corpus_epoch, derived_epoch)]` for each fanned-out store. Results are `settled` only when every store is settled.
- The reader's D-mode retrieval contract (exact kNN, no optional ML, total order) is applied to its fan-out queries too. Shared spaces are small and curated (SS-L3), so exact kNN over them is cheap [E].
- Because a shared space's derived state changes only with its corpus epoch, "equal fingerprints ⇒ identical results" holds.
- If D-mode is not built (S4 E1), the extension costs nothing beyond carrying epochs.

**Recommendation**:
- Answer: the reader's, one-way. The shared scope's: none in V2 initial.
- Enforce SS-L1 by type (read views), not by convention.
- Record the reader-side overlay at launch, and apply it in ranking only after calibration.
- Fix shared spaces at `learning_mode = curated`.
- Extend the deterministic fingerprint with shared-store epochs.
- Defer the aggregated feedback channel to post-launch, owner-review only.

---

### Q: "How does '1 client : 1 project integrity boundary' evolve (server-enforced per-project claim)?"

**Answer**: The boundary holds for **writes and learning** and is relaxed only for **reads**. Restated: **one client binding has exactly one write-and-learning target (the URL project). It has 0..`G_max` read-only fan-out targets, which the server resolves from grants keyed on that home project and the client never names as an addressing input.** The "server-enforced per-project claim" that #5836 names as the enterprise relaxation is a stack of five server checks. None of them relies on the client.

**Mechanism (SS-B1…SS-B6)**

| Rule | Statement | Evidence / precedent |
|---|---|---|
| SS-B1 URL = the only write address | Unchanged S1 R1/R3. Every mutation applies to the URL project's store, after a per-request write-grant check on exactly that project. | #5836 ("the resolved store handle is the sole write capability"); `seam.rs` |
| SS-B2 Type-separated handles | `resolve_store` returns the home `StoreHandle` (read, write, learning sinks). A new `resolve_grants(home)` returns `Vec<ReadView>` that expose only `search_candidates`, `get` and `lookup`. Neither the `UsageService` sinks nor the store mutation API accept a `ReadView`. The guarantee is compile-time, not a predicate. | Extends the single-funnel contract (vnc-034 ADR-003; S1 Q2). It is the read-side analogue of LI-2 "sinks enforce, not routes". |
| SS-B3 Mis-targeting unrepresentable | Foreign ids are only ever `"{alias}:{id}"` strings (SS-R3). The strict id deserializer (F7) rejects them in every mutation tool. Votes (`helpful`) on a foreign id route to the reader's overlay (SS-L2), never to a home entry with the same number. | vnc-034 FR-X2 ("mis-targeting unrepresentable"); `serde_util.rs:35-38` |
| SS-B4 Payload may narrow, never widen (refines S1 R1) | `scope` and qualified ids may *select among* the server-side grant set. A store not granted to the home project returns the same fixed 404 as an unknown slug (no enumeration, vnc-050 R-03.3). A payload never selects a write target. | S1 R1/R2 |
| SS-B5 Attested project claim (T2+) | Unchanged S1 R3 addendum. A T2 write whose attested `cwd`/`location` project ≠ the URL project gets 409. So a harness in repo P1 **cannot write into shared space S**, even if the human holds Writer on S. Shared-space writes come from a binding *whose* project is S (an `org-decisions` repo bound to S), or from T1 clients (CLI publish, claude.ai connector to S, a CI trust policy on S). Fan-out reads need no claim. | S1 R3; S6 |
| SS-B6 Shared spaces are never consumers | A `kind = shared` project cannot hold grants (depth 1), and a client bound to S has no fan-out. | No transitive leakage, no fan-out amplification (constraint 7) |

What this changes relative to V1: in V1 the 1:1 boundary was enforced *by the absence of any second store* on a request. In V2 a request may touch several stores, so the boundary must be enforced by **handle type** (SS-B2) and **id representation** (SS-B3). Both can be tested in isolation. The bidirectional isolation gate (S1 Q2 first proof) gains three cases: a consumer cannot write S; a consumer cannot record learning in S; a non-consumer cannot read S.

**Recommendation**:
- Restate the #5836 clause as "1 client binding : 1 write-and-learning store; reads may fan out to enumerated, granted, read-only stores".
- Enforce it with SS-B1…SS-B6.
- Make SS-B2 (type-separated `ReadView`) and SS-B3 (non-numeric qualified ids) launch-blocking for any fan-out code.
- Fix the qualified-id wire format before V2's MCP tool schemas are published, even if fan-out ships later. Changing every tool's `id` contract afterwards is a breaking change for every client.

---

## Conformance with the architect's nine [S3] constraints

| # | Constraint | How this design satisfies it |
|---|---|---|
| 1 | Qualified `(store_id, entry_id)` refs | SS-R3 (wire), SS-R4 (at rest, using immutable `project_id`), SS-L5 (learned graph state never takes foreign ids) |
| 2 | No cross-store chain links or synced copies | Writes enter only S's chain (SS-W3). Publish and fork are genesis copies with provenance metadata, never `previous_hash` (SS-W4/W5). No sync. F1 shows a cross-store link would fail `chain_verify` anyway |
| 3 | One-way learning toward the reader | SS-L1 (by type), SS-L2 (reader overlay), SS-L3 (curated shared space). The feedback channel is explicit, granted, aggregated and owner-review-only (post-launch) |
| 4 | Per-store derived state + merge rule | Per-store candidate generation (PPR, suppression, supersession within each store), then home-frame re-fusion with defined neutral values. RRF as the fallback |
| 5 | Determinism | Shared spaces are curated, so their derived state is epoch-driven. The fingerprint is extended with per-store epochs and the grant set version. The D-mode contract is applied to fan-out. Total order includes `store_id` |
| 6 | Placement | V2 initial is intra-tenant only (H4 verdict), so the tenant's projects and spaces are pinned to one cell and one region. Grant creation rejects a region mismatch. Cross-cell fan-out (post-launch) uses authenticated RPC with SS-R8 home-only degradation |
| 7 | Bounded fan-out | `G_max = 3`, `max_results = ⌈k/2⌉` per grant, depth 1 (SS-B6) |
| 8 | Same embedding model | Checked at grant creation against the cell model catalog. Shared spaces use identity adaptation (SS-L6) |
| 9 | Resident and budgeted | A shared space is resident while any consumer is resident. Its memory and scheduler cost are charged to the owning tenant (the same tenant at launch). Curated mode makes its tick cost corpus-change-only |

None of the nine is escalated as wrong. One refinement: constraint 3 allows "any shared-scope feedback channel … S3's design call". I chose to **exclude it from V2 initial** rather than design it into ranking.

---

## H4 go/no-go (V2 initial release)

**Verdict: CONDITIONAL GO, narrowed to intra-tenant shared spaces. NO-GO on cross-tenant sharing for V2 initial.**

**In V2 initial**
- `kind = shared` spaces.
- Enumerated, consumer-accepted grants **within one tenant**.
- Read-only fan-out across search, briefing, injection and compaction.
- Curated (no implicit learning).
- Writes only by the owning tenant's members via the space's own URL.
- Publish and fork copies.
- Reader-side overlay recording.

**Why go**
- H4's stated need, enterprise shared decisions, is *intra-tenant*: an org tenant's decisions shared across that org's projects.
- The human's own first use case is also intra-tenant: conventions shared across their development and research projects in one personal tenant.
- The intra-tenant form makes constraints 6 and 9 trivial (one tenant, one cell, one region, one bill payer).
- It passes the Hard isolation constraint with no new cross-tenant trust relationship.

**Why not cross-tenant at launch**
- It adds two-party consent and billing.
- It needs placement co-location of two tenants, or cross-cell RPC.
- Cross-tenant region mismatch becomes possible.
- A cross-tenant write or proposal path would need its own isolation proof.
- No validating tenant needs it: Med Search does not.
- The grant row already keys on `(consumer_tenant, consumer_project)`, so enabling it later is a policy flag plus those proofs, not a re-architecture.

**Conditions (all launch-blocking for the shared feature; the first two are launch-blocking for V2 regardless)**
1. The qualified-id wire format (SS-R3) is fixed in the V2 MCP tool schemas from the first V2 release.
2. A CI gate forbids `ATTACH` or multi-database SQL (S2's one-way door).
3. `ReadView` type separation (SS-B2).
4. The parity, twin and three-case bidirectional isolation tests pass.
5. Sequencing: shared spaces ship **after** the residency and scheduler work (S2) and the cross-tenant isolation gate (S1). Fan-out depends on resident stores and dynamic resolution.

**Challenge to H4's framing**: H4 says "readable by ≥1 tenant/project". The evidence supports "readable by ≥1 **project of the same tenant**" for V2 initial. "Tenant" in that sentence should be deferred.

---

## Escalations for human

- **E1 — H4 scope.** Approve intra-tenant-only shared spaces for V2 initial, with cross-tenant sharing deferred (recommended). The alternative, including cross-tenant at launch, adds consent, billing, placement and isolation-proof work with no validating tenant.
- **E2 — Shared spaces are curated (no usage learning, not even from the owner's own traffic) at launch.** This is a deliberate exception to the self-learning goal ("every deployment improves from use") for one store kind. It trades shared-space self-improvement for determinism, poisoning resistance and a clean isolation argument. Reader-side learning about shared entries is kept (SS-L2).
- **E3 — Forks survive grant revocation.** A consumer that forked shared content keeps the reader-owned copy after the grant is revoked. This is no worse than a human copying text, but it is a data-governance position an enterprise customer will ask about. The alternatives are to forbid forking or to purge forks on revoke. The latter needs a per-record erasure exception that conflicts with Principles 1–2 (S2 Q4).
- **E4 — No cross-tenant writes and no consumer proposal path at launch.** Contributors must be invited into the owning tenant. Confirm this is acceptable for the "enterprise shared decisions" story.
- **E5 — Public-chat exposure is opt-in per grant** (SS-R6, default excludes K4/K5). This confirms that shared content never reaches Med Search-style anonymous users unless the Owner says so explicitly.

---

## Unanswered Questions

- **Fan-out latency and NLI-over-union cost.** Not measured, because no fan-out code exists. The ≤ ~132 point reads at `G_max = 3` figure is arithmetic on S2 M12. Measure it at delivery as an acceptance criterion, in-process over HTTP (architect C11).
- **Confidence calibration between an adaptive home and a curated shared store.** Home confidence includes usage and freshness terms. Curated shared confidence does not, so foreign entries may be systematically under- or over-ranked. Needs the twin test on a real corpus pair. Only one real corpus exists (S2 §0).
- **Cross-tenant placement and cross-cell RPC design.** Deferred with cross-tenant sharing (H4 verdict). Belongs to whichever spike or design re-opens it.
- **ONNX cross-host reproducibility under fan-out.** Inherits S4's open residual risk unchanged, because shared stores are embedded with the same model on the same cell at launch.
- **Whether `utility_delta(None)` is the right neutral `util` for foreign entries.** It depends on the effectiveness classifier's semantics for unclassified entries. This is a delivery-time check, not resolvable without the calibration test.

---

## Out-of-Scope Discoveries

- **`content_hash` covers only `title: content`** (`hash.rs:7`). Status, category, tags and attribution are outside both the hash and the chain. A raw-DB status flip (un-deprecate, un-quarantine) or a recategorisation is undetectable by `chain_verify`. This feeds KI-CHAIN-XV-STRONG (#5563). It is more serious under sharing, because one edit reaches N consumers.
- **The search audit row's `target_ids` are bare integers** (`search.rs:1437-1447`). Once any fan-out exists, they become ambiguous. They are covered by SS-R4, and also needed by any forensic tool built before then.
- **The id deserializer accepts numeric strings** (`serde_util.rs:35`). That is fine today, but it makes a numeric-looking qualified id (for example `"100042"` with an offset scheme) a silent mis-target. Record as a pattern candidate: "foreign ids must never be digit-only".
- **The `phase_explicit` neutral value for entries absent from a populated bucket is 1.0** (`search.rs:1316-1330`), the term's maximum. Unseen entries may therefore receive the full phase term while seen entries receive their (≤1.0) rank score. This may be intended ("no suppression"), but it is worth a calibration check, because the merge rule reuses it for foreign entries.
- **The per-row learning counters on `entries` (F3) are a structural reason the V1 schema cannot host multi-reader learning.** Any future "shared learning" design needs an overlay or aggregate table, not new columns.

---

## Recommendations Summary

- **Q1 (read and merge)**:
  - Consumers read only active, non-superseded entries of granted shared spaces, via search, briefing, injection and compaction (federated by default) and qualified `get`. Lookup is home-only unless scoped.
  - Foreign ids are the non-numeric `"alias:id"` on the wire and `(project_id, entry_id)` at rest.
  - Merge by home-frame re-fusion: per-store candidates (PPR and suppression within each store), NLI over the union, store-relative terms neutral for foreign entries, reader weights, a total order including `store_id`.
  - `G_max = 3` and `⌈k/2⌉` per grant. K4/K5 are excluded by default.
  - Prove it with the parity, twin and bidirectional isolation tests.
- **Q2 (write, chain, audit, propagation)**:
  - Only owning-tenant members with Writer or Owner on the shared space write, via its own URL.
  - Writes enter only the shared chain and audit, with full principal attribution.
  - Publish and fork are hash-pinned provenance copies, never `previous_hash` links.
  - Corrections propagate by live fan-out. Forks get `upstream_changed` detection.
  - Grants need consumer acceptance.
  - The reader's audit records `content_hash` of every served foreign entry.
- **Q3 (learning)**:
  - The reader's, one-way. The shared scope's: none.
  - Shared spaces are fixed `curated`. Reader traffic never touches the shared store (enforced by type).
  - Reader-side `foreign_usage` overlay is recorded at launch and applied in ranking after calibration.
  - No foreign ids in co-access, graph or phase state.
  - The deterministic fingerprint is extended with shared epochs.
  - An aggregated, owner-review-only feedback channel comes post-launch.
- **Q4 (1:1 boundary)**:
  - Restate it as "1 binding : 1 write-and-learning store, plus N granted read-only stores".
  - Enforce it with type-separated `ReadView` handles, non-numeric qualified ids rejected by every mutation tool, payload-narrows-never-widens, the unchanged attested project claim (a harness in P1 cannot write S) and depth-1 grants.
  - Fix the qualified-id wire format in the first V2 release.
- **H4**:
  - **CONDITIONAL GO for intra-tenant shared spaces in V2 initial. NO-GO for cross-tenant.**
  - Ship it after the residency/scheduler and isolation-gate work.
  - The qualified-id contract and the `ATTACH` ban are launch-blocking regardless.

---

## Sources (industry)

- [Elasticsearch — Reciprocal rank fusion](https://elastic.co/guide/en/elasticsearch/reference/8.x/rrf.html) (rank-based fusion for result sets with incomparable scores; used here as the fallback merge rule)
- [Elastic Search Labs — weighted RRF](https://www.elastic.co/search-labs/jp/blog/weighted-reciprocal-rank-fusion-rrf)
- [Mem0 — organizations and projects](https://docs.mem0.ai/api-reference/organizations-projects) (org/project scoping with per-project configuration)
- [Mem0 — organizational memory (private beta)](https://mem0.ai/organizational-memory) (shared org memory scoped by user/agent/team, with no default "visible to everyone" mode; consistent with S1 R8)
- [Mem0 — multi-tenant data isolation guide](https://mem0.ai/guide/ai-memory-tools-multi-tenant-data-isolation)
