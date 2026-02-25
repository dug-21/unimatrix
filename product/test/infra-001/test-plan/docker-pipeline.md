# Test Plan: C1 — Docker Build Pipeline

## Scope

The Docker pipeline is validated by building the image and running the test suite end-to-end. Most validation is through AC-01 (docker compose up exits 0).

## Validation Points

| AC | Test | Method |
|----|------|--------|
| AC-01 | Full pipeline builds and runs | `docker compose up --build --abort-on-container-exit` exits 0 |
| AC-02 | Clean teardown | `docker compose down -v` leaves no containers/volumes |
| AC-17 | Offline tests | Model pre-downloaded in image layer |

## Build Stage Validation

| Check | How Validated |
|-------|--------------|
| Rust workspace compiles | Build stage runs `cargo build --release` |
| Unit tests pass | Build stage runs `cargo test --lib` |
| Binary produced | test-runtime stage copies binary and pytest uses it |
| ONNX Runtime matches | Server starts and completes MCP handshake (P-01) |

## Runtime Stage Validation

| Check | How Validated |
|-------|--------------|
| Python 3.12 available | pytest runs |
| pytest installed | Tests execute |
| Binary executable | server fixture spawns it |
| ONNX Runtime accessible | Search tests trigger embedding |
| Model pre-downloaded | Search tests work without network |
| Scripts executable | run.sh is the entrypoint |
| Results directory writable | JUnit XML and JSON report written |
| tmpfs mounted | Volume tests write to /tmp without error |

## Risk Coverage

| Risk | Docker Responsibility | Validation |
|------|---------------------|------------|
| R-04 | ONNX Runtime version matches compilation | Server starts + search works (P-01, T-13) |
| R-05 | Model pre-downloaded | AC-17 (offline test) |
| R-08 | tmpfs configured | docker-compose.yml specifies tmpfs; fallback to regular /tmp works |
| R-11 | tmpfs size adequate | Volume suite stores 5K entries within 512MB |

## Manual Verification

Some Docker tests require manual execution:
1. Build: `docker compose build` (verifies Dockerfile syntax + compilation)
2. Run: `docker compose up --build --abort-on-container-exit` (full pipeline)
3. Teardown: `docker compose down -v` (cleanup)
4. Offline: disconnect network after build, run tests
