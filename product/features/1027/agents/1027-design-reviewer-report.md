# Design Review: 1027-design-reviewer

**Assessment: APPROVED WITH NOTES**

## Findings
| # | Severity | Finding |
|---|---|---|
| 1 | Info | `transcript_hold.rs:268` — `?` inside the `let (arc, mismatch_record) = { ... }` block returns from `readopt_inner` (returns `Option`) exactly as `return None` does; lock guard drops on return in both forms. No-held-buffer path stays silent, mismatch path stays fail-loud — ADR-008 crt-052 (#5442) unaffected. Function is not cfg-gated. |
| 2 | Low | Preferred shape keeps the borrow visibly scoped and avoids shadowing the later `let held = guard.remove(..)`: `let cycle_matches = guard.get(session_id).map(\|held\| !registering_feature_cycle.is_empty() && held.feature_cycle == registering_feature_cycle)?;` Investigator's two-statement form is also correct (NLL). Keep the existing "scoped read, then remove" comment either way. |
| 3 | Info | `main.rs:666,690` — `Command::Serve` fields (`daemon`, `stdio`, `foreground`) have no cfg gates; `stdio: _` + `..` is a pure wildcard, deletion cannot change match semantics. Arm order (foreground first, ADR-001 nan-014 #4569) untouched. Hunks are >100 lines from #1028's 543-556 / 825-834 — no merge conflict. |
| 4 | Info | `verbosity.rs:192,208` — inside `#[cfg(test)] mod tests`; test-only. `std::iter::repeat_n` stable since 1.82; workspace MSRV 1.89 — OK. |
| 5 | Low | Verify with `cargo clippy --workspace --all-targets --all-features -- -D warnings` too (crate has a `test-support` feature with cfg-gated code) so no feature-gated lint remains hidden. |
| 6 | Note (out of scope) | No workflow in `.github/workflows/` runs clippy; CI pins 1.89 while lints come from local 1.97. The "CI gate gap" is not fixable in this bug — file a separate issue if wanted (ci.yml changes go via issue). |

Hot path: none (readopt runs on SessionRegister; identical codegen-level control flow). Security surface: none. Blast radius: single crate, no wire/config/schema change.

## Knowledge Stewardship
- Queried: context_briefing — #5921/#5406 (pre-existing clippy triage); context_search decisions — #5442 (ADR-008 crt-052), #4569 (ADR-001 nan-014).
- Stored: nothing.
- Declined: lint-specific mechanical fixes; not generalizable, no new ADR needed.
