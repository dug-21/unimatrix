# crt-053 — Active-Only PPR Expansion Seeds (Surgical Search-Heuristic Correctness)

**Status**: SCOPE (rescoped 2026-06-10 after ass-073 #720 and ass-074 #721 ran)
**GH Issue**: #717
**Supersedes**: the prior crt-053 draft (status-trust re-evaluation + steepness sweep). That framing is **retired** — the research below changed it.

---

## READ THIS FIRST — to the delivery team

**This is the bread-and-butter of the platform.** Search-result quality is the single most critical element of the product. If we don't return quality results, nothing else matters. You are about to open the most sensitive code in the system.

**The dimensions of this feature were researched (ass-073, ass-074) and decided by human judgment.** They are not open for re-optimization. When you open the search pipeline you **will** see other things that look like gaps — the injection-path penalty bypass, the vnc-017 redirect ceiling, deprecated entries appearing in results. **Do not close them.** Each was examined and deliberately left as-is, because:

1. **There is no tool that measures search-heuristic effectiveness.** You cannot empirically validate that a "more technically defensive" choice produces *better results for an agent*. Building a relational-relevance corpus that could measure this is separate future work (ass-074's primary discovery).
2. **The most technically correct or defensive option is frequently NOT the most helpful one.** A semantic/graph search heuristic is judgment, not theorem-proving. Suppressing more, penalizing harder, and resolving every edge case can quietly make search *worse* while looking *safer*.
3. **These calls were already made by a human with the product context.** Re-deciding them with a local technical lens, against no measurement, is exactly the failure this section exists to prevent.

**Do exactly what is in scope. Nothing more. If you believe something in scope is wrong, stop and raise it — do not "fix" it.**

---

## The change (one surgical edit)

**PPR / graph_expand expansion seeds from ACTIVE entries only.**

In the expander seed collection (`services/search.rs`, Phase 0, the `seed_ids` build at ~`:915`), filter the candidate pool to `status == Active` before `graph_expand` walks from it. The seeds become active-only; everything downstream of the BFS is unchanged.

That is the feature. One filter on the seed set.

## What this does and does not touch (precise boundary)

- **PPR now expands only from current knowledge.** It surfaces entries related to something *active*. It no longer anchors graph expansion on a deprecated entry.
- **HNSW ranking is UNCHANGED.** Deprecated and superseded entries still enter the candidate set, still receive their topology penalty, still appear in Flexible (search) results. The two-mode design stands (see Locked Decisions).
- **PPR-only.** No other admission stage, scoring path, penalty, or mode is modified.

## Human-judged decisions (LOCKED — do not reopen, do not "complete")

1. **Two modes stand.** Flexible (search) = penalize-but-keep-visible; Strict (briefing) = evict. This is a deliberate product design. Returning deprecated entries in search is **not an error**. Not in scope to change.
2. **The bar is "deprecated must not outweigh a comparable active," not "deprecated must be absent" (in Flexible).** The HNSW topology penalty already meets that bar on the ranking path; the active-only seed filter closes the one path (PPR injection) where it didn't. That is sufficient by judgment.
3. **No steepness work.** ass-073 and ass-074 both confirmed penalty *magnitude* is not the lever (baseline already ranks deprecated below comparable actives on the penalized path). crt-053 is **not** a tuning feature. Q6/Q8 are dropped entirely.
4. **No injection-side redirect or penalty machinery.** Do **not** add `find_terminal_active` resolution to injected entries, and do **not** extend `penalty_map` to injected entries. The active-only seed filter is the chosen mechanism. The residual cases it does not cover (a deprecated *neighbor* of an active seed reachable only via the vnc-017 >50-edge redirect ceiling) are **knowingly accepted**, not bugs to patch here.
5. **The vnc-017 redirect ceiling (50) is not this feature's problem.** It is a pre-existing, separately-tracked limitation. Leave it.

## Disposition of the original four issues

| Issue | Disposition |
|---|---|
| **#704** (deprecated surfacing / outranking active) | **Closes via this feature.** The active-only seed filter removes the one path (PPR injection at penalty 1.0) by which a deprecated entry could outrank a comparable active; the HNSW penalty + 6b terminal-active head injection already handle the ranking path. Close on this PR. |
| **#406** (multi-hop terminal-active injection "fails") | **Does NOT reproduce** in the eval graph rebuild (ass-073 #720) — the multi-hop redirect resolves correctly. Treat the failing test as a **test/snapshot-construction artifact to investigate**, not a retrieval fix. Do not "fix" retrieval for it. |
| **#585** (edge generation pulls deprecated into candidate scoring) | **Out of scope — separate concern.** It is about keeping the *edge graph* free of deprecated targets (write-time hygiene), which is what keeps the active-only seed filter robust over time. Decide and track separately on #585; do not bundle it here. |
| **#405** (deprecated confidence flake) | **Split out (locked earlier).** Independent timing flake; not this feature. |

## Research provenance (the judged basis)

- **ass-073 (#720, ran):** on the penalized HNSW path, baseline already ranks deprecated below comparable actives — steepness is not the lever; eviction vs penalty is a mode question, not a magnitude one. #406 does not reproduce. Relevance bound for any future change: gate on **MRR**, not soft-GT P@5 (correctly evicting/demoting stale entries mechanically drops soft-GT P@5 — the #500 trap).
- **ass-074 (#721, ran):** the PPR expander **works and is now enabled in prod**; it injects in ~48% of queries, every injection edge-traced to a seed. The leak (injected stale at penalty 1.0) is **latent** today only because the graph is all Active→Active. PPR seeds from the full Flexible pool (incl. deprecated), and `graph_expand` applies no status filter — the source of the seed-side exposure this feature closes. **Primary discovery: the platform cannot measure its own graph-relational layer** (every automated signal reduces to cosine), which is *why* these are human-judgment calls.

## Validation

Behavior-based only (crt-013 #703 — assert ranking/presence outcomes, never penalty constants):

- **The seed filter excludes deprecated/superseded from the expander seed set.** Construct a fixture pool containing an active entry and a deprecated entry, both with positive out-edges; assert the BFS expands from the active seed and **not** from the deprecated one (no entry reachable only via the deprecated seed is injected). This is testable on the nan-018 fixture corpus (with the positive-edge revision ass-073 requested) or the Python integration suite.
- **Bit-for-bit unchanged when the expander is off** (`ppr_expander_enabled = false`): the seed-filter code path adds zero behavior change in the default-off configuration.
- **HNSW ranking unchanged**: existing search/penalty tests pass untouched — deprecated entries still appear and are still penalized in Flexible.

Do **not** add tests that assert deprecated *absence* in Flexible — that contradicts the two-mode design.

## Constraints

- C-01: Active-only filter is the **only** production change. No other file/stage edited for status behavior.
- C-02: `ppr_expander_enabled = false` path stays bit-for-bit identical.
- C-03: Flexible/Strict mode semantics untouched (ADR-001 #481).
- C-04: Status tests assert ranking/presence outcomes, never penalty constants (crt-013 #703).

## Tracking

GH Issue: #717. Research basis: ass-073 (#720), ass-074 (#721). Cluster: #704 closes here; #406 → test-artifact investigation; #585 → separate edge-hygiene; #405 → split. The "no adlibs" directive above is binding on delivery.
