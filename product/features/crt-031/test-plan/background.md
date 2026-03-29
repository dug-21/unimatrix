# Test Plan: background.rs (maintenance tick lifecycle guard stub)

Component from IMPLEMENTATION-BRIEF.md §Component Map row 5.

---

## Risks Addressed

- **R-02** (Critical): `run_single_tick` at ~line 446 constructs `StatusService::new()`
  directly. It must receive the operator-loaded `Arc<CategoryAllowlist>`, not a freshly
  constructed default.
- **R-05** (Medium): `spawn_background_tick` gains a 23rd parameter — verify
  `#[allow(clippy::too_many_arguments)]` is present.
- **R-06** (Low): Guard stub must call `list_adaptive()` once, not per-category `is_adaptive()`.
  No lock held across `.await`.
- **R-10** (High): Tests for AC-10 and AC-11 must be present in `background.rs` before gate 3b.
- **I-04** (High): `run_single_tick` must thread the startup `Arc`, not reconstruct inline.

---

## Pre-Implementation Step: run_single_tick Inspection (I-04, R-02)

Before writing any code, inspect `background.rs` line ~446:

```bash
grep -n "StatusService::new" crates/unimatrix-server/src/background.rs
```

Expected: one hit at `run_single_tick`. After the signature change, this site will NOT
produce a compile error if `CategoryAllowlist::new()` is inserted inline. The test plan
explicitly guards against this pattern via `test_run_single_tick_uses_operator_arc`.

---

## Unit Test Expectations

### Guard stub fires on non-empty adaptive list (AC-10)

**`test_maintenance_tick_stub_logs_adaptive_categories`** (AC-10 scenario 1)
- Arrange: construct `CategoryAllowlist::from_categories_with_policy(all_5, vec!["lesson-learned"])`
  wrapped in `Arc`. Construct all other `maintenance_tick` parameters as minimal stubs.
  Use `tracing_test` (or equivalent subscriber capture) to capture debug events.
- Act: call `maintenance_tick(... category_allowlist)` or the minimum testable unit
- Assert: a `tracing::debug!` event is captured containing `"lesson-learned"` in the
  lifecycle guard context
- Note: if `maintenance_tick` is difficult to unit-test directly (due to many parameters),
  test the guard logic via a thin extracted helper or verify via code review + AC-11 grep

**`test_maintenance_tick_stub_silent_when_adaptive_empty`** (AC-10 scenario 2)
- Arrange: `CategoryAllowlist::from_categories_with_policy(all_5, vec![])` — empty adaptive
- Act: call `maintenance_tick` with this allowlist
- Assert: no debug event for the lifecycle guard fires
- Covers: E-01 behavior in the tick context

---

### Guard stub TODO comment present (AC-11)

**`test_background_rs_has_todo_409_comment`** (AC-11)
This is a grep verification, not a code test:

```bash
grep -n "TODO(#409)" crates/unimatrix-server/src/background.rs
```

Assert: returns at least one hit inside the Step 10b block. The comment must be
`// TODO(#409): ...` as specified in the IMPLEMENTATION-BRIEF.

---

### Lock hygiene: no lock held across await (R-06)

**`test_lifecycle_stub_no_lock_across_await`**
- Verification: code review of the guard stub
- Assert: the `list_adaptive()` call result is bound to a local variable BEFORE any `.await`
  point. The `RwLock` read guard is NOT held across an `.await`.
- Expected implementation pattern:
  ```rust
  {
      let adaptive = category_allowlist.list_adaptive();  // guard dropped here
      if !adaptive.is_empty() {
          tracing::debug!(categories = ?adaptive, "...");
          // TODO(#409): ...
      }
  }
  ```
- The owned `Vec<String>` from `list_adaptive()` is `Send`; no lock guard survives
  the block boundary.

---

### run_single_tick uses operator Arc (R-02 scenario 4, I-04)

**`test_run_single_tick_uses_operator_arc_not_fresh`** (R-02 scenario 4)
- This is a code-structure verification, not a behavioral test (since with default config,
  `CategoryAllowlist::new()` and the operator-configured Arc produce the same adaptive policy).
- Verification method: grep for `CategoryAllowlist::new()` inside `run_single_tick`:

  ```bash
  grep -A 30 "fn run_single_tick" crates/unimatrix-server/src/background.rs | \
    grep "CategoryAllowlist::new"
  ```

  Expected: zero hits. A hit indicates the silent failure pattern was introduced.

- The `category_allowlist` parameter must be threaded from `background_tick_loop` through
  to `StatusService::new()` inside `run_single_tick`.

---

### Parameter count and clippy attribute (R-05)

**`test_spawn_background_tick_has_allow_too_many_arguments`**
- Verification: grep for the clippy allow attribute on `spawn_background_tick`:

  ```bash
  grep -B1 "fn spawn_background_tick" crates/unimatrix-server/src/background.rs
  ```

  Expected: `#[allow(clippy::too_many_arguments)]` appears immediately before the function.

---

## Assertions Summary

| Assertion | Test | AC/Risk |
|-----------|------|----|
| Debug event fires when adaptive list non-empty | `test_maintenance_tick_stub_logs_adaptive_categories` | AC-10 |
| No debug event when adaptive list empty | `test_maintenance_tick_stub_silent_when_adaptive_empty` | AC-10 |
| `TODO(#409)` comment present in Step 10b | grep | AC-11 |
| No `CategoryAllowlist::new()` inside `run_single_tick` | grep | R-02/I-04 |
| `#[allow(clippy::too_many_arguments)]` on `spawn_background_tick` | grep | R-05 |
| No RwLock guard held across `.await` | code review | R-06 |

---

## Integration Test Expectations

The background tick runs on a timer interval. Integration-level validation of tick behavior
is out of scope for crt-031's integration tests (availability suite handles tick liveness).

The R-02 end-to-end guarantee — that `context_status` called from the tick path returns
non-empty `category_lifecycle` — is covered by the unit test on `compute_report()` in
`status.md` combined with the `run_single_tick` grep verification here.
