# Pseudocode: C5 — pytest mark registration

## File
`product/test/infra-001/pytest.ini`

## Change

Add `availability` to the `markers` section.

Current markers:
```ini
markers =
    smoke: Critical-path tests for quick validation (~15 tests, <60s)
    slow: Tests that take more than 10 seconds
    volume: Scale and stress tests
    security: Security validation tests
```

New markers (add `availability` at the end):
```ini
markers =
    smoke: Critical-path tests for quick validation (~15 tests, <60s)
    slow: Tests that take more than 10 seconds
    volume: Scale and stress tests
    security: Security validation tests
    availability: Time-extended reliability tests (tick liveness, sustained operation, mutex pressure). Pre-release gate only (~15-20 min).
```

## Verification
After adding the mark, `pytest --collect-only -m availability` should collect without warnings.
`pytest --markers` should show the `availability` mark description.
