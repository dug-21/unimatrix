# FINDINGS: Ground the hook-client size threshold (constraint + measure + number + growth policy)

**Spike**: ass-086
**Date**: 2026-06-28
**Approach**: investigation + quick targeted measurement (latency probe)
**Confidence**: directional

---

## Findings

### Q1: "What real constraint should the cap encode?" — rule each candidate in/out

**Answer**: The binding constraint is **(c) maintainability + zero-dep hand-auditability discipline**. It is a **soft discipline with no hard downstream binder**. (a), (b), and (d) are ruled **out** with evidence below.

**(a) load/parse latency per hook invocation — RULED OUT.**
Evidence (latency probe, Node v24.16.0, 8-10 runs each, medians; see Q-latency for full table):
- Bare `node -e ""` startup: ~11.7 ms.
- `node` + `require()` of the full `index.js` graph (19 modules, ~149 KB): ~22.5 ms, i.e. **~10.8 ms over bare startup**.
- Pure parse cost is byte-insensitive: synthetic single files of 50->100 KB add **0.3 ms**; 150 KB adds 1.5 ms; 300 KB adds 3.6 ms. Roughly **~16 us per added KB**.
- The ~10.8 ms require delta is dominated by *module count and top-level work* (resolving 19 modules, compiling regexes, building tool schemas), **not raw bytes** — a synthetic 150 KB single file costs only ~1.5 ms to parse vs the real graph's 10.8 ms.
Implication: doubling the client (~150 KB -> ~300 KB) would add **~2 ms** of parse — noise against the ~12 ms Node startup the client already pays per event, and against the network round-trip a hook event performs. Bytes do not move per-event latency. Latency cannot ground the cap.

**(b) npm package / install footprint — RULED OUT (real but non-binding).**
Evidence: `npm pack --dry-run` -> published tarball is **373 KB unpacked / 115 KB packed, 48 files**. The hook-client tree is ~180 KB raw (~half the JS tarball). The heavy install weight — the platform Rust binary + onnxruntime — ships in the **separate** `optionalDependencies` packages (`@dug-21/unimatrix-linux-x64`), not this tarball. So hook-client bytes are <1% of a full local install and ~half of a tiny JS-only install. Doubling the client adds ~90 KB packed — immaterial for an npm package. Footprint is real but nowhere near a binding threshold.

**(c) maintainability + zero-dep hand-auditability — RULED IN (binding).**
Evidence: the de-facto purpose, confirmed by origin. Lesson #4780: the cap exists because verbose oracle-citation JSDoc (each JS module is a line-for-line Rust parity port) drifted the client over budget — a *documentation/readability* pressure, not a physical one. ADR-005 (#4806) split the measure precisely to protect hand-readability: strip comments so docs don't compete with the logic budget. The companion `check-zero-deps.js` enforces the structural half (no runtime deps, every `require()` resolves to a built-in or relative path). The size cap is the **review-trigger half** of the same hand-auditability invariant: keep the client small enough to read and reason about, and force a human checkpoint when it grows.

**(d) a hard downstream byte limit — RULED OUT. There is no hard downstream limit. Stated plainly: the client bytes are never inlined or transmitted; the client is always spawned from disk by path.**
Evidence across the entire load/packaging path:
- `lib/merge-settings.js` `buildHookClientCommand()` writes `node <path> <event>` into `settings.json` — the **absolute path**, never the bytes.
- `lib/init.js:456` resolves the client via `require.resolve("./hook-client/index.js")` and writes the **path** into the hook command; the credential/bundle goes to an out-of-tree store, not the command line.
- `bin/unimatrix.js:54` invokes the bridge via `require("../lib/hook-client/mcp-bridge.js")` and `.mcp.json` runs `node <bridge> <projectHash>` — by **path**.
- `lib/hook-client/bundle.js` is a connection-token *decoder* (`decodeBundle`), **not** a packaging bundle; there is no build/minify/inline step (confirmed per SCOPE prior-art note).
No env var, argv, transport frame, or inlined payload ever carries the client bytes. No argv/env length bound applies. Raw size is a soft concern only.

**Recommendation**: Treat the cap explicitly as a **soft maintainability/auditability discipline with no hard binder**. Stop framing it (in headers, issues, or reclaim tasks) as if a physical limit is being protected — that framing is what produced the inherited, ungrounded `100,000` and the reactive ratchet.

---

### Q2: "Is the current measure still correct?"

**Answer**: **Yes — keep comment-stripped PRIMARY + raw BACKSTOP, measured over the whole tree.** The measure is correctly aligned to the binding constraint even though the *number* was not. Do not switch to loaded-subset or raw-only.

**Evidence / rationale (tied to the binding constraint):**
- **Stripped vs raw**: a maintainability constraint cares about *shipped logic readability*, and wants documentation to grow freely. Comment-stripped PRIMARY is exactly that metric — it is the mechanism that fixed #4780 (oracle-citation comments no longer compete with code for budget). A raw-only measure would resurrect the #4780 failure (byte pressure punishing documentation). So stripping **still serves a real purpose** under the binding constraint; it is aligned, not orthogonal. Keep it as PRIMARY.
- **Keep the raw BACKSTOP**: its job is anti-gaming / anti-miscount — a stripper bug can't admit unbounded files, and minified-style code can't game the stripped budget unnoticed. That role is independent of which constraint binds. Keep it.
- **Whole-tree vs loaded-subset**: the loaded-subset denominator (the `index.js` require-closure, ~19 of 26 modules) would only matter if **latency** bound — and it does not (Q1). For an auditability constraint, **every shipped byte must be auditable, including the `mcp-bridge/` subtree** that loads only on the bridge subcommand. Switching to the require-closure would (1) stop gating the bridge bytes entirely and (2) couple the gate to fragile require-graph analysis. **Keep whole-tree.**

**Recommendation**: Retain the dual stripped-PRIMARY / raw-BACKSTOP measure over `lib/hook-client/**/*.js`. The measure is right; only the numbers need regrounding.

---

### Q3: "What is the right, grounded number(s) + growth policy?"

**Answer**: Because the constraint is soft, the number **cannot be derived from a physical ceiling** — and chasing a round physical-looking number (`100,000`) is the #4780 failure mode reborn. Ground it instead as a **headroom-budget**: `cap = deliberate baseline + a fixed review margin of ~one module`, paired with a growth policy that makes the margin a *review-scheduling device*, not a silent-growth allowance.

**Derivation (shown):**
- Current measured (2026-06-28): stripped **101,445 / 102,000** (555 B headroom); raw **179,944 / 180,000** (56 B headroom).
- Module unit (the natural budget grain): 26 modules, raw total 179,944 -> **avg ~6,921 B raw / ~3,902 B stripped per module**; typical real modules span ~3-11 KB raw (`session-id.js` 2,672 ... `transport-http.js` 11,009). Take **one module of review margin ~= ~5 KB stripped / ~10 KB raw** (generous-typical).
- **PRIMARY**: baseline = post-#840 stripped size (~=100,000 if the #839 temp scaffolding is genuinely recovered; <=101,445 otherwise) **+ ~5 KB review margin** -> **PRIMARY ~= 105,000 stripped**.
- **BACKSTOP**: baseline = post-#840 raw size (~178,500) **+ ~10 KB review margin** -> **BACKSTOP ~= 190,000 raw**. This also restores real headroom: the current 56 B is itself a latent reactive-bump trigger (any addition trips it), so BACKSTOP must be regrounded too, not just PRIMARY. (Cross-check: the measured raw/stripped ratio is 179,944/101,445 = **1.77**; 190,000/105,000 = 1.81 — consistent with the tree's actual comment density plus margin.)

**Recommended numbers**: **PRIMARY 105,000 stripped, BACKSTOP 190,000 raw.**

**Growth policy (ends reactive bumping) — "headroom-budget with mandatory pre-merge re-budget":**
1. The cap is `measured baseline + one module of margin (~5 KB stripped / ~10 KB raw)`. The margin is **runway, not permission to grow silently**.
2. A raise is granted **only for new shipped LOGIC** (new behavior). It is **never** granted to accommodate documentation (the stripped measure already absorbs docs) and **never** to accommodate temporary scaffolding (that gets reclaimed — the #839 pattern).
3. When a planned change would consume the margin, the author must either fit under the current cap **or** open the ADR-005 CAP-CHANGE RULE human review on the issue **before merging** — never a post-hoc raise to turn a red gate green (that remains a forbidden vacuous pass).
4. Each approved raise **resets the baseline to the new measured size and re-adds exactly one module of margin** — so the cap tracks real growth plus constant runway and never ratchets on round numbers.
5. The cap is *reviewed* (not necessarily changed) at every release that touches `lib/hook-client/`, making it a living budget.
This gives the cap a defensible "no": the default answer to "just bump it" is no, because the policy names exactly what qualifies (new logic, human review, baseline reset). (Open Question 4 — yes, the budget is tied to a measurable unit: the per-module average, not a hand-picked byte count. External practice agrees directionally: `size-limit`/`bundlesize`-style budgets are an absolute byte ceiling + fixed margin + explicit human bump — the same headroom-budget shape.)

---

### Q4: "What should #840 actually do?"

**Answer**: Re-derive, don't reset to 100,000. `100,000` does **not** stand.

- **Reclaim target**: recover only the **genuinely temporary #839 in-bridge scaffolding** (the ~1.4 KB the #839 comment flagged as TEMP) **if and only if it is real waste**. Do **not** trim live documentation or live logic to chase a round number — that is the #4780 failure mode. If the #839 bytes turn out to be permanent availability logic, keep them and set the baseline accordingly.
- **Which cap(s) to move**: **both.** PRIMARY 102,000 -> **105,000** (replaces the TEMP 102,000 with a grounded baseline+margin). BACKSTOP 180,000 -> **190,000** (the 56 B current headroom is a latent reactive-bump trigger; reground it now in the same change).
- **Does `100,000` stand?** **No — re-derived to 105,000 (PRIMARY).** Lowering to 100,000 would force a documentation/logic reclaim fight to hit an inherited number that has no defensible basis and would get bumped again on the next addition.
- **Lockstep meta-assertion updates #840 must make (Pattern #5312) — all in one change, or the standalone gate goes green while `node --test` fails:**
  1. `test/check-hook-client-size.js:34` — `PRIMARY_LIMIT` constant -> new value.
  2. `test/check-hook-client-size.js:35` — `BACKSTOP_LIMIT` constant -> new value.
  3. `test/check-hook-client-size.js` header doc block (lines ~8-9) — the literals `100,000` / `180,000` in the prose. The header-doc test greps the source with `src.includes("100,000")` and `src.includes("180,000")`, so the narrative literals must change **and** the grep targets in the test must change together.
  4. `test/hook-client/size-gate.test.js:267` — `assert.strictEqual(PRIMARY_LIMIT, 102000)` -> new value.
  5. `test/hook-client/size-gate.test.js:271` — `assert.strictEqual(BACKSTOP_LIMIT, 180000)` -> new value.
  6. `test/hook-client/size-gate.test.js:255-256` — `assert.ok(src.includes("100,000"))` / `assert.ok(src.includes("180,000"))` header-doc greps -> new literals.
  (Per ADR-005 / #4806 CAP-CHANGE RULE: this is a **human decision recorded on #840**, with the rationale "regrounded as baseline+one-module margin per ass-086," not an agent adjustment.)

---

## Latency-probe result (AC-07)

**Rule-out.** Load/parse latency is **not** a binding constraint and does not change with client size in any meaningful range.

Node v24.16.0, medians (8-10 runs each):

| Measurement | Median | Delta over bare startup |
|---|---|---|
| bare `node -e ""` | 11.7 ms | — |
| `node` + `require(index.js)` graph (19 mods, ~149 KB) | 22.5 ms | +10.8 ms |
| parse synthetic 50 KB single file | 12.0 ms | +0.3 ms |
| parse synthetic 100 KB | 12.0 ms | +0.3 ms |
| parse synthetic 150 KB | 13.2 ms | +1.5 ms |
| parse synthetic 200 KB | 14.7 ms | +2.9 ms |
| parse synthetic 300 KB | 15.3 ms | +3.6 ms |

Takeaways: (1) raw parse is ~16 us/KB — a 2x client growth adds ~2 ms; (2) the require graph's 10.8 ms is module-count/top-level-work, not bytes (a 150 KB single file parses in 1.5 ms vs the graph's 10.8 ms); (3) per-event cost is dominated by the ~12 ms Node process spawn, which size does not touch. Confirms and refines the preliminary ~10-14 ms figure. (Probe script: `scratchpad/probe.js`.)

---

## Unanswered Questions

- **Is the specific ~1.4 KB of #839 genuinely reclaimable temp scaffolding, or live availability logic?** Requires reading the #839 bridge diff in detail — that is #840's implementation work, not this directional spike. The recommendation is robust to either answer (baseline shifts; PRIMARY still lands ~105,000), but the exact reclaim target is #840's to confirm.
- **Depth of external benchmarking.** Per SCOPE the external scan was light and policy-only; the growth policy was derived from internal module structure plus the directional `size-limit`/`bundlesize` shape, not a deep survey. A deeper comparison is available if wanted but is not load-bearing for directional confidence.

---

## Out-of-Scope Discoveries

- **BACKSTOP is already at 56 B headroom — a latent reactive-bump independent of #840's stripped reclaim.** The next pure-JS addition (even comment-only) trips the raw gate regardless of the stripped budget. Folded into the Q3/Q4 recommendation (move BACKSTOP to 190,000), but flagged because it would otherwise force a *second* reactive bump right after #840. Why it matters: regrounding only PRIMARY leaves the same ratchet alive on the raw axis.
- **Header-doc test is a substring grep, so the header narrative can silently drift from the live constant** (already noted in #5312). A test that asserts the header literal *equals* the active constant (rather than merely containing some `"100,000"` string) would close the drift. Candidate test-hardening item — not pursued here.
- **Per-event cost is dominated by fresh-process spawn (~12 ms), not bytes.** If per-event hook latency ever becomes a real concern, the lever is the spawn model (process reuse / a resident bridge), not byte trimming. Carry-forward for any future latency work — do not attack it by shrinking the client.

---

## Recommendations Summary

- **Q1 (constraint)**: Binding constraint is **(c) maintainability/hand-auditability — a soft discipline with no hard binder**; (a) latency, (b) footprint, (d) hard downstream limit all ruled out. The client is always spawned from disk by path; its bytes are never inlined or transmitted.
- **Q2 (measure)**: **Keep** comment-stripped PRIMARY + raw BACKSTOP over the **whole tree**. The measure is correctly aligned to the maintainability constraint; do not switch to loaded-subset or raw-only.
- **Q3 (number + policy)**: Reground as **headroom-budget**: PRIMARY **105,000 stripped**, BACKSTOP **190,000 raw** (baseline + ~one-module margin: ~5 KB stripped / ~10 KB raw). Growth policy: raises only for new *logic*, only via ADR-005 human review *before* merge, each raise resets baseline + re-adds one module of margin — ending reactive bumping.
- **Q4 (#840)**: `100,000` does **not** stand — re-derive to **105,000**. Recover only genuine #839 temp waste (never trim live docs/logic to hit a number); move **both** caps; update all **six** lockstep sites (#5312) in one human-recorded change on #840.
- **Latency probe**: **Ruled OUT** — ~16 us/KB parse; a 2x client adds ~2 ms; per-event cost is the ~12 ms Node spawn, size-independent.
