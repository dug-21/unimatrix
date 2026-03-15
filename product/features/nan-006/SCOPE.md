# Scope: nan-006 — Availability Test Suite

## Goals

- Add `UNIMATRIX_TICK_INTERVAL_SECS` env var to background.rs (falls back to 900)
- Add `fast_tick_server` pytest fixture (30s tick)
- Add `test_availability.py` suite (5 runnable + 1 deferred skip)
- Update USAGE-PROTOCOL.md with Pre-Release Gate section
- Register `availability` mark in pytest.ini

## Not in Scope

- Fixing the underlying bugs (#275, #276, #277) — tests document current behavior
- Modifying any existing test suites or fixtures

## Feature Type

Test-only infrastructure. No new MCP tools. No user-visible server behavior changes (env var is internal).
