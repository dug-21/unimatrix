## ADR-002: Three Axes for `context_cycle_review` — the Third Is a Read-Only Scoped `transcript{}` Retrieval (NO Destructive Axis); `format` Render-Only (`"summary"` Dropped); `force` Recompute-Only

Feature: crt-057 · GH #894 · Re-scoped by human 2026-07-04 after ass-091 (#898)
Reworked: the prior `include_transcript_candidates: bool` (fused emit+purge) is superseded by the
read-only scoped `transcript{}` block. Retrieval mechanism / clock / window default: ADR-006.

### Context

`context_cycle_review` conflated caller intents onto one ungated path (crt-052). The re-scoped
contract exposes three **independent** parameters, and — the load-bearing change — **none of them is
destructive**. ass-091 established that the transcript axis should be a scoped, read-only retrieval,
not a boolean that also purges. Two API-surface choices are the architect's under that contract: the
*shape* of the transcript axis, and how the dead `"summary"` render alias is resolved.

Transcript-axis shape: a bare boolean (old contract) forced whole-stream-or-nothing AND fused the
purge; a verbosity-style `expand`/`detail` control is the wrong shape too (the report is
buffer-content-independent and byte-identical whether or not the buffer is present — ARCHITECTURE §5).
The right shape is a scoped filter block over the existing candidate pipeline, returning candidates +
loss, purging nothing.

Dead alias (SR-06): today `"markdown" | "summary"` are aliases at four render-dispatch loci
(`tools.rs:2532`, `:3359`, `:4268`, `:4324`); `"json"` is the other value; anything else →
`ERROR_INVALID_PARAMS`. `"summary"` is undocumented and distinct from the unrelated `context_get`
`format` axis. Left unresolved it is a hidden third render path that can drift.

### Decision

Expose three independent parameters on `RetrospectiveParams`; they do not intersect, there is no
precedence rule, and effects compose freely:

1. **`format: "markdown" | "json"`** (default `markdown`) — **render-only.** Chooses serialization;
   report content is identical either way. Never retrieves, never purges. **Drop the `"summary"`
   alias:** render dispatch accepts exactly `"markdown"` and `"json"`; `"summary"` now falls to the
   `ERROR_INVALID_PARAMS` arm at all four loci. Drop over fold-to-markdown: folding keeps a silent
   third accepted string that can diverge. Back-compat is a non-issue — `markdown` is the default.

2. **`force: bool`** (default `false`) — **recompute-only.** Rebuilds the report from the durable
   observation table (bypass memoization). Never retrieves candidates, never purges. Unchanged from
   crt-033.

3. **`transcript: Option<TranscriptScope>`** (`#[serde(default)]`; omit = none) — the **read-only
   scoped retrieval**. `TranscriptScope { phase?, anchor?, r#match?, window? }` is a set of
   all-optional, AND-composed filters over the EXISTING candidate pipeline via read-only `snapshot()`
   (mechanism: ADR-006). Omit = summary only (lean, non-destructive default). `transcript:{}` (present,
   all-None) = the full candidate set under the existing per-cycle cap ≡ `match:".*"` — the degenerate
   full dump. Returns candidates + per-session `SessionLossInfo` (ADR-003). **Purges NOTHING** — there
   is no destructive axis on this tool (ADR-001, NG-6). The block owns Plane B only and never touches
   summary derivation (the summary ⟂ Plane-B invariant).

**Clock normalization is part of the interface (Goal 5).** The agent expresses its query in its own
units — a finding/anchor id, a phase id, a regex, a window in events or time. Unimatrix normalizes
internally to the stored Plane-B unit (parse candidate `ts` to canonical epoch, windowed join,
`byte_offset` fallback for `ts:None`). The agent never needs to know Plane B's clock. Mechanism and
the window default (±120 s / ±3 blocks): ADR-006.

Load-bearing invariants: **the tool has no purge verb** (any path leaves the buffer intact); and
**`force` × `format` × `transcript` are mutually orthogonal** — `force` operates on the report,
`format` on serialization, `transcript` on a read-only buffer snapshot; disjoint state, nothing to
arbitrate.

### Consequences

Easier: no caller can hit a destructive path by any parameter combination (there is none); the lean
default is restored; retrieval is scoped and repeatable rather than whole-stream one-shot; the
force-vs-extract precedence gap dissolves (`force` never touches the buffer); the render contract is
exactly two values.

Harder: a caller (or test) passing `format:"summary"` now receives an error instead of silently
getting markdown — an intentional, contract-aligning break; the old `include_transcript_candidates`
boolean is removed, so every consumer migrating from it must switch to the `transcript{}` block (D-6);
the tool description and consumers must document the three axes precisely and state plainly that the
tool has no purge verb.

Cross-refs: ADR-001 (purge removed / residency), ADR-003 (loss propagation), ADR-004 (fold-read-only
seam gating), ADR-006 (scoped-retrieval mechanism + clock + window default), vnc-011 AC#10 (#952),
#4750, #4848 (single content reader).
