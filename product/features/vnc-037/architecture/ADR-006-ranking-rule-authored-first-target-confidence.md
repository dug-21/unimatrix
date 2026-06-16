## ADR-006 (vnc-037): Ranking rule — authored-first, then inferred by target-entry `entries.confidence`; exact `ORDER BY`; edge `weight` is NOT used

### Context

The next-hop reframe cut the display cap from 10 to 3 (D-05). With only three slots, *which
three* is the entire feature (D-09). A wrong order produces a plausible-but-worse affordance
with **no build signal** — silent degradation (SR-09). The rule must be precise, locked as
an AC, and grounded.

Two candidate rank keys exist for inferred edges:
- **`graph_edges.weight`** — frozen first-write-wins (`INSERT OR IGNORE`) and mapped from
  cycle outcome (`outcome_to_weight` → `1.0` success / `0.5` otherwise). Closed cycles are
  ~always `success` (the delivery workflow reworks failure before close), so weight is
  effectively constant — **no discriminating signal** (ass-079). A weak ranker.
- **`entries.confidence`** of the **target** entry — a `REAL NOT NULL DEFAULT 0.0` column
  directly on `entries` (`db.rs:549`), a cached six-component Bayesian Beta-Binomial
  composite (the helpfulness term is a Beta-Binomial posterior mean, cold-start α₀=3.0 /
  β₀=3.0 — "Wilson" is shorthand, it is **not** literally a Wilson interval). It is the
  canonical per-entry quality score and is joinable via the target endpoint. The meaningful
  discriminator.

Authored edges (`Prerequisite`/`Contradicts`/`Supports`, `source='agent'`) are the scarce,
high-trust signal — a human/agent deliberately declared them — and must take slots first
(D-09.1). Carried-forward (vnc-035) and `context_edge` edges are **agent-declared, stamped
`source='agent'`** (ADR-002 vnc-035, #4984), so they classify as authored and keep priority
— SR-10 hinges on the write path actually stamping `'agent'`, which #4984 confirms it does.

### Decision

Rank in SQL, authored-first then inferred by target confidence, with a deterministic
tiebreak — implemented as the locked clause:

```sql
ORDER BY (source = 'agent') DESC,        -- 1. authored edges fill slots first (D-09.1)
         t.confidence       DESC NULLS LAST,  -- 2. inferred ranked by TARGET-ENTRY confidence (D-09.3)
         target_id          ASC          -- 3. deterministic tiebreak (stable, reproducible)
LIMIT ?                                   -- D-05 display cap, BOUND to GET_EDGE_DISPLAY_LIMIT (not a literal 3)
```
where `t` is `entries LEFT JOIN`ed on the *other* endpoint (ADR-004), applied **after**
symmetric canonicalization (ADR-007).

#### The display cap is ONE named constant, not a magic literal (maintainability)

The display cap value (3, per D-05) is defined **once** as a named constant and referenced
everywhere the cap is used — the SQL `LIMIT`, the `…and N more` rendering affordance, and
the tests. Changing how many edges render is a **one-line edit** to the constant, with no
hunt through SQL strings, render code, or test fixtures.

```rust
/// Display cap for the context_get next-hop edge affordance (D-05, vnc-037).
/// At most this many ranked edges render on a single context_get; totals
/// (COUNT) are UNCAPPED and unaffected by this value (see Consequences).
/// i64 to match sqlx bind conventions for the SQLite `LIMIT ?` parameter
/// (parallel to CO_ACCESS_GRAPH_MIN_COUNT, ADR-002 crt-034).
pub const GET_EDGE_DISPLAY_LIMIT: i64 = 3;
```

- **Placement & re-export.** Defined in `unimatrix-store/src/read.rs` immediately below
  `CO_ACCESS_GRAPH_MIN_COUNT` and re-exported from `lib.rs` in the existing
  `pub use read::{…}` block — the established edge-constant convention (ADR-002 crt-034 for
  `EDGE_SOURCE_*` / `CO_ACCESS_GRAPH_MIN_COUNT`; ADR-008 vnc-015 for `EDGE_SOURCE_AGENT`).
  Rejected: a fresh `constants.rs` sub-module (over-engineering for one constant following
  an established co-location pattern — same call ADR-002 crt-034 made).
- **The SQL `LIMIT` binds the constant.** The ranked query's `LIMIT ?` is bound to
  `GET_EDGE_DISPLAY_LIMIT` (a sqlx parameter bind, same as `CO_ACCESS_GRAPH_MIN_COUNT` is
  bound elsewhere) — never a literal `3` in the query string.
- **Rendering references the constant.** The `…and N more — use context_graph` affordance
  (D-08) fires on `total > GET_EDGE_DISPLAY_LIMIT`; no render path hardcodes 3.
- **Tests derive from the constant.** AC-10's discriminating ranking/cap tests seed and
  assert *relative to* `GET_EDGE_DISPLAY_LIMIT` (e.g. seed `GET_EDGE_DISPLAY_LIMIT + 2`
  edges; assert the result length `== GET_EDGE_DISPLAY_LIMIT`), never literal `3`/`5`, so
  changing the constant cannot break the suite spuriously. This directly answers lesson
  #3886 — the discriminating cap test must be seeded relative to the cap, not a hardcoded
  count.

Rules enforced by this clause:
1. **Authored first.** All `source='agent'` edges sort ahead of all inferred edges. If ≥3
   authored edges exist, **no inferred edge appears** (D-09.2) — the `LIMIT 3` is consumed by
   authored rows.
2. **Inferred fills the remainder only when authored < 3**, ranked by the **target entry's**
   `entries.confidence` descending.
3. **`graph_edges.weight` is NOT in the `ORDER BY`** — it is frozen and non-discriminating
   (ass-079). Using it would be a defect; tests must confirm weight does **not** decide order.
4. **`NULLS LAST`** so dangling targets (LEFT JOIN, no `entries` row → NULL confidence) and
   cold-start `0.0` targets rank deterministically at the bottom rather than sorting
   unpredictably (SR-11). Dangling edges are **retained**, just last (D-02).
5. **`target_id` tiebreak** makes the order stable and reproducible across runs (important
   for the discriminating tests under cap-3).

Carried-forward / `context_edge` edges classify as authored via `source='agent'` (#4984);
AC-10 locks a named test asserting this (SR-10) — silent demotion otherwise.

### Consequences

- **Easier:** the scarce, high-trust authored signal is always preferred; inferred edges are
  ranked by a *real* quality discriminator, not a frozen constant; the order is deterministic
  and testable; the rule is one SQL clause, the single source of truth (no parallel Rust
  sort to drift).
- **Easier (cap maintainability):** the display cap is one named constant
  (`GET_EDGE_DISPLAY_LIMIT`), so retuning how many next-hops render is a one-line edit — no
  literal `3` scattered across SQL, render, and tests to find and reconcile.
- **The cap is display-only, decoupled from totals.** `GET_EDGE_DISPLAY_LIMIT` governs only
  the ranked `LIMIT` (how many edges *render*). The split `COUNT(*)` totals are **uncapped**
  and do **not** reference the constant; symmetric canonicalization (ADR-007) is independent
  of it. Therefore changing the constant changes only the rendered set size — never the
  inbound/outbound totals, the empty-box feedback loop, or `↔` canonicalization. The single
  coupled consequence is the `…and N more` threshold (`total > GET_EDGE_DISPLAY_LIMIT`),
  which is correct by construction.
- **Harder:** if `entries.confidence` is uniformly `0.0` (cold-start default, `db.rs:549`)
  the inferred tier degenerates to `target_id` order — acceptable (deterministic, not wrong)
  but worth noting (A4); the rule's correctness produces **no build signal** — only a worse
  affordance — so AC-10 must carry **discriminating** tests: higher-confidence target ranks
  first; weight does NOT decide; authored-priority-under-cap; inferred-fill-only-when-
  authored<3; carried-forward-classifies-authored (SR-09/SR-10/SR-13).
- **Cross-ref:** ADR-001 (the ranked query issuing this `ORDER BY`), ADR-004 (the confidence
  JOIN + `source` column it depends on), ADR-007 (canonicalization that must run before this
  ranking), ADR-002 (the projection drops the confidence/weight — never surfaced).
- **Grounded in:** ass-079 (frozen Informs weight), `confidence.rs` (Beta-Binomial composite),
  #4984 / #4425 (carry-forward / `context_edge` stamp `source='agent'`).
