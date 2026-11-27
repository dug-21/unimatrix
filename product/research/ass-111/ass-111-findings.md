# FINDINGS: ass-111 S7 — Synthesis: the V1/V2 decision

**Spike**: ass-111 S7 (campaign ass-111, GH #1050)
**Date**: 2026-11-26 (Revision 4: 2026-11-27)
**Approach**: evaluation (synthesis of S1–S6, both S2 architect reviews, and the S8 and S9 add-ons; Revision 3 applies human direction on CI, release and branching)
**Confidence**: directional. Numbers keep their tags: [M] measured, [D] derived from measured, [E] estimate, [P] published price or vendor documentation. Infrastructure cost figures come from S2 and are [E]; S8's effort figures are [D]. Revision 3's CI figures are [M] from the repo's own Actions history, [P] from GitHub and npm documentation, or [D] from both. No new research was done; the spot checks are listed below. In Revision 4, S9's latency, throughput and loss figures are [M] (its scratch benchmark, crash probes and a read-only copy of the live store), and architect review 2's platform prices are [P].
**Agent**: researcher-ass-111-s7 (uni-spike-researcher)

**Revision 4 (2026-11-27): S9 and architect review 2 integrated.** Two inputs are new: S9 (audit trustworthiness, a human-approved add-on; `ass-111-s9-findings.md`) and uni-architect review 2 (application-layer scale-out, verdict "yes, conditionally"; `ass-111-s2-architect-review-2.md`). The verdict is unchanged. Changed:
- **Model name.** "Tenant-pinned cells" becomes "project-placed, tenant-affine cells". The project is the placement unit; tenant affinity is a placement policy, not a schema constraint.
- **Hard constraints and principles.** P2 holds only after Stage 1's new gap G18. P2 is restated by audit class, P5 gains one sentence (T22), and P6 and P7 take architect review 2's amendments.
- **Gaps.** S9 extends G9, G10 and G11 and adds G18, the audit write path.
- **Constraints C16–C28** from architect review 2 are placed in Stages 1–4, 6 and 7. C16, the two-cell CI topology from Stage 2, binds KC2, α entry and D23's CI tiers.
- **Cell sizing** is added to Q1, the Decision memo and D3. The cost totals hold, because cost is linear in cores. A per-cell tenant cap would set the cell count at scale (T24).
- **Stage 0.** If D27 takes option (a), step 3's v0.12.x patch also carries the audit export-fidelity fix. Step 0 (v0.12.0) is unchanged.
- **Decisions.** D24–D27 (from S9) and D28–D30 (architect review 2's E-A, E-B and E-C) make 30. Audit retention folds into D15. D3, D6, D15, D16, D22 and D23 are amended, and the list is re-ranked with stable IDs.
- **Risks** R28–R31 are added; R2, R4, R11, R12, R15 and R21 are amended. **Tensions** T22–T24 are added; T5 and T20 are amended.
- **Also updated**: Q1–Q5, KC2, KC4, V1 defects (rows 26–31), non-human actions (with the ADR actions from both inputs), Unanswered Questions, Out-of-Scope Discoveries and the Recommendations Summary.
- **Recorded, not decided** (human direction): a tenant-scoped, read-only auditor role is a post-GA roadmap item.
- **Security.** The repo and #1050 are public. Like S9's, this file's body names defects in full; the Recommendations Summary refers only to defect rows and S9 inventory rows. Keep the body's defect specifics out of public posts and commits until the fixes land (R27).

Numbering: "architect §N" and C1–C15 refer to the first S2 architect review; "architect review 2 §N" and C16–C28 to the second. Bare C-numbers are always architect constraints. Personal-cloud capabilities are written "capability C…", and S5's constraints "S5 C…".

Spot checks for this revision:
- `default_nli_enabled()` returns `false` (`infra/config.rs`), as architect review 2 states.
- Unimatrix #5546 (KI-AUDIT) carries `delivery:proven`; #84 is the combined-audit-transaction ADR; #2574 is crt-022 ADR-003 (the contradiction scan as one rayon task, pool floor 4); #2700 is crt-023 ADR-001 (a single NLI `Mutex<Session>`, dynamic pool floor).
- S8's module map lists `store/write_ext.rs` for G9 but not `store/write.rs`, which is 427 raw lines, under the limit.

**Revision 3 (2026-11-26): CI, release and branching, from human direction.** The primary relayed the human's direction. It is applied as given and not re-argued. The scope is the roadmap only, and the verdict is unchanged. The direction:
1. Stage 0 starts by porting the existing Rust and integration CI to GitHub Actions on `main`. Eval stays local and runs before and after major splits.
2. `main` is V2. `release/v1` is a maintenance branch that accepts any V1 update. V1 tags keep V1's frozen release workflow, and `v*` becomes a V1-only pattern.
3. CI cost is managed with tiers, path filters and caching, plus a self-hosted runner if the repo goes private. Runner RAM is checked against the #878 link OOM.
4. Eval on tenant snapshots is a data-handling risk. Before β, eval uses α or synthetic data only.
5. V2 ships an npm client; this is settled. Its naming (dist-tag or rename) is a D23 sub-decision. Whether the npm client is also the OAuth client is a recommendation under D4 (the human leans yes). Client releases run on their own cadence, separate from continuous server deploys.

Recommendations this changes:
- **Stage 0 order.** It supersedes S8's and Revision 2's order (T16):
  1. CI port.
  2. Green baseline.
  3. V1 security fixes.
  4. V1 branch-point tag and `release/v1`.
  5. Size ratchet in the same CI.
  6. Module splits gated by CI.

  KC1 moves later: about 6–10 sessions now come before the first tick fix, up from 4–5 (KC4).
- **Q2 and D22.** "Freeze → retire" becomes "branch → maintain on `release/v1` → retire at α exit".
  - "Security fixes only" is withdrawn. On `main`, V1-only surfaces still get no new work.
  - A porting rule, and its cost, are added.
  - "Final V1 tag" becomes "V1 branch-point tag".
- **Q3 and Q5.**
  - The export format is fixed at the branch-point tag on both branches (T20).
  - Stage 1 reaches V1 users only by backport (T21).
  - Stage 3 gains the V2 release pipeline.
- **Decisions.** D23 is added at rank 2, making 23 decisions. D1, D2, D4, D7, D15, D21 and D22 are amended.
- **Risks.** R8 and R19 are updated, and R20–R26 are added.
- **Also updated**: the Q1 effort row, non-human actions, V1 defect dispositions, Unanswered Questions, Out-of-Scope Discoveries and the Recommendations Summary.

Spot checks for this revision:
- `.github/workflows/` (`ci.yml`, `nan-024-e2e.yml`, `release.yml`), `.cargo/config.toml` and `.claude/rules/rust-workspace.md`;
- `product/test/`: the infra-001 markers, eval-baselines and ass-098;
- Actions run history through `gh`: job minutes and the 30-day run count;
- GitHub and npm documentation: runner specs, Actions billing, required-check behavior, cache limits, npm provenance and trusted publishers;
- Unimatrix #5919 and #4578.

**Revision 3 addendum (2026-11-26): a V1 minor release first.** This is human direction, relayed by the primary. Roadmap only.
- New Stage 0 step 0, before the CI port: cut V1 minor release v0.12.0 from `main` with the existing `/uni-release` and `release.yml`, unchanged. The latest tag is v0.11.3, with 87 commits on `main` since.
- v0.12.0 ships without the two known security fixes. They follow from `main` as a v0.12.x patch before the branch cut (Stage 0 step 3), as already planned. The V1 branch-point tag (step 4) stays as planned.
- Edited:
  - the Stage 0 list and its sequencing note;
  - KC4, T16 and the decision dependencies;
  - the Q1 and Q2 recommendations;
  - D1, the decision-list intro, D22 and D23 item 7;
  - V1 defect rows 1–2;
  - the Recommendations Summary;
  - new risk R27.

**Revision 2 (2026-11-26): S8 integrated.** S8 (V1 refactor debt, a human-approved add-on; `ass-111-s8-findings.md`) is now an input. The verdict is unchanged. Changed:
- Decision memo: Why go 1, Why conditional 4 and 6, the decision dependencies, KC4 and the α entry condition.
- New tensions T15–T19: SQL-site counts, refactor order versus KC1, freeze wording, new crates versus C6, W1 versus the ratchet.
- Q1: corrected SQL counts and the effort row. Q2: freeze wording, final-tag timing and the code-level retirement list. Q3: `export.rs` stays untouched.
- Q5: a refactor track in Stage 0, ordered so KC1 is not delayed, and the pre-split rule for Stages 1–6.
- Risks: R8 updated, R19 added.
- Decisions for human: D20–D22 added; D2 and D7 amended; the list re-ranked with IDs kept stable (22 items).
- Non-human actions, V1 defects (rows 24–25), Unanswered Questions, Out-of-Scope Discoveries and the Recommendations Summary.

Unimatrix entries consulted: goals #5690, #5681, #5682, #5684, #5689, #5732, #5734, #5836 (full text); the capability index (85 active entries, title and delivery status); #5714 BACKUP-RESTORE, #5705 PL-4, #5595 NX-DURABILITY, #5582 C17 (full text); the personal-cloud capability titles (topic lookup). The briefing also surfaced #5528, #5542, #5544, #5551, #5552, #5556, #5558, #5563, #5566, #5569, #5580, #5587, #379, #820, #100, #760, #4895 and #4180 (summaries only). Revision 3 added #5919 (the current version of #5790, the JS/TS CI procedure) and #4578 (the tag-triggered release pattern), full text. Revision 4 added #5546, #2574, #2700 and #84 (titles and tags, as spot checks). No Unimatrix writes.

**Spot checks.** Each was run to settle a tension, not to research:
- #5714: import covers all tables, runs `chain_verify`, and refuses a target that already has audit history.
- #5705: the current PL-4 `done_when` wording.
- #5595: NX-DURABILITY covers shedding under load, not loss on box failure.
- #5582 and the personal-cloud capability titles: which capabilities carry forward, are restated, or retire (Q4).
- `.claude/protocols/uni/uni-primary-authority.md`: the authority classes used in "Decisions for human".
- Quoted figures and wording were re-checked against the S1–S6 files.

---

## Decision memo

**Verdict: CONDITIONAL GO.** Build V2 as one hosted multi-tenant service on S2's Option A: a per-project SQLite + HNSW data plane, a SQLite control plane, and project-placed, tenant-affine cells behind a router (architect review 2). Deliver it as three gated releases (α, β, GA), built by roadmap Stages 0–7 (Q5). Opening the service to external tenants (GA) is a separate decision, taken later.

| Release | Who is on it | Enter when |
|---|---|---|
| **α (dogfood)** | The human's own projects, in one personal tenant | Decisions D1–D4 and D24 are made. Stage 0 needs D23, D20–D22 and D27 first; Stage 1 also needs D6 and D30; Stage 2 needs D5–D7; the first hosted deploy needs D28 with D16. Stages 1 and 2 are complete: Stage 1's measurements meet kill criterion KC1, and Stage 2's cross-tenant isolation gate passes across two cells (KC2, C16). Stage 3's identity, onboarding and migration capabilities are built. α is Stage 3's soak; it exits when every project is migrated and V1 is retired. |
| **β (second tenant)** | Med Search, as its own tenant: K4 ingest, K5 public chat, deterministic mode, retraction | α has exited and V1 is retired. Stage 4 is complete: the fair-share bound and the cold-start bound are both measured. Decisions D8–D11 and D25 are made. |
| **GA (external tenants)** | Self-serve signup | Stage 7 is passed: D3 (the HA part), D15, D16, D26 and D29 are decided; tenant export and purge have shipped; operability is in place; post-fix cost has been re-measured on two real corpora; a security review has passed. |

**Why go**
1. **The design holds every Hard constraint and extends existing seams instead of replacing them** (S1 Q2; S2 Q1; architect §1, APPROVE WITH CONDITIONS).
   - Per-project SQLite keeps isolation physical and per-tenant restore proven.
   - The storage decision can still be reversed (S2 reversibility).
   - The data plane, the per-project work units and the per-project learning state already shard by project (architect review 2 §1).
   - The V2 tenancy seams are already small files within the size limit. The size debt sits in the V1 core they call into. Paying it down costs ≈28–31 split sessions [D], against ≈60+ feature-sized deliveries for a rewrite [D] (S8; D20).
2. **Infrastructure cost is small.** Background ML dominates it, and that is fixable; storage barely moves it.
   - After tick fixes 1–3 plus change-driven gating, 1,000 tenants need about 1–2 cells: €235–470/mo plus at most $193/mo of backup [E].
   - Launch fits on one cell at €73–235/mo [E] (S2 Q1).
   - Architect review 2 sizes the cells: one 16 vCPU/64 GB cell (€235/mo [P]) while NLI runs in the cell, then standard 8 vCPU/32 GB cells (€118/mo [P]), added rather than grown. Cost is linear in cores, so these totals hold. A per-cell tenant cap (D29) could set the cell count at scale instead of CPU (T24).
3. **Launch operations stay small.** They are today's daemon plus a backup sidecar, one object-store bucket and metrics. There is no Kubernetes and no external database (S2 Q3). Architect review 2 also rejects Kubernetes for V2 and recommends self-managed Hetzner VMs (D28).
4. **Most of V2's value for the two validating tenants does not depend on tenancy.** These items would be built for the human's projects and Med Search on V1's multi-project server anyway:
   - cost fixes and vector durability;
   - privilege fixes;
   - same-transaction audit (G18);
   - learning isolation per client class;
   - retraction and deterministic mode;
   - T2 agent identity;
   - intra-tenant sharing.

   The SaaS-specific bet is limited to three things: the control plane, OAuth onboarding and the obligations that come with GA. That bet is bounded, and it can be reversed.

**Why conditional**
1. The cost claim is [E]. Post-fix tick cost has never been measured (S2 U3).
2. Until entry vectors are stored in SQLite, failover RTO is "restore + re-embed the whole fleet". An RPO of up to 60 s loses writes that were already acknowledged (architect C1 and E4 addendum).
3. Four gaps must be closed before a second tenant exists. In a multi-user tenant, each turns a V1 nuisance into a privilege-escalation or attribution defect (S1 Q2; S9):
   - G6: Admin is granted from a self-asserted `agent_id`.
   - G8: the legacy token reaches every project.
   - G9: audit rows carry no authenticated principal.
   - G18: audit rows are written outside the state-change transaction and can be lost, so P2 does not hold until it closes.
4. S8 sized only the refactor-debt part of the effort: ≈28–31 split sessions [D], from one rate sample. Feature effort for Stages 1–3 is unsized, and no spike priced the hosted authorization server (AS).
5. The erasure and legal posture for other people's data is undecided (S2 E3).
6. The V1 core that V2 extends is concentrated in 37 over-limit production files (48.8k lines, 50.5% of production code), and no ratchet holds them. Six of these files must be split before Stage 1 changes them (S8; T16).
7. Seven components are process-global or assume one process today: the in-process composition of router, cell and control plane; one inference pool with unbounded ORT threads; the in-cell NLI arena; host-local fencing only; fleet jobs with no home; per-principal quotas; and co-located shared scopes. Shipped that way, the first second cell is a re-architecture. Each is cheap to fix while there is one cell (C16–C27), and C16 keeps the fixes from rotting (architect review 2).

**Dependencies on human decisions.** Stage 0 opens with the v0.12.0 release (step 0), which needs no decision. Its CI port (step 1) needs D23 (CI, release and branching), and its v0.12.x patch and V1 cut (steps 3–4) need D27 (V1 export fidelity). Its refactor track then needs D20 (refactor, not rewrite), D21 (limit policy) and D22 (V1 maintenance wording). Stage 1 needs D24 (audit classes and failure semantics), D6 (ADR reversals) and D30 (NLI worker timing). α needs D2 (hypothesis H1, local deployment), D3 (durability posture) and D4 (hosted AS), and its first hosted deploy needs D28 (hosting platform) with D16 (region). GA also needs D15 (data governance, including audit retention), D26 (audit anchor strength) and D29 (per-cell blast-radius cap).

**Kill and re-plan criteria**
- **KC1 (cost).** After tick fixes 1–3, measure on the heavy corpus plus one real light corpus. If one 16-core cell cannot keep a 900 s tick interval for 100 mixed projects, stop before GA and re-scope background ML per tenant class.
- **KC2 (isolation).** If the bidirectional isolation gate cannot cover the hook-path learning sinks and audit-derived learning (LI-2, LI-3), or cannot pass across two cells with a project moved between them (C16), do not admit a second tenant.
- **KC3 (backup).** If the Litestream checkpoint PoC fails (C12) and the binary-only snapshot RPO is worse than what D3 accepts, run a libSQL/Turso spike before β.
- **KC4 (effort).** Re-confirm GO after the V2 design session has sized Stages 1–3.
  - S8 sized the refactor-debt component: ≈15–18 split sessions [D] for its Stage 0 set (W0 ratchet, W1 test extraction, W2 six prerequisite modules), ≈13 more inside Stages 1–6 [D], ≈28–31 in total (range 15–45).
  - Stage 0 now opens with the v0.12.0 release, then the CI port and a green baseline (Revision 3 and its addendum). No spike sized them. This synthesis puts them at ≈2–5 sessions [E], with the unknown flaky-test count driving the range. About 6–10 sessions therefore come before the first tick-fix delivery: these plus S8's 4–5 [D]. The v0.12.0 release (step 0), the two V1 security bugfix sessions and, if D27 takes option (a), one export-fidelity session [E] also run first.
  - Ports between `release/v1` and `main` are outside this sizing. Count them per port (D22).
  - The refactor rate rests on one sample. Re-derive it from the first two W1/W2 sessions before re-confirming GO.
  - Feature effort is still unsized, and Revision 4 adds to it: G18 (S9) and C16–C27 (architect review 2). The V2 design session sizes them with the rest of Stages 1–3.

---

## Campaign verdicts

### Hard constraints

| Constraint | Status under the synthesized plan | Basis |
|---|---|---|
| P1: hash chain | Holds. Each store owns its chains. Migration carries hashes unchanged. Publish, fork and migration lineage is recorded as provenance metadata, never as `previous_hash` | S2 Q1; S3 SS-W3–W5; Q3 below |
| P2: append-only audit | Holds **only after Stage 1 G18** (S9). V1 audit is written outside the state-change transaction, with errors swallowed or only logged. Twelve kinds of state change or security event write no row, and the live store's audit log has a six-month hole. Under G18, class-A audit is written in the same transaction and fails closed, on a per-store hash chain (D24). V1 rows are kept unchanged, with the columns the V1 export carries (D27). V2 attribution columns are additive. The control plane gets its own audit log (G11) | S1 G9/G11; architect §1; S9 Q1–Q4 |
| P3: capability check after identity | Holds, and is strengthened. Authorization comes from the principal's role, not from a self-asserted `agent_id` (G6). A cell trusts only an authenticated router assertion (`RoutedRequest`, C17) | S1 Q3/Q4; architect C9; architect review 2 C17 |
| P8: no secrets in any DB | Holds under condition C7. The control plane stores token hashes and public keys only | architect C7; S1 Q4 |
| Tenant isolation, including learning; sharing only through explicit, enumerated grants, never with every client | Holds. Projects are physical stores. Learning flows one way, into the reader's store only. Grants are enumerated, at most 3 per consumer, depth 1. K4/K5 clients do not receive shared content by default | S1 R8; S2; S3 SS-*; S4 LI-8 |
| P5: graceful degradation | Holds. A cold start serves degraded results. A slow or failed shared store falls back to home-only results. An absent NLI worker falls back to the previous behaviour. In deterministic mode, optional ML is switched off visibly rather than degraded. Audit is the exception by design: a failed class-A audit insert fails the write (D24, T22) | S2 P7; S3 SS-R8; S4; S9 |
| Knowledge engine, not orchestrator | Holds. No session memory, no hosting of harness definitions, no webhooks. CI calls Unimatrix; Unimatrix never calls CI | S4 Q1; S5 |

### Hypotheses H1–H6

| # | Verdict | Rationale | Source |
|---|---|---|---|
| H1 | **Modified** | SaaS is the only supported product. The same binary, run locally as an HTTP daemon with one project ("local N=1"), keeps working but is unsupported. It serves as the exit, migration, air-gap and dev path. The UDS/stdio server path is removed or kept dev-only, and is never a second identity path | S2 H1; S1 G16; architect C5 |
| H2 | **Partly accepted** | HTTPS is the only server transport. The Node stdio→HTTPS bridge stays as a client adapter. For humans, OAuth is the primary path, not an addition. Scoped tokens are for non-human principals. CI uses OIDC token exchange | S1 Q3 |
| H3 | **Rejected as framed** | Claude Code reaches the same attestation tier (T2) as OpenCode, so giving up harness ubiquity buys no identity. Use one Unimatrix-owned attestation contract, with OpenCode as a pinned, CI-verified first-class adapter | S6; S1 Q4 |
| H4 | **Conditional go, narrowed** | Shared spaces within one tenant ship in V2 initial. Cross-tenant sharing is deferred | S3 |
| H5 | **Accepted, with restatement** | P6 and P7 are restated in the architect's text, with architect review 2's amendments: roles communicate only through published interfaces, and a project is resident on one cell at a time. The restatement holds only with embedded storage | S2; architect §5; architect review 2 §5 |
| H6 | **Rejected, with two small go items** | No transient session-memory class and no hosting of harness definitions. Two small items go ahead: cycle-scoped `proposed` knowledge, and references to harness artifacts | S5 |

---

## Cross-spike tensions resolved

**T1. Identity vocabulary.**
- *Tension*: S6's headline calls per-call attestation "non-forgeable … not unique to OpenCode" and lists T0 as a tier. S1 (final) says "attested, not non-forgeable", with T0 as an annotation only.
- *Resolution*: Use S1's vocabulary.
  - T1 is the floor for authorization. T0 is only an annotation on the call.
  - Claims say "harness-attested (tiered)".
  - "Non-forgeable" is reserved for a validated T3.
- *Basis*: S1 Q4 (reconciled with S6); S6 headline and limits L1–L3.

**T2. Invalid attestation.**
- *Tension*: S6 downgrades an invalid attestation. S1 rejects it.
- *Resolution*: The server rejects an invalid attestation (S1). S6's fail-open adapter rule is extended to cover the rejection:
  - On an invalid-attestation rejection, the adapter retries once without attestation (T1) and warns.
  - Detection survives, because the rejection writes an audit row.
  - A harness upgrade that breaks the args hash can no longer lock the harness out. OpenCode ships about 5 v2 tags a week (S6 Q3).
  - The rejection must be an MCP-level error, not HTTP 401. Under S1's own spec obligations, a 401 tells the client to start OAuth (re-)authorization.
- *Basis*: S1 Q3 (spec obligations) and Q4 (enforcement table); S6 "What S1 can safely conclude".

**T3. RTO and cost.**
- *Tension*: S2's original RTO and cost claims versus architect conditions C1 and C10.
- *Resolution*:
  - Until vectors are stored in SQLite, RTO is stated as "restore + re-embed the whole fleet". After that it is "restore + rebuild without inference".
  - Cost is computed with the re-ranked fixes. Gating the contradiction scan comes first, because its output is status-only.
  - Every cost figure stays [E] until measured.
- *Basis*: architect C1 and C10; S2 revision.

**T4. Retraction authority.**
- *Tension*: S4 M3 says the K4 ingest pipeline must be able to retract. S1 says Admin is held only by human Owners.
- *Resolution*: Admin and quarantine stay human-only. Retraction is treated as a knowledge-lifecycle act, not a security act:
  - The Owner sets the PCD (per-project configuration document) policy `deprecated_visibility: exclude`.
  - Any Writer can then deprecate an entry with a structured `retracted` reason (S4 E3, option a).
  - The authority to set the policy stays human; using it needs only Write. Decision D9.
- *Basis*: S1 role model; S4 M3/E3; S5 C6.

**T5. Configuration hot-apply cost.**
- *Tension*: S5 wants per-project config hot-apply. S2 owns the cost of rebuilding the hot path.
- *Resolution*: A PCD apply is an admin event.
  - The active config is swapped atomically in memory (S5 C3).
  - The project is marked dirty, and the change-driven scheduler handles it like a write (S2 Q2).
  - Cost is O(applies), not O(tenants × ticks). A heavy graph rebuild takes 0.26–0.42 s [M4].
  - Each key is classed as swap-only, rebuild, or re-embed.
  - The model catalog is a cell attribute, and the model is not a PCD key. Each project's vectors record `model_id` and `dim`, and a model upgrade is a per-project epoch (C24).
  - Measure the cost at delivery.
- *Basis*: S2 Q2; S5 Q4.

**T6. Config-as-code from CI.**
- *Tension*: S5 C9 lets CI push config-as-code. S1 gives service and workload principals no control-plane rights.
- *Resolution*: CI may upload a staged PCD version; a human Owner applies it. The control plane stays human-Owner-only. Decision D18.
- *Basis*: S1 role table; S5 C6/C9.

**T7. Local mode under H1.**
- *Tension*: S1 says "retire UDS/stdio". S2 says "keep local N=1 working". Architect C5 corrects S2's single-code-path claim.
- *Resolution*: The three positions are compatible.
  - The product is SaaS.
  - Local N=1 means the same binary running as an HTTP daemon with one project. It is unsupported.
  - The UDS/stdio server path is removed or kept dev-only.
  - In local N=1, Admin operations go through an operator CLI on the host, the local counterpart of S1's out-of-band operator plane. No token ever carries Admin; S1 grants Admin only through a human Owner role. Decision D2.
- *Basis*: S1 G8/G16; S2 H1; architect C5.

**T8. Definition pinning for T3.**
- *Tension*: T3 needs pinned agent definitions (S1 E3, S6 E3). S5 says Unimatrix must not host harness definitions.
- *Resolution*: Pinning is not distribution.
  - The server holds a policy table mapping `agent_type` to the allowed definition digests (G17). The digests come from released artifacts, such as the per-file sha256 in `.collective`.
  - The harness's own channels distribute the files.
  - The H6 no-go stays intact. Decision D13.
- *Basis*: S1 G17; S5 Q3; S6 P8.

**T9. Curated shared spaces before the deterministic-mode decision.**
- *Tension*: S3's curated shared spaces rely on parts of S4's deterministic mode, which is still pending.
- *Resolution*: `learning_mode` is needed either way.
  - Curated mode is S4 requirements D1, D2, D3 and D5.
  - Deterministic mode adds D6, D7, D9 and D10 on top. Decision D8.
- *Basis*: S3 SS-L3; S4 Q3.

**T10. Phase conditioning in deterministic projects.**
- *Tension*: S5 W4 has Med Search ingest cycles getting phase-conditioned retrieval. S4 D8 forces workflow terms to 0 in deterministic mode.
- *Resolution*: In a deterministic project, a declared cycle gives attribution and cycle review, but no phase-conditioned ranking.
- *Basis*: S4 D8; S5 W4.

**T11. Two uses of `Status::Proposed`.**
- *Tension*: S5 uses it for cycle-scoped provisional knowledge. S3 uses it for post-launch cross-tenant proposals.
- *Resolution*: V2 initial uses it only for S5's case. If S3's proposal path is built later, add a field that tells the two apart.
- *Basis*: S3 F9/SS-W6; S5 Q1.

**T12. Portable wiring.**
- *Tension*: S1 Q5 asks for portable harness wiring with no absolute paths. The human's direction from the nan-024 design (2026-11-25) is that portability is not an imperative, and that OAuth makes a per-client install mandatory.
- *Resolution*: Keep "no secrets in wiring" (P8, N2). Drop portability as a V2 requirement.
- *Basis*: S1 Q5; human direction.

**T13. Med Search's tenant.**
- *Tension*: SCOPE calls Med Search the "second validating tenant". S3 notes it needs no cross-tenant sharing. S1 E2 leaves its tenant shape open.
- *Resolution*: Onboard Med Search's research corpus and public chat as a separate tenant.
  - The isolation gate then gets a real second tenant before any external one.
  - The K5 public surface is contained to that tenant.
  - Med Search's code repo can sit in the personal tenant, so it can read shared dev conventions within that tenant. Decision D10.
- *Basis*: S1 Q2 (first proof); S3 H4; S4 K5.

**T14. Tenants versus projects in the cost model.**
- *Tension*: S2's cost model counts one tenant as one project.
- *Resolution*: Background cost scales with active projects. A tenant with n projects counts n times; the change-driven scheduler makes idle projects free.
- *Basis*: S2 Q1/Q2.

**T15. SQL-site counts.** (Revision 2)
- *Tension*: S2 and architect C6 cite about 800 SQL sites (297 server, 499 store crate) as a reason to reject Option B. S8 measured production code only: 100 server sites in 21 files and 413 in the store crate. S8 says the C6 lock-in and the A→B reversal cost are about a third of what S2 stated.
- *Resolution*: Adopt S8's counts, with one correction to its ratio.
  - The server SQL that C6 must move into the store crate is about a third of S2's figure (100 vs 297).
  - The production SQL an A→B port would touch is about 513, roughly two thirds of S2's ~800, because the store crate's 413 sites still port.
  - Option A stands. S2's other grounds for rejecting B do not depend on the count: RLS, pgvector post-filtering, and P7 needing tenant-affine caches anyway. A 513-site port is still large.
  - C6 gets cheaper. The architect carries the corrected counts into the C6 ADR.
- *Basis*: S2 Q1; architect C6; S8 measurement basis and out-of-scope discoveries.

**T16. Refactor order versus the first proof.** (Revision 2)
- *Tension*: S8 puts waves W0–W2 (≈15–18 sessions) in Stage 0, before any Stage 1 code lands. S7's first prove-first item is the post-fix tick cost (KC1), which needs Stage 1's tick fixes. Taken literally, S8's order delays the go decision's largest uncertainty by ≈15–18 sessions.
- *Resolution*: Keep S8's rule, applied per module: each of the six prerequisite modules is split before the first Stage 1 change to it. Order the work by Stage 1's needs:
  1. W0 (ratchet), then W1a (inline tests moved out of line).
  2. Split `background.rs`, and pre-split `services/status.rs` and `services/nli_detection_tick.rs`, so the tick fixes and the KC1 measurement land first. That puts about 4–5 sessions [D, at S8's rate] before the first tick-fix delivery, instead of ≈15–18.
  3. Then `store/migration.rs` and `store/db.rs` (C1, the G9 columns), `mcp/tools.rs` and `server.rs` (G9), and `uds/listener.rs` (LI-2).
  4. W1b (test files over 1,000 lines, split by concern) leaves the critical path.

  Total effort is unchanged; only the order moves.
- *Revision 3*: Human direction puts five steps ahead of W0: the v0.12.0 release (the addendum), the CI port, a green baseline, the V1 security fixes (as a v0.12.x patch) and the V1 branch cut. The per-module order above is unchanged, and CI now gates every PR in it. The first tick-fix delivery moves back by the CI work (≈2–5 sessions [E]), so about 6–10 sessions now come before it.
- *Basis*: S8 Q1 (module map) and Q2 (waves, rates); S7 prove-first order; S2 U3.

**T17. Freeze wording.** (Revision 2)
- *Tension*: S8 Q3 says S7's "freeze V1, security fixes only", read literally, would block the refactor waves.
- *Resolution*: No conflict in substance. S7 Q2 already scoped the freeze to V1-only surfaces ("V1 is not a separate code line"). S8's wording makes that explicit and adds three rules:
  - when a frozen file's size baseline may rise;
  - a retire list that is never refactored;
  - when the final V1 tag is cut.

  It becomes D22's recommendation, and D2 is restated to match. The human decides D2 and D22 together.
- *Revision 3*: Human direction replaces the freeze with maintenance on `release/v1`, where any V1 update is allowed. On `main`, V1-only surfaces still get no new work and are deleted at α exit. D22 is reworded to match, and T20 covers the export format.
- *Basis*: S7 Q2; S8 Q3.

**T18. New crates versus C6.** (Revision 2)
- *Tension*: S8 recommends new V2 subsystems as new crates, the control plane included ("inside or beside the store crate"). Architect C6 bans raw SQL outside the store crate.
- *Resolution*: If the control plane gets its own crate, word C6 per storage crate. SQL lives only in storage crates — the store crate, plus a control-plane storage crate behind the `ControlPlaneStore` trait — and never in the server crate. This is the architect's call in the C6 ADR, not a human decision.
- *Basis*: S8 Q2 (D7); architect C6; S2 Q1 (`ControlPlaneStore` trait).

**T19. W1 versus the ratchet.** (Revision 2)
- *Tension*: S8 lands the ratchet (W0) before W1. W1 moves inline tests into new sibling test files, many over the 1,000-line test limit (`mcp/tools.rs` alone carries 7,663 test lines). Under the ratchet's rule (e), each new baseline entry would need a human-approved raise.
- *Resolution*: D21 pre-approves, under its own issue, the baseline entries W1 creates for extracted test files. Each entry is capped at the test lines removed from its parent, so total excess does not grow and W1 needs no per-file decision. W1b then burns these entries down.
- *Basis*: S8 Q2 (W0 before W1) and Q4 (ratchet rules a and e).

**T20. V1 maintenance versus the migration contract.** (Revision 3)
- *Tension*: Human direction allows any V1 update on `release/v1`. Q3's importer accepts one V1 export format, which Revision 2 froze at the final V1 tag.
- *Resolution*: The V1 branch-point tag fixes the export format on both branches.
  - A V1 update on `release/v1` that changes the schema or the export format must also extend the V2 importer and its verification. It is priced as a port (D22).
  - The importer targets the newest V1 release in use at migration time.
- *Revision 4 (D27)*: The V1 export carries 8 of 12 audit columns (S9). If D27 takes option (a), the fix lands on `main` in Stage 0 step 3, before the branch-point tag, so the tag fixes a 12-column format and this rule is unchanged. Under (b), the tag fixes the 8-column format, and every migration receipt declares the loss (R30).
- *Basis*: human direction (Revision 3); Q3; #5714; S8 Q3; S9 inventory rows 26–27.

**T21. Stage 1's value to V1 users versus `main` = V2.** (Revision 3)
- *Tension*: Revision 2 had Stage 1 shipping to V1 users from `main`. With `main` as V2, and V1 tags only on `release/v1`, nothing on `main` reaches V1 users without a port.
- *Resolution*: Stage 1 lands on `main`.
  - An item reaches V1 users only by backport (D22). Schema-changing items, such as C1's stored vectors, also fall under T20.
  - Why go 4 still holds, because the same items reach the human at α.
  - A no-go returns `main` to V1 releases (D23), so Stage 1 work is not lost.
- *Basis*: human direction (Revision 3); Q5 Stage 1; the Decision memo's Why go 4.

**T22. Graceful degradation versus fail-closed audit.** (Revision 4)
- *Tension*: P5 says degradation is graceful. S9 makes a failed class-A audit insert fail the write (D24).
- *Resolution*: P5 governs ML and serving. Audit is an integrity guarantee under P2.
  - Class A fails closed, so an audit defect shows as a write outage, not as silent loss (R28). V1's six-month silent outage is the case against degrading.
  - Reads keep working, because their audit is class C, which degrades with loss accounting.
  - P5's draft text gains one sentence: integrity guarantees are not degraded.
- *Basis*: S9 Q1 (live evidence) and Q2 (failure semantics); P5.

**T23. Audit chain genesis versus the import contract.** (Revision 4)
- *Tension*: S9 writes a genesis row when a V2 project is created, and C19 writes a `project_opened` row on every open. The importer refuses a target that already has audit history (#5714, C-5), and S9's monotonic-insert trigger rejects an `event_id` at or below the table's maximum. Either row, written before the import, blocks it.
- *Resolution*: In a migrated project, the import comes first.
  - The importer runs in the project-creation saga, before the first open. The V1 rows land first, unchained and keeping their ids.
  - The `v1_import_digest` row, written in the same transaction, is the chain's genesis. A project created empty writes its genesis row on creation.
  - C-5 and the trigger stay as they are. The Stage 3 design confirms the mechanics.
- *Basis*: S9 Q2 (tamper evidence) and Q3 (C19); #5714 (C-5); Q3 below.

**T24. Cell count: CPU versus blast radius.** (Revision 4)
- *Tension*: S2's cost model puts 1,000 post-fix tenants on 1–2 large cells. Architect review 2 caps cells for blast radius (C28) and proposes about 100 tenants per cell, which needs at least 10 cells at that scale.
- *Resolution*: Both hold; they answer different questions.
  - Cost is linear in cores [P], so the cell shape does not change the CPU cost.
  - Under a 100-tenant cap, 1,000 tenants need at least 10 cells: about €1,180/mo at the standard 8 vCPU/32 GB class, or about €735/mo on 4-vCPU cells if their load allows [E]. By CPU alone, S2 puts it at €235–470/mo.
  - The cap, not CPU, then sets the cell count. D29 sets the number; a revenue-share cap or a higher number lowers the count.
- *Basis*: S2 Q1; architect review 2 §3 and C28.

---

## Findings

### Q: "Is single multi-tenant SaaS feasible for a solo operator at acceptable cost and risk? Go / no-go / conditional."

**Answer**: **Conditional go.** The service is feasible on architecture, isolation, infrastructure cost, operations, security and agent identity. Engineering effort and legal posture are unproven. Staging and kill criteria are in the Decision memo.

| Dimension | Verdict | Evidence |
|---|---|---|
| Architecture | Feasible | Option A meets isolation, the hash-chain and audit guarantees, and per-tenant restore, without the port of about 513 production SQL sites that Option B would need (S8's count; S2's ~800 included tests; T15). The `resolve_store` contract extends through an additive `ProjectKey::Tenant` variant. What breaks today — the static project map, flat slugs, `tokens.json`, and authorization on a self-asserted string — is replaced by a new control plane and a rewritten resolver behind the same funnel (S1 Q2 G1–G17; S2 Q1; architect §1, §3) |
| Scale-out (application layer) | Feasible, conditionally | The data plane, per-project work units and learning state already shard by project. Seven process-global components need boundaries while there is one cell (C16–C27), and CI runs two cells from Stage 2 (C16). Query-path embedding stays in the cell; per-project read replicas and intra-project sharding are rejected. Cells are added on a capacity or blast-radius trigger, not by redesign (architect review 2 §1–§3) |
| Isolation | Feasible, by construction | All learned state is per project, and there is no global learned state today (architect §1; S4 ground truth). A shared scope is a separate store read by fan-out, through type-separated `ReadView` handles, with one-way learning (S3) |
| Infrastructure cost | Low, unproven [E] | See the table below. The storage choice moves cost by tens of dollars a month; background ML moves it by thousands (S2 Q1) |
| Operations (solo operator) | Feasible on one cell | Router, cell and control plane run in one process, with Litestream → R2 backup, metrics and tick-liveness alerts (S2 Q3). They are composed through explicit boundaries, so a second cell is a deployment change (C17, C18). A single box failure is a full outage for every tenant on it; D29 caps how many. The recommended platform is self-managed Hetzner VMs; Fly Machines cost about 3× the compute (D28) |
| Durability | Acceptable only if D3 accepts it | RPO is at most the 60 s sync interval; on box failure the chain is truncated, not broken. Until vectors are in SQLite, RTO is restore plus re-embedding the fleet (about 85 s per heavy project at ~15 cores [M4 analogue]). After that, RTO is restore plus a rebuild without inference (at least 0.7 s per project [D, lower bound]). V1 is backed up by volume snapshot today (#5836), so this is not a regression. On Hetzner, a reattachable network volume could give a host-failure path with RPO ≈ 0, pending a WAL-latency PoC (architect review 2 §4; D28) |
| Security | Feasible once G6, G8, G9 and G18 are closed | Today, any caller on the legacy token or on stdio who passes `agent_id:"human"` gets Admin (S1 ground truth). Scoped tokens are safe. In V2, Admin must come from a human Owner's role. V1 audit can lose rows, and the live store's audit log has a six-month hole; G18 writes class-A audit in the same transaction and fails closed (S9) |
| Agent identity | T2 now, T3 later | Per-call attestation is validated on Claude Code 2.1.332 and OpenCode 2.4.1. An LLM with file-write or shell access can still forge it (limits L1–L3). Codex stays at T1 (S6; S1 Q4) |
| Strategic dependencies | Acceptable | The hosted AS (unpriced; S1 E1); Hetzner (or Fly, D28) and R2 [P]; harness APIs (OpenCode broke its plugin API once already, #998); GitHub-hosted runners, free only while the repo is public, and npm provenance, generated only from a public repo (D23b) |
| Engineering effort | **Partly sized** | Refactor debt: ≈28–31 split sessions [D], ≈15–18 of them in Stage 0, from one rate sample (S8). The CI port and green baseline add ≈2–5 sessions [E]; ports between `release/v1` and `main` are unsized (Revision 3). A rewrite would be ≈60+ feature-sized deliveries [D]. Feature work is unsized: S1 classes each gap only by size (handler, schema+handler, architectural), and G2 and G6 are architectural. Revision 4 adds G18 and C16–C27 to Stages 1–3, also unsized |
| Legal and erasure | **Undecided** | Per-record erasure conflicts with P1 and P2. The posture needs a human decision, and possibly legal input (S2 Q4, E3) |

Cost (S2 Q1; every cell is [E]):

| Tenants | Current algorithms | Fix 1 (gate the contradiction scan) | Fixes 1–3 + change-driven gating | Backup (R2) |
|---|---|---|---|---|
| 10 | 1 × CCX43, €235 | 1 × CCX23, €73 | 1 × CCX23, €73 | ≈ $0 |
| 100 | The tick misses its interval; 2 cells ≈ €470 | 1 × CCX43, €235 | 1 × CCX33–43, €118–235 | ≈ $0–4 |
| 1,000 | ~13 cells ≈ €3,050 | ~7 cells ≈ €1,650 | 1–2 × CCX43, €235–470 | $35–193 |

The hosted AS is not priced anywhere in the campaign.

Cell sizing (architect review 2 §3; prices [P]): one 16 vCPU/64 GB cell (CCX43 class, €235) while NLI runs in the cell. Once Stage 1 has bounded NLI, pinned ORT threads and gated the scan, standard 8 vCPU/32 GB cells (CCX33 class, €118), added rather than grown. Cost is linear in cores, so the totals above hold. On Fly the compute costs about 3× [D, from [P] prices] (D28). A per-cell tenant cap can set the cell count instead of CPU (D29, T24).

**Evidence**: S1 Q2–Q4; S2 Q1–Q4 and its measured results M1–M13; architect §1–§6; S3 H4; S4 ground truth; S6 verdict; S8 measurement basis and Q2; architect review 2 §1–§4; S9 Q1–Q3.

**Recommendation**: Approve the conditional go with α → β → GA staging (D1). Start Stage 0 with the v0.12.0 release and the CI port (D23), then Stage 1 behind the refactor splits it needs (T16), because Stage 1:
- runs on today's product;
- retires the two largest uncertainties: the unmeasured post-fix cost and vector durability;
- is worth doing even if the answer later becomes no-go.

Treat GA as its own decision, taken after β.

---

### Q: "V1 disposition: freeze, retire, or maintain — and what that costs."

**Answer**: **Branch V1 at a V1 branch-point tag, maintain it on `release/v1` until α exit, then retire it. `main` is V2.** This is Revision 3, from human direction; it replaces Revision 2's "freeze now, no parallel maintenance". V2 is the same binary and codebase, evolving on `main` (Option A; restated P6). So "V1" is a maintenance branch plus a set of V1-only product surfaces and a support promise:
- the local UDS/stdio server path (vnc-038 ADR-006);
- the legacy all-slug token;
- `tokens.json`;
- operator `project register` followed by a restart;
- `client-bundle` and certificate fingerprint pinning;
- the self-host positioning.

| Option | What it means | Cost | Verdict |
|---|---|---|---|
| Maintain on `main` | V1 self-host stays a supported product on the V2 line | Two support matrices on one line. The UDS/stdio path stays a second identity path into the product, which S1 G16 forbids. ADR-006 stays in force. Every V2 change must be regression-tested on V1 modes, for one known user | Reject |
| Retire now | Remove the V1 surfaces before V2 serves anyone | The human has no working product while Stages 1–3 are built | Reject |
| Freeze → retire (Revision 2) | V1-only surfaces get security fixes only, on `main`, and are removed at α exit | Withdrawn: the human allows any V1 update | Superseded |
| **Branch → maintain → retire** | V1 ships from `release/v1`, cut at the branch-point tag, and any V1 update is allowed there. On `main`, V1-only surfaces get no new work. They keep working, because Stages 1–3 are built and tested through them, and are deleted at α exit. Shared code is V2 work on `main` (D22) | See the list below | **Recommend** (human direction) |

**What branch → maintain → retire costs**
- **Engineering**
  - (a) Deletions at retirement: G8 (handler-only; `legacy_token_enabled=false` already exists), G16 (handler-only), `tokens.json` (moves to the control plane, G5), and the bundle and pinning flows.
    - The code-level retire list is deleted with its size-baseline entries and is never refactored (S8): `uds/hook.rs` (the deprecated Rust hook client, kept only as the parity oracle; #4751), the UDS transport (`uds/mcp_listener.rs` and `handle_connection` in `uds/listener.rs`), and `tokio_main_stdio` (444 lines) in `main.rs`.
    - `uds/` cannot be deleted wholesale. `dispatch_request` also serves HTTP `/observe`, and it leaves `uds/` in Stage 0 (S8).
  - (b) A migration importer that accepts the V1 export format fixed at the branch-point tag (T20), built on BACKUP-RESTORE, which is already proven (#5714).
  - (c) On `main`, the V1 surfaces keep working until α exit. That costs roughly the existing tests (S2 H1).
  - (d) **Porting.** A V1 update to shared code is made on both branches, or deliberately skipped on one. Cherry-picks apply until W1/W2 move the code. After that, each port is a manual re-application with its own PR and CI run. Ports are outside KC4 (D22).
  - (e) **Two release lines.** V1's release workflow and V2's pipelines share npm, GHCR and GitHub Releases. Their `latest` pointers need one change before the cut (D23).
  - (f) S8's refactor debt is not a V1 cost. Shared code is V2 work: about 15–18 Stage 0 sessions [D].
- **Product**
  - The self-host promise ends for OSS users at α exit; until then they keep receiving V1 updates. Their number is unknown; the campaign framing says there is effectively one.
  - Personal-cloud capabilities C7, C9, C16 and C17 retire, and capabilities C4 and C0 lose their local half (Q4). These are capability IDs, not architect constraints.
  - Mitigation: a deprecation notice and a migration guide; the last V1 artifacts stay downloadable; the local N=1 byproduct stays as the exit path.
- **Accepted gaps**: On `main`, V1-only-surface defects that are not security issues stay unfixed until deletion. This includes the retire list's size violations. `release/v1` may fix them. Accepting them on `main` is an accepted-gap item (authority class H4), bundled into D2 and D22.
- **V1 audit**: it stays as it is on `release/v1`. G18 is not backported, because a site-by-site retry cannot close the commit-then-audit window (S9; R29). This is an accepted gap under D24.
- **Opportunity**: nothing open is lost. Open V1 items on shared code carry into V2: PD4 #919, the SL1/SL2/SL4 proofs, KI-CONTRADICT, KI-QUARANTINE, KI-PROVENANCE, DA1, DA10, PL-2 and PL-4.

The dispositions of the V1 defects found by the campaign are in "V1 defects and disposition".

**Evidence**: human direction (Revision 3); S2 H1 and the reversibility table; S1 G8, G16 and Q5 ("what gets removed from V1"); architect C5; #5836 (the V1 surfaces and claims); #5714; S8 Q3 and the retire rows of its Q1 map; `release.yml` (tag-triggered; it publishes to the `latest` pointers); S9 Q2 and rows 26–27.

**Recommendation**:
1. Keep V1 on `release/v1` from the branch-point tag, with any V1 update allowed there. On `main`, V1-only surfaces get no new work (D22).
2. Release v0.12.0 from `main` first, with the existing `/uni-release` and `release.yml` unchanged (Stage 0 step 0; human direction). It ships without the two known security fixes (R27).
   - Then, before the cut, land those fixes on `main` through bugfix sessions and release them as a v0.12.x patch, so both lines carry them: `agent_id:"human"` reaching Admin, and `context_cycle_review` having no `require_cap`.
   - If D27 takes option (a), the same patch also makes export and import carry every audit column (S9), so the branch-point tag fixes a lossless format.
   - After that patch, make the one-time change to the V1 release workflow (D23).
3. Fix the export format at the branch-point tag, on both branches (T20), with every audit column if D27 takes option (a). After the tag, `export.rs` on `main` gets test extraction only. The tag also gives the human a pre-refactor binary to fall back to.
4. Port shared-code fixes `main`-first, and backport one only when a V1 user needs it (D22).
5. Retire `release/v1`, the V1-only surfaces and the code-level retire list at α exit.
6. Record the accepted V1 gaps on `main` as an accepted-gap decision (authority class H4), within D2 and D22.

---

### Q: "V1 → V2 migration path (data, hash chains, audit, clients) for existing projects."

**Answer**: Each V1 store becomes one fresh V2 project, by export and import, verified twice. Nothing is re-hashed, no audit row is rewritten, and no two stores are merged. V1 audit rows carry the columns the V1 export emits: all 12 if D27 fixes the export before the V1 cut, otherwise 8 of 12. Clients re-attach through `npx unimatrix init`. No V1 credential carries over.

**Identity mapping**

| V1 | V2 |
|---|---|
| A slug (personal cloud) or a path-hash store (local) | A `(tenant, project)` with an immutable `project_id`. The storage directory is `{project_id}/` (S1 R7; C14). The old slug becomes a routing alias |
| `config.toml` plus the per-slug overlay | PCD v1, created by TOML import and checked by the same validator (S5 C3, C9). Cell-level settings (models, connection pools) stay in operator config |
| `tokens.json`, the legacy token, client bundles | Not migrated. Re-issued as control-plane principals. The legacy token is retired (S1 G5, G8) |
| The agent registry, including the bootstrap `human` agent with Admin | Imported as history, but inert: V2 authorizes on the principal's role (G6). The agent-type policy starts empty, which behaves exactly like T1 |
| The HNSW dump and per-slug adaptation state | Not migrated. Vectors are re-embedded on import into the SQLite BLOB column (C1), recording `model_id` and `dim` (C24), and HNSW is built from them. This also clears the historical adaptation drift |

**Data**
1. Freeze writes on the V1 project.
2. Export with `unimatrix export` (all tables, lossless, bit-exact confidence) and run `chain_verify` on the source (#5714).
3. The Owner registers an empty V2 project (`init`, no restart; S1 Q5) and imports into it before its first open (T23).
   - The importer refuses a target that already has audit history (#5714, declared non-goal C-5). Every import therefore goes into a fresh project.
   - Two V1 stores are never merged into one project, because entry ids are per store and would collide (architect [S3] constraint 1). Pick the canonical store and archive the others as export bundles.
4. The import re-embeds into the BLOB column, and HNSW is built from the stored vectors.
5. Verify twice:
   - run `chain_verify` on the target;
   - re-export from V2 and diff the result against the V1 export, ignoring the new V2 columns.

   The diff is needed because `content_hash` covers only `title: content`. Status, category and tags fall outside it (S3 F2).
6. Give the tenant a migration receipt: the source and target chain reports with head hashes, the V1 audit digest, the V2 audit chain's genesis head, and the known gaps in the V1 audit history (S9; R30). This is the first chain anchor a tenant holds (S2 out-of-scope discovery 9; D26).

**Hash chains**: carried over unchanged. `content_hash`, `previous_hash`, `version` and entry ids are all preserved, so `supersedes`/`superseded_by` and every bare-id reference stay valid. Nothing creates a new chain, re-hashes an entry, or adds a link between stores.

**Audit**
- V1 rows are inserted unchanged and never updated afterwards. The DDL triggers forbid updates, so there is no backfill.
- V1 rows carry the columns the V1 export emits (D27). Under option (b), or from an export older than the branch-point tag, the four attribution columns are NULL, and the receipt says so.
- V2's `principal_id`, real `credential_type` and `attestation_tier` are additive columns (ass-060 invariant 4), NULL on V1 rows. Tools must display NULL as "V1, self-asserted attribution".
- The import itself appends two rows:
  - one in the project audit log, the `v1_import_digest` row (source slug, export digest, V1 version). It is written in the import transaction, attributed to the importing operator rather than `"system"`, and is the chain's genesis; the V1 rows before it stay unchained (S9; T23);
  - one in the control-plane audit log, a class-A saga step that records the importing principal under the same correlation id (G11, C18).

**Learning state**
- `co_access`, `query_log`, `injection_log`, observations and confidence migrate as they are. They are per project, so they raise no isolation issue.
- They carry no principal, so purging by principal (LI-7) cannot be applied to them.
- Sessions and transcript holds are not migrated.

**Clients**

| Client | Migration |
|---|---|
| Claude Code, OpenCode, Codex (K1) | Run `npx unimatrix init` once per repo on each machine. It logs in with OAuth, attaches to `/v1/{tenant}/{project}` and replaces the V1 wiring. Claude Code and OpenCode also get a device key and the attestation adapter (Stage 5) |
| CI using vnc-050 scoped tokens (capability C19) | Re-issue a service token at α. Switch to a GitHub OIDC trust policy once Stage 4 ships token exchange |
| Other automation | Re-issue service tokens |
| claude.ai | New: paste the project URL in as a custom connector |

**Order, downtime and rollback**
- **Order**:
  1. a small research project first;
  2. then the other dev projects;
  3. Unimatrix-on-Unimatrix last. It is the heaviest corpus (5,933 entries, 42% of them quarantined). Migrate it as-is; purging quarantined entries is a separate lifecycle decision.
  4. Med Search migrates into its own tenant at β.
- **Downtime**: minutes per project. The heavy re-embed takes about 85 s at ~15 cores [M4 analogue].
- **Rollback**:
  - V1 stores stay read-only until α exit, so rolling back means pointing clients at V1 again.
  - Once V2 has accepted writes there is no path back to V1, because V1 cannot open the newer schema.
  - From then on, the exit path is a V2 export run on the V2 binary's local N=1 mode. That is why option (b) in D2 matters.

**Evidence**: #5714 (AC-10: all tables, `chain_verify`, source untouched; non-goal C-5); S2 Q4 and H1; architect C1, C14, C15 and [S3] constraint 1; S1 R7, G5, G6, G8, G9, G11 and Q5; S3 F2; ass-060 invariant 4; S9 Q2 and rows 26–27.

**Recommendation**: Fix the V1 export format at the V1 branch-point tag, on both branches (T20), with every audit column if D27 takes option (a). On `main`, leave `export.rs` source untouched after the tag; only its tests move (S8). On `release/v1`, a V1 update that changes the schema or the export format must also extend the importer. Build a V2 importer that accepts that format and targets `(tenant, project)`, splitting `import/mod.rs` when the importer is built (Stage 3). Verify every import with `chain_verify` on both sides, the re-export diff, and `verify` on the V2 audit chain (G18). Migrate the smallest project first. Keep V1 read-only until α exit.

---

### Q: "Draft V2 strategic goal set (which V1 goals survive, merge, die, or are new) and restated architectural principles."

**Answer**: Three V1 goals survive in restated form. `personal-cloud` dies and is replaced by a new `hosted-service` goal. `trusted-identity` is promoted from emerging and absorbs authentication and authorization. `platform` and `domain-agnostic` merge. Of the eight principles, P1–P5 and P8 survive, clarified or extended, with P2 restated by audit class (S9); P6 and P7 take the architect's text with architect review 2's amendments; three are new (P9–P11). Everything here is draft; the human approves it (D5).

**Goal set (draft)**

| V1 goal | Disposition |
|---|---|
| `integrity` (#5681) | **Survives, extended** |
| `self-learning` (#5684) | **Survives, restated** |
| `proactive-delivery` (#5682) | **Survives, restated as additive** |
| `personal-cloud` (#5836) | **Dies; replaced by the new `hosted-service` goal** |
| new: `hosted-service` | **New** |
| `trusted-identity` (#5734) | **Survives; promoted from emerging; absorbs authN/authZ** |
| `platform` (#5689) + `domain-agnostic` (#5732) | **Merge** (the alternative is to keep both) |
| Root vision (#5690) | **Restated** (text below) |

**`integrity`: survives, extended**
- *Draft intent*: Every entry and every state change is attributed to an authenticated principal, is tamper-evident, and is current. Corrections and retractions propagate. Contradictions are served according to a declared policy. Machine-inferred links can be told apart from asserted ones. Each tenant can verify its stores' chain heads without trusting the operator.
- *What changes*:
  - The claim floor adds principal attribution (G9), retraction (M3), KI-QUARANTINE and KI-PROVENANCE.
  - The north-star KI-CHAIN-XV-STRONG rises in priority, because in SaaS the operator is the raw-DB adversary (S2 out-of-scope discovery 9; S3 F2).
  - "RBAC out of scope" is dropped from this goal; RBAC moves to `trusted-identity`.
  - The audit claim becomes class-based, same-transaction and tamper-evident (P2; S9). KI-AUDIT is re-opened, because its `done_when` is not met today (S9).

**`self-learning`: survives, restated**
- *Draft intent*: Every adaptive project measurably improves from its own use, with no hand-calibration. Learning is a per-store mode: adaptive by default, curated, or deterministic. Learning never crosses a project or tenant boundary. Every signal is attributed to the principal and client class that produced it.
- *What changes*:
  - The learning-isolation rules LI-1 to LI-8 become capabilities.
  - SL-OPTOUT becomes `learning_mode` (D8).
  - The audit log is never a learning source.
  - SLN1 is keyed on the principal.

**`proactive-delivery`: survives, restated as additive**
- *Draft intent*: Knowledge arrives before agents search for it, conditioned on declared workflow context (cycle, phase, goal) in any domain. Without declared context, every surface returns core-engine results.
- *What changes*:
  - PDN1 parity becomes the proof that the workflow layer is additive (S5 W2).
  - Briefing is described as goal-conditioned, which fixes the drift S5 found.
  - Hook injection is available to K1 clients only.
  - Cross-project delivery comes into scope, as federation from intra-tenant shared spaces (S3 SS-R5).

**`personal-cloud`: dies, replaced by `hosted-service`**
- *What dies*:
  - the "one container, one bearer token, one command" self-host promise;
  - operator-declared slugs plus restart;
  - client bundles;
  - certificate pinning;
  - parity between local and cloud as a claim;
  - air-gap as a product.
- *What survives*: the zero-friction intent, which moves into `hosted-service`.

**`hosted-service`: new**
- *Draft intent*: One hosted, multi-tenant Unimatrix serves many tenants, with isolation by construction.
  - A developer goes from sign-in to a serving, attached project in minutes, with nothing to run and no secrets in the repo.
  - Every client class attaches with least privilege.
  - A tenant can export everything and leave.
  - One operator runs it at a stated cost, a stated durability (RPO/RTO) and a stated per-tenant fairness bound.
- *What changes* (personal-cloud capability IDs and titles, not architect constraints):
  - New capabilities: control plane, tenancy, onboarding, residency and scheduler, export and purge, quotas, operability.
  - Carried forward as they are: C3, C5, C13, C14, C18, C19, N1–N7, BACKUP-RESTORE, and NX-DURABILITY (extended with RPO).
  - Restated for SaaS:
    - C0: pipeline fidelity over HTTPS, with no local comparison.
    - C1: sign-in to a serving project, instead of deploying an instance.
    - C2: an Owner registers self-serve, instead of the operator.
    - C4: one isolation seam, without the local-UDS half.
    - C8, C10 and C11: OAuth or scoped credentials over CA-issued TLS, instead of one bearer over pinned TLS.
    - C12: credential safety without "sole bearer".
    - C15: a tenant verifies its own client.
  - Moves to the merged platform goal: C6 (per-slug configuration becomes the PCD).
  - Retires at α exit: C7 (fingerprint pinning), C9 (bundle attach), C16 (air-gap deploy), C17 (the installer seeds server config files; the PCD replaces it).

**`trusted-identity`: survives, promoted from emerging, absorbs authN/authZ**
- *Draft intent*: Every call is made by an authenticated principal: a user (OAuth 2.1), a workload (federated OIDC) or a service (scoped token). It is authorized server-side by role ∩ credential ceiling. Agent identity is harness-attested where the harness allows. It is tiered, and it only ever narrows authorization. Claims are worded per tier; "non-forgeable" is claimed only at a validated T3.
- *What changes*:
  - A `done_when` can now be written, using tiers T1–T3.
  - PL-4 is amended (D13).
  - Roles are Owner, Writer and Reader; Admin is human-only.

**`platform` + `domain-agnostic`: merge**
- *Draft intent*: Any domain runs on the same engine, configured rather than rebuilt. Configuration goes through a versioned, validated, audited PCD per project, with domain-pack templates. Solutions extend the presented seams and never reach into internals:
  - L1: the PCD;
  - L2: identity, plus the Unimatrix-owned attestation format;
  - L3: the MCP wire contract and the Node SDK.

  The engine is validated on software delivery, research, and medical-research aggregation.
- *What changes*:
  - "One instance, one domain" becomes "one project, one domain".
  - DA1, DA10 and PL-2 become the PCD (S5 Q4).
  - PL-5 includes the qualified-id format (S3).
- *Why merge*: V1 already defines `domain-agnostic` as the platform's L1 proof (#5689), and in V2 the PCD is both.

**Root vision: draft text** (adapted from S5 Q5; the human approves it, D5)

> **Vision.** Unimatrix is a workflow-aware, self-learning knowledge engine, delivered as a hosted multi-tenant service. It retains, curates and retrieves trustworthy knowledge for any client — agents in a delivery workflow, autonomous task agents and pipelines, chat applications. When a client declares its workflow context — the cycle, phase and goal it is working in — Unimatrix uses that context to deliver the right knowledge before it is asked for. **Workflow-awareness is the differentiator, and it is additive:** knowledge retained and retrieved outside any workflow gets the full integrity and retrieval engine, and never pays for, or is distorted by, workflow features it does not use. Each project's knowledge, audit trail and learning are isolated by construction; sharing happens only through explicit, enumerated grants.
>
> **Boundary.** Unimatrix is not an orchestration engine. It does not coordinate agents, schedule work, hold task state or manage workflows. It answers questions; it never initiates actions, and no workflow depends on state only Unimatrix holds.
>
> **Key mental model — three layers.** *Harness definitions* (workflows, protocols, agents, skills) are released artifacts, distributed by the harness's own channels; Unimatrix references them by version and digest and never hosts them. *Living knowledge* (decisions, patterns, lessons, findings) evolves with every cycle; Unimatrix manages it. *Engine configuration* (categories, relationship vocabulary, domain packs) is each project's versioned contract with the engine, managed through Unimatrix.

**Architectural principles (draft)**

| # | Disposition | Draft text |
|---|---|---|
| 1 | Survives, clarified | **Hash chain integrity is immutable.** `content_hash` and `previous_hash` are on every entry and are never skipped, backdated or made optional. Each store owns its chains. No chain link crosses stores; lineage across stores (publish, fork, migration) is recorded as hash-pinned provenance metadata. |
| 2 | Restated by audit class (S9) | **The audit log is append-only, complete and tamper-evident.** Every committed change to authoritative state — in a knowledge store or the control plane, by a client, a background job or an operator — writes its audit entry in the same transaction, or does not commit. Security events that change no state (authentication failures, denials, rejected attestations) are recorded before the response. Access to knowledge is recorded as access; any loss is itself recorded. Derived and learned state (indexes, confidence, co-access, telemetry) is not audited state. Each store's audit log is a hash chain whose head is anchored outside the store. Each entry is attributed to the authenticated principal, credential type and attestation tier, kept separate from the self-asserted agent annotation. DDL triggers forbid update, delete and non-monotonic insert. The audit log is never a learning source. |
| 3 | Survives, restated | **Capability checks happen at the service layer, after identity resolution.** Every call is an authenticated principal over HTTPS. Tenant and project come from the URL, and authorization state comes from the control plane. Neither ever comes from the payload, which may narrow access but never widen it. Effective capability = role ∩ credential ceiling ∩ tier gate ∩ agent-type policy. Agent identity only narrows. Only human Owners hold Admin. A cell trusts only an authenticated router assertion. |
| 4 | Survives, extended | **The relationship graph is typed.** Relationships are explicit, typed and traversable, and agents declare edges at write time. Machine-inferred edges carry their origin, and can be excluded from traversal and serving. No edge crosses stores. |
| 5 | Survives, extended | **Degradation is graceful.** Every ML capability has a defined fallback; an absent or failed model means the previous behaviour. A cold start, a slow shared store or a missing ML worker degrades service (plain ranking, home-only results) rather than failing it. In deterministic mode, optional ML is off, not degraded. Integrity guarantees are not degraded: a write whose class-A audit row fails does not commit (Principle 2). |
| 6 | Restated: the architect's text (C4), amended by architect review 2 §5 | **One binary, minimal required infrastructure.** The same server binary fills every role. The roles are router, cell, control plane, ML worker and local single-project. They communicate only through their published interfaces, so running several roles in one process is a deployment choice that configuration can undo, never an architectural coupling. The data plane needs only that binary, local disk, and an object store for continuous backup. There is no external database, queue or cache service. Identity is delegated to an external authorization server by design, so that signing keys and client secrets stay out of Unimatrix (Principle 8). A backup replicator, if one is used, is a stateless sidecar. A binary-only fallback (snapshot + upload) always exists. Clients remain adapters: the Node edge client, never the binary. |
| 7 | Restated: the architect's text (C3), amended by architect review 2 §5 | **In-memory hot path, per resident project.** All ranking-relevant derived state of a resident project (vector index, typed graph, phase frequencies, effectiveness, confidence) is served from memory and never computed from the database at query time. Every piece of it is rebuildable from the project's own durable store without ML inference. A project becomes resident on first request and builds its read-side state before serving, within a stated cold-start bound. Past that bound it serves degraded (Principle 5) rather than failing. It stays current through change-driven rebuilds and is evicted under a per-cell memory budget without losing durable state. Background work is scheduled by change and fair-shared across tenants. One tenant's background work never degrades another tenant's queries beyond a stated, measured bound. A project is resident on at most one cell at a time, the cell holding its current assignment. Derived state is never shared or replicated between cells. Query-path inference runs in the serving cell; background inference may run in a separate worker tier. |
| 8 | Survives, extended | **No secrets in any database.** OAuth client secrets, signing keys (including the token-exchange key), API keys, TLS private keys and crypto-shred keys never enter any Unimatrix database, the control plane included. Only token hashes and public keys are stored. |
| 9 | New | **Isolation by construction.** The project (or shared space) is the physical store, with its own database, vector index, chains, audit log and learned state. A tenant is a grouping in the control plane. No SQL statement spans two stores. A reference from one store to another is qualified as `(store_id, entry_id)`. Learning lands only in the store the request addresses, or flows one way into the reader's own store under an explicit grant. Every isolation gate is tested in both directions. |
| 10 | New (S5 P-new-a) | **Workflow-awareness is additive.** Without declared workflow context, every surface returns core-engine behaviour. Parity is tested. |
| 11 | New (S5 P-new-b) | **Unimatrix answers and never initiates.** It never initiates an action and never holds state whose loss would break a workflow. It offers no locks, claims, queues or notifications. |

**Evidence**:
- Goal entries #5681, #5682, #5684, #5689, #5690, #5732, #5734, #5836.
- The capability index.
- S1 Q1–Q4; S2 P6/P7 and Q4; architect §5 (C3, C4); S3 SS-*; S4 LI-*, D-mode and M3; S5 Q3–Q5 (draft vision, P-new-a/b); S6 tiers; architect review 2 §5 (P6, P7); S9 Q4 (P2).

**Recommendation**: Once the human approves (D5), adopt this goal set and these principles as the baseline for the V2 design session. /uni-zero then applies the goal and capability changes; this campaign edits nothing.

---

### Q: "Sequenced V2 roadmap at capability altitude (what must be proven first)."

**Answer**: Sequence the work in eight stages:
- Prove the [E] cost and durability claims on today's product first, behind the CI safety net and only the refactor splits that work needs.
- Then build multi-tenant isolation.
- Then identity and onboarding, which reaches α.
- Then the second tenant (β), and finally GA.

T2 agent identity and shared spaces are built after their prerequisites, off the critical path.

**Pre-split rule (S8; Stages 1–6).**
- The first delivery to touch an over-limit module pre-splits it as its first commit (the nan-010 ADR-001 pattern, #3586). The IMPLEMENTATION-BRIEF records it as a split candidate (#4531).
- The ratchet (D21) blocks growth meanwhile.
- A giant function is decomposed only when a stage changes it.
- New V2 subsystems are new crates (D7).

Each stage below lists the modules bound to it.

**Stage 0 — Decide, de-risk and clear the blocking debt**
- *Decide and de-risk*:
  - Decisions D1–D7, D20–D24 and D27; D30 by the Stage 1 design session. Decide D23b (repo visibility) before the CI port is built, because it sets the runner class, and D27 before step 3, because it sets the v0.12.x patch's content.
  - A Litestream checkpoint PoC (C12).
  - An AS evaluation that includes a live claude.ai test of CIMD and DCR (S1 unanswered question). If D4's related recommendation is taken, it also tests the npm client's flows.
  - Effort sizing for Stages 1–3 (KC4), using S8's refactor sizing and a rate re-derived from the first two W1/W2 sessions.
- *V1 release, safety net, V1 cut and refactor track*. Human direction sets this order (Revision 3 and its addendum), superseding S8's and Revision 2's:
  0. **Cut V1 minor release v0.12.0** from `main` with the existing `/uni-release` and `release.yml`, unchanged (addendum). The latest tag is v0.11.3, with 87 commits on `main` since. v0.12.0 ships without the two known security fixes (step 3; R27).
  1. **Port CI to Actions on `main`, as-is** (D23).
     - Port the local Rust gates: `cargo build --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace` and the infra-002 guards.
     - Port the infra-001 integration suites, in tiers: smoke per PR, the full suites on merge.
     - Trigger on PRs to `main` and `release/**`, so `release/v1` inherits the CI.
     - The jobs stay non-required until they are green (the nan-024 rule, #5919).
     - The workflow change goes through a GH issue.
  2. **Green baseline.**
     - Fix or quarantine the flaky tests, and triage the 15 xfail. Each quarantine is an issue-linked waiver (authority class H8).
     - Remove the blind retry in `release.yml` (`cargo test --release || cargo test --release`), which hides flakes.
     - Then make the PR tier required.
  3. **V1 security fixes and, under D27, export fidelity** (Q2): `agent_id:"human"` reaching Admin, and `context_cycle_review` without `require_cap`. They are bugfix sessions on `main`, released as a v0.12.x patch from `main` before the branch cut, so both lines carry them. If D27 takes option (a), the same patch makes V1 export and import carry every audit column (S9; one small session [E]).
  4. **Cut the V1 branch-point tag and `release/v1`** (D22, D23).
     - After the v0.12.x patch and before the cut, narrow the V1 release workflow to a V1-only tag pattern and make its `latest` pointers channel-aware (D23).
     - The tag fixes the export format (T20), with every audit column under D27 (a), and is the human's pre-refactor fallback.
  5. **W0, the size ratchet (D21)**: an in-repo script and a committed baseline, run as a job in the same CI.
  6. **Module splits, gated by CI.** Every step is behavior-neutral and compiles. Gate each step with `cargo check -p <crate> --tests`, each module with `cargo test -p <crate> --lib`, and the workspace once per wave (#3328, #2957). CI gates every PR.
     1. W1a: move inline test modules over 100 lines out of line. Module paths are preserved, so there is no production diff. This clears 48 of the 85 non-test violators.
     2. W2: split the six prerequisite modules behind re-export facades, in the order Stage 1 needs them (T16):
        - `background.rs`, with `services/status.rs` and `services/nli_detection_tick.rs` pre-split alongside, for the tick fixes;
        - `store/migration.rs` (one file per version step) and `store/db.rs` (DDL per table group), for C1, the G9 columns and G18's audit DDL;
        - `mcp/tools.rs` (per-tool-group routers combined with `+`, and `context_cycle_review` moved to its own exempt file) and `server.rs`, for G9 and G18;
        - `uds/listener.rs`: `dispatch_request` moves into a transport-neutral hook-dispatch module and is decomposed per arm, for LI-2.
     3. W1b: split test files over 1,000 lines by concern, off the critical path.
- *Eval* stays local and uses α data only (R20).
  - Run the eval baseline (`product/test/eval-baselines`, the ass-098 odometer) before W2 and after each W2 split.
  - A metric change beyond the odometer's run-to-run noise blocks the next split until it is explained.
- *Exit*:
  - The PoC result is in, an AS is chosen and the sizing is done.
  - v0.12.0 and the v0.12.x patch have shipped. The PR tier is green and required, `release/v1` exists, and W0 has landed.
  - Each of the six modules leaves the size baseline before the first Stage 1 change to it, so all six are off by Stage 1 exit.
- *Burn-down targets* (S8): the retire list's baseline entries are deleted at α exit. At GA only exempt entries remain, each with a reason.

**Stage 1 — No-regret foundations.** These run on today's product. They land on `main`, and an item reaches V1 users only by a backport to `release/v1` (D22, T21).
- *Durable state*:
  - Store entry vectors in SQLite, with `model_id` and `dim` (C24), and rebuild HNSW without inference (C1). C1 is mandatory for containers: shutdown is never the persistence point (C27). On the insert path, the entry, its vector and its audit row commit in one transaction (with G18; S9 Q3).
  - Inventory the state that lives outside the SQLite file (C2).
  - Measure adaptation-state drift, then either persist the state or switch to identity adaptation.
  - Make compaction run in multi-project mode.
- *Tick fixes, in this order*:
  1. Gate the status-only contradiction scan.
  2. Bound graph-enrichment source S1.
  3. Run NLI only on new pairs.

  Then cap the NLI memory arena, or move NLI into a separate worker process (D30). Either way, background ML goes behind an `InferenceBackend` seam with local and out-of-process implementations (C21). Inference runs in separate interactive and background lanes, with ORT thread counts set from the cell's CPU budget (C23).
- *Attribution and learning sinks*:
  - Put `principal_id` and a real `credential_type` on audit rows and entries (G9). S9 extends G9: a `request_id`/`correlation_id` that joins `http_access`, tool and control-plane rows; `epoch` (C19); a millisecond timestamp; `system:<job>` on background rows; and the importing operator, not `"system"`, on import provenance.
  - **G18, the audit write path** (S9; D24): class-A audit in the same transaction, failing closed; SQLite rowid assignment instead of the counter allocator; a monotonic-insert trigger; a per-store hash chain; store-crate `*_with_audit` methods (C6); audit rows for the state changes that write none today (S9 inventory rows 3, 10, 11 and 13–15); and no per-tick GC warning. Class B and class C follow in Stage 2.
  - Add learning predicates to the hook-path sinks (LI-2).
  - Delete graph-enrichment source S8 (S4's name for it, not spike S8), which reads `audit_log`, or re-source it from `query_log` (LI-3).
- *Guard*: a CI gate that bans `ATTACH` and any SQL naming a second database, filed as a GH issue.
- *Pre-split at first touch*:
  - `services/status.rs` (fix 1, compaction) and `services/nli_detection_tick.rs` (fix 3, the NLI arena), done with the `background.rs` split (T16);
  - `store/write_ext.rs` (G9, G18). `store/write.rs` (427 raw lines) is under the limit, so the ratchet watches it.

  LI-2 lands in the extracted hook-dispatch module, not in `uds/listener.rs`. Whether `run_maintenance` and `run_graph_inference_tick` are decomposed before or with their fixes is a Stage 1 design call (S8).
- *Must prove*:
  - **Post-fix tick cost [M]** on the heavy corpus. This is the input to KC1.
  - HNSW rebuilt from stored vectors with no inference, with restore time measured.
  - An **audit census test**: every mutating path in S9 inventory rows 1–15 writes exactly one audit row, in its own transaction. It extends the existing store and server fixtures; no new scaffolding.
  - A **SIGKILL probe** and a **write-hog probe**, each with zero mutations committed without their audit row.
  - `verify` passes on a store upgraded by the schema migration.
- *Gate*: KC1.

**Stage 2 — Multi-tenant core**
- *Control plane*: an async `ControlPlaneStore` trait backed by SQLite (C6, C7). Its audit is class A, same-transaction and chained (G11, extended by S9). It also holds operator CLI actions, the platform denial log, project chain anchors and saga correlation rows. A tenant data export is recorded there as a class-B event.
- *Routing*:
  - `ProjectKey::Tenant` and the `/v1/{tenant}/{project}` route grammar (G1).
  - A dynamic resolver that returns owned, lazily opened stores, with a routing map that can change at runtime (G2, G3).
  - Immutable ids, with deleted slugs tombstoned (R7, C14).
  - The routing row is `project_id → cell_id, epoch, region`. A cell asked for a project it is not assigned returns `Misdirected`, and the router re-resolves (the #4950 correction).
- *Principals and tokens*:
  - Principal variants and token records in the control plane (G5, G7).
  - The legacy token never reaches the tenant plane (G8).
  - Denials are class-B audit, written before the response, to the principal's own tenant and the platform log (G10). Every denial reaches the platform log, including one for a principal with no home slug, and HTTP 401s and MCP capability, content and rate denials are included (S9).
  - Session keys are namespaced by tenant, project and principal (G12).
- *Wire contract*: reserve the qualified-id format in the tool schemas now (S3 SS-R3).
- *Scale-out boundaries* (architect review 2):
  - C17: identity is resolved once at the router into `RoutedRequest {tenant_id, project_id, principal, assignment_epoch}`. The cell validates it as if a remote router sent it, even in-process. The router holds no durable state and routes only by URL path.
  - C18: no transaction spans the control plane and a project store. Cross-plane operations (create, grant, purge, move) are idempotent sagas that write class-A rows in both audit logs under one correlation id (S9). Routers and cells keep serving established principals from cache for a stated TTL while the control plane is down.
  - C19: single-writer fencing by assignment epoch. A cell opens a project only while it holds the current lease. A move is drain → snapshot → epoch+1 → open on the destination; it writes `project_drained` and `project_opened` audit rows, and the chain head goes to the control plane at drain (S9). Litestream replica paths key on `project_id/epoch`. `flock` stays a host-local guard.
  - C20: the scheduler enumerates only this cell's assignment set. Fleet-wide jobs get a control-plane home under a single-runner lease, built out in Stage 4.
  - C25's shape: `ReadView` is async and returns owned, serializable results. It is fixed with the qualified-id reservation.
- *Audit* (S9): the class-B writer (with G10); the anchoring saga (D26); and the class-C queue, which is bounded, drained at shutdown, and records drops as `audit_dropped` rows (D25).
- *Pre-split at first touch*:
  - `infra/config.rs` (G13 and the G8 config). `merge_configs` is not decomposed, because the PCD replaces it in Stage 3.
  - `main.rs`: G2 replaces router construction in `tokio_main_daemon`.
  - `infra/session.rs` (G12).
  - `store/read.rs`: new queries land in the store crate (C6).

  The control plane is a new crate (D7, T18).
- *Must prove*: the **bidirectional cross-tenant isolation gate** — tenant A cannot reach B, and B cannot reach A — across read, write, learning, hooks and audit.
  - It runs on the **two-cell CI topology** (C16): two cells, a router and a control plane as separate processes, with a local ML backend allowed.
  - The same suite moves a project between cells, with writes before and after. Entry chains and the audit chain verify with no gap or fork, and no vector is re-embedded (S9).
  - The field census classifies every server-state field as project, cell-stateless or forbidden-global, so a new process-global field fails CI.
- *Gate*: KC2. C16 also blocks α.

**Stage 3 — Identity, onboarding and migration (reaches α)**
- *Authentication*:
  - An OAuth 2.1 resource server (protected-resource metadata, audience validation, step-up) on the hosted AS.
  - OAuth in the Node client, with one credential shared by MCP and observe. This is recommended under D4, not yet decided.
  - `npx unimatrix init` for self-serve register and attach.
- *Authorization*:
  - Owner, Writer and Reader roles plus service principals.
  - Admin derived from the Owner role (G6, structural fix).
  - Client classes K1–K5, with the learning contribution stored on the grant (LI-1, LI-4–LI-7).
  - Quotas replace the rate limits keyed on `agent_id`. Exact limits key on `(principal, project)` at the cell; principal- and tenant-wide limits are eventually consistent through the control plane, with published slack (C26). Audit bytes never count against an enforceable quota, so fail-closed class A cannot turn a quota hit into a refused audit row (S9).
- *Configuration*: the PCD core (PL-2) with TOML import and export, plus DA1 runtime discovery.
- *Migration and operations*:
  - The migration importer and its verification (Q3). The import runs before the project's first open (T23). Receipts carry the V1 audit digest and the V2 genesis head, and state the V1 audit history's gaps (S9; R30).
  - Backup with Litestream, or with the binary-only snapshot fallback.
  - Metrics and tick-liveness alerts.
  - MCP and hook session state is cell-local and loss-tolerant: after a cell restart or a project move, the Node client re-initializes, with a test (C22).
- *Hosting* (C27; D28 with D16):
  - The V2 cell image is one binary with a role env (`router | cell | control | ml-worker`).
  - Models are baked in, or pulled hash-verified from the operator's store; there is no first-boot internet download.
  - Public TLS terminates at the edge, and cells listen on internal mTLS only.
  - Readiness is separate from liveness, so a degraded project does not fail the cell's healthcheck.
  - Nothing relies on graceful shutdown for durability (C1).
  - If D28 relies on reattachable volumes, the network-volume PoC runs first (Unanswered Question 6).
- *Optional for α*: the claude.ai connector.
- *Release* (D23):
  - the server's continuous-deploy pipeline from `main`, with the two-cell cross-tenant isolation gate (C16) and the container smokes as pre-deploy gates;
  - the first V2 npm client release, on the `client-v*` line, with the compatibility window in force.
- *Pre-split at first touch*:
  - `store/analytics.rs` (LI-7);
  - `services/observation.rs` and `services/behavioral_signals.rs` (learning sinks, LI-1), unless LI-2 touched them first;
  - `import/mod.rs` (the importer).

  The PCD replaces `merge_configs`. The OAuth resource server is a new crate (D7).
- *Must prove*: all real projects migrated with receipts, and an α soak period with no fallback to V1. Then **V1 is retired**: `release/v1` stops releasing, and the code-level retire list is deleted from `main` with its baseline entries (Q2).
- *Gate*: α exit.

**Stage 4 — Second tenant (reaches β)**
- *Residency*:
  - Lazy load with build-on-load.
  - LRU eviction that never loses durable state.
  - A memory budget per project.
  - A policy for running schema migration on load (C15).
  - Eviction that cannot reopen an attestation replay window (C8).
- *Scheduling*: a change-driven, fair-share scheduler, superseding crt-056 ADR-005.
- *Fleet jobs* (C20): tenant export and purge, backup verification and retention expiry, quarantine purge, usage rollup, model-epoch re-embed rollout, token and certificate rotation, and the fleet audit-verification job (S9). They run in the control-plane role under a single-runner lease and execute as per-project work items on the owning cells.
- *What Med Search needs*:
  - retraction (M3, D9);
  - a contradiction-serving policy (M2);
  - edges whose origin can be excluded from serving (M6, #1013);
  - `learning_mode` (D8) with the fingerprint contract;
  - DA10 edge vocabulary and a research / med-search template;
  - K4 quotas and the K5 read token;
  - GitHub OIDC token exchange.
- *Pre-split at first touch*:
  - `services/search.rs` (`learning_mode`, retraction visibility). `search()` is decomposed only if these changes need it.
  - `infra/validation.rs` (DA10).
- *Must prove*:
  - the fair-share bound, measured in-process over HTTP with the inference lanes in place (C11, C23);
  - the cold-start bound, including the effectiveness build and disk-cold reads;
  - the deterministic test: equal fingerprints give identical results across queries, ticks and wall-clock time;
  - K5 traffic writes zero learning rows.
- *Gate*: β.

**Stage 5 — Agent identity at T2** (runs in parallel with Stage 4)
- *Server*:
  - A device-key registry and a single-use call-id cache. The cache is keyed per project and is persisted, or drained for its TTL, before a project move completes (extends C8; architect review 2).
  - Attestation extracted before typed deserialization.
  - The `external_identity` seam activated (G15, G17).
- *Adapters*:
  - A Claude Code `PreToolUse` hook.
  - The OpenCode v2 plugin (#998), pinned to a version range and fire-tested in CI.
- *Policy*: tier-gated operations, with PL-4 amended (D13).
- *Prerequisite*: remove `edit: allow` from the repo's `opencode.json`.
- *New crate*: attestation (D7). G15's 18 call sites are already in the split `tools.rs` and `server.rs` (S8).
- *Must prove*:
  - The forgery probes P1–P7 re-run against the real server over remote HTTPS.
  - Codex verified at T1.
- *Gate*: none. Stage 5 does not block GA, because the default policy behaves like T1.

**Stage 6 — Intra-tenant shared spaces** (after Stages 2 and 4)
- *Capabilities*:
  - `kind = shared` stores. A shared store and its granted consumers form one placement group, which co-locates and moves as a unit (C25).
  - Grants that the consumer must accept.
  - `ReadView` handles (SS-B2) and a `FederatedSearch` layer.
  - Publish and fork, with provenance.
  - The `foreign_usage` overlay.
  - Curated learning mode.
- *Pre-split at first touch*: none new. `services/search.rs` takes the `FederatedSearch` wrapper, and `store/write_ext.rs` takes publish/fork provenance; both were split earlier.
- *Must prove*: the parity test, the twin test, and the three-case bidirectional isolation test (S3).
- *Gate*: none.

**Stage 7 — GA gate**
- *Requirements*:
  - HA per D3 (a warm standby, if required).
  - Erasure and retention published, including audit retention (D15).
  - Tenant export, carrying every audit column and the chain head (S9), and purge.
  - Audit chain verification green fleet-wide (S9).
  - A per-cell blast-radius cap in placement policy (C28, D29).
  - Region chosen (D16).
  - Quotas and pricing set.
  - A security review passed, covering S9 inventory rows 21–30.
  - Cost re-measured with a real light corpus (Med Search).
- *Must prove*: every requirement above.
- *Gate*: GA.

**Post-GA**
- T3: the Claude Code sandbox plus digest pinning.
- NLI contradiction candidates (ass-092 Option B).
- A feedback channel for shared spaces.
- Cross-tenant sharing.
- A second region.
- A Postgres control plane, once an HA trigger is hit.
- A purge lifecycle for quarantined entries.
- Store-level chain-head anchoring, with continuous tenant-visible anchors (KI-CHAIN-XV-STRONG; D26).
- The auditor role: a tenant-scoped, read-only role that views the tenant's audit logs. Human direction: roles are already tenant-scoped by the OAuth design, and audit logs are tenant-scoped. It waits for an audit-viewing interface; it is a roadmap item, not a decision.
- Sealed archival of cold audit segments to object storage (S9).
- A network-remote ML worker tier, once background ML rather than serving sets the cell count (KC1 data). The stateless worker fits serverless hosting (architect review 2).
- Separate router processes and a rebalancer, on router-CPU, edge-HA or capacity triggers (architect review 2).

**What must be proven first, in order**
1. **Post-fix tick cost [M].** The go decision's cost claim rests on [E] numbers (S2 U3; KC1).
2. **Vectors in SQLite, rebuilt without inference.** The architect made this a blocking condition for any RTO, backup or eviction claim (C1).
3. **G9 and G18 in Stage 1, then G6 with roles in Stage 3.** All three must be closed before a second tenant exists (S1 Q2; S9). G18's proof is the audit census test, with the kill and write-hog probes.
4. **The bidirectional cross-tenant isolation gate, across two cells (C16).** It is S1's first V2 proof (lesson #5348).
5. **The fair-share and cold-start bounds.** Restated P7 promises measured numbers (C11).

**Sequencing notes**
- Stage 1 comes first for three reasons: it needs no tenant dimension; it retires the largest uncertainties; and it is not lost under a later no-go, which returns `main` to V1 releases (D23).
- The v0.12.0 release comes first, shipping the 87 commits since v0.11.3 on the existing pipeline (addendum). The CI port and the green baseline follow, so every refactor PR is gated (human direction). Next comes the V1 cut, so `release/v1` starts from the unsplit code. Then the ratchet lands, so the debt cannot grow while it is paid down. The refactor runs ahead of Stage 1 module by module, not as a block (T16). The tick modules are split first, so only the CI work delays KC1.
- α needs Stages 2 and 3 but not residency. Up to 10 projects fit on one cell with the serial tick after fix 1 [E].
- Residency and the scheduler come before β, for fairness once there is a second tenant. They also come before shared spaces (S3 condition 5).
- T2 is off the critical path, because the default policy behaves like T1.
- The qualified-id wire format is reserved in Stage 2 even though fan-out ships in Stage 6. Changing every tool's `id` contract later would break every client (S3 Q4).
- G6 waits for Stage 3 because principal roles first exist there. The narrow V1 bugfix covers the interim.
- G18 lands in Stage 1 with G9, because both touch the same write sites. Class-B and class-C audit follow in Stage 2, with G10 and the control plane.
- From Stage 2, CI runs two cells while production runs one (C16). The second production cell comes on a capacity or blast-radius trigger (D29), not by redesign.

**Evidence**:
- S1 Q2: the gap list, plus "close G6/G8/G9 before a second tenant".
- S2 Q2/Q3 and its unanswered question 3.
- Architect §6 (C1–C15); architect review 2 §1–§6 (C16–C28, cell sizing).
- S9 Q2–Q4 (G18 and the audit stages).
- S3 H4 conditions 1–5.
- S4 Q3/Q4.
- S5 Q4 (PL-2 → DA1 → DA10 → templates).
- S6 Q2/Q3.
- S8 Q1 (module map, call sites per gap), Q2 (waves, sizing) and Q4 (ratchet, burn-down targets).
- Human direction (Revision 3): the Stage 0 order, branching and CI.

**Recommendation**: Run the stages in this order. Bind each "must prove" item into the acceptance criteria of the delivery that builds it. Record each pre-split as the delivery's first commit in its IMPLEMENTATION-BRIEF.

---

## Unresolved risks

| # | Risk | Mitigation, or where it is retired | Residual |
|---|---|---|---|
| R1 | The cost estimate is wrong: post-fix tick cost is unmeasured, and the light-tenant cost is [E] | Measure in Stage 1. Re-measure with the Med Search corpus before GA | Open until measured |
| R2 | Vector durability: HNSW is dumped only at graceful shutdown, so failover means re-embedding the fleet. In a container, a short stop grace period turns this into vector loss on every SIGKILL | Stage 1 (C1), before the first containerized cell. Nothing relies on graceful shutdown (C27) | None after Stage 1 |
| R3 | An RPO of up to 60 s loses acknowledged writes; the chain is truncated | D3 accepts it. If D3 rejects it, writes must be durable off-box before they are acknowledged, at a latency cost | Accepted, or redesigned |
| R4 | No HA at launch: one box failure is a full outage for every tenant on the cell | D3. A per-cell tenant cap bounds how many tenants one failure takes (D29). Add a warm standby before GA if required. On Hetzner, a reattachable volume could shorten host-failure recovery (D28) | Accepted for α/β |
| R5 | An isolation defect becomes a cross-customer incident | Stage 2 gate; LI-2/LI-3; the `ATTACH` ban; type-separated handles | Ongoing |
| R6 | Agent identity is overclaimed: an LLM can still forge T2 through L1–L3 | Claims worded per tier (D13); T3 after GA | The T2 limits remain |
| R7 | Harness APIs keep changing (OpenCode v2's release pace; #998 has already been rebuilt once) | A pinned adapter, a CI fire-test, an attestation format Unimatrix owns. T2 is never the sole identity root | Ongoing maintenance |
| R8 | The engineering effort is large for one person, with two release lines until α exit | Staging; a no-regret Stage 1; effort sizing (KC4); the porting rule (D22) | Refactor component sized by S8 [D] at ≈28–31 split sessions; the CI port and green baseline at ≈2–5 [E]. Feature effort for Stages 1–3 and V1 ports are unsized |
| R9 | Legal exposure: erasure, residency, medical-research content, public chat | D15 and D16; legal input before GA. K5 traffic persists no query text (S4 Q2) | Undecided |
| R10 | Dependency on the hosted AS: cost unpriced; claude.ai's CIMD/DCR behaviour disputed | Stage 0 evaluation with a live test. The AS must offer DCR as well as CIMD | Vendor lock-in |
| R11 | The operator is the raw-DB adversary, and `content_hash` does not cover status, category or tags. The operator can also rewrite a whole audit chain until its anchors leave the operator's sole control (S9) | Migration receipts that carry chain heads; a reader-side audit of the hash of every served shared entry (S3); audit heads anchored in the control plane from Stage 2 and tenant-visible in receipts by GA (D26); KI-CHAIN-XV-STRONG after GA | Open until heads are anchored outside the operator's control |
| R12 | The NLI arena (0.5 → 5.2–9.4 GB [M8]) could run a cell out of memory. Under a container memory limit, an OOM-kill takes every tenant on the cell. ORT sessions also default to per-host thread counts, which oversubscribes a CPU-quota container | Stage 1: cap the arena, or move NLI behind `InferenceBackend` to a worker (C21, D30). Set ORT threads from the cell's CPU budget (C23). Set no container memory limit before then | Low after Stage 1 |
| R13 | Adaptation drift from the unpersisted per-slug state, of unknown size | Stage 1: measure, then persist the state or switch to identity | Unknown until measured |
| R14 | Deterministic mode across hosts: ONNX numeric reproducibility is untested | A spike before the contract is claimed. Pin the instance class, use one thread, or quantise similarities | Open |
| R15 | A model upgrade means re-embedding the fleet and changing deterministic tenants' fingerprints | Treat each upgrade as an announced per-project epoch. Stored vectors carry `model_id` and `dim`, placement checks catalog compatibility, and a fleet job rolls the upgrade out (C24, C20) | Operational |
| R16 | Fan-out latency and confidence calibration across stores are unmeasured | Stage 6 acceptance tests; RRF as the fallback merge | Open until Stage 6 |
| R17 | `client_type_map` is keyed on `Mcp-Session-Id`, so it breaks under MCP's stateless 2026-07-28 revision. That fix and the move to rmcp 3.x land in the same release window as tenancy (G14) | Stage 3 | Schedule risk |
| R18 | Claude Code auto mode drops `updatedInput`, so some calls arrive unattested | They are downgraded to T1 by design | Low |
| R19 | A refactor wave regresses behavior on `main`, or the refactor rate overruns. The rate rests on one sample (vnc-008), and W2 source splits may need visibility widening that was not counted (S8) | The ported CI gates every wave PR (D23), and the local eval baseline runs before W2 and after each W2 split. `release/v1`, cut before the waves, is the fallback line. Waves are behavior-neutral, with per-step crate gates and a workspace gate per wave (#3328, #2957). The rate is re-derived from the first two W1/W2 sessions (KC4) | Open until the first two sessions |
| R20 | Eval on tenant data. V2 eval on a tenant DB snapshot copies tenant data out of the service. The ass-098 judge also sends entry and query text to a hosted model through `claude -p` (`judge_one.py`) | Before β, eval uses only α data (the human's own tenant) or synthetic corpora (human direction). Build a synthetic eval corpus before β. From β, D15 rules on tenant-data eval; the recommended default is none without the tenant's consent. Snapshots are never committed (the existing eval-baselines rule) | Open until D15 |
| R21 | CI runner memory and disk against the #878 link OOM. The committed `jobs = 6` applies on any runner. Six concurrent heavy links peak near 6.7 GB [D, from #878's 1,112 MB per link]: inside a public runner's 16 GB, but not a private runner's 8 GB [P]. Runners have 14 GB of disk [P]; the local debug target is 19 GB [M], though that includes stale and incremental artifacts. From Stage 2, C16's two-cell suite adds four server processes, about 1.6 GB of RSS [D, from M1's ~350 MB base plus the ~90 MB model in each cell] | Set `CARGO_BUILD_JOBS` per runner class in the workflow; the #878 rule allows lowering `jobs`, while raising it needs re-measurement. Set `CARGO_INCREMENTAL=0`, and cache dependencies rather than the whole target. The infra-002 link smoke runs in CI (D23). Run the two-cell suite after the build, not beside the link step | Low on public runners. Measure in the CI port |
| R22 | Going private. Standard runners are free only for public repos [P]. At private rates, today's Node CI costs about $0.89 per PR run [D, one measured run per workflow]. That is about $31/month at the last 30 days' 35 PR runs [M], against $12–18/month of included usage [P], and 77% of it is macOS. The Rust port adds about $0.2–0.4 per run on 2-vCPU runners [E]. npm generates no provenance from private repos [P] | D23b: stay public unless there is a business reason. If private: a self-hosted Linux runner for the Rust and infra jobs (self-hosted minutes are free [P]); the macOS/Windows grid trimmed to Node 18 and 24 (authority class H5); and the loss of provenance decided as a security-posture change (H1) | Decided under D23b |
| R23 | A flaky baseline. Required CI with flaky tests either blocks merges or teaches that red is normal. Release-time Rust tests already retry once, which hides flakes, and 15 integration tests are xfail | Stage 0 step 2: fix each flaky test, or quarantine it with a linked issue (H8). Remove the blind retry. Make checks required only once they are green (#5919) | Open until the green baseline |
| R24 | Release-channel collisions. V1's release workflow publishes npm `latest` (all three packages, with no `--tag`), marks its GitHub Release as Latest, and pushes GHCR `:latest`. After V2's first release, a V1 maintenance release would move those pointers back to V1. A `v*` tag on a `main` commit would run `main`'s copy of the V1 pipeline | Change the workflow once before the cut (D23): a V1-only tag pattern, an ancestry guard and channel-aware `latest` pointers. V2 uses `client-v*` tags and deploys the server by image digest | None once the pre-cut change lands |
| R25 | Porting cost. Each shared-code V1 update is made twice or deliberately skipped. After the W1/W2 splits, cherry-picks between `release/v1` and `main` stop applying | The porting rule (D22): `main` first; backport only on V1-user need; no forward-port of V1-only-surface changes; no V1 schema or export-format change without an importer update (T20). Count ports per session | Grows until α exit retires `release/v1` |
| R26 | Client–server version skew. The hosted server deploys continuously, while installed npm clients update on their users' schedules. A server change can break older clients' MCP calls or hook payloads | The compatibility window (D23): versioned wire contracts that change only additively within a client major; a minimum client version, enforced by an MCP-level error, with hooks failing open; contract tests against the oldest and newest supported clients | Ongoing |
| R27 | v0.12.0 ships with two known security defects (V1 defects rows 1–2) until the v0.12.x patch | Both are already in v0.11.3 (spot-checked at the tag), so v0.12.0 extends the exposure window rather than opening one. Ship v0.12.x as soon as the two bugfix sessions land (Stage 0 step 3). Until then, keep the defect specifics out of public posts and committed files: the repo is public, and this summary goes to #1050 | Closed by v0.12.x |
| R28 | Audit-coupled availability. Fail-closed class-A audit turns an audit defect into a write outage (D24) | Rowid assignment removes the counter-desync failure behind V1's outage; the audit census test; an alert on audit-insert errors. Reads keep working (T22), and audit bytes never trip a quota (C26) | Accepted by design |
| R29 | V1 audit is not transactional and partly missing, with known tamper paths (S9 inventory rows 1–30; V1 defects rows 26–31) | Stage 1 G18, then Stage 2's class-B and class-C writers. On `release/v1`, V1 audit stays as it is (D24); backport only under D22 | None for V2 after Stage 2; the V1 line keeps it |
| R30 | V1 audit history cannot be repaired: the live store has a six-month hole, and the export drops four attribution columns | Declare both in migration receipts (Stage 3). D27 (a) stops the column loss before the cut | Permanent |
| R31 | Scale-out rot. Process-global state or in-process coupling ships in Stages 1–3, and the first second cell becomes a re-architecture (architect review 2) | C16 from Stage 2: two cells in CI, and a field census that fails CI on a new process-global field. C17–C20 and C27 fix the known cases | Low once C16 runs |

---

## Decisions for human

The list is ranked by what each decision gates. IDs are stable. The S8 items keep D20–D22 and Revision 3's item keeps D23; D24–D27 come from S9 and D28–D30 from architect review 2 (Revision 4). Each sits at its rank. The list consolidates 38 spike escalations, three items this synthesis added (D1, D6, D7), and one item from human direction (D23). The escalations are 26 from S1–S6; 4 from S8, whose D7 addendum folds into D7; 5 from S9, whose audit-retention item folds into D15; and 3 from architect review 2 (E-A to E-C). Each entry gives what it merges, its authority class, the recommendation, and what it gates.
- Decide first: D1, D23, D20, D2 with D22, D27, D3, D24, D4 and D21. D23 gates Stage 0's CI port (step 1), D20–D22 gate its refactor track, D27 gates the v0.12.x patch and the V1 cut (steps 3–4), D24 gates Stage 1's audit work (G18), and D1–D4 gate α directly. Step 0, the v0.12.0 release, needs no decision.
- Next: D5–D7 and D30. D6 and D30 also gate Stage 1 work (C21, C23, the NLI worker), and D5–D7 gate Stage 2, which α also needs.
- D28 with D16 gates the first hosted deploy (Stage 3). D25, D26 and D29 gate β or GA.

Two numbering schemes share the H prefix. *Hypothesis H1–H6* are the campaign hypotheses in SCOPE.md. *Authority class H1–H11* are the human-only items in `uni-primary-authority.md`, named in brackets below (for example, H2 is scope and H4 is accepting a gap). Bare C-numbers are architect constraints (C1–C28); personal-cloud capabilities are written "capability C…".

1. **D1 — V2 go/no-go and staging.**
   - *Merges*: new (S7).
   - *Recommendation*: Conditional go, staged α → β → GA, with GA as a separate later decision. Start Stage 0 now with the v0.12.0 release and the CI port (D23), then the refactor track and Stage 1, in T16's order.
   - *Authority class*: H2 (scope).
   - *Gates*: everything.
2. **D23 — CI, release and branching model.** (Revision 3, human direction)
   - *Merges*: human direction (Revision 3, items 1–5); S1 Q5 (the Node client and `npx unimatrix init`); S6 (the harness adapters); #878 (`.cargo/config.toml`); #5919; the current workflows.
   - *Context*:
     - PR CI (`ci.yml`) runs the Node client suites and one Rust convention script. No Rust build, lint or test runs per PR; keeping Rust CI local was a cost choice.
     - `nan-024-e2e.yml` builds the server per PR, but it is non-required.
     - Rust tests run on Actions only at `v*` tags, in `release.yml`.
     - The integration suites and eval run locally.
   - *Recommendation*:
     1. **CI port** (Stage 0 step 1). Reuse nan-024's server-build pattern: the pinned toolchain, the ORT install script and the `Cargo.lock`-keyed cache.
     2. **Tiers**:
        - per PR, required once green: build, clippy, unit tests, the size ratchet (D21), the infra-002 link smoke, infra-001 `-m smoke`, and the existing Node jobs;
        - on merge to `main`: the full infra-001 suites except `availability` and `volume`;
        - nightly, and before a V1 release: `availability` and `volume`. A V2 deploy needs the last nightly run to be green;
        - from Stage 2, the two-cell topology (C16): per PR, the field census test and a two-cell smoke; on merge and before every deploy, the full cross-cell isolation gate and the project-move test, with audit chain verification (S9);
        - eval: local only.
     3. **Path filters at job level.** A workflow skipped by `paths` leaves its required checks pending and blocks the merge; a job skipped by `if:` reports success [P]. Rust jobs skip docs-only changes. Node jobs that use the built server also run on `crates/**` changes.
     4. **Caching.**
        - Cache Cargo dependencies per profile and arch, not the whole target. The repo cache quota is 10 GB by default [P], and the local debug target is 19 GB [M].
        - Also cache the embedding model and Docker layers.
        - Set `CARGO_INCREMENTAL=0`.
     5. **Runner sizing (#878).** Set `CARGO_BUILD_JOBS` per runner class: 6 on a public 4-vCPU/16 GB runner (≈6.7 GB peak link RSS [D]), and 2 on a private 2-vCPU/8 GB runner [P].
     6. **Branching.**
        - `main` is V2.
        - `release/v1` is cut at the V1 branch-point tag (Stage 0 step 4). It carries V1 until α exit, and any V1 update is allowed on it. D22 has the porting rule.
        - A no-go returns `main` to V1 releases, and `release/v1` is merged back.
     7. **V1 release line.** Tag workflows run from the tagged commit, so V1 keeps its frozen release workflow. Change that workflow once, on `main`, through the GH issue: after the v0.12.x patch, because its new guard would refuse a tag on `main`, and before the cut:
        - Narrow the trigger from `v*` to V1's version line (`v0.*`), and add a guard that the tagged commit is on `release/v1`.
        - Make its `latest` pointers channel-aware. npm `latest` applies only while V1 is the newest major; otherwise V1 publishes under the `v1` dist-tag. The GitHub Release "Latest" mark and GHCR `:latest` follow the same rule.

        After that change the workflow is frozen.
     8. **V2 release model.** The server and the client release on different cadences.
        - *Hosted server*: continuous deploy from `main`, identified by commit and image digest, with no version tags.
          - Each deploy passes the merge tier and the pre-deploy gates: the container smoke, embed readiness, and the cross-tenant isolation gate on the two-cell topology (C16), which today blocks the V1 manifest.
          - Rollback redeploys the previous digest. Schema migrations run forward only, so a deploy that migrates takes a backup first.
          - The UDS parity lane retires with UDS at α exit.
        - *npm client*: semver releases from `main`, on a `client-v*` tag line.
          - A release is cut when client code, a harness adapter, or the bundled skills and protocols change. OpenCode ships about 5 v2 tags a week (S6 Q3).
          - npm allows up to 10 trusted publishers per package [P], so the V1 and V2 workflows can both publish.
        - *Compatibility window*: the server accepts the current and previous client minor versions, for at least 90 days after a newer minor ships. The V2 design session confirms the numbers.
          - Clients report their version for compatibility only, never for authorization (defect 7).
          - Below the floor, MCP calls get an MCP-level "upgrade required" error, not HTTP 401 (T2). Hook calls fail open with a warning (S6).
          - Wire contracts (the tool schemas and the hook/observe payload) change only additively within a client major.
          - The client parity corpus (Layer 1 goldens and Layer 2) becomes the client–server contract test. Server CI runs it against the oldest and newest supported clients; client CI runs it against `main`'s server.
   - *Sub-decision D23a — npm naming.* V2 ships an npm client. The human settled this: hook data comes from the harness, and the client must format and send it (S1 Q5; S6). V1 and V2 therefore share the npm channel.
     - Recommendation: keep `@dug-21/unimatrix`. The V2 client is its next major, on `latest`. V1 maintenance publishes under a `v1` dist-tag (item 7).
     - Rename only if V1 and V2 must be installed side by side. Both ship a `unimatrix` bin, which collides in a global install.
     - The platform packages (`-linux-x64`, `-linux-arm64`) carry the Rust binary. If the V2 client stays Node-only (the Rust binary is not a client), they stay V1-only and need no dist-tag. V2's unsupported local N=1 binary then ships as a release asset or a container image (H1).
   - *Sub-decision D23b — repo visibility.* Recommendation: stay public unless there is a business reason to go private. Going private would have three effects:
     - It ends free standard runners [P]. Today's Node CI would cost about $31/month [D] against $12–18 of included usage [P], and 77% of that is macOS (R22).
     - It shrinks runners to 2 vCPU and 8 GB [P] (item 5).
     - It ends npm provenance, which npm does not generate from private repos [P].

     If the repo goes private:
     - Add a self-hosted Linux runner for the Rust and infra jobs, on a dedicated box rather than the agents' devcontainer host.
     - Trim the macOS/Windows grid to Node 18 (the `engines` floor) and Node 24.
     - Never attach a self-hosted runner to a public repo, because fork PRs would run untrusted code on it [P].

     If the aim is to keep V2's service code closed, D7's split does that and keeps the public repo's free runners and provenance.
   - *Authority class*: H2 (scope: the release and branching model, and V1's maintenance line). D23b is also H1, for provenance and runner trust. Each green-baseline quarantine is H8 (waiving a gate). Trimming the OS grid is H5 (degrading platform coverage).
   - *Gates*: Stage 0 step 1 (the CI port) and step 5 (the ratchet job); the V1 cut (step 4), which needs item 7 first; the two-cell CI jobs (Stage 2, C16); the first V2 server deploy and the first V2 client release (Stage 3).
3. **D20 — Refactor-then-evolve, or a new codebase.** (Revision 2, S8)
   - *Merges*: S8 Q2 (S8 escalation D20).
   - *Recommendation*: Refactor-then-evolve.
     - A rewrite is ≈60+ feature-sized deliveries [D].
     - The full V2-relevant burn-down is ≈28–31 split sessions [D], each smaller than a feature delivery and adding no logic. The Stage 0 set is ≈15–18 of them.
     - The choice holds at 2× S8's rate on the full burn-down, even counting each split session as a whole delivery, and at 3× for the Stage 0 set.
     - A rewrite would still need the V1 export format and importer, and it would drop the premise the go rests on: Option A extends existing seams.
   - *Authority class*: H2 (scope).
   - *Gates*: the Stage 0 refactor track (W1–W2) and KC4 sizing. A new-codebase answer would also reopen D1.
4. **D2 — Hypothesis H1 (local deployment) and V1 disposition.**
   - *Merges*: S2 E1, S1 E5, S1 G16, architect C5; S8 Q3 through D22.
   - *Recommendation*:
     - SaaS-first.
     - The same binary's local N=1 HTTP daemon keeps working, unsupported.
     - The UDS/stdio server path is removed, or kept dev-only at most.
     - The legacy all-slug token is retired, and operator access moves to a separate admin plane.
     - V1 is maintained on `release/v1` until α exit, then retired (D22, D23). On `main`, V1-only surfaces get no new work, and their non-security gaps are accepted there until deletion. Shared code is V2 work.
     - Decide together with D22.
   - *Authority class*: H5 (drops a supported transport and install mode), H3 (contradicts approved ADR vnc-038 ADR-006), H4 (accepts V1 gaps).
   - *Gates*: Stage 2 design; the migration exit path.
5. **D22 — V1 maintenance wording.** (Revision 2, S8; reworded in Revision 3. It was "V1 freeze wording".)
   - *Merges*: S8 Q3 (S8 escalation D22), and human direction (Revision 3: `release/v1`, with any V1 update allowed). It restates D2's disposition (T17).
   - *Recommendation*:
     - Revision 2's "security fixes only" is withdrawn. V1 ships from `release/v1`, cut at the V1 branch-point tag, and any V1 update is allowed there.
     - Before the cut, V1 releases still come from `main`: v0.12.0 first (Stage 0 step 0), then the v0.12.x patch (step 3: the security fixes, plus the export-fidelity fix under D27). From the cut on, they come from `release/v1`.
     - On `main`, V1-only surfaces get no new work. They keep working until α exit and are then deleted.
     - The code-level retire list is never refactored on `main`: `uds/hook.rs`, the UDS transport, `tokio_main_stdio`, `uds/mcp_listener.rs`, the legacy token, `tokens.json` and the client bundle. It is baselined as frozen and deleted at α exit, together with its baseline entries.
     - A security fix that grows a retire-list file on `main` may raise that file's baseline, with the issue linked. This is the only pre-authorized raise. Any other raise is a human decision (D21).
     - Shared code is not frozen: it is V2 work on `main`. Extracting shared code out of a V1-only module is also V2 work. For example, `dispatch_request` leaving `uds/` is V2 work, and G16 retirement depends on it.
     - **Porting rule.**
       - Fix shared code on `main` first. Backport to `release/v1` only when a V1 user needs the fix.
       - Make V1-only-surface changes on `release/v1` only. They are not forward-ported, because `main` deletes those surfaces.
       - Cherry-picks apply until W1/W2 move the code. After that, each port is a manual re-application with its own PR and CI run.
       - Count ports per session; they are outside KC4.
     - **Export format** (T20): fixed at the branch-point tag, on both branches. On `main`, `export.rs` gets test extraction only. A V1 update that changes the schema or the export format must also extend the V2 importer. Under D27 (a), the format carries every audit column.
     - Cut the branch-point tag before the refactor waves (Stage 0 step 4).
   - *Authority class*:
     - H4: it accepts V1-only-surface gaps and the retire list's size violations on `main` until deletion, and pre-authorizes one class of baseline raise.
     - H2 for the porting and export-format rules, because they bound what V1 updates may do.
     - Allowing V1 updates is the human's own decision (Revision 3) and is not reopened.
   - *Gates*: the V1 cut (Stage 0 step 4); the Stage 0 refactor waves (W1–W2); every backport.
6. **D27 — V1 export fidelity before the V1 cut.** (Revision 4, S9)
   - *Merges*: S9 escalation D27; T20; the Stage 0 order, including v0.12.0 as step 0.
   - *Context*: V1 export and import carry 8 of the 12 audit columns, dropping `credential_type`, `capability_used`, `agent_attribution` and `metadata` (S9 inventory rows 26–27). The branch-point tag fixes the export format on both branches (T20).
   - *Recommendation*: Option (a).
     - Fix export and import to carry every audit column, on `main`, in Stage 0 step 3: one small session [E], shipped in the v0.12.x patch with the security fixes.
     - v0.12.0 (step 0) ships unchanged. The branch-point tag then fixes a 12-column format, so T20's rule stands.
     - Exports from earlier releases still import, with the four columns NULL and the gap stated in the receipt (R30).
     - Option (b) accepts the loss: the tag fixes the 8-column format, and every receipt declares it.
     - S9 pairs this with restarting the running V1 daemon. That is an operational action, not a decision (see "Actions that are not human decisions").
   - *Authority class*: H2 (scope: a V1 change before the cut). Option (b) would be H4 (accepting a permanent gap).
   - *Gates*: Stage 0 step 3 (the v0.12.x patch's content) and step 4 (the V1 cut, which fixes the format).
7. **D3 — Durability, availability and scale posture.**
   - *Merges*: S2 E4 and the architect's E4 addendum; architect review 2 §3–§4 (cell sizing, the host-failure path).
   - *Recommendation*:
     - Run α and β on a single cell with no HA.
     - RPO is at most 60 s with Litestream, or the snapshot interval with the binary-only fallback. Accept that a box failure loses acknowledged writes.
     - State RTO as restore + fleet re-embed until Stage 1 lands C1.
     - Treat 1,000 tenants as a horizon, not a launch target.
     - Decide on HA (a warm standby) before GA, together with the second production cell (D29).
     - Size the cells per architect review 2 §3: one 16 vCPU/64 GB cell while NLI runs in the cell, then standard 8 vCPU/32 GB cells, added rather than grown (D30).
     - The host-failure path depends on D28. On Hetzner, a reattachable network volume could give RPO ≈ 0 on host failure, pending a WAL-latency PoC; Fly volumes are host-pinned.
   - *Authority class*: H4 (accepts a gap).
   - *Gates*: Stage 1 priority (C1); the Stage 3 backup choice; GA.
8. **D24 — Audit classes and failure semantics.** (Revision 4, S9)
   - *Merges*: S9 escalation D24; S9 Q2; restated P2.
   - *Recommendation*: Adopt S9's three classes, and reject fire-and-forget for all of them.
     - **A, state change**: every committed change to authoritative state, in a project store or the control plane, by a client, a background job or an operator. It is written in the same transaction; if the audit insert fails, the operation fails.
     - **B, security event with no mutation**: authentication failures, authorization and slug denials, rejected attestations, and rate and content rejects. It is written awaited, before the response, with a bounded timeout. On failure the request is still denied, with an ERROR log, a metric and a gap marker in the next row that lands. Floods are aggregated per principal and time window.
     - **C, access**: successful reads, search and briefing. It goes through a bounded queue with batched commits, drained at shutdown. Drops are counted and recorded as `audit_dropped` rows, so the log is complete or evidently incomplete.
     - Not audited: derived and learned state (S9 inventory row 31). For edges, class A covers agent-asserted edges and every edge delete or redirect; machine-derived edge inserts are derived state, marked by origin (P4).
     - Mechanics (G18): rowid assignment instead of the counter allocator, a per-store hash chain, a monotonic-insert trigger, store-crate `*_with_audit` methods (C6), and a control-plane anchor (D26).
     - Cost [M]: faster than today, at +34% single-writer throughput and +26% with 8 writers, because one commit replaces two. The chain adds about 61 µs p50.
     - Accept R28: an audit defect becomes a visible write outage instead of silent loss. Reads keep working (T22).
     - On `release/v1`, V1 audit stays as it is. A site-by-site retry cannot close the commit-then-audit window, so G18 is not backported (R29).
   - *Authority class*: H1 (security posture); H2 (scope: Stage 1 gains G18); H4 (V1 audit stays as it is on `release/v1`).
   - *Gates*: Stage 1 (G18); P2's verdict; α.
9. **D4 — Authorization server: buy, and which vendor.**
   - *Merges*: S1 E1.
   - *Recommendation*:
     - Buy a hosted AS that supports CIMD, DCR and GitHub login.
     - Self-hosted Keycloak would make identity infrastructure something the operator must run, which goes against restated P6.
     - Pick the vendor after Stage 0's pricing and live claude.ai test.
     - *Related recommendation, not a decision* (Revision 3; the human leans yes): make the npm client the OAuth client, for MCP and for the hook/observe channel.
       - It is a public client: loopback redirect plus PKCE, with the device flow (RFC 8628) on headless boxes.
       - It keeps the refresh token in the OS keychain or a 0600 file, and serves the MCP bridge and observe from that one credential (S1 Q5).
       - This adds AS criteria for the Stage 0 evaluation: registration for the npm client (CIMD or DCR), the device-authorization grant, refresh-token rotation and resource indicators (RFC 8707).
       - If the answer is no, harness-native OAuth covers MCP only, and the hook channel needs a second credential and a second login (S1).
   - *Authority class*: H1 (security posture).
   - *Gates*: Stage 3 (OAuth, onboarding, the claude.ai connector, deriving the client class from `client_id`, and the V2 npm client's auth design).
10. **D21 — File-size limit policy and ratchet.** (Revision 2, S8)
    - *Merges*: S8 Q4 (S8 escalation D21).
    - *Recommendation*: Adopt S8's rules R1–R5, held by a ratchet.
      - R1: tests live out of line. New inline test modules are banned; existing ones are baselined.
      - R2: production files at most 500 raw lines (`wc -l`); split proactively at 450. Keep 500, because 252 of 289 production files already comply; the old rule failed for lack of a ratchet, not because of its number.
      - R3: test files at most 1,000 raw lines.
      - R4: production functions at most 150 code lines, via `clippy::too_many_lines`. The 36 existing violators get a counted `allow`. Ratchet toward 100 later.
      - R5: exemptions only through the committed baseline.
        - Baselined files may not grow, and stale entries fail.
        - `--tighten` only lowers numbers.
        - Any raise needs an issue carrying a human decision (the #5372 rule).
      - Enforce it with an in-repo script that needs no toolchain. It runs as a job in the ported CI (Stage 0 step 5; D23), in the pre-PR gate, and at Gate 3b in place of the validator's subjective check. The workflow change goes through a GH issue.
      - Synthesis addition (T19): pre-approve, under the D21 issue, the baseline entries that W1 creates for extracted test files. Each is capped at the test lines removed from its parent.
    - *Authority class*: H2 (scope; it redefines a workspace rule). The ci.yml step goes through a GH issue.
    - *Gates*: W0, the first refactor commit.
11. **D5 — V2 vision, goal set and principles.**
    - *Merges*: S5 E4 and this memo's Q4 drafts.
    - *Recommendation*: Approve the drafts as the design baseline. /uni-zero applies them to PRODUCT-VISION.md and the goal entries after approval.
    - *Authority class*: vision change (human-only).
    - *Gates*: the V2 design session.
12. **D6 — ADR reversals that follow from D1 and D2.**
    - *Merges*: new (architect §1); architect review 2 §5 (Revision 4).
    - *Recommendation*: Approve these changes:
      - supersede vnc-038 ADR-007 (#5086, the restart-applied registry);
      - supersede crt-056 ADR-005 (#5168, the serial tick), with the scheduler scoped to the cell's assignment set (C20);
      - supersede crt-023 ADR-001 (#2700: a single NLI `Mutex<Session>` with a dynamic pool floor) and crt-022 ADR-003 (#2574: the contradiction scan as one task on a single rayon pool sized `num_cpus/2`, with a floor of 4). Replace them with separate interactive and background inference lanes, ORT threads set explicitly from the cell's CPU budget, and NLI behind `InferenceBackend` (C21, C23);
      - remove or dev-limit the vnc-038 ADR-006 path (#5087).

      The corrections need no approval; the architect makes them: #4950, #5167, crt-056 C-3/AC-2, the V2-image scoping of #4652, and #84 and #267 (S9). See "Actions that are not human decisions".
    - *Authority class*: H3 (contradicts approved ADRs).
    - *Gates*: Stage 1 for #2700 and #2574; Stage 2; Stage 4 for #5168.
13. **D30 — NLI worker timing.** (Revision 4, architect review 2 E-C)
    - *Merges*: architect review 2 E-C, §2 (rank 3) and C21; R12.
    - *Context*: NLI is off by default (`default_nli_enabled()` returns `false`) and runs in the background only. While its arena (up to 5.2–9.4 GB [M8]) lives in the cell, it sets the minimum cell size: 16 vCPU/64 GB instead of the 8 vCPU/32 GB standard cell.
    - *Recommendation*:
      - Build the `InferenceBackend` seam in Stage 1 in any case (C21), with the local implementation.
      - Cap the ORT arena first. Build the out-of-process NLI worker in Stage 1 only if the cap does not bound NLI within the standard cell's memory budget.
      - Build it unconditionally instead if any project is to run with NLI on by β (the architect's condition). As planned, neither β tenant needs it: NLI is off by default, and deterministic mode switches optional ML off (P5).
      - Go network-remote only when background ML, not serving, sets the cell count (KC1 data). Query-path embedding stays in the cell either way.
    - *Authority class*: H2 (scope: Stage 1 content).
    - *Gates*: Stage 1 (the NLI step after tick fixes 1–3); the standard cell size (D3).
14. **D7 — Repo and licence boundary for V2.**
    - *Merges*: new from this synthesis; S8 Q2 (S8's D7 addendum).
    - *Context*: V1's `personal-cloud` goal placed OAuth, RBAC, multi-tenancy and the control plane in a "separate private repo". The campaign framing made that non-binding.
    - *Recommendation*:
      - Decide before Stage 2 code lands.
      - Default to a single repo unless there is a commercial reason to split. Neither the campaign nor S8 found a technical one. Under restated P6 one binary fills every role, and S8 measured the crate, not the repo, as the build unit.
      - New V2 subsystems are new crates from day one: the control plane, the OAuth resource server and attestation. A later split then stays cheap without first extracting existing modules. C6 is worded per storage crate to allow this (T18).
      - Branching follows D23: one repo, with `main` as V2 and `release/v1` until α exit. V1 does not get a second repo. The npm client stays in this repo (`packages/unimatrix`), on its own version line.
      - Repo visibility (D23b) is a separate question from the repo boundary. Making the whole repo private gives up free runners and npm provenance. Splitting the new crates into a private repo would keep both for the public one.
    - *Authority class*: H2 (scope).
    - *Gates*: Stage 2 (the control plane is the first new crate).
15. **D28 — Hosting platform.** (Revision 4, architect review 2 E-A)
    - *Merges*: architect review 2 E-A and §4. Decide it with D16, because the region is a location on the chosen platform; it also sets D3's host-failure path.
    - *Recommendation*: self-managed Hetzner Cloud VMs (CCX class) running the cell container, with a managed load balancer or Caddy at the edge.
      - It is the cheapest, and cost dominates as cells grow. Fly costs about 3× the compute: 8 vCPU/32 GB is about $360/mo [D, from [P] prices], against €118 [P].
      - Local NVMe matches the single-writer design. Hetzner Cloud Volumes reattach within a location, which could give a host-failure path with RPO ≈ 0: detach, attach to a replacement, flip routing. A PoC must first measure SQLite WAL latency on a network volume against local NVMe.
      - The operator owns OS patching, the firewall, the TLS edge, deploys and private networking: about a day to set up, then routine.
      - Alternative: Fly Machines, if the human prefers not to own OS, TLS and networking, at about 3× the compute (€120–250/mo more at one cell, growing linearly). Fly volumes are host-pinned, so a host failure means a restore elsewhere. `fly-replay` needs a PoC for SSE and for bodies over 1 MB, or the router proxies over Fly's private network.
      - Rejected for V2: Kubernetes (placement is an application concern; revisit beyond about 10 cells), and Cloud Run or other stateless serverless for cells (no durable local disk, no single-writer guarantee). Serverless fits only the stateless ML worker, later.
      - Build the cell image platform-neutral either way (C27).
    - *Authority class*: H2 (scope: a strategic dependency and a recurring cost).
    - *Gates*: the first hosted deploy (Stage 3, C27); D16; D3's host-failure path.
16. **D16 — Launch region and residency.**
    - *Merges*: S2 E2. Decided with D28 (architect review 2 E-A).
    - *Recommendation*:
      - Launch in one region, with `region` on the routing map from day one.
      - Residency becomes a later capability.
      - Shared spaces never span regions.
      - Which region to launch in is the human's call. It is a location on the D28 platform: a Hetzner location or a Fly region.
    - *Authority class*: H2 (scope).
    - *Gates*: the first cell (Stage 3); GA.
17. **D26 — Audit anchor strength.** (Revision 4, S9)
    - *Merges*: S9 escalation D26; R11; KI-CHAIN-XV-STRONG (S2 out-of-scope discovery 9; S3 F2).
    - *Options*: anchor audit chain heads in the control plane only, which detects everything except an operator who controls both; or also give tenants the heads, in export and migration receipts, which detects operator rewrites.
    - *Recommendation*: both, in steps.
      - Control-plane anchors from Stage 2, at an interval no longer than the backup RPO, so a truncation is detectable.
      - Tenant-visible audit heads in migration receipts from Stage 3, and in export receipts by GA.
      - Continuous tenant-visible anchoring after GA, with KI-CHAIN-XV-STRONG.
    - *Authority class*: H1 (security posture: what a tenant can verify without trusting the operator).
    - *Gates*: Stage 2 (the anchoring saga); Stage 3 (the receipt format); GA.
18. **D8 — Learning as a per-store mode.**
    - *Merges*: S4 E1 (SL-OPTOUT #1015/#5796), S3 E2.
    - *Recommendation*:
      - `learning_mode ∈ {adaptive (default), curated, deterministic}`.
      - Curated is fixed for shared spaces.
      - Deterministic is opt-in per project, under the fingerprint contract.
      - Restate the self-learning goal to match.
      - Shared spaces need curated mode whatever is decided about deterministic mode (T9).
    - *Authority class*: vision change (SL-OPTOUT is a pending human decision).
    - *Gates*: Stage 4 (β), Stage 6.
19. **D9 — Retraction authority.**
    - *Merges*: S4 E3, and the tension with S1 (T4).
    - *Recommendation*: Option (a).
      - The Owner sets the PCD policy `deprecated_visibility: exclude`.
      - A structured `retracted` deprecation reason needs only Write.
      - Quarantine and Admin stay human-only.
    - *Authority class*: H1 (security posture).
    - *Gates*: Stage 4. This is Med Search's only outright blocker (M3).
20. **D10 — Tenant shapes.**
    - *Merges*: S1 E2.
    - *Recommendation*:
      - Auto-create a personal tenant at signup.
      - Add org tenants by invite later.
      - Give Med Search's research corpus and public chat their own tenant (T13).
    - *Authority class*: H2 (scope).
    - *Gates*: Stage 3 onboarding; Stage 4.
21. **D11 — Public-chat boundary.**
    - *Merges*: S1 E4, S4 E4, S3 E5.
    - *Recommendation*: Confirm five rules:
      - End users never receive a Unimatrix credential.
      - Med Search's backend holds a K5 `read` service token.
      - No end-user identifier is passed to Unimatrix.
      - K5's learning contribution is fixed at `none`.
      - Shared content reaches K4/K5 clients only when an Owner opts a grant in.
    - *Authority class*: H1 (security posture).
    - *Gates*: Stage 4.
22. **D25 — Access-audit completeness for SaaS tenants.** (Revision 4, S9)
    - *Merges*: S9 escalation D25.
    - *Question*: is best-effort, loss-counted access audit (class C) acceptable for every tenant, including Med Search? Or must some tenants get complete, synchronous read audit, at about class B's cost on every read?
    - *Recommendation*: class C by default for every tenant, with a per-tenant upgrade to synchronous read audit.
      - A separate awaited audit commit measured about 200 µs p50 [D, from S9's 478 against 279 µs]. That is small against a 12 ms search p50 [M10], but it puts every read on the project's single write connection.
      - Decide it with D11 (the public-chat boundary) and D15 (retention).
    - *Authority class*: H1 (security posture: what "complete" means to a tenant).
    - *Gates*: the Stage 2 class-C design; β (Med Search).
23. **D12 — Hypothesis H3: harness strategy.**
    - *Merges*: S6 E1, the S1 H3 note, S5 E2 (its `.well-known/opencode` part).
    - *Recommendation*:
      - Reject H3 as framed and keep harness ubiquity.
      - Use one attestation contract that Unimatrix owns.
      - Make Claude Code and OpenCode the first T2 adapters. Pin OpenCode to a version range and verify it in CI.
      - No V2 `done_when` may depend on OpenCode alone.
      - Do not serve `.well-known/opencode` from the SaaS.
      - If OpenCode-first is wanted for reasons other than identity, such as local models, state that explicitly; the campaign found no identity reason.
    - *Authority class*: H2 (scope), because hypothesis H3 is the human's own. Adopting H3 as framed would also be H5 (drops harness support).
    - *Gates*: Stage 5.
24. **D13 — Required identity tier, claims and definition provenance.**
    - *Merges*: S1 E3, S6 E2, S6 E3.
    - *Recommendation*:
      - Per-agent-type policy runs at T2 in V2; it is off by default, which behaves like T1.
      - Word claims as "harness-attested (tiered)". Never claim "non-forgeable" before a validated T3.
      - Build T3 Claude Code-first, after GA.
      - Unimatrix pins definition digests without hosting definitions (T8).
      - Amend PL-4's `done_when` to: "absent ⇒ T1, never auto-enrolled; invalid ⇒ rejected before context build".
    - *Authority class*: H1 (security posture), H7 (redefines PL-4's `done_when`).
    - *Gates*: Stage 5; the `trusted-identity` goal text.
25. **D14 — Hypothesis H4: shared scope.**
    - *Merges*: S3 E1, S3 E4.
    - *Recommendation*:
      - V2 initial ships intra-tenant shared spaces only.
      - No cross-tenant sharing, writes or proposals.
      - Contributors are invited into the owning tenant.
    - *Authority class*: H2 (scope).
    - *Gates*: Stage 6.
26. **D15 — Data governance.**
    - *Merges*: S2 E3, S3 E3; S9 (audit retention, Revision 4).
    - *Recommendation*:
      - Publish a backup-retention window (for example 30 days) as the erasure SLA, or crypto-shred each project with keys kept outside the DB (C7).
      - No per-record erasure in live projects (P1, P2).
      - No personal data in knowledge or audit beyond pseudonymous ids.
      - Forks survive grant revocation.
      - Eval on tenant data (Revision 3; R20): before β, α or synthetic data only (human direction). From β, no tenant snapshot is used for eval without the tenant's consent.
      - Audit retention (S9): keep audit for the life of the project, with no time-based GC, at least until GA. Audit leaves only with a whole-project purge, recorded as class A in the control-plane log. Publish the period at GA; sealed archival of cold segments comes after GA.
      - Get legal input before GA.
    - *Authority class*: H4 (accepts a gap), plus legal input.
    - *Gates*: GA, including the published audit retention; tenant-data eval from β (R20).
27. **D29 — Per-cell blast-radius cap.** (Revision 4, architect review 2 E-B)
    - *Merges*: architect review 2 E-B, §3 and C28.
    - *Recommendation*:
      - Without HA (D3), one cell failure is a full outage for every tenant on it. Cap each cell in placement policy by tenants and tenant class, not only by memory. The architect proposes at most about 100 tenants, or one "large" tenant class, per cell at GA [E, policy input]; the human sets the number.
      - Make the second production cell mandatory at GA, decided with D3's HA posture. At β the only tenants are the human's own and Med Search, so one cell stays acceptable through β (D3).
      - CI runs two cells from Stage 2 regardless (C16), so adding the second cell is a deployment change.
      - Cost: under a 100-tenant cap, 1,000 tenants need at least 10 cells (T24). A cap by revenue share, or a higher number, lowers that.
    - *Authority class*: H2 (scope: placement policy and its cost).
    - *Gates*: GA (C28).
28. **D17 — Default learning contribution per client class.**
    - *Merges*: S4 E2.
    - *Recommendation*: K1 `full`, K2 `usage`, K3 `explicit`, K4 and K5 `none`. K5 is not configurable.
    - *Authority class*: H2 (scope).
    - *Gates*: Stage 3.
29. **D18 — Taxonomy and configuration authority.**
    - *Merges*: S5 E3, T6.
    - *Recommendation*:
      - Agents can discover the taxonomy but not change it (DA1).
      - Only a human Owner applies a PCD version.
      - No surface for agent proposals at launch.
      - CI may upload staged PCD versions for the Owner to apply.
    - *Authority class*: H1 (security posture).
    - *Gates*: Stage 3 (PCD).
30. **D19 — Hypothesis H6: engine boundary.**
    - *Merges*: S5 E1, S5 E2 (its hosting part).
    - *Recommendation*:
      - Confirm: no transient session-memory class, and no hosting of harness definitions.
      - Approve the two small items: cycle-scoped `proposed` knowledge, and harness-artifact references with `harness_release` recorded on cycle start.
      - Re-open session memory only for a concrete example that is neither coordination state nor provisional knowledge.
    - *Authority class*: H2 (scope).
    - *Gates*: nothing launch-blocking.

---

## Actions that are not human decisions

| Action | Owner | Source |
|---|---|---|
| Correct vnc-034 ADR-003 (#4950): replace the tick-rebuilt clause with residency and build-on-load, and recut `adapter_for`. Architect review 2 adds three points: drop "bounded by single-operator N"; the resolver resolves only projects assigned to this cell, and an unassigned project returns a new `RouteError::Misdirected` (the router re-resolves), not `UnknownProject`; model memory is per cell or per worker pool, never per project | uni-architect (no approval needed) | architect §1; architect review 2 §5 |
| Correct crt-056 ADR-004 (#5167): the claim "Step B needs ZERO work-unit changes" is false, and the scheduler scope is the cell's assignment set (C20) | uni-architect | architect §1; architect review 2 §5 |
| Supersede #5086, #5168, #2700 and #2574, and retire the #5087 path | uni-architect, after D6 | architect §1; architect review 2 §5 |
| Restate crt-056 C-3/AC-2 ("one loaded NLI model per process") as per cell or per worker pool | uni-architect | architect review 2 §5 |
| Scope nan-015 ADR-003 (#4652) and pattern #4658 (no baked model, first-boot download) to the OSS image; the V2 cell image bakes its models in or pulls them hash-verified (C27). vnc-034 ADR-007 (#5135) likewise stays for the OSS image, and the V2 image takes a role env | uni-architect, in the V2 design session | architect review 2 §5 |
| Correct vnc-002 ADR-001 (#84) and vnc-006 ADR-003 (#267). They record a combined write-and-audit transaction, which the code replaced with fire-and-forget writes (#302, #308) without a superseding ADR. G18 restores their design | uni-architect (a correction; no approval needed) | S9 out-of-scope 1 |
| Write ADRs for C6, C7, C8, C9, C13, C14, C15 and C16–C27, and for G18's audit design once D24 is decided. C6 (no SQL outside the storage crates) uses S8's production counts: 100 server sites in 21 files and 413 store sites (T15, T18). C28 is placement policy, set by D29 | uni-architect, in the V2 design session | architect §6; architect review 2 §6; S8; S9 |
| Add a CI gate that bans `ATTACH` and multi-database SQL | GH issue (ci.yml changes go through an issue) | S2 reversibility; S3 |
| Restate DA1 design-lock (1) as "immutable until next apply" | /uni-zero (capability owner) | S5 out-of-scope |
| Amend PL-4's `done_when` | /uni-zero, after D13 (authority class H7) | S1 Q4 |
| Add capability clusters for `hosted-service`; restate capabilities C0–C2, C4, C8, C10–C12 and C15 for SaaS; move capability C6 to the PCD. At α exit, retire capabilities C7, C9, C16 and C17 | /uni-zero, after D5 | Q4 |
| Re-open KI-AUDIT (#5546), now `delivery:proven`: its `done_when` ("each mutation writes an attributed row") is not met (S9 inventory rows 1–15) | /uni-zero (capability owner) | S9 out-of-scope 2 |
| Specify the invalid-attestation rejection as an MCP-level error, not HTTP 401, with the adapter's single unattested retry | uni-architect, in Stage 5 design | T2 |
| File the GH issue for the CI port (D23 items 1–5) and the one-time pre-cut change to the V1 release workflow (D23 item 7) | GH issue (workflow changes go through an issue), after D23 | D23; human direction |
| File the GH issue for the CI Rust file-size ratchet, using S8's draft issue text, as a job in the ported CI | GH issue (ci.yml changes go through an issue), after D21 | S8 Q4 |
| Restate the size line in `rust-workspace.md`, and point `uni-validator.md:109` and `uni-rust-dev.md` at the script | Rule-file owner, after D21 | S8 Q4 |
| Record in the G16 retirement design that `dispatch_request` serves HTTP `/observe`, so `uds/` is never deleted wholesale | uni-architect, with the Stage 0 W2 split | S8 out-of-scope |
| Once the CI port lands, correct #5919 ("never report missing cargo-test CI as a gap") and the `.cargo/config.toml` comment that calls GH CI JS-client-only | The CI-port delivery's retro (/uni-retro) for #5919; the CI issue for the comment | Revision 3 grounding |
| Restart the running V1 daemon on a current binary, now | The operator (the human); operational, no decision | S9 out-of-scope 3 |
| File the GH issue for the two-cell CI jobs (C16) when Stage 2 starts | GH issue (workflow changes go through an issue) | C16; D23 |

---

## V1 defects and disposition

The dispositions follow from the Q2 answer:
- Security defects get a bugfix on `main` before the V1 cut, shipped as a v0.12.x patch (Stage 0 step 3), so both lines carry it. v0.12.0 ships before them (R27).
- Defects in shared code are fixed on `main` as V2 stage work. They reach V1 users only by backport to `release/v1` (D22).
- Defects in V1-only surfaces that are not security issues are accepted on `main` until retirement; `release/v1` may fix them. Accepting them on `main` is an accepted-gap item (authority class H4), bundled into D2.
- Audit defects (rows 26–31, from S9) are fixed on `main` by G18 in Stage 1 and the class-B writer in Stage 2. On `release/v1`, V1 audit stays as it is (D24), apart from the export fix under D27.

| # | Defect | Source | V1 impact today | Disposition |
|---|---|---|---|---|
| 1 | Under the legacy token or over stdio, passing `agent_id:"human"` grants Admin | S1 ground truth; G6 | Any LLM, or a prompt injection, on stdio or the legacy token can quarantine or enroll | **Bugfix on `main` before the V1 cut, shipped as a v0.12.x patch** (Stage 0 step 3; v0.12.0 ships without it, R27). A narrow fix; the bugfix session picks the mechanism. The structural fix is G6, in Stage 3 |
| 2 | `context_cycle_review` has no `require_cap` | S4 out-of-scope | Any authenticated caller, including a `read` token, can trigger cycle-review writes and the transcript purge | **Bugfix on `main` before the V1 cut, shipped as a v0.12.x patch** (Stage 0 step 3; v0.12.0 ships without it, R27) |
| 3 | Per-slug `AdaptationService` state is never persisted in multi-project HTTP mode | architect out-of-scope 1; S2 out-of-scope 11 | After each restart, query embeddings drift from the indexed vectors. The size of the drift is unknown | Stage 1: measure it, then persist the state (durable, per C2) or switch to identity adaptation. The re-embed at migration clears past drift |
| 4 | Per-slug HNSW is dumped only on graceful shutdown | architect out-of-scope 2; S2 out-of-scope 12 | A crash loses every vector added since the last clean stop, until heal re-embeds them | Stage 1 (C1). Accepted until then (authority class H4, in D2) |
| 5 | Graph-enrichment source S8 (S4's name, not spike S8) reads `audit_log`. It is dead in production, and "fixing" its operation filter would leak read-token traffic into CoAccess edges | S4 out-of-scope; LI-3 | None today, because it is dead | Stage 1: delete it, or re-source it from `query_log`. Never fix the filter |
| 6 | Tool-call audit rows hard-code `credential_type:"none"`; the authenticated principal appears only on `http_access` rows | S1 out-of-scope; G9 | ass-060 invariant 4 is not met | Stage 1 (G9). V1 rows stay as recorded |
| 7 | The `agent_attribution` doc comment calls `clientInfo.name` "Non-spoofable" | S1 out-of-scope | Could mislead future code into treating it as an authorization input | Bundle with #1 (documentation only) |
| 8 | The contradiction scan re-embeds the whole corpus at boot and every 4th tick, taking 83–89 s at ~15 cores. Its output feeds status only | S2 out-of-scope 1; architect §2.4 | CPU burn. Search p50 is about 4× slower while it runs [M11, with caveat] | Stage 1, fix 1 |
| 9 | Graph-enrichment source S1 (S2's name, not spike S1) takes 28 s single-core per tick and writes 0 edges | S2 out-of-scope 2 | Wasted compute | Stage 1, fix 2 |
| 10 | The NLI arena grows from 0.5 to 5.2–9.4 GB and is not released | S2 out-of-scope 3 | Memory pressure. The live daemon runs at 1.33 GB [M9] | Stage 1: cap it, or move NLI to a worker |
| 11 | Compaction never runs in multi-project mode; the production store is 67% free pages | S2 out-of-scope 6 | Disk and backup size about 3× what they should be | Stage 1. Export/import also compacts each migrated store |
| 12 | The hook-path learning sinks have no predicate; only the observe route's 403 protects them | S4 out-of-scope; LI-2 | Safe for now, while that route is the only entry | Stage 1, before #998 adds a second hook entry |
| 13 | The rate limit is keyed on the self-asserted `agent_id` | S4 out-of-scope | Bypassed by rotating `agent_id` | Stage 3 per-principal quotas. Accepted in V1 |
| 14 | Slug reuse: deleting a project and re-registering its slug hands stale clients' writes to the new store | S1 out-of-scope; R7 | Minor with one operator | Stage 2 (immutable ids, tombstoned slugs). Accepted in V1 |
| 15 | The tick runs continuously once a pass takes longer than the interval | S2 out-of-scope 5 | Only at around 17 or more heavy projects | Stage 4 scheduler |
| 16 | `content_hash` covers only `title: content` | S3 out-of-scope; F2 | `chain_verify` cannot detect a change of status, category or tags | Integrity north-star (a store-level head over id, hash and status). Migration uses the re-export diff in the meantime |
| 17 | MicroLoRA training is unreachable in production | S4 out-of-scope | Dead code | V2 design decision under C13: remove it, or wire it per project |
| 18 | The dogfood OpenCode v2 plugin has drifted from 2.4.1: MCP tool naming, `callID`/`output`, and the missing `fork.parentID` | S6 out-of-scope | The cycle intercept never fires | #998 (Stage 5, or earlier if the human dogfoods OpenCode) |
| 19 | The repo's `opencode.json` sets `edit: allow`, and agent definitions hot-reload, so the LLM can mint agent types | S6 out-of-scope; L1 | Harmless while identity is not enforced | Stage 5 prerequisite |
| 20 | `phase_explicit` gives entries absent from a populated bucket the term's maximum value, 1.0 | S3 out-of-scope | Possible ranking bias today | Check it with the eval harness before Stage 6 reuses the value. No fix is assumed |
| 21 | Search audit `target_ids` are bare integers, and the id deserializer accepts numeric strings | S3 out-of-scope | None until fan-out exists | Stage 2 (wire format) and Stage 6 (SS-R4) |
| 22 | 42% of the production corpus is quarantined and never purged | S2 out-of-scope 10 | Storage cost | A purge lifecycle after GA (RETAIN) |
| 23 | The briefing goal text says "phase-boundary handoff", but the code conditions on the goal only | S5 out-of-scope | Vision drift | Fixed in the V2 goal text (Q4) |
| 24 | The size rule has no ratchet. 37 production files exceed 500 lines (48.8k lines, 50.5% of production code; 138 files by raw count), and nine functions exceed 500 code lines (`run_main_migrations` is 1,161). Gates apply the rule to new files only | S8 measurement basis | Review and agent-context cost. Every schema change lengthens `run_main_migrations` | Stage 0: the W0 ratchet in the ported CI, then W1 and W2 in T16's order; then pre-split at first touch (D20, D21, D23) |
| 25 | `dispatch_request` lives under `uds/` but also serves HTTP `/observe` | S8 out-of-scope | Deleting `uds/` at G16 would break `/observe` | Stage 0 W2: move it into a transport-neutral hook-dispatch module before G16 retirement |
| 26 | Audit rows for state changes are written after the change commits, by a detached task in their own transaction; errors are swallowed or only logged | S9 inventory rows 1 and 4–9 | A committed change can lack its audit row, and the silent sites leave no log | Stage 1 G18 on `main`. Accepted on `release/v1` (D24, R29) |
| 27 | Some state changes write no audit row: agent edge writes (including edges declared at store time), agent auto-enrolment, and three background mutators (auto-extraction, dead-knowledge deprecation, orphaned-edge compaction) | S9 inventory rows 3, 10, 11 and 13–15 | Changes to the graph, the registry and entry status that cannot be attributed | Stage 1 G18 adds their rows on `main` |
| 28 | Security events write no audit row: failed bearer authentication, MCP capability denials, content and rate rejects, and denials for a principal with no home slug | S9 inventory rows 21–24 | Probing is invisible | Stage 2: the class-B writer, with G10 |
| 29 | The live store's audit log has a six-month hole | S9 live evidence; S9 out-of-scope 3 | No audit trail for that period. The hole is permanent | The operational restart in the actions table; declared in migration receipts (R30) |
| 30 | Export and import carry 8 of 12 audit columns. Import provenance is written after COMMIT and attributed to `"system"` | S9 inventory rows 26–27; S9 out-of-scope 5 | Attribution is lost on every export and restore. A crash between COMMIT and the provenance write leaves an import with no provenance row, and #5714's C-5 then refuses a re-import into that target. The export itself writes no audit row | D27 (a) before the V1 cut, for the columns. The V2 importer writes its provenance row inside the import transaction, attributed to the operator (Stage 3; T23). The V2 export is a class-B event in the control plane (Stage 2) |
| 31 | The audit-log triggers block UPDATE and DELETE only: an actor with raw database access can still overwrite or backfill rows, and nothing detects it | S9 inventory row 30 | Tampering by anyone with raw DB access goes undetected | Stage 1 G18 (monotonic-insert trigger, hash chain) and anchors (D26). Accepted on `release/v1` (R11) |

---

## Unanswered Questions

Merged and de-duplicated across S1–S6, S8, S9 and both architect reviews, plus the gaps this synthesis found.

**Needs a follow-on spike**
1. **Hosted AS evaluation.** Vendor pricing; support for CIMD and DCR; claude.ai's live behaviour against a CIMD-capable AS; whether the OAuth `client_id` (CIMD or DCR) can carry the client class (S1 E1 and its unanswered questions; S4 unanswered questions). Gates D4. Run it as a Stage 0 spike.
2. **Litestream checkpoint compatibility.** Whether `wal_checkpoint(TRUNCATE)` works against the read transaction Litestream holds (S2 unanswered question 5; C12). S9's claim that every restored replica is transaction-consistent under same-transaction audit also rests on it. Run it as a Stage 0 PoC; the libSQL/Turso fallback becomes its own spike under KC3.
3. **Cross-host ONNX reproducibility for deterministic mode** (S3 and S4 unanswered questions). Needs a spike before Stage 4 claims the deterministic contract.
4. **T3 confinement efficacy.** Whether Claude Code's sandbox (`denyRead` plus an egress allowlist) confines a shell-capable LLM while the hook keeps working (S1 and S6 unanswered questions). A post-GA spike. It gates any "non-forgeable" claim.
5. **Codex per-call agent identity**, and whether `updatedInput` applies to MCP tools (S1 and S6 unanswered questions). A small live check. T1 is acceptable meanwhile.
6. **Network-volume SQLite latency.** SQLite WAL latency on a Hetzner Cloud Volume against local NVMe. It gates D28's reattachable-volume host-failure path (RPO ≈ 0). If D28 picks Fly, PoC `fly-replay` with SSE and with bodies over 1 MB instead (architect review 2 §4).

**Measure at delivery** (these become acceptance criteria)

| # | Measurement | Source | Stage |
|---|---|---|---|
| 7 | Post-fix tick cost | S2 unanswered question 3 | Stage 1 (KC1) |
| 8 | Adaptation-drift magnitude | architect out-of-scope 1 | Stage 1 |
| 9 | Realistic light-tenant tick cost, and exact-kNN cost at Med Search scale | S2 unanswered question 2; S4 | Stage 4, on the Med Search corpus |
| 10 | Warm-project RSS plateau under long uptime, and resident projects per standard cell (20–30 MB per warm heavy project [E]) | S2 unanswered question 4; architect review 2 §3 | Stage 4 (residency budget) |
| 11 | Fair-share bound, in-process over HTTP, with the inference lanes in place | C11, C23; M10/M11 caveat | Stage 4 |
| 12 | PCD hot-apply cost under N tenants | S5 unanswered question; T5 | Stage 3 |
| 13 | Attestation over remote streamable HTTP with rmcp 3.x; OpenCode's `tool.transform` carrier alternative | S1, S6 | Stage 5 |
| 14 | Per-call latency of the Claude Code `PreToolUse` hook's Node spawn; auto mode dropping `updatedInput` | S6 | Stage 5 |
| 15 | Call-id cache sizing | S1 G17 | Stage 5 |
| 16 | Fan-out latency; NLI-over-union cost; home vs curated confidence calibration; whether `utility_delta(None)` is a neutral value | S3 | Stage 6 |
| 17 | Refactor rate per session, re-derived from the first two W1/W2 sessions (today it rests on one sample, vnc-008) | S8 unanswered question 1 | Stage 0 (KC4) |
| 18 | Visibility widening (`pub(super)`, `pub(crate)`) that the W2 source splits need | S8 unanswered question 2 | Stage 0 (W2) |
| 19 | CI port effort, the flaky-test count, and Rust CI minutes per tier on the chosen runners (the input to D23b's cost); from Stage 2, the two-cell suite's runtime and memory | Revision 3 (this synthesis); C16 | Stage 0 (KC4, D23); Stage 2 |
| 20 | Whether the ORT arena cap bounds NLI within the standard cell's memory budget | architect review 2 §2; D30 | Stage 1 |

**Decided in a design session**

21. Whether `run_maintenance` and `run_graph_inference_tick` are decomposed before or with their tick fixes. It depends on how their tests isolate phases (S8 unanswered question 4). Decided in the Stage 1 design session.
22. The client compatibility window (how many versions, for how long) and the minimum-version policy (D23). Decided in the V2 design session.
23. Whether the V2 npm client bundles any binary, and how V2's unsupported local N=1 binary ships (D23a; H1). Decided in the V2 design session.
24. Audit and boundary parameters: the class-B timeout and flood window, the class-C flush interval, and the anchor interval, at most the RPO interval (S9); the auth-cache TTL, which is also the revocation bound, and how long cells serve through a control-plane outage (C18). Decided in the Stage 1 and Stage 2 design sessions.

**Needs human or external input**

25. The feature effort for Stages 1–3, now including G18 and C16–C27. S8 sized only the refactor component; the V2 design session should size the features (KC4).
26. Tenant pricing and per-class quota values (S4 unanswered questions). Needed before GA.
27. The legal posture on erasure, residency and medical-research content (S2 E3). Needed before GA.
28. A concrete session-memory example, if one exists (S5 unanswered questions; D19). Optional supporting evidence: count the coordination failures in `STALLS.md` and retros.

**Deferred with their feature**

29. Cross-tenant placement and cross-cell RPC (S3 unanswered questions). Deferred with cross-tenant sharing. C25 keeps cross-cell fan-out an implementation, not a redesign.
30. Control-plane HA, and its audit cost on Postgres (S2 unanswered question 6; S9). Deferred to the HA trigger (D3; Option C). Router–cell trust is no longer open: a validated `RoutedRequest` (C17) over internal mTLS (C27).

**Closed, so dropped here**
- Networked-database latency: Option B was rejected.
- Shared-scope learning: answered by S3.
- Cross-project coordination semantics: S1 and S6 place everything beyond the attested project claim in the harness.
- Who amends PL-4: D13 and the capability owner.
- scc and dictator-rust baseline support: not needed, because the ratchet is an in-repo script (S8 unanswered question 3).
- Whether V2 ships an npm client: yes. The client formats and sends the hook data the harness produces (human, Revision 3).
- The per-cause split of the live store's audit loss: it cannot be determined, because the silent sites left no log (S9).
- A transactional outbox for audit: redundant inside one SQLite file; `audit_log` plus a watermark serves downstream shipping (S9).
- Off-box durability before acknowledgement: it stays R3 under D3, independent of the audit design (S9).

---

## Out-of-Scope Discoveries

Merged and de-duplicated across S1–S6, S8, S9 and both architect reviews. The defects are already in the table above.

1. The `agent_registry.allowed_categories` and `allowed_topics` columns exist but are not enforced. They are a ready home for agent-type policy (S1).
2. Harness-set `_meta` (`ai.opencode/sessionID`, `claudecode/toolUseId`) is a session correlator the LLM cannot write. It could replace the LLM-supplied `session_id` for observation attribution (S6; SLN1).
3. OpenCode 2.4 defaults MCP tools to Code Mode, naming them `<server>_<tool>` and calling them from JS. Tool descriptions and any name heuristics need to account for this (S6).
4. From Code Mode, `opencode://api/session/current/agent` can switch the session's agent in dev builds. It is closed in releases; treat it as a watch item (S6).
5. OpenCode keeps its v2 service password in a plaintext `service.json`, and this devcontainer runs with `ptrace_scope=0`. Both are further instances of L2, outside Unimatrix (S6).
6. `.collective/` is a working artifact surface with pinned digests, and the natural source of digests for T3 pinning (S5; T8).
7. `Status::Proposed` exists but nothing writes it. It is the cheap enabler for cycle-scoped provisional knowledge (S3; S5).
8. The learning counters live on each `entries` row, so the V1 schema cannot host learning from many readers. Shared learning needs overlay tables (S3).
9. Default confidence includes a wall-clock freshness term (`w_fresh = 0.18`), so "results change only from usage" is never true (S4).
10. ass-063's workflow graph was never built; workflow control moved to the harness instead. This is evidence for the orchestration boundary (S5).
11. Chain-head anchoring matters more in SaaS. Two cheap first steps exist: migration and export receipts, and a reader-side audit of the hash of every served shared entry (S2; S3).
12. Found by this synthesis: an invalid-attestation rejection must not be an HTTP 401, because a 401 is MCP's re-authorization trigger (T2).
13. Found by this synthesis: most of V2's value is independent of tenancy, which makes Stages 1, 4, 5 and 6 largely no-regret (Decision memo).
14. S2's SQL-site count included test code. Production code has 100 server sites in 21 files and 413 store sites (S8). Carried into T15 and the C6 ADR action.
15. Compile cost is per crate. An incremental check costs about 13 s whichever file changes, and unimatrix-server holds 68% of production lines. Splitting the server crate, not splitting files, is the build-time lever. It may deserve its own spike if Stage 2+ compile cycles climb (S8; #3328).
16. The hook-client size gate (vnc-027 ADR-005, #5372) is the in-repo precedent for a cap that only a human can raise. The Rust ratchet cites it (S8).
17. Found by this synthesis: W1's test extraction collides with the ratchet's rule against new entries unless D21 pre-approves the moved entries (T19).
18. Found by this synthesis (Revision 3): two texts go stale when the CI port lands. `.cargo/config.toml` calls its `jobs = 6` cap "local/agent/devcontainer only (GH CI is JS-client-only)", yet the committed cap applies on any runner, and `nan-024-e2e.yml` already builds the server per PR. #5919 says never to report missing cargo-test CI as a gap.
19. Found by this synthesis (Revision 3): `pytest.ini` describes the smoke marker as about 15 tests under 60 s, but the suites carry 37 `pytest.mark.smoke` markers. Size the PR tier from a collected run, not from the comment.
20. Found by this synthesis (Revision 3): `release.yml` publishes to npm with `id-token: write` and an empty `node-auth-token`, which is the trusted-publishing (OIDC) setup. Pattern #4578 still lists an `NPM_TOKEN` secret as required. npm allows up to 10 trusted publishers per package [P], so both release lines can publish without a shared secret.
21. S9: write-pool contention also drops writes outside audit (observations, cycle events, analytics batches) and surfaces agent-registry errors to clients. It is security-relevant until G18 lands; the details stay in S9 (out-of-scope discovery 4).
22. S9: two ADRs (#84, #267) still describe a combined write-and-audit transaction that the code abandoned, and KI-AUDIT (#5546) is marked proven although its `done_when` is not met. Both are in the actions table.
23. S9: the multi-transaction insert paths can leave an entry with no vector mapping and no audit row while returning an error that invites a retry, so duplicates are possible. Stage 1 closes this by committing the entry, its vector and its audit row together (C1 with G18).
24. S9: audit writes bridged with `block_in_place` would panic, unseen, inside a detached task on a current-thread runtime. It is a test-runtime hazard only, and G18 removes the bridge.

**Pattern candidates.** These go to Unimatrix only after a delivery validates them; none is stored here.
- Foreign ids are never digit-only (S3).
- The audit log is never a learning source (S4).
- Background jobs whose output is status-only stay off the ranking tick (S2).
- Tenant-isolation gates are tested in both directions. This one is already lesson #5348.
- A size limit with standing violations needs a no-growth baseline. Without one, validators silently apply it only to new files (S8).
- A required check is never path-filtered at workflow level, because a skipped workflow leaves the check pending. Filter at job level instead (D23).
- An audit row shares its mutation's transaction, its id allocator cannot trail the table, and a failed audit insert fails the write (S9; the six-month silent outage is the evidence).
- CI runs the N=2 topology while production runs N=1, so scale-out seams cannot rot (architect review 2, C16).

**New spikes warranted**:
- AS evaluation (Stage 0; gates D4).
- Litestream PoC (Stage 0; gates the backup choice).
- ONNX reproducibility across hosts (before Stage 4; gates the deterministic contract).
- T3 confinement (after GA; gates any "non-forgeable" claim).
- Network-volume WAL latency PoC (before D28 relies on reattachable volumes).
- A server-crate split for build time, only if Stage 2+ compile cycles climb (S8). Not warranted yet.

---

## Recommendations Summary

*Revision 4 integrates S9 (audit trustworthiness) and architect review 2 (application-layer scale-out). Revision 3 added the CI, release and branching model (D23) from human direction, and its addendum put a V1 minor release (v0.12.0) first in Stage 0. Revision 2 integrated S8 (V1 refactor debt). The verdict is unchanged. This summary refers to defects by row number only.*

- **Q1, feasibility: CONDITIONAL GO.**
  - Build one hosted multi-tenant service on S2 Option A: per-project SQLite + HNSW, a SQLite control plane, project-placed, tenant-affine cells.
  - The application layer scales out the way the data plane shards, conditionally: seven process-global components get boundaries while there is one cell, and CI runs two cells from Stage 2 (architect review 2, C16–C28).
  - Stage it α (the human's projects) → β (Med Search as its own tenant) → GA (external tenants, a separate later decision).
  - Stage 0 opens with a V1 minor release (v0.12.0), which needs no decision. Its CI port needs D23 (CI, release and branching), its V1 patch and cut need D27 (V1 export fidelity), and its refactor track needs D20, D21 and D22. Stage 1 needs D24 (audit classes and failure semantics). α depends on D2 (hypothesis H1, local deployment), D3 (durability posture) and D4 (hosted AS), and its first hosted deploy on D28 (hosting platform) with D16 (region). GA also depends on D15 (data governance), D26 (audit anchors) and D29 (per-cell cap).
  - Kill and re-plan criteria KC1–KC4 apply. KC2 now requires the isolation gate to pass across two cells, with a project moved between them.
- **Cost [E].**
  - Launch fits on one cell, €73–235/mo.
  - With tick fixes 1–3 and change-driven gating, 1,000 tenants cost about €235–470/mo plus at most $193/mo of backup. Without them, about €3,050/mo.
  - Background ML drives the cost, not storage.
  - Cells (architect review 2): one 16 vCPU/64 GB cell while NLI runs in the cell, then standard 8 vCPU/32 GB cells, added rather than grown. Cost is linear in cores, so the totals hold. A per-cell tenant cap (D29) would set the cell count at scale: at least 10 cells for 1,000 tenants (T24).
  - Platform (D28): self-managed Hetzner VMs. Fly would cost about 3× the compute.
  - Option A stands after S8's SQL correction: about 513 production SQL sites, not ~800 (T15).
- **Effort.**
  - Refactor-then-evolve (D20): ≈28–31 split sessions [D] burn down the V2-relevant debt, ≈15–18 of them in Stage 0. A rewrite would be ≈60+ feature-sized deliveries [D].
  - The CI port and green baseline add ≈2–5 sessions [E] ahead of the splits. Ports between `release/v1` and `main` are counted per port.
  - Feature effort for Stages 1–3 and hosted-AS pricing are not estimated. Revision 4 adds unsized scope: the audit write path (G18) and the scale-out constraints C16–C27.
- **Q2, V1: branch, maintain on `release/v1`, retire at α exit.** This is human direction; Revision 2's "freeze, security fixes only" is withdrawn.
  - `main` is V2. `release/v1` is cut at the V1 branch-point tag, and any V1 update is allowed there.
  - v0.12.0 ships first from `main` on the existing release process, without the two known security fixes (V1 defects rows 1–2). The fixes follow from `main` as a v0.12.x patch before the cut, so both lines carry them (R27). If D27 takes option (a), the patch also makes V1 export carry every audit column, so the branch-point tag fixes a lossless format.
  - On `main`, V1-only surfaces get no new work and are deleted at α exit. The code-level retire list (`uds/hook.rs`, the UDS transport, `tokio_main_stdio`) is never refactored.
  - Shared code is V2 work. Fix it on `main` first, and backport only when a V1 user needs it. After the W1/W2 splits, each port is a manual re-application (D22).
  - The export format is fixed at the branch-point tag, on both branches (T20).
  - Operational, no decision needed: restart the running V1 daemon on a current binary (S9).
- **Q3, migration.**
  - Each V1 store becomes one fresh V2 project, by export/import that re-embeds vectors into SQLite.
  - Verify with `chain_verify` on both sides plus a re-export diff. On `main`, `export.rs` source stays untouched after the branch-point tag.
  - Hashes, ids and audit rows are kept verbatim, with the audit columns the V1 export carries (D27). V2 attribution columns are additive; NULL means V1, self-asserted. A migrated project's audit chain starts at an import-digest row, and the receipt states the known gaps in the V1 audit history.
  - No stores are merged, and no V1 credentials carry over. Clients re-attach with `npx unimatrix init`.
  - V1 stays read-only until α exit. Afterwards, the exit path is the V2 binary's local N=1.
- **Q4, goals.**
  - Survive: `integrity` (extended), `self-learning` (with per-store learning modes) and `proactive-delivery` (additive).
  - `personal-cloud` dies and is replaced by a new `hosted-service` goal.
  - `trusted-identity` is promoted and absorbs authN/authZ.
  - `platform` and `domain-agnostic` merge.
- **Principles.**
  - P1–P5 and P8 survive, clarified or extended. P2 is restated by audit class (S9): a state change is audited in its own transaction or does not commit; a security event is recorded before the response; access is recorded, and any loss is itself recorded. Each store's audit log is a hash chain anchored outside the store. P2 holds only after Stage 1's audit write-path gap (G18) closes.
  - P6 and P7 take the architect's text, amended by architect review 2: roles communicate only through published interfaces, and a project is resident on one cell at a time.
  - Three are new: P9 isolation by construction, P10 additive workflow-awareness, P11 answers but never initiates.
- **Q5, roadmap.**
  - Stage 0 runs in the human's order:
    0. Cut V1 minor release v0.12.0 with the existing `/uni-release` and `release.yml`, unchanged.
    1. Port the Rust and integration CI to Actions.
    2. Reach a green baseline (flaky tests, 15 xfail).
    3. Land the V1 security fixes, plus the audit export-fidelity fix under D27, and release them as a v0.12.x patch.
    4. Cut the V1 branch-point tag and `release/v1`.
    5. Add the size ratchet to the same CI.
    6. Split modules, gated by CI: W1a; then W2 per module in Stage 1's order, tick modules first; then W1b.

    Eval stays local and runs before W2 and after each W2 split. About 6–10 sessions come before the first tick-fix delivery (KC1), up from 4–5 (T16).
  - Stage 1: no-regret foundations on `main`: vectors in SQLite, the tick fixes, G9, the same-transaction audit write path (G18), LI-2 and LI-3, and the inference seam and lanes (C21, C23). They reach V1 users only by backport (T21).
  - Stage 2: the multi-tenant core with explicit router, cell and control-plane boundaries (C17–C20), security-event and access audit, and the bidirectional isolation gate on a two-cell CI topology with a project move (C16).
  - Stage 3: OAuth, roles (G6), onboarding, PCD and migration, the V2 cell image (C27), plus continuous server deploys and the first V2 npm client release. This reaches α, and V1 is retired.
  - Stage 4: residency, the fair-share scheduler, fleet jobs (C20) and Med Search's needs. This reaches β.
  - Stage 5: T2 identity, in parallel.
  - Stage 6: intra-tenant shared spaces.
  - Stage 7: the GA gate, including a per-cell blast-radius cap (D29) and published audit retention (D15).
  - Post-GA: a tenant-scoped, read-only auditor role, once an audit-viewing interface exists (human direction; a roadmap item, not a decision).
  - In Stages 1–6, each delivery pre-splits any over-limit module it is first to touch. New V2 subsystems (control plane, OAuth resource server, attestation) are new crates in the single repo (D7).
- **CI, release and branching (D23).**
  - CI tiers:
    - per PR: build, clippy, unit tests, the ratchet, the link smoke, infra-001 smoke and the Node suites;
    - on merge: the full infra suites;
    - nightly: `availability` and `volume`;
    - from Stage 2: the field census and a two-cell smoke per PR, and the cross-cell isolation gate and project-move test on merge and before every deploy (C16).
  - Path filters go at job level, dependencies are cached, and `CARGO_BUILD_JOBS` is set per runner class against the #878 link OOM.
  - After the cut, V1 uses `v0.*` tags on `release/v1` only and keeps its frozen release workflow. That workflow is changed once before the cut, so the npm, GHCR and GitHub Release `latest` pointers stay on V2 once V2 ships.
  - V2's hosted server deploys continuously from `main` by image digest, behind the merge tier and the pre-deploy gates. The npm client releases on its own `client-v*` line, with a compatibility window (the current and previous minor, for at least 90 days).
  - D23a: V2 ships an npm client (settled). Keep `@dug-21/unimatrix`: V2 is its next major on `latest`, and V1 moves to a `v1` dist-tag. Rename only if V1 and V2 must be installed side by side.
  - D23b: stay public unless there is a business reason. Going private would:
    - cost about $31/month for today's Node CI [D], against $12–18 included;
    - halve runner size;
    - end npm provenance.

    If the repo does go private, add a self-hosted Linux runner for the Rust jobs.
  - Under D4, as a recommendation not yet decided (the human leans yes): the npm client is also the OAuth client, with one credential for MCP and hooks.
- **Size policy (D21).**
  - Production files: 500 lines, counted with tests out of line. Test files: 1,000 lines. Functions: 150 code lines.
  - An in-repo ratchet holds it: baselined files may not grow, and only a human can approve a raise.
  - It runs as a job in the ported CI. The workflow change goes through a GH issue.
- **Prove first.**
  - Post-fix tick cost [M].
  - HNSW rebuild without inference.
  - G9, G18 and G6 closed before a second tenant exists; G18 proven by an audit census test plus kill and write-hog probes.
  - Bidirectional isolation, across two cells.
  - The fair-share and cold-start bounds.
- **Hypotheses.** H1 modified. H2 partly accepted. H3 rejected. H4 conditional (intra-tenant only). H5 accepted, with P6/P7 restated. H6 rejected, with two small go items.
- **Decisions for human.**
  - 30 decisions, ranked, with stable IDs. They come from 38 spike escalations (S1–S6, S8, S9 and architect review 2), 3 added by this synthesis, and 1 from human direction (D23).
  - New in Revision 4: D24 audit classes and failure semantics; D25 access-audit completeness; D26 audit anchor strength; D27 V1 export fidelity before the cut; D28 hosting platform; D29 per-cell blast-radius cap; D30 NLI worker timing. Audit retention folds into D15.
  - Decide first: D1, D23, D20, D2 with D22, D27, D3, D24, D4, D21. D6 and D30 gate Stage 1 work, D5–D7 gate Stage 2, and D28 with D16 gates the first hosted deploy.
- **Top risks.**
  - Unmeasured post-fix cost.
  - RPO and no HA.
  - An isolation incident.
  - v0.12.0 ships with two known security defects until the v0.12.x patch (R27).
  - Solo-operator effort: the refactor part is sized; the features and V1 ports are not.
  - Refactor regressions, with CI gating every wave and `release/v1` as the fallback.
  - Eval on tenant snapshots: α or synthetic data only before β (R20).
  - CI and release (R21–R26):
    - runner memory against the link OOM;
    - private-repo cost and provenance;
    - a flaky baseline;
    - `latest`-pointer collisions between the two lines;
    - porting cost;
    - client–server version skew.
  - Audit (R28–R30): fail-closed audit can turn an audit defect into a write outage, accepted by design; V1 audit is not transactional and partly missing (V1 defects rows 26–31), which G18 fixes on `main` while the V1 line keeps it; V1 audit history has permanent gaps, declared in migration receipts.
  - Scale-out rot (R31): process-global state creeping back while production runs one cell, held off by the two-cell CI topology.
  - Blast radius: without HA, one cell failure takes down every tenant on it (R4, D29).
  - The legal posture.
  - The dependency on the hosted AS.
