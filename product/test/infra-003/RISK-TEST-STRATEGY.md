# Risk-Based Test Strategy: infra-003

> Test-only feature. This strategy is a **test-of-a-test**: the risks below are
> the ways this isolation gate can lie — false-RED (the gate fails while the
> property holds), false-GREEN / vacuous-pass (the gate passes while the property
> is broken or untested), and harness fragility (the gate cannot run faithfully).
> A vacuous pass on this gate is worse than no gate (Problem Statement) — the
> mis-route corrupts the wrong project's hash chain unrollbackably and silently,
> so false-GREEN is the dominant risk class and is weighted accordingly.
>
> **Updated for the bidirectional 2×2 design.** The gate now drives **four**
> distinctly-marked writes — `A-obs`, `B-obs`, `A-mcp`, `B-mcp` — through both
> slugs' routes on both surfaces and asserts, per surface, the full matrix: each
> store holds **only its own** slug's marker (present-in-own / absent-in-other),
> in **both** directions. Slug B is `isolation-b`. AC references below track the
> updated SPEC/SCOPE numbering (AC-01…AC-14).
>
> **Latest revision (soundness fixes adopted in ARCH/ADR-002/ADR-003):** three
> hazards are now resolved-by-design and reflected below — (1) the unsound
> aggregate `store_size` durability barrier is replaced by a marker-keyed
> **read-as-barrier** (R-05, reclassified); (2) the two MCP handshakes each use
> their **own** `Mcp-Session-Id`, never crossed (R-17, new); (3) the four markers
> are **mutually non-substring** so `LIKE '%marker%'` cannot cross-match (R-18,
> new). R-15/R-16 carry concrete GitHub linkage (#815 in-PR lockstep; #788
> standing-lane comment).
>
> Historical evidence consulted: #3624 (zero-regression gate validates the no-op
> path only — positive integration test is mandatory), #5180 (self-skipping smoke
> must fail the job on a distinct exit code, never pass green), #5177/#5173
> (vacuous-pass when earlier-surface ACs are under-tested), #5296 + #5129
> (rmcp **forces** SSE on `/v1/{slug}/mcp` — a JSON-only Accept is refused),
> #4708 (`Mcp-Session-Id` is a UUID minted at `initialize`, distinct from any
> tool `session_id` param), #5193 (WAL-robust store-grew = `du -s` over the slug
> DIR, never a `.db` stat).

## Headline Coverage Gain — bidirectional 2×2 closes the symmetric N3 gap

The single-direction design (write only through A; assert A-has-marker / B-empty)
**passes GREEN on a real isolation break**: if B's route mis-resolves into A's
store, B's own on-disk store stays correctly empty, so a one-directional negative
control reads clean and certifies isolation that is broken. Route-liveness
(non-404) does not catch it either — a mis-resolved B route still responds and
runs the handler.

The bidirectional design closes this. The **B-direction positive control** — B's
store must contain B's own marker (`B-obs` / `B-mcp`) — is the new teeth: if B's
writes silently land in A, B's store stays empty and the B positive control fails
**RED** instead of passing vacuously. The matching cross-cell (A's store must NOT
contain `B-obs`/`B-mcp`) catches the leaked-into-A direction directly. This is the
headline coverage gain: the previously-uncovered symmetric failure mode is now a
hard RED on both the positive (B empty) and negative (B marker in A) cells, on
both surfaces. The old single-direction false-pass mode is **retired** (see R-07,
repointed). The cost is ~2 extra writes against reads that already hit both stores.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | MCP streamable-HTTP handshake mis-built (missing `initialize`→`Mcp-Session-Id`→`initialized`→`tools/call`, or `Accept` not `application/json, text/event-stream`) — run **per direction** (`/v1/A/mcp` and `/v1/B/mcp`) → C4 cannot drive the write; handshake/session failure is INFRA, not RED | High | High | **Critical** |
| R-02 | Load-bearing MCP probe passes vacuously — a write no-ops or the marker never reaches `entries.content`, leaving the shipped MCP-isolation gap (SR-10) uncovered while the gate goes GREEN; now in both directions | High | Med | **Critical** |
| R-03 | Positive-gates-negative inversion — a cross-contamination "other store clean" cell reported on a silently-failed own-write → vacuous GREEN; now **four** positive controls must each gate their negative | High | Med | **Critical** |
| R-04 | WAL pre-checkpoint false-empty read — single-file `vol cat` without `-wal`/`-shm`; own-marker reads empty (false-RED) **or a leaked cross-marker sits in the other store's uncopied WAL and the cross-cell false-passes** (false-GREEN); now across both stores | High | Med | **Critical** |
| R-05 | *(Reclassified — soundness fix adopted)* Durability-barrier unsoundness: the old aggregate `store_size` ("A grew AND B grew") barrier was satisfied by the FIRST of a store's two writes and proved nothing about the second → content read races an unsynced write → positive control FALSE-RED. Resolved by the marker-keyed read-as-barrier; residual is correct INFRA-vs-RED discrimination + a sane bounded retry | Med | Low-Med | **Med** |
| R-06 | Read-dependency absent (`sqlite3`/`vol`) → silent empty capture → all-cells-empty; if a positive gate is bypassed this empty-passes the cross-cells (SR-01) | High | Med | **High** |
| R-07 | *(Repointed)* Route-liveness treated as the isolation verdict, or an unregistered/missing B store read as "0 rows" instead of INFRA — the single-direction false-pass is closed by the B-direction controls, but liveness-as-verdict and the missing-B-db INFRA distinction remain | Med | Med | **High** |
| R-08 | Marker not per-run-unique or stale store from a prior run — a prior marker satisfies a positive without this run writing, or pollutes a cross-cell; now **four** markers (NFR-05) | Med | Med | **High** |
| R-09 | A marker does not round-trip verbatim into the asserted column (`topic_signal`→`observations.topic_signal`; `content`→`entries.content`); transform/drop → positive false-RED, Q2 unverified | Med | Med | **High** |
| R-10 | INFRA / RED / GREEN tri-state collapsed — folding INFRA into `fail()` exit 1 (noise) or, fatally, rounding any non-pass toward GREEN (#5180) | Med | Med | **High** |
| R-11 | *(Resolved by design)* Slug-B literal collision with a real/eval store — adopting the neutral `isolation-b` removes the `eval-baseline` collision risk; residual is only an on-volume pre-existing `isolation-b` | Low | Low | **Low** |
| R-12 | A marker contains SQL/LIKE metacharacters (`%` `_` `'`) → broken or spuriously-matching query, or injection into the `sqlite3` predicate → false verdict | Med | Low-Med | **Med** |
| R-13 | Cumulative coupling to posture-smoke libs (SR-12) — an upstream change to a sourced primitive silently alters this gate | Med | Low-Med | **Med** |
| R-14 | Overclaim / scope creep — N3 reported as closed (SR-05) or a parity-matrix shape reintroduced (SR-06) | Low | Low | **Low** |
| R-15 | New-smoke-script invariant trip (#815) — ADR-001 adds a new top-level standalone smoke script; the known invariant test that flags new smoke scripts (open via #815 since #810 added a 2nd) will trip on this script unless updated in lockstep | Med | High | **High** |
| R-16 | Standing-gate orphan — a point-in-time gate never wired to the N5/#788 recurring lane proves isolation once at delivery and never again; a later regression on this integrity-critical seam goes silently uncaught after merge | High | Med | **High** |
| R-17 | Crossed/reused `Mcp-Session-Id` — A's session replayed against B's route (or vice versa) across the two handshakes mis-attributes the very isolation under test → **false verdict** (the write/read it certifies belongs to the wrong tenant context) | High | Med | **High** |
| R-18 | Marker substring collision — the MCP read is `content LIKE '%marker%'`; "distinct" is insufficient, a marker that is a substring of another false-matches a cross-direction negative control → GREEN on a real leak. Resolved by four mutually NON-SUBSTRING literals | High | Low | **Med** |

---

## Risk-to-Scenario Mapping

### R-01: MCP streamable-HTTP handshake is a new, fragile failure surface (×2 directions)
**Severity**: High **Likelihood**: High
**Impact**: rmcp's `StreamableHttpService` **forces SSE** (#5296/#5129) and
requires a session: `initialize` → capture the `Mcp-Session-Id` UUID header
(#4708) → `initialized` → `tools/call`. A probe modelled as a single `curl` POST
(the observe mental model) is refused, or its `application/json`-only parse chokes
on the `text/event-stream` body. The bidirectional design runs this handshake
**twice** (`/v1/A/mcp` and `/v1/B/mcp`) — double the exposure. The verdict is the
C6 content read, so a broken handshake manifests as a **false-RED positive** (own
marker absent because the write never executed), masquerading as a broken
isolation property.

**Test Scenarios**:
1. For each direction, drive the full handshake; assert `initialize` returns a non-empty `Mcp-Session-Id` and that exact header is replayed byte-stable on `tools/call` (#5296 wire-witness pattern).
2. Send `tools/call` with `Accept: application/json, text/event-stream`; assert the response is parsed as SSE framing and the JSON-RPC result is extracted from the SSE event, not a bare body.
3. Handshake negative control: a JSON-only `Accept` must be refused (proves real SSE, not a JSON shortcut).
4. Assert a handshake failure (missing session id, `-32099 SESSION_NOT_FOUND`, SSE-parse error) raises **INFRA**, not RED — a transport failure is not an isolation failure — and is attributed to the correct direction.

**Coverage Requirement**: Both MCP writes provably execute through the real
streamable-HTTP session (session minted, replayed, SSE parsed) before any verdict;
transport failure is INFRA, distinct from RED, per direction. The *correctness* of
which session is used against which route is its own risk — see R-17.

### R-02: The load-bearing MCP probe passes vacuously (both directions)
**Severity**: High **Likelihood**: Med
**Impact**: SR-10 is the entire reason this surface is in scope — the
`entry.store == adapter-store` invariant is a `debug_assert!` compiled **out** of
the release container (`seam.rs:345`), so the shipped artifact has **zero**
MCP-isolation coverage. If a `context_store` write no-ops, writes the wrong
column, or the positive control accepts RPC-success / a `du`-delta instead of a
genuine `entries.content` read, the gap stays uncovered while the gate reports
GREEN. Direct analog of #3624 (a broken function produces a green gate when only
the no-op path is exercised). Now both `A-mcp`→A and `B-mcp`→B must be genuine reads.

**Test Scenarios**:
1. Each positive control is a content read: `SELECT ... FROM entries WHERE content LIKE '%<marker>%'` returns ≥1 row in the own store — never RPC-success-only, never a `du` delta (FR-03.2).
2. Confirm `context_store` (not `context_correct`, which needs a prior entry to correct) actually persists an `entries` row carrying the marker; if `context_correct` is chosen, a prior store must exist or the write silently has nothing to correct.
3. Mutation/fault-injection sanity: a deliberately wrong marker must return 0 rows and force RED — proving the assertion has teeth (#5296).

**Coverage Requirement**: Each MCP positive control fails RED iff its marker is
genuinely absent from `entries` in its own store; success-RPC with no row and
size-delta-with-no-marker must not pass.

### R-03: Positive-gates-negative inversion — now four positive controls
**Severity**: High **Likelihood**: Med
**Impact**: If verdict logic evaluates any cross-contamination cell independently
of its direction's positive control, a silently-failed own-write yields a vacuous
"other store clean" GREEN — trivially true when nothing was written. The
bidirectional design has **four** positive controls (`A-obs`/A, `B-obs`/B,
`A-mcp`/A, `B-mcp`/B), each of which must gate its own cross-cell (AC-05/AC-09,
SR-10). The recurring vacuous-pass class (#5177/#5173).

**Test Scenarios**:
1. Per (slug, surface) direction, assert the gate evaluates the own-marker positive first and, on failure, emits RED and **never** reaches that direction's cross-contamination pass path.
2. Inject a positive-control failure per direction (force the own marker absent — e.g. read an empty store) and assert that direction reports RED, not GREEN.
3. Assert the four directions and two surfaces are independent — one direction's RED does not let another pass on residue; distinct markers make cross-attribution impossible.

**Coverage Requirement**: No cross-contamination cell is reported for a direction
whose own positive control has not already passed.

### R-04: WAL pre-checkpoint false-empty read (both stores, both directions)
**Severity**: High **Likelihood**: Med
**Impact**: WAL `synchronous=NORMAL` writes land in `-wal` before checkpoint. A
single-file `vol cat` reads a pre-checkpoint snapshot: an own-marker missing →
false-RED positive; more dangerously, a genuinely cross-routed row sitting in the
**other store's uncopied `-wal`** → the cross-cell reads empty and
**false-GREENs the very leak the gate exists to catch**. With the matrix read over
both A and B, every one of the four marker reads must operate on a WAL-complete
copy. SR-02; #5193.

**Test Scenarios**:
1. Assert `vol cat` copies `unimatrix.db` **plus** `-wal` and `-shm` for **both** A and B before every `sqlite3` query.
2. A missing main db = INFRA; an absent `-wal` (already checkpointed) is acceptable only when the main db is present and durable (FR-06.4).
3. Read-back consistency: each query against the copied-with-WAL snapshot agrees with a post-explicit-checkpoint snapshot (no pre-checkpoint blind spot), per store.

**Coverage Requirement**: All four marker reads operate on WAL-complete copies
taken after the (re-baselined) barrier; cross-cells read the same WAL-complete
view as positives.

### R-05: *(Reclassified — soundness fix adopted)* Durability-barrier unsoundness
**Severity**: Med **Likelihood**: Low-Med
**Hazard (now resolved in design)**: The original aggregate `store_size`
("A grew AND B grew") barrier was **unsound**. `store_size` is a `du` delta over a
slug dir, and each store now receives **two** writes (A gets `A-obs` then `A-mcp`).
"A grew" is satisfied the moment the *first* write lands and says **nothing** about
whether the *second* is durable; writes are `tokio::spawn` fire-and-forget with WAL
`synchronous=NORMAL`, so a content read gated only on "A grew" races the unsynced
second write and **false-REDs** the positive control.

**Resolution (ADR-002 C5 / SPEC):** the positive-control content read **is** the
barrier — a **marker-keyed retry-until-present**: writes are issued strictly
sequentially per store; each write is immediately followed by a bounded
deadline-poll that `vol cat`s the store (db + `-wal` + `-shm`) and queries for
*that cell's* marker, retrying until present. A not-yet-synced write simply has not
appeared yet → keep polling. **Own-store timeout → INFRA, never RED**; a genuine
mis-route still surfaces as **RED** at the C6 cross-store cell (the marker found in
the *wrong* store), independent of the positive outcome. `store_size` is demoted to
**liveness/boot waits only**, never the durability barrier.

**Residual risk**: the retry loop must be implemented soundly — a bounded deadline
(no unbounded hang, no fixed `sleep` substituted for the poll), correct
INFRA-vs-RED discrimination (own-store absence-timeout = INFRA; wrong-store
presence = RED), and the cross-store negative evaluated only after the positive
reaches PRESENT. A retry that silently treats timeout as pass, or that never
terminates, re-opens the trap.

**Test Scenarios**:
1. Confirm `store_size` is used only for C2 liveness/boot waits, not as the durability barrier (no aggregate "store grew" gate before any content read).
2. Each positive control is a marker-keyed retry-until-present read with a bounded deadline; verify it polls db+`-wal`+`-shm` for the specific cell marker.
3. An own-store marker that never appears within the deadline classifies as **INFRA**, never RED and never a vacuous pass.
4. A marker injected into the *wrong* store surfaces as **RED** at the cross-store cell even when the own-store positive timed out as INFRA (the mis-route is never masked).

**Coverage Requirement**: The aggregate `store_size` barrier is gone; every
positive is a bounded marker-keyed read-as-barrier with own-store-timeout=INFRA and
wrong-store-presence=RED, and the cross-store negative is gated on PRESENT.

### R-06: Read-dependency absent → silent empty-pass
**Severity**: High **Likelihood**: Med
**Impact**: Absent `sqlite3` or `vol` sidecar yields an empty capture → 0 rows in
every cell; if a positive gate is bypassed (R-03), the cross-cells empty-pass
GREEN. SR-01; the false-green class (#5180).

**Test Scenarios**:
1. Preflight `command -v sqlite3` and a `vol` mount check before any write; absence → hard **INFRA** on a distinct exit code, never warn+continue (#4473).
2. Assert an empty/failed capture is INFRA, never coerced to "0 rows = pass."
3. If wired into CI, the gate's skip/INFRA exit must fail the job, never pass green (#5180) — see R-15/R-16.

**Coverage Requirement**: Every read dependency is presence-asserted before use;
absence is INFRA on a distinct exit state.

### R-07: *(Repointed)* Route-liveness-as-verdict and the missing-B INFRA distinction
**Severity**: Med **Likelihood**: Med
**Impact**: The single-direction false-pass — B's store read as clean while B's
route mis-resolves into A — is **closed** by the bidirectional B-direction
controls (see Headline). What remains: (a) treating route-liveness (non-404) as
the isolation verdict — the SPEC explicitly demotes it to a precondition (AC-01,
C-06), because a mis-resolved route still responds non-404; and (b) an
unregistered or missing B store being read as "0 rows" instead of INFRA, which
would now corrupt B's **positive** control (B can't contain `B-obs`/`B-mcp` if B's
db doesn't exist) into a false-RED, or mask a registration fault. SR-11.

**Test Scenarios**:
1. Route-liveness asserts all four routes non-404 before any write, recorded as a **precondition**, never an isolation pass.
2. A missing B `unimatrix.db` at read time is **INFRA**, not a 0-row cell (FR-06.4).
3. Both A and B registered before the single restart; confirm each store file exists and is the genuine per-slug store before any cell is trusted.

**Coverage Requirement**: Non-404 is never an isolation verdict; the matrix runs
only against provably-existing per-slug stores, and absence is INFRA.

### R-08: Stale-store / non-unique markers (four markers)
**Severity**: Med **Likelihood**: Med
**Impact**: A reused container/volume can leave a prior marker in a store
(positive passes without this run writing) or a prior leaked marker (cross-cell
fails spuriously). With four markers the surface is wider. NFR-05.

**Test Scenarios**:
1. All four markers carry a per-run nonce/PID so no prior artifact can satisfy or pollute any cell.
2. Markers are distinct literals across cells; observe markers (`observations`) and MCP markers (`entries`) live in distinct tables — cross-attribution structurally impossible (SR-07, FR-07.1).
3. On a fresh volume, all cross-cells read 0 before any write (baseline sanity).

**Coverage Requirement**: Every cell is attributable to this run's write via a
unique marker; no cell can match another cell's or a prior run's row.

### R-09: Marker does not round-trip into the asserted column
**Severity**: Med **Likelihood**: Med
**Impact**: The positive controls assume `topic_signal` lands verbatim in
`observations.topic_signal` and `content` lands in `entries.content` (Q2). If a
write path normalizes, truncates, or routes the field elsewhere, the positive
control false-REDs despite correct isolation — and Q2 is left unverified, the trap
#5177 warns against (deferring an unobservable AC to the tester).

**Test Scenarios**:
1. Confirm against schema (`db.rs:865` observations; `db.rs:541-568` entries) that the queried column is the one the write populates, with no transform of the marker.
2. A positive read-back of a known-written marker before asserting isolation (self-test the column mapping) per surface.
3. Documented fallback: if `topic_signal` is dropped by a future payload shape, `observations.input` substring is the spec-named fallback (not an ad-hoc guess).

**Coverage Requirement**: The marker-to-column mapping is verified, not assumed,
for both surfaces.

### R-10: INFRA / RED / GREEN tri-state collapse
**Severity**: Med **Likelihood**: Med
**Impact**: The gate must discriminate three outcomes. Folding INFRA into RED
(`fail()` exit 1) produces noise; rounding any non-pass toward GREEN is fatal
(#5180 exit-code discrimination). The architecture mandates distinct exit states
(GREEN / RED / INFRA / SKIP exit 3) — a regression re-opens the false-green class.

**Test Scenarios**:
1. Each of: missing dep, pre-/stale-barrier read, absent route, missing main db → INFRA exit, distinct from RED.
2. Property-broken (any own marker absent, or any cross-marker present) → RED exit 1.
3. Docker absent → SKIP exit 3; assert SKIP/INFRA never round to exit 0.

**Coverage Requirement**: Three (plus SKIP) distinct exit states; no non-GREEN
outcome maps to exit 0.

### R-11: *(Resolved by design)* Slug-B literal collision
**Severity**: Low **Likelihood**: Low
**Impact**: The architect/spec converged on the neutral, test-scoped literal
**`isolation-b`** (allowlist-valid under ADR-004 `^[a-z0-9][a-z0-9-]{0,62}$`),
explicitly rejecting `eval-baseline` because it reads like a live eval-harness slug
that could collide with a pre-existing store on the test volume (Q4 resolved,
C-04). The collision risk is **resolved**. Residual is only an unlikely
pre-existing `isolation-b` store on the volume.

**Test Scenarios**:
1. Confirm no pre-existing `isolation-b` store/registration before the run (fresh-volume or pre-clean assertion).
2. Per R-08, per-run-unique markers mean even a pre-populated B cannot carry this run's marker.

**Coverage Requirement**: B is a fresh, non-colliding `isolation-b` for the run.

### R-12: Marker SQL/LIKE metacharacters
**Severity**: Med **Likelihood**: Low-Med
**Impact**: The MCP read uses `content LIKE '%<marker>%'`. A marker containing
`%`/`_` matches spuriously (false-GREEN positive / false-RED cross-cell); a `'`
breaks or injects the host-side `sqlite3` predicate. A nonce with such characters
silently corrupts a verdict — across four markers now.

**Test Scenarios**:
1. Constrain all four markers to `[a-z0-9-]` (or hex nonce) so no LIKE wildcard or quote can appear.
2. Assert markers are parameter-safe in query construction; a marker with a deliberate `%` must not match an unrelated row.

**Coverage Requirement**: Markers cannot alter query semantics; predicates match
the literal marker only.

### R-13: Cumulative coupling to posture-smoke primitives
**Severity**: Med **Likelihood**: Low-Med
**Impact**: Sourcing `docker-http-posture-smoke.sh` / `cloud-bundle-lib.sh`
primitives means an upstream change to `vol()`, `store_size()`, or the WAL-aware
copy could silently alter this gate. SR-12.

**Test Scenarios**:
1. The gate is a separate top-level script with self-contained assertions; it sources only define-on-source libs and does not graft onto Gates 1–4.
2. An upstream primitive change surfaces here as an explicit failure (INFRA/RED), not a silent skip.

**Coverage Requirement**: This gate's assertions stand alone; reuse is of
primitives, not of the posture-smoke flow.

### R-14: Overclaim / parity reintroduction
**Severity**: Low **Likelihood**: Low
**Impact**: Reporting N3 (#5161) as closed (the N5/#788 regression gate is
unwired — see R-16) overclaims; reintroducing a parity-matrix shape re-opens the
removed D6 (#845). SR-05/SR-06.

**Test Scenarios**:
1. Capability evidence wording = "advances, does not close N3" (NFR-04).
2. No UDS behavioral probe and no parity-harness entry are added (AC-14); ADR-006 `FORBIDDEN_IN_LOCAL` is referenced, not re-run.

**Coverage Requirement**: Output claims point-in-time proof only; no parity shape.

### R-15: New-smoke-script invariant trip (#815) — delivery-coordination *(linkage now concrete)*
**Severity**: Med **Likelihood**: High
**Impact**: ADR-001 hosts the gate as a **new top-level standalone smoke script**
(`multi-tenant-isolation-smoke.sh`). There is a known invariant test that flags
the addition of new smoke scripts — **#815 is open precisely because #810
legitimately added a second smoke script**. This new script will very likely
**trip that invariant**, surfacing as a gate surprise during delivery (a RED that
is not an isolation failure but an unaccounted-for script). If discovered late it
stalls the PR.

**Resolution (concrete, in-PR lockstep):** the invariant update lands in the
**same delivery PR** as the new script — not a follow-up — and the obligation is
**cross-linked on #815** (issue comment posted by the leader), so the invariant is
extended (and #815's intent closed) in the same change that introduces the script.
This is now a real linkage, not a feature-doc row.

**Test Scenarios**:
1. The same delivery PR that adds `multi-tenant-isolation-smoke.sh` also updates the new-smoke-script invariant to register it as a known/expected script (in-PR lockstep, referencing #815).
2. After the update, the invariant test passes with the new script present and would still fail if an *unaccounted* future script were added (the invariant keeps its teeth).
3. Confirm the new script honors the same verify-by-name / exit-code contract the invariant enforces (#5180).

**Coverage Requirement**: The invariant update ships in the same PR as the script
(cross-linked on #815) and recognizes it as expected, without losing its guard
against unregistered future scripts.

### R-16: Standing-gate orphan — point-in-time gate never wired to a recurring lane *(linkage now concrete)*
**Severity**: High **Likelihood**: Med
**Impact**: This is a **point-in-time** gate; N3 stays `partial` because the
N5/#788 recurring regression lane is unwired (NFR-04, Non-Goals). A point-in-time
gate that is never adopted into a recurring lane proves isolation **once at
delivery and never again** — a later regression on this integrity-critical seam
(the unrollbackable cross-tenant hash-chain corruption) goes silently uncaught
after merge (a de facto revert to untested-after-merge). The gate's value decays
to zero the moment the next change lands.

**Resolution (concrete, durable linkage):** a durable linkage comment has been
posted on **#788** (by the leader) requiring N5/#788 to **adopt infra-003's gate
into the recurring lane** — advancing N3 from point-in-time toward maintained. The
hand-off is now a real, tracked GitHub linkage (#788 comment), not just a
feature-doc row; the standing-lane obligation is recorded where N5 work will see it.

**Test Scenarios**:
1. The #788 comment requiring N5 to adopt infra-003's gate into the recurring lane is present and durable (tracked linkage, not an informal note).
2. Capability evidence states the gate is point-in-time *and* names the N5/#788 adoption as the path to "maintained," so no reader misreads a delivery pass as a standing guarantee (R-14).
3. When N5 work proceeds, the recurring lane runs this gate on N5's cadence (verified at that time against the #788 linkage).

**Coverage Requirement**: The N5/#788 adoption obligation is captured as a durable
#788 linkage; the point-in-time proof is explicitly handed off to a standing lane
rather than orphaned.

### R-17: Crossed / reused `Mcp-Session-Id` across the two MCP handshakes
**Severity**: High **Likelihood**: Med
**Impact**: The MCP handshake now runs **twice** (`/v1/A/mcp`, `/v1/B/mcp`). A
session id minted on A's `initialize` and replayed against B's route (or vice
versa) would mis-attribute the very isolation under test — the write or read it
certifies belongs to the wrong tenant's session context — producing a **false
verdict** that is neither an obvious transport error nor a clean RED. This is a
distinct failure mechanism from R-01 (R-01 is "the handshake doesn't work"; R-17 is
"the handshake works but with the wrong session"). #4708: `Mcp-Session-Id` is a
per-session UUID, easy to capture once and accidentally reuse.

**Resolution (ADR-003):** each probe runs its **own** handshake and captures and
uses its **own** `Mcp-Session-Id`; A's session is never replayed against B's route
and vice-versa. INFRA-vs-RED holds for both directions: a handshake/session failure
is INFRA; a marker landing in the wrong store is RED.

**Test Scenarios**:
1. Assert the session id used on `/v1/A/mcp` `tools/call` is the one minted by A's `initialize`, and likewise for B — no cross-route reuse (a fresh `Mcp-Session-Id` per route).
2. Negative: a deliberately crossed session (A's id against B's route) must not be the path the gate exercises; the gate captures each route's own id.
3. A session failure on either route is **INFRA** (non-verdict for that direction), distinct from a wrong-store-marker **RED**, attributed to the correct direction.

**Coverage Requirement**: Each MCP direction uses its own captured
`Mcp-Session-Id`; no crossed/reused session is possible in the gate's path, and
session failures are INFRA per direction.

### R-18: Marker substring collision under `LIKE '%marker%'`
**Severity**: High **Likelihood**: Low
**Impact**: The MCP read is `content LIKE '%<marker>%'` — a **substring** match.
"Distinct" markers are insufficient: if one marker is a substring of another, a
cross-direction negative control silently `LIKE`-matches the wrong cell and
**false-GREENs a real leak** (or false-REDs). With four markers across two stores
this is a direct false-verdict surface.

**Resolution (ADR-002):** four **mutually non-substring** literals
`infra003-{obs,mcp}-{a,b}-<run>` — a shared per-run nonce plus a disjoint per-cell
tag, differing at the `obs/mcp` and `a/b` positions before the shared suffix, so no
`LIKE '%marker%'` read can match a different cell. The per-run nonce also prevents
matching residue from a prior run on a reused volume. (Distinct from R-12, which is
about SQL/LIKE *metacharacters* inside a marker; R-18 is about one marker being a
substring of another.)

**Test Scenarios**:
1. Assert the four marker literals are pairwise non-substring (no marker is contained in another) before any write.
2. With all four present in their own stores, a `LIKE '%<marker>%'` cross-cell query returns 0 — confirming no silent cross-match.
3. Markers carry the shared per-run nonce so a prior run's residue cannot satisfy any cell (ties to R-08).

**Coverage Requirement**: The marker set is mutually non-substring and per-run
unique; no `LIKE` read can match across cells or across runs.

---

## Integration Risks

- **Four writes, one container, marker-keyed read-as-barrier (R-05):** the four
  writes share two slug dirs and the old aggregate `store_size` barrier was unsound
  (satisfied by the first of a store's two writes). Resolved by the per-cell
  read-as-barrier (own-store-timeout=INFRA, wrong-store-presence=RED); the residual
  integration concern is sound INFRA-vs-RED discrimination in that loop.
- **MCP routing/dispatch divergence, both directions (R-01/R-02):** observe and
  MCP share `parse_project_key` + the same `Arc<dyn StoreResolver>` instance, but
  MCP diverges post-key into a per-slug `McpAdapter` with a boot-captured store
  (`adapter_for`). Both `A-mcp`→A and `B-mcp`→B must traverse that divergence; the
  observe pass is only transitive evidence for the shared key/lookup. Each MCP
  direction must use its **own** captured `Mcp-Session-Id` — a crossed session
  (R-17) mis-attributes the divergence it is meant to measure.
- **vol sidecar ↔ sqlite3 host boundary (R-04/R-06):** four marker reads cross
  from the distroless volume (busybox `vol cat`) to host `sqlite3`;
  WAL-completeness and dependency presence are the boundary's two failure modes.
- **Registration → restart → liveness-precondition → matrix (R-07):** both slugs
  must materialize as `ProjectEntry`s at the single boot; liveness is the
  precondition checkpoint, the 2×2 content read is the verdict.
- **Harness ↔ release-pipeline invariant (R-15) and ↔ standing lane (R-16):** the
  new script must be reconciled with the smoke-script invariant and adopted into
  the N5 recurring lane — both are delivery-time integration obligations, not
  in-gate logic.

## Edge Cases

- B store file does not exist yet (unregistered/missing) → INFRA, not a 0-row cell; now also corrupts B's positive (R-07).
- `-wal`/`-shm` already checkpointed and absent → acceptable only with a present, durable main db (R-04).
- A marker contains `%`, `_`, or `'` → query corruption/injection (R-12).
- One marker is a substring of another → `LIKE '%marker%'` cross-matches the wrong cell → false-GREEN; forbidden by the mutually non-substring `infra003-{obs,mcp}-{a,b}-<run>` set (R-18).
- Stale store/volume from a prior run carrying a prior marker → blocked by the per-run nonce (R-08/R-18).
- `context_correct` chosen with no prior entry to correct → write no-ops (R-02).
- MCP session evicted mid-handshake (`-32099`) → INFRA + re-init, not RED, per direction (R-01).
- A's `Mcp-Session-Id` accidentally reused against B's route → false attribution; blocked by per-route own session (R-17).
- Own-store marker never appears within the read-as-barrier deadline → INFRA, never RED, never a vacuous pass (R-05).
- New script trips the smoke-script invariant on first CI run (R-15).
- Restart not yet complete / HTTP transport not active when liveness probes run → bounded wait, then INFRA (R-05/R-07).

## Security Risks

This is a test harness, but it drives a real container and reads a shared volume —
its "untrusted inputs" are the slug literals, markers, and credentials, and its
blast radius is a real tenant store.

- **Slug literal as routing input (R-11, resolved):** the slug is taken from the
  transport path (`ProjectKey::Slug`, never payload — #4950 invariant 1,
  structurally sound). The harness risk was a slug literal colliding with a real
  project slug, causing a marker write into — or a verdict read from — a production
  tenant's store; `isolation-b` removes it.
- **One bearer token authorizes all four writes (by design):** because the slug is
  in the URL path, a single bearer token writes to both A and B. This is **not** a
  credential-isolation hole — identity comes from the path, not the token (#4950);
  the token authorizes *the caller*, the path selects *the tenant*. The gate must
  still establish each store's identity for the **read** side per-slug.
- **Credential cross-use on reads (SR-04 → R-07 context):** each store is read
  on-disk via `vol` (no per-slug read credential needed); if ever read over the
  wire, a slug must use its own credentials — one slug's credential never reads
  another's store, and the shared write token is never repurposed as a read proof.
- **SQL predicate injection via marker (R-12):** a marker reaching the host
  `sqlite3` query with a `'` is a (self-inflicted) injection surface across four
  markers; constrain the marker charset.
- **Volume read is read-only (`-v $VOL:/data:ro`):** the `vol` sidecar mounts
  read-only — the content read cannot mutate either store; preserve the `:ro`
  mount (a `:rw` regression would let the read corrupt the property it measures).
- **No `docker exec` into the distroless runtime:** all inspection via the sidecar
  (NFR-02) — keeps the test off the runtime's process surface.

## Failure Modes

| Condition | Required behavior |
|-----------|-------------------|
| Own marker present in own store (read-as-barrier reaches PRESENT) | gates that direction's negative cell (R-03/R-05) |
| Own-store marker never appears within the read-as-barrier deadline | **INFRA** for that direction; never RED, never a vacuous pass (R-05) |
| Cross-marker present in the other store (any of 4 cross-cells) | **RED** — the property is broken (incl. B mis-routed into A), independent of the positive outcome (R-05/R-03) |
| `sqlite3`/`vol` absent, missing main db, uncopied WAL | **INFRA**, distinct exit; never 0-row pass (R-04/R-06) |
| B route/store not live or unregistered | **INFRA**, never a phantom-store cell (R-07) |
| MCP handshake/SSE failure (either direction) | **INFRA** (transport), not RED (isolation) (R-01) |
| Crossed/reused `Mcp-Session-Id` | Structurally excluded — each route uses its own captured session (R-17) |
| Route responds non-404 but mis-resolves | Caught by the 2×2 content matrix, not by liveness (Headline, R-07) |
| Docker absent | **SKIP** exit 3; never green (R-10, #5180) |
| New script unreconciled with the smoke-script invariant | In-PR lockstep update, cross-linked on #815 (R-15) |
| All four positives PRESENT + all four cross-cells absent | **GREEN** — "ALL GATES PASSED" run-marker emitted |

Each state is a distinct, non-GREEN-coercible exit. INFRA and RED are never
rounded to GREEN; SKIP is never rounded to pass.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (sqlite3/`vol` absent → empty-pass) | R-06, R-10 | Preflight presence-assert; absence = hard INFRA on distinct exit (C1); never coerced to 0-row pass |
| SR-02 (WAL sidecars not copied → false-empty) | R-04 | `vol cat` copies db + `-wal`/`-shm` for both A and B; missing main db = INFRA (C6) |
| SR-03 (durability barrier unsound / pre-barrier read mistaken for verdict) | R-05, R-10 | Aggregate `store_size` barrier removed as unsound; positive-control read **is** the barrier (marker-keyed retry-until-present); own-store timeout = INFRA, wrong-store presence = RED (C5/C7) |
| SR-04 (two-slug credential surface / cross-credential) | R-07 (Security) | Each store read on-disk via `vol`; one write token (slug in path) authorizes writes only; no cross-credential read (C6) |
| SR-05 (overclaim N3) | R-14, R-16 | "advances, does not close N3"; N5/#788 adoption made a durable #788 linkage as the path to "maintained" (NFR-04) |
| SR-06 (parity reintroduction) | R-14 | No UDS probe, no parity entry; ADR-006 guard referenced not re-run (FR-08) |
| SR-07 (marker collision / substring false-match across cells) | R-08, R-18 | Four **mutually non-substring** markers (`infra003-{obs,mcp}-{a,b}-<run>`) in distinct tables; per-cell independent verdict; per-run nonce (C3/C4/C6/C7) |
| SR-08 (slug allowlist drift) | R-11 | A reuses `arch-research`; B is the written literal `isolation-b`; ADR-004 regex never re-typed (C2) |
| SR-09 (scope creep into H2/scaffold) | R-13 | Hold to H1 cumulative shell extension; H2 deferred (NFR-01) |
| SR-10 (MCP `debug_assert` compiled out → zero shipped coverage) | R-01, R-02, R-03, R-17 | MCP probe is a real `context_store` write + genuine `entries` content read, both directions, each with its own `Mcp-Session-Id`, positive-gates-negative (C4/C5/C6/C7) |
| SR-11 (single-restart ordering / B route) | R-07 + Headline | Register both before the one restart; liveness is a precondition (catches unregistered B); the bidirectional 2×2 read catches B mis-resolving into A (C2/C6/C7) |
| SR-12 (cumulative coupling to Gates 1–4) | R-13 | Separate self-contained script; sources only define-on-source primitives (C7) |

All twelve scope risks (SR-01…SR-12) are traced to at least one architecture-level
risk and resolution. R-11 (slug-B) is resolved by adopting `isolation-b`. R-05
(barrier soundness), R-17 (crossed session), and R-18 (substring collision) are
resolved-by-design in the latest ARCH/ADR-002/ADR-003 revision. R-15 (smoke-script
invariant) and R-16 (standing-gate orphan) are delivery-coordination risks beyond
the original SR set, now with concrete #815 / #788 linkage.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | 14 |
| High | 8 (R-06, R-07, R-08, R-09, R-10, R-15, R-16, R-17) | 26 |
| Medium | 4 (R-05, R-12, R-13, R-18) | 13 |
| Low | 2 (R-11, R-14) | 4 |

Notes: R-05 reclassified Critical → Med (barrier soundness fix adopted: marker-keyed
read-as-barrier; residual is sound INFRA-vs-RED discrimination). R-17 (crossed
`Mcp-Session-Id`) added as a High sibling of R-01; R-18 (marker substring collision)
added at Med (resolved-by-design via mutually non-substring markers). Register total
is **18** risks (R-01…R-18): 4 Critical, 8 High, 4 Medium, 2 Low.
