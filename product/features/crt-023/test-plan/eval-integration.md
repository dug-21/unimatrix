# Test Plan: Eval Integration (`unimatrix-server/src/services/eval.rs`)

## Component Scope

File: `crates/unimatrix-server/src/services/eval.rs` (or equivalent eval module)

Changes: W1-4 stub in `EvalServiceLayer::from_profile()` filled in; `wait_for_nli_ready(60s)`
added; SKIPPED annotation for missing model.

## Risks Covered

R-14 (High): Eval SKIPPED profiles misread as gate pass.
R-21 (Med): Eval latency measurement contaminated by background NLI tasks.
AC-18, AC-22, FR-26, FR-27, FR-28, FR-29.

---

## Unit Tests: from_profile() Behavior

### AC-18: NLI-Enabled Profile Constructs NliServiceHandle

```rust
#[tokio::test]
async fn test_from_profile_nli_enabled_constructs_nli_handle() {
    // AC-18: profile with nli_enabled=true and resolvable model → NliServiceHandle wired.
    // Mock: NliServiceHandle that records whether it was constructed.
    let profile = EvalProfile {
        nli_enabled: true,
        nli_model_path: Some(PathBuf::from("/path/to/model.onnx")),
        ..EvalProfile::default()
    };
    // Use a test-injectable factory instead of real NliServiceHandle construction.
    let layer = EvalServiceLayer::from_profile_with_factory(profile, mock_nli_factory()).await;
    assert!(layer.is_ok());
    assert!(layer.unwrap().has_nli_handle(),
        "NLI-enabled profile must wire NliServiceHandle into EvalServiceLayer");
}

#[tokio::test]
async fn test_from_profile_nli_disabled_no_nli_handle() {
    // AC-18: profile with nli_enabled=false → uses cosine path, no NliServiceHandle.
    let profile = EvalProfile {
        nli_enabled: false,
        ..EvalProfile::default()
    };
    let layer = EvalServiceLayer::from_profile(profile).await.unwrap();
    assert!(!layer.has_nli_handle(),
        "NLI-disabled profile must not construct NliServiceHandle");
}

#[tokio::test]
async fn test_from_profile_nli_disabled_completes_immediately() {
    // Integration risk: adding NliServiceHandle.wait_for_nli_ready(60s) to from_profile()
    // must not block profiles where nli_enabled=false.
    let profile = EvalProfile { nli_enabled: false, ..EvalProfile::default() };
    let start = Instant::now();
    let _ = EvalServiceLayer::from_profile(profile).await;
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_secs(1),
        "from_profile with nli_enabled=false must complete immediately, took: {:?}", elapsed);
}
```

### R-14, ADR-006: SKIPPED Profile Annotation

```rust
#[tokio::test]
async fn test_from_profile_missing_model_produces_skipped() {
    // R-14: profile with nli_enabled=true but absent model → SKIPPED annotation.
    let profile = EvalProfile {
        nli_enabled: true,
        nli_model_path: Some(PathBuf::from("/nonexistent/model.onnx")),
        ..EvalProfile::default()
    };
    let result = EvalServiceLayer::from_profile(profile).await;
    // After 60s timeout (or shorter in tests — inject short timeout), result is SKIPPED.
    assert!(matches!(result, Err(EvalError::Skipped { reason, .. }) if reason.contains("NLI model")),
        "Missing NLI model must produce EvalError::Skipped with reason, got: {:?}", result);
}

#[test]
fn test_eval_skipped_error_contains_reason_string() {
    // R-14: SKIPPED annotation must carry a reason string for report generation.
    let err = EvalError::Skipped {
        reason: "NLI model not available".to_string(),
        profile_name: "candidate".to_string(),
    };
    assert!(err.to_string().contains("NLI model not available"));
    assert!(err.to_string().contains("SKIPPED"));
}
```

### wait_for_nli_ready Timeout

```rust
#[tokio::test]
async fn test_wait_for_nli_ready_respects_timeout() {
    // ADR-006: from_profile() waits up to 60s for NLI readiness.
    // If timeout fires → SKIPPED (not an error/panic).
    // In tests: use 100ms timeout to avoid slow test.
    let layer = EvalServiceLayer::new_with_ready_timeout(Duration::from_millis(100));
    let not_ready_handle = NliServiceHandle::new(); // never started
    let result = layer.wait_for_nli_ready_with_handle(&not_ready_handle).await;
    assert!(matches!(result, Err(NliNotReadyError)));
}

#[tokio::test]
async fn test_wait_for_nli_ready_succeeds_when_handle_becomes_ready() {
    // If NLI becomes ready within timeout → proceed with evaluation.
    let handle = make_handle_that_becomes_ready_in(Duration::from_millis(50));
    let layer = EvalServiceLayer::new_with_ready_timeout(Duration::from_secs(5));
    let result = layer.wait_for_nli_ready_with_handle(&handle).await;
    assert!(result.is_ok(), "Must succeed when NLI becomes ready within timeout");
}
```

## Integration Test: Two-Profile Eval Run

```rust
#[tokio::test]
async fn test_eval_run_produces_two_result_sets() {
    // AC-18: eval run with baseline.toml and candidate.toml produces two result files.
    // Uses fixture snapshot (pre-populated SQLite) and synthetic scenarios.
    let snapshot = create_fixture_snapshot_with_entries(10);
    let scenarios = create_fixture_scenarios(3);

    let baseline_profile = EvalProfile { nli_enabled: false, ..EvalProfile::default() };
    let candidate_profile = EvalProfile {
        nli_enabled: true,
        nli_model_path: Some(mock_model_path()),
        ..EvalProfile::default()
    };

    let runner = EvalRunner::new(snapshot, scenarios);
    let results = runner.run_all_profiles(vec![baseline_profile, candidate_profile]).await;

    assert_eq!(results.len(), 2,
        "eval run must produce results for both profiles, got: {}", results.len());
    assert!(results.iter().any(|r| r.profile_name == "baseline"));
    assert!(results.iter().any(|r| r.profile_name == "candidate" || r.is_skipped()));
}
```

## R-21: EvalServiceLayer Does Not Call StoreService::insert During Replay

```rust
#[test]
fn test_eval_service_layer_has_no_store_insert_path() {
    // R-21: EvalServiceLayer must not call StoreService::insert during scenario replay.
    // Verification: EvalServiceLayer does not hold a mutable StoreService reference.
    // This is structural — verified by code inspection at Stage 3b.
    // Test: construct EvalServiceLayer and verify no insert method is accessible.
    // (If the type system enforces read-only access, this is a compile-time guarantee.)
    //
    // If EvalServiceLayer is constructed with a read-only snapshot,
    // any insert attempt will fail at the DB layer (SQLITE_READONLY).
    // Test that: insert attempt on read-only snapshot returns Err.
    let read_only_store = make_read_only_store();
    let result = read_only_store.insert_entry(make_test_entry_record());
    assert!(result.is_err(),
        "Read-only snapshot must reject insert attempts — eval cannot contaminate latency");
}
```

## AC-22: Eval Gate Waiver Documentation

This is a manual verification step (human review), not a unit test.
Required at Stage 3c:

1. If `unimatrix eval scenarios` returns 0 rows: document waiver in delivery report
   with reason "no query history available".
2. Regardless of waiver: AC-01 (NliProvider unit test) must pass.
3. Test plan assertion: AC-01 is not guarded by an `#[ignore]` or `#[cfg(feature)]`
   that depends on eval gate state.

```rust
// Verified at Stage 3c: AC-01 test is present and does not depend on eval gate.
// No waiver exempts AC-01 from passing.
```
