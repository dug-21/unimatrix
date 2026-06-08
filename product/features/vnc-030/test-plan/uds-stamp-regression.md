# Test Plan — UDS-path stamp regression (TEST ONLY)

Source: ADR-002 §7. AC: AC-10. Risk: R-23 (folds R-06 replay, R-16 wire tolerance). **No vnc-030 production source** — a regression test against the vnc-027-MERGED `transport-uds.encodeFrame` seam (`transport-uds.js:55-62`), discharging vnc-027's post-merge obligation owed to #699.

File: `packages/unimatrix/test/hook-client/uds-stamp-regression.test.js` (NEW). Full test bodies are specified in `seam-and-roundtrip.md` §3 (gate-blocking). This file records the component boundary and the platform-guard discipline.

## Seam
Decoration mutates the in-memory `request` upstream of `selectTransport` (`index.js:410`); both transports `JSON.stringify` the same object, so `cycle_stamp` must be byte-identical on the UDS frame and the HTTP body.

## Tests (detail in seam-and-roundtrip.md §3)
1. `test_uds_encodeframe_carries_cycle_stamp` — offline, **UNGUARDED** (no socket).
2. `test_uds_http_cycle_stamp_byte_equivalent` — offline, **UNGUARDED**.
3. `test_uds_replayed_queue_frame_carries_stamp` — offline, **UNGUARDED** (R-06 replay over UDS).
4. `test_uds_live_roundtrip_stamp_recorded` — live daemon, **`{ skip: IS_WINDOWS }`** (extend `parity-layer2-uds.test.js`).

## Platform guard (BINDING, lesson #4832)
The offline byte-compare tests (1-3) open no socket and MUST stay **unguarded** so windows-latest keeps coverage. Test 4 binds/connects the real UDS socket and MUST carry `const IS_WINDOWS = process.platform === "win32"` + `{ skip: IS_WINDOWS }` (the established repo idiom in state.test.js/parity-layer2-uds.test.js). The Stage 3c tester and Gate 3c validator must reason about windows-latest × every Node version in the CI matrix, not just the local Linux dev OS — a UDS-binding test that passes every Linux gate still breaks the matrix with `listen EACCES` if unguarded.

## Coverage requirement
The stamp is proven over UDS, not just HTTP; the seam is pinned at `transport-uds.encodeFrame`; offline byte-compare unguarded, live-socket guarded; the test adds no vnc-030 production source.
