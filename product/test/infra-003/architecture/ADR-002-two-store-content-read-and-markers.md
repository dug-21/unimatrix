## ADR-002: Two-store content read — read-as-barrier retry model + non-substring four-marker 2×2

### Context

The feature exists to avoid a vacuous test. The positive control must prove a
slug's store **contains its own marker** (not a `du` size delta); the negative
control must prove the **other** store does **not** (a genuine two-store read, not
a dir-count / `other_count` heuristic — the removed D6 trap, ass-084 OoS#2). The
test is bidirectional, so the verdict is a full **2×2 matrix per surface**.
SR-01/02/03/07/10 cluster here.

Two design-gate hazards drove this revision:

1. **An aggregate `store_size` ("store grew") barrier is unsound.** `store_size`
   is a `du` delta over a slug dir. Each store now receives **two** writes (A gets
   `A-obs` then `A-mcp`); "A grew" is satisfied the moment the *first* write lands
   and says **nothing** about whether the *second* is durable. Writes are
   `tokio::spawn` fire-and-forget with WAL `synchronous=NORMAL`, so a content read
   gated only on "A grew" can race the unsynced `A-mcp` write and **false-RED** the
   positive control.

2. **`LIKE '%marker%'` is a substring match.** The MCP read queries `entries`
   with `content LIKE '%<marker>%'`. If one marker is a substring of another, a
   cross-direction negative control silently `LIKE`-matches and **false-GREEN**s a
   real leak. "Distinct" is not enough — the literals must be mutually
   non-substring.

Code investigation fixes the queryable columns: Observe —
`ImplantEvent.topic_signal: Option<String>` (`wire.rs:267`) → `observations.topic_signal
TEXT` (`db.rs:865`, `analytics.rs:539-554`). MCP — `context_store` persists an
`entries` row; marker in `content` → `entries.content TEXT` (`db.rs:544`).

### Decision

One parameterized two-store content-read primitive, built fresh (it does not
extend the removed D6 count logic):

1. **Four mutually non-substring markers** (SR-07), one per matrix cell, from a
   shared per-run nonce `<run>` plus a disjoint per-cell tag:
   - `infra003-obs-a-<run>` → `RecordEvent.topic_signal` via `POST /v1/A/observe`.
   - `infra003-obs-b-<run>` → `RecordEvent.topic_signal` via `POST /v1/B/observe`.
   - `infra003-mcp-a-<run>` → `context_store` `content` via `POST /v1/A/mcp`.
   - `infra003-mcp-b-<run>` → `context_store` `content` via `POST /v1/B/mcp`.
   These four are pairwise non-substring (they differ at the `obs/mcp` and `a/b`
   positions before the shared `<run>` suffix), so no `LIKE '%marker%'` read can
   silently match a different cell. The per-run nonce also prevents matching
   residue from a prior run on a reused volume. Delivery MUST use these literals
   (or another provably mutually-non-substring set), not ad-hoc tokens.

2. **Read mechanism** (reusing the `cloud-bundle-lib.sh` idiom verbatim, SR-02):
   `vol cat` each per-slug `unimatrix.db` **plus its `-wal`/`-shm` sidecars** out to
   a host sandbox, then query **host-side** with `sqlite3`. A single-file copy reads
   a pre-checkpoint false-empty snapshot — the sidecars are mandatory. A missing
   main db is INFRA; a missing already-checkpointed WAL sidecar is fine.

3. **sqlite3 discipline** (SR-01/AC-11): `command -v sqlite3` asserted in preflight;
   absence is a **hard INFRA fail** (provisioned host-side like `node`), never a
   silent empty capture that empty-passes.

4. **Read-as-barrier positive control** (SR-03, replaces the aggregate barrier):
   writes are issued **strictly sequentially per store**, and each positive control
   is a **bounded retry-until-present** read keyed to *that cell's* marker — the
   read is its own durability barrier, keyed to the specific marker rather than the
   ambiguous size proxy. A not-yet-synced write has simply not appeared yet → keep
   polling. **Timeout → INFRA, never RED** (AC-10): if the own marker never appears
   within the deadline, durability could not be established — a non-verdict, not a
   property failure. The old `store_size`-grew barrier is removed; `store_size` is
   kept only for boot/liveness waits (ADR-004).

5. **Cross-store negative control** is a **single** read (not a retry) for the
   other store, evaluated **after** that direction's positive control reached
   PRESENT. Finding the marker in the other store is **RED** (a real leak),
   independent of the positive outcome.

6. **Full 2×2 matrix**, presence-based: Observe — A has `obs-a` not `obs-b`; B has
   `obs-b` not `obs-a`. MCP — A has `mcp-a` not `mcp-b`; B has `mcp-b` not `mcp-a`.

7. **Positive-gates-negative, per direction** (AC-05/AC-09): a direction's negative
   cell is reported GREEN only after its positive reached PRESENT. The B-direction
   cross-store cell is what catches B mis-resolving into A.

8. **Each store read on-disk via `vol`** (AC-12/SR-04): no slug's credential reads
   another's store; one bearer token authorizes only the writes (slug in path).

### Consequences

- **Easier:** the read-as-barrier keys durability to the *exact* marker, so a
  two-write-per-store sequence can never race; a slow sync is INFRA (retry), never
  a false-RED — and a real mis-route is still RED via the cross-store cell.
- **Easier:** mutually non-substring markers make a `LIKE` cross-match impossible;
  the per-run nonce isolates runs on a reused volume.
- **Easier:** each positive is a true presence assertion on a named column
  (`observations.topic_signal`, `entries.content`) — fails/holds exactly on the
  property (#5177/#2758); reuses the proven `vol cat`+WAL+sqlite3 idiom.
- **Harder:** the positive read is now a deadline-poll loop (more than a single
  query), and INFRA-vs-RED must be discriminated carefully (own-store timeout =
  INFRA; wrong-store presence = RED). Accepted — it is the soundness fix.
- **Harder:** depends on host `sqlite3` and two server columns; a schema rename
  breaks the query (accepted — load-bearing seams; a break surfaces as RED/INFRA).
- The four marker literals and the MCP verb (`context_store` vs `context_correct`)
  are pinned in the spec; slugs A = `arch-research`, B = `isolation-b`.

Related: ADR-001 (the shell gate that hosts this), ADR-003 (the bidirectional
per-session MCP markers), ADR-004 (registration ordering + `store_size` retained
for liveness, not the barrier).
