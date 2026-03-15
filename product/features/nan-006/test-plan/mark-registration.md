# Test Plan: C5 — pytest mark registration

## Verification Approach

### Mark registered in pytest.ini
- Read pytest.ini and verify `availability:` line in `[pytest]` markers section
- `pytest --markers` output should contain "availability"
- `pytest --collect-only -m availability` should not emit PytestUnknownMarkWarning

## Acceptance
- pytest.ini markers section contains line starting with "availability:"
- Running `pytest -m availability` from product/test/infra-001/ collects tests without warnings
