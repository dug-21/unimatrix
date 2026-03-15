# Pseudocode Overview: nan-006 — Availability Test Suite

## Components and Data Flow

```
C1: background.rs (Rust)
  └─ read_tick_interval() -> u64
       reads UNIMATRIX_TICK_INTERVAL_SECS env var
       falls back to 900
       used in background_tick_loop() at startup

C2: harness/client.py (Python)
  └─ UnimatrixClient.__init__(extra_env: dict | None)
       merges extra_env into env before Popen

C2: harness/conftest.py (Python)
  └─ fast_tick_server fixture
       creates UnimatrixClient with extra_env={"UNIMATRIX_TICK_INTERVAL_SECS": "30"}

C3: suites/test_availability.py (Python)
  └─ 5 runnable tests + 1 skip stub
       all marked @pytest.mark.availability
       uses fast_tick_server fixture (C2)
       wall-clock deadlines via time.time()

C5: pytest.ini
  └─ availability mark registered

C4: USAGE-PROTOCOL.md
  └─ Pre-Release Gate section added
     When to Run table updated
```

## Sequencing Constraints

1. C1 (Rust env var) must compile before Python tests can validate fast tick behavior
2. C2 (fixture + client extension) must be committed before C3 (tests) can use it
3. C3 (tests) and C4 (docs) can proceed in parallel in Wave 3
4. C5 (mark registration) can be in Wave 2 with C2

## Shared Types / Interfaces

- `UNIMATRIX_TICK_INTERVAL_SECS` env var: string representation of u64, parsed at startup
- `UnimatrixClient(extra_env: dict[str, str] | None = None)`: new optional parameter
- `fast_tick_server`: pytest fixture, same interface as `server` fixture

## Wave Plan

- Wave 1: C1 (Rust env var)
- Wave 2: C2 (client extra_env + fast_tick fixture) + C5 (mark registration)
- Wave 3: C3 (test_availability.py) + C4 (USAGE-PROTOCOL.md update)
