# ass-086 — Ground the hook-client size threshold: what constraint should it encode, what is the right measure, and what is the right number(s) + growth policy (input to #840)

## Problem Statement

The hook-client size cap (`packages/unimatrix/test/check-hook-client-size.js`) is
**ungrounded**. The measurement *mechanism* — comment-stripped PRIMARY + raw BACKSTOP via a
dependency-free state-machine stripper (ADR-005, Unimatrix #4806) — is sound and well-audited:
an embedded self-test corpus runs before every measurement and fails the gate closed on a
stripper bug. But the *number* `100,000` was inherited verbatim from vnc-026's old raw cap
(lesson #4780) and was **never tied to a real downstream constraint**.

It has since been bumped reactively under release pressure:
- PRIMARY: `100,000` → `101,000` (#839) → `102,000` (current file, `check-hook-client-size.js:34`)
- BACKSTOP: `160,000` → `180,000` (#775, the vnc-039 stdio→HTTPS MCP bridge, ~24KB new pure-JS)

The client is now pinned against **both** caps simultaneously (measured 2026-06-28):
**stripped 101,445 / 102,000** (555 B headroom) and **raw 179,944 / 180,000** (56 B headroom).
#840 wants to reclaim ~1.5 KB and lower PRIMARY back to `100,000` — but that just resets an
ungrounded number that will get bumped again the next time the client grows. The pattern is a
ratchet: a cap with no derivation has no defensible "no," so every increment wins.

This spike grounds the cap in whatever it actually protects **before** #840 re-sets it, so the
number #840 lands on is *derived*, not inherited, and arrives with a growth policy that ends the
reactive bumping.

## Goal

Answer four questions:

1. **What real constraint should the cap encode?** Rule each candidate in or out with evidence:
   - (a) **load/parse latency per hook invocation** — the client is spawned as a fresh
     `node .../lib/hook-client/index.js <EVENT>` process on every Claude Code hook event
     (`lib/merge-settings.js` builds this command form; `lib/init.js:456` wires the path), so
     parse cost is paid per event, not amortized.
   - (b) **npm package / install footprint** — `lib/` ships in `package.json` `files`; the
     hook-client is part of the published `@dug-21/unimatrix` tarball.
   - (c) **maintainability + zero-dep hand-auditability discipline** — the current de-facto
     purpose; the client must stay hand-readable and is separately gated zero-dependency
     (`check-zero-deps.js`).
   - (d) **an actual hard downstream limit** — something that inlines or transmits the client
     where bytes genuinely bite (an env/argv length bound, an inlined payload, a transport
     frame). If no hard downstream limit exists, **state that plainly** — a "soft discipline"
     answer is a valid and likely outcome.
2. **Is the current measure still correct?** Given the binding constraint, is comment-stripped
   PRIMARY + raw BACKSTOP the right metric, or does the constraint call for a different one
   (parse/load time, raw bytes only, per-file vs total, or the *loaded-subset* bytes rather than
   the whole tree — note a hook event loads ~19 of 26 modules; the `mcp-bridge/` subtree loads
   only for the bridge subcommand)?
3. **What is the right, grounded number(s) + growth policy?** Derive the number from the binding
   constraint rather than inheriting it, and define a growth policy so the cap stops getting
   bumped reactively. Account for the current near-zero headroom on **both** caps.
4. **What should #840 actually do?** In light of 1–3: the reclaim target, which cap(s) it should
   move, and whether "lower PRIMARY to 100,000" is still the right move or the number should be
   re-derived.

## Breadth

`code+ecosystem` — **code-dominant**. The primary surface is internal: our hook-client tree
(`lib/hook-client/`), the size gate, the zero-dep gate, the `package.json` `files`/`bin`, and
how the client is loaded (`bin/unimatrix.js`, `lib/init.js`, `lib/merge-settings.js`,
`postinstall.js`) and packaged. A **light** external scan of how comparable
zero-dependency / injected JS clients budget size is in scope only insofar as it informs the
growth policy — **not** industry-depth.

## Approach

`investigation` + a **quick targeted** `measurement` (latency probe).

- **Investigate** the load/packaging path to identify the binding constraint: what loads the
  client, what subset is loaded per event, what (if anything) imposes a hard byte ceiling
  downstream.
- **Probe** actual `require()`/parse cost across a few representative byte sizes **only** to rule
  load latency in or out — not a full empirical sweep. A preliminary measurement already exists
  (see Prior art): cold `require()` of the full index.js graph (~149 KB, 19 modules) is ~10–14 ms
  and is dwarfed by Node process startup; the spike should confirm/refine this with a small
  size-varied probe, not an exhaustive characterization.

## Confidence required

`directional`. A grounded recommendation (constraint + measure + number + policy) with **enough**
measurement to rule load latency in or out. A data-backed empirical sweep is **explicitly NOT
required** — a small probe sufficient to settle the latency question is the bar.

## Target outputs

FINDINGS.md containing:
- The **identified binding constraint** with evidence (which of a/b/c/d, and an explicit "no hard
  downstream limit" statement if that is the finding).
- The **recommended measure** — keep the current dual-limit or change it — with rationale.
- The **recommended grounded number(s)** + a **growth policy** that ends reactive bumping.
- An **explicit #840 recommendation**: reclaim target, which cap(s), whether `100,000` stands or
  is re-derived.
- The **latency-probe result** (rule-in / rule-out, with the few data points behind it).
- **Unanswered Questions** and **Out-of-Scope Discoveries**.

## Constraints

**Hard** (fixed; changing one means changing shipped code or breaking the gate's integrity):
- The client stays **zero-dependency and hand-auditable**. `check-zero-deps.js` enforces this:
  no runtime `dependencies` in `package.json` (the `optionalDependencies` platform binaries are
  the LOCAL Rust path, not consumed by the pure-JS remote client) and every `require()` reachable
  from `lib/hook-client/` resolves to a Node built-in or a relative path. Any recommendation must
  preserve this.
- **CAP-CHANGE RULE (ADR-005, #4806):** any change to either limit is a **human decision recorded
  on the GH issue** — never an agent adjustment. Raising a cap to make a failing gate pass is a
  **vacuous pass and is forbidden**. This spike *recommends*; it does not change the cap.
- The **stripper self-test must stay green** — the gate measures only after the embedded
  `SELF_TEST_CORPUS` passes and **fails closed** on a stripper bug. No recommendation may weaken
  this fail-closed property.
- **Lockstep meta-assertions (#5312):** the cap constants are duplicated in
  `test/hook-client/size-gate.test.js` (`test_limits_are_decimal` hard-codes the PRIMARY value)
  and the header-doc test greps the source for the literal `"100,000"`. Any number the spike
  recommends must note that #840 has to move these in lockstep, or the standalone gate goes green
  while the full `node --test` run fails.
- **Read-only in Unimatrix.** No code committed, no PR. This is a research spike feeding #840.

**Hypothesis** (challengeable positions to test, NOT assumptions to carry):
- "**100,000 is a meaningful threshold**" — it is inherited from vnc-026's raw cap, not derived.
  Challenge it.
- "**Comment-stripped is the right primary measure**" — challenge whether the binding constraint
  actually cares about stripped bytes (a latency or install-footprint constraint would care about
  *raw* or *loaded* bytes; comment-stripping serves a documentation-fairness goal that may be
  orthogonal to the real constraint).
- "**The client must keep shrinking**" — the right answer may be a grounded *higher* cap plus a
  policy, not endless reclaim. Endless reclaim that fights documentation is the #4780 failure mode
  in a new guise.

## Background Research / Prior art

Read directly for this scope:
- **Gate file** `packages/unimatrix/test/check-hook-client-size.js` — dual limit, decimal bytes,
  current `PRIMARY_LIMIT = 102000` / `BACKSTOP_LIMIT = 180000`, the inline raise-rationale
  comments citing #839 and #775, the state-machine stripper, the embedded self-test, and the
  DI seams (`runGate`/`measureTree`/`stripFn`).
- **Zero-dep gate** `packages/unimatrix/test/check-zero-deps.js` — static require-scan; the
  hard zero-dependency invariant the size recommendation must not disturb.
- **Packaging** `package.json` — `files: [bin/, lib/, skills/, postinstall.js, protocols/]`,
  `bin.unimatrix = bin/unimatrix.js`, `optionalDependencies` are the platform Rust binaries.
- **Load path** — the client is spawned per hook event as a fresh `node .../hook-client/index.js
  <EVENT>` process (`lib/merge-settings.js` builds the command; `lib/init.js:456` resolves the
  path; `bin/unimatrix.js:54` lazy-loads `mcp-bridge.js` only for the `mcp-bridge` subcommand).
- **NOTE — do not mistake the decoder for a bundle:** `lib/hook-client/bundle.js` is a
  connection-token **decoder** (`decodeBundle`, used by `lib/init.js`), **not** a packaging
  bundle / minified artifact. There is no build/minify step; every byte gating against the cap is
  hand-written source. This rules out a "shrink the bundle" framing.

Current measured state (2026-06-28):
- Tree: **26** `.js` files under `lib/hook-client/`, raw total **179,944 B**, stripped total
  **101,445 B**. Largest: `index.js` (17,066 raw), `build-request-tools.js` (12,483),
  `state.js` (11,382), `config.js` (11,097), `transport-http.js` (11,009).
- **Per-event load subset:** requiring `index.js` pulls in **19** of the 26 modules (~149 KB
  raw); the `mcp-bridge/` subtree (~17 KB) and a few others load only for the bridge subcommand,
  not for hook events. The cap measures the **whole tree**; a hook event parses only a **subset**
  — a relevant input to Goal 2 (is "total tree" even the right denominator for a latency
  constraint?).
- **Preliminary latency probe:** cold `require()` of the full `index.js` graph measures
  ~10–14 ms (5 runs), and is small relative to Node's own process-startup cost. Directionally this
  points toward latency **not** being the binding constraint, but the spike should confirm with a
  small size-varied probe before ruling it out.

Unimatrix knowledge:
- **ADR-005 (#4806)** — the dual-limit design and rationale: comment-stripped 100,000 PRIMARY
  measures *shipped logic only* (so oracle-citation comments don't compete with code — the #4780
  driver); raw BACKSTOP caps total growth so a stripper miscount can't admit unbounded files;
  decimal interpretation; cap changes are human decisions on the issue. **Note ADR-005's own
  numbers (100,000 / 160,000) are the *inherited* baseline this spike interrogates.**
- **Lesson #4780** — vnc-026 origin: the client shipped at 104,240 B against a single 100,000 B
  **raw** cap; root cause was verbose parity-port JSDoc, not logic bloat. Takeaway: "trim comment
  prose only, never minify or inflate the limit." This is where the decimal 100,000 came from —
  and it was a *raw* cap, repurposed as a *stripped* cap by ADR-005, so the number never matched
  the new measure's semantics.
- **Pattern #4820** — the dual-limit pattern statement; how to budget hook-client additions
  against the stripped budget.
- **Pattern #5312** — the lockstep meta-assertion trap (see Constraints): `test_limits_are_decimal`
  hard-codes PRIMARY, and the header-doc test greps for `"100,000"`; the #839 bump tripped exactly
  this (cap moved to 101000 but the meta-assertion stayed at 100000).

Issues:
- **#839** — the COMPLETE critical-availability fix (transport/connect timeout + silent-eviction
  self-heal + F6 mid-stream idle-read deadline + SSE-timeout heal routing) that drove PRIMARY
  100000 → 101000 → 102000 after ~1420 B in-bridge reclaim.
- **#775** — the BACKSTOP 160000 → 180000 raise for the vnc-039 stdio→HTTPS MCP bridge (~24 KB).
- **#840** — the reclaim task this spike feeds: reclaim ~1.5 KB and (currently) lower PRIMARY back
  to 100,000.

## Proposed Approach

Settled with the human and research leader (do not re-litigate): a code-dominant investigation of
the load and packaging path to identify the binding constraint, plus a quick latency probe to rule
load-latency in or out, producing a directional recommendation — constraint + measure + grounded
number(s) + growth policy + explicit #840 guidance. Light external scan only to inform policy.

## Acceptance Criteria

- **AC-01:** Each constraint candidate (a) load/parse latency, (b) install footprint,
  (c) maintainability/zero-dep discipline, (d) a hard downstream byte limit — is explicitly ruled
  **in** or **out** with evidence; if no hard downstream limit exists, that is stated plainly.
- **AC-02:** The single **binding** constraint (or the explicit finding that the cap is a soft
  discipline with no hard binder) is named.
- **AC-03:** A recommendation on the **measure** — keep comment-stripped PRIMARY + raw BACKSTOP, or
  change it — is given with rationale tied to the binding constraint, addressing whole-tree vs
  loaded-subset and stripped vs raw.
- **AC-04:** A **grounded number (or numbers)** is recommended, *derived* from the binding
  constraint, with the derivation shown — not inherited from vnc-026.
- **AC-05:** A **growth policy** is defined that gives the cap a defensible basis for future
  raise/hold decisions, so increments stop being reactive.
- **AC-06:** An **explicit #840 recommendation**: reclaim target, which cap(s) to move, and whether
  `100,000` stands or is re-derived — including the lockstep meta-assertion (#5312) update #840
  must make.
- **AC-07:** The **latency-probe result** is reported as an explicit rule-in / rule-out, with the
  handful of data points behind it.
- **AC-08:** FINDINGS.md includes **Unanswered Questions** and **Out-of-Scope Discoveries**.

## Open Questions

- Is there any consumer that **inlines or transmits** the client bytes (an env var, an argv, a
  transport frame, a future bundling step) where a hard byte ceiling genuinely exists — or is the
  client only ever spawned from disk by path, making raw size a soft concern?
- If the binding constraint is latency, the right denominator is the **loaded subset per event**,
  not the whole tree — should the gate then measure the `index.js` require-closure rather than all
  `*.js`? (And how would that interact with the `mcp-bridge/` subtree that loads on a different
  path?)
- Does comment-stripping still serve a purpose if the binding constraint cares about raw/loaded
  bytes rather than shipped logic, or is the PRIMARY/BACKSTOP split solving the #4780 documentation
  problem at the cost of obscuring the real constraint?
- Can the growth policy be tied to a measurable budget (e.g. a latency ceiling per event, or a
  fraction of the install footprint) rather than a hand-picked byte count?

## Tracking

GH research issue **#861** (ass-086). Single spike → executed via `uni-spike-researcher` once
scope is confirmed complete. Findings feed **#840** (the reclaim + cap-reset task).
