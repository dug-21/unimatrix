# Gate 3a Report: vnc-045

> Gate: 3a (Design Review) — REWORK RE-CHECK (iteration 1)
> Date: 2026-07-07
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | 5 components map 1:1 to ARCH §2; only active ADR-001/002/004/008/009 designed; no protected_tags/config/validator/min_trust_level/cadence surface anywhere. |
| 2. Specification coverage | PASS | All 11 FRs + 7 NFRs have corresponding pseudocode; no scope additions. |
| 3. Risk coverage | PASS | All 8 risks mapped to scenarios; R-01/R-02/R-03 High comprehensive. |
| 4. Interface consistency | **PASS** (was FAIL) | `derive_namespace` and `check_tag_lifecycle` now designed as `pub(crate)` module-scope seam fns in OVERVIEW + context-tag-handler.md; handler Steps 5/7 CALL them; R-06 boundary table + R-05 guard decision bind the extracted fns (runnable without a `RequestContext`). Reworked as required. |
| 5. Knowledge stewardship | PASS | Both design-phase agent reports carry `## Knowledge Stewardship`; pseudocode (read-only tier) Queried entries (MCP unavailable — documented) + rework note; testplan Queried + Stored-with-reason. |

**Prior gate returned REWORKABLE FAIL on Check 4 only.** The pseudocode agent extracted both inline decisions into designed `pub(crate)` seam fns. Re-verified holistically: no regression in Checks 1/2/3/5, all delivery-critical items intact, no deferred surface introduced, no handler unit tests planned.

## Rework Verification (Check 4 — the sole prior blocker)

**Status:** PASS (was REWORKABLE FAIL)

Both decisions are now first-class designed seam fns, not inline handler logic:

- **OVERVIEW.md "Handler seam functions" (lines 85–106):** declares
  `pub(crate) fn derive_namespace(tag: &str) -> Option<String>` and
  `pub(crate) fn check_tag_lifecycle(status: &Status) -> Result<(), LifecycleRejection>`
  (with `pub(crate) enum LifecycleRejection { Quarantined }`), both at module scope in `mcp/tools.rs`,
  explicitly "NOT inside the `#[tool]` fn body," "unit-callable, NO `RequestContext`."
- **context-tag-handler.md Step 5 (line 74):** `let namespace: Option<String> = derive_namespace(&tag)` — CALLS the extracted fn; annotated "NOT inline — pattern #5389/#5468."
- **context-tag-handler.md Step 7 (lines 92–96):** `match check_tag_lifecycle(&entry.status)` — CALLS the extracted fn; handler maps `Err(Quarantined)` → `invalid_params`.
- **Bodies designed** in context-tag-handler.md "Extracted seam functions" (lines 113–144): `derive_namespace` = substring before first `:`; `check_tag_lifecycle` = `Quarantined→Err`, else `Ok`. Status-only (action not consulted) so the R-05 table is exhaustive over the three `Status` values.
- **Sequencing wording reconciled** (OVERVIEW lines 168–175): the prior "Derivation lives in the HANDLER" now reads "the handler CALLS the extracted `pub(crate) fn derive_namespace` — the logic is NOT inline," plus a matching "Lifecycle decision is an extracted seam" bullet.

**Test bindings are runnable against the extracted fns:**
- **R-06** (test-plan/context-tag-handler.md lines 16–27): a 6-row table calling `derive_namespace` directly (pure, no store, no `RequestContext`) — standard/colon-terminated/colon-less/multi-colon/mid-string/empty. Runnable.
- **R-05** (test-plan/store-tag-service.md lines 23–29): the section's flagged "Open question to Stage 3b: exact guard placement" is now RESOLVED by the pseudocode's extraction choice. The pure guard decision binds to `check_tag_lifecycle(status)` over the three `Status` values (pseudocode context-tag-handler.md scenario #4, lines 189–191). Runnable without a `RequestContext`.

## Regression Re-check (Checks 1/2/3/5 + delivery-critical)

Rework touched only `pseudocode/OVERVIEW.md` and `pseudocode/context-tag-handler.md`. store-tag-primitive.md, store-tag-service.md, audit-op-list.md, and all test-plan files are unchanged from the prior PASS assessment. Re-confirmed:

| Item | Status | Where |
|------|--------|-------|
| All 11 FRs + 7 NFRs covered | PASS | unchanged from prior gate; FR-11 lifecycle now cleaner via extracted `check_tag_lifecycle` (still re-implemented in-op, not inherited). |
| No deferred `protected_tags`/config/validator/`min_trust_level`/cadence surface | PASS | OVERVIEW "What is explicitly NOT in these files" (lines 181–185) intact; handler Seams #1/#2 remain comments only; no new type added by the rework. |
| replace = ONE atomic tx + rollback | PASS | store-tag-primitive.md `replace_tag` (unchanged) — single txn, namespace-scoped `DELETE LIKE ... ESCAPE '\'` + INSERT, one commit, `?` early-return rolls back. |
| colon-less degrade-to-add | PASS | store-tag-service.md Replace/`None` branch (lines 83–85) → `add_tag`, `prior_value:null`. |
| complete audit shape, prior_value mandatory on remove/replace | PASS | store-tag-service.md `build_tag_metadata` + OVERVIEW per-action table (lines 137–142). |
| value-opacity, no `validate_outcome_tags` | PASS | handler Seam #1 (line 101) "Do NOT invoke `validate_outcome_tags`"; service line 65 "NO value-hygiene here." |
| R-08 LIKE-escape + all-bound-params | PASS | store-tag-primitive.md `like_escape` + `ESCAPE '\'`, positional binds; handler §5b (unchanged). |
| in-op lifecycle guards (refuse quarantined, ALLOW deprecated) | PASS | handler Step 7 via `check_tag_lifecycle`; deprecated/active → `Ok`. |
| no handler unit tests planned (#5468) | PASS | test-plan/context-tag-handler.md line 5 "plans NO handler unit tests"; OVERVIEW Test-Seam Constraint intact. |
| stewardship blocks present | PASS | pseudocode report `## Knowledge Stewardship` (Queried; MCP-unavailable documented) + a "Gate 3a rework" note; testplan report Queried + Stored-with-reason. |

## Detailed Findings (Checks 1/2/3/5 — carried, unchanged)

Checks 1, 2, 3, and 5 were PASS in the prior gate and are unaffected by the rework (evidence in the prior report revision, git history). Re-read confirms no drift: components still map 1:1 to ARCH §2; all FR/NFR pseudocode present with no scope additions; all 8 risks mapped with High risks comprehensive; both agent reports carry stewardship blocks.

## Non-blocking notes (fold into Stage 3b/3c — do NOT gate on these)

1. **R-05 seam routing clarity.** test-plan/store-tag-service.md R-05 test descriptions (lines 27–29) read as service-seam refusals ("assert add/remove/replace each refused… no audit row"), but per the pseudocode the lifecycle guard lives in the HANDLER via `check_tag_lifecycle` — `StoreTagService::tag` does NOT re-check status (store-tag-service.md line 66). The plan is coherent read across files: route the **pure decision** assertions to `check_tag_lifecycle(status)` (unit), and the **end-to-end** "no `entry_tags` write / no audit row on quarantine" to the integration route (already deferred — test-plan/context-tag-handler.md line 46, security suite). Stage 3b/3c should place them accordingly; no design change needed.
2. **Absent-remove audit isolation (Q2, carried).** Add an explicit assertion that removing a never-present tag still records `prior_value = tag` (intent-of-record), so ADR-009 is proven, not implied.
3. **Idempotent no-op specificity (Q3, carried).** Assert the pinned idempotent no-op (`ON CONFLICT DO NOTHING`; absent-remove → `Ok(None)`), not the error variant.

## Open Contract Questions — resolution assessment (carried; all consistent)

1. `remove_tag -> Result<Option<String>>` additive vs BRIEF `Result<()>` — RESOLVED, ADR-009-consistent (service sources `prior_value = Some(tag)`).
2. Absent-tag remove records `prior_value = tag` — RESOLVED, intent-of-record conformant; WARN nudge (note 2) to isolate the case.
3. Duplicate-add / absent-remove idempotent — RESOLVED, pinned in pseudocode + asserted; WARN nudge (note 3) to assert the specific no-op.
4. R-08 reject-vs-escape — RESOLVED, escape-in-store pinned; siblings-survive test holds.

## Knowledge Stewardship
- Stored: nothing novel to store -- the underlying failure pattern (pseudocode designs a decision inline while the test plan requires it extracted as a `pub(crate)` seam fn for a non-constructible `#[tool]` handler) is already captured as pattern #5389 + the #5468 test-seam constraint; this gate instance is feature-specific and lives in this report, not Unimatrix. The successful rework confirms #5389 as the correct remediation.
