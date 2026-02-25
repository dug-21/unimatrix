# Test Plan: C1 Status Extension

## File: crates/unimatrix-store/src/schema.rs

### Unit Tests (7 tests)

All tests in the existing `#[cfg(test)] mod tests` block within `schema.rs`.

#### Test 1: test_status_quarantined_try_from
```
#[test]
fn test_status_quarantined_try_from():
    let status = Status::try_from(3u8).unwrap()
    assert_eq!(status, Status::Quarantined)
```
**AC**: AC-01
**Risk**: R-01 (exhaustive match)

#### Test 2: test_status_try_from_invalid_updated
```
// MODIFY existing test_status_try_from_invalid
// Old: asserts try_from(3u8) is Err
// New: asserts try_from(4u8) is Err
#[test]
fn test_status_try_from_invalid():
    assert!(Status::try_from(4u8).is_err())
```
**AC**: AC-01, AC-21
**Risk**: R-01

#### Test 3: test_status_quarantined_display
```
#[test]
fn test_status_quarantined_display():
    assert_eq!(format!("{}", Status::Quarantined), "Quarantined")
```
**AC**: AC-01
**Risk**: R-01

#### Test 4: test_status_quarantined_counter_key
```
#[test]
fn test_status_quarantined_counter_key():
    assert_eq!(status_counter_key(Status::Quarantined), "total_quarantined")
```
**AC**: AC-01
**Risk**: R-01

### Tests in crates/unimatrix-server/ (unit tests verifying match arms)

#### Test 5: test_base_score_quarantined
```
#[test]
fn test_base_score_quarantined():
    assert_eq!(base_score(Status::Quarantined), 0.1)
```
**AC**: AC-01
**Risk**: R-01 (confidence.rs match arm)

#### Test 6: test_status_to_str_quarantined
```
#[test]
fn test_status_to_str_quarantined():
    assert_eq!(status_to_str(Status::Quarantined), "quarantined")
```
**AC**: AC-01
**Risk**: R-01 (response.rs match arm)

#### Test 7: test_parse_status_quarantined
```
#[test]
fn test_parse_status_quarantined():
    assert_eq!(parse_status("quarantined").unwrap(), Status::Quarantined)
    assert_eq!(parse_status("Quarantined").unwrap(), Status::Quarantined)
    assert_eq!(parse_status("QUARANTINED").unwrap(), Status::Quarantined)
```
**AC**: AC-01
**Risk**: R-01 (validation.rs match arm)

## Risk Coverage

| Risk | Scenarios Covered |
|------|-------------------|
| R-01 | All 7 match sites verified: TryFrom (test 1-2), Display (test 3), counter_key (test 4), base_score (test 5), status_to_str (test 6), parse_status (test 7) |
