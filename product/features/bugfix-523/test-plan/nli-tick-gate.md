# Test Plan: NLI Tick Gate (Item 1) — AC-01 / AC-02 / AC-03

## Component

`run_graph_inference_tick` in `crates/unimatrix-server/src/services/nli_detection_tick.rs`

## Risks Covered

| Risk | Priority | AC |
|------|----------|----|
| R-01: Path A or Path C accidentally gated | Critical | AC-02 |
| R-02: Gate at wrong structural boundary | Critical | AC-02 (structural proof via Path C running) |
| R-09: NLI-enabled path broken by Item 1 | Med | AC-03 |
| R-11: behavioral-only coverage unacknowledged | Med | Gate report statement |

---

## Test Functions

### T-01: `test_nli_gate_path_b_skipped_nli_disabled` (AC-01)

**File**: `services/nli_detection_tick.rs` `#[cfg(test)]`
**Type**: `#[tokio::test]` async

**Arrange**:
- Open an in-memory test store via `unimatrix_store::test_helpers::open_test_store`.
- Insert at least two test entries with categories.
- Build `InferenceConfig { nli_enabled: false, ..InferenceConfig::default() }`.
- Construct `candidate_pairs: Vec<(u64, u64, f32)>` with **at least one pair** — e.g.,
  `vec![(1, 2, 0.85_f32)]`. This is mandatory: if the pair list is empty, the
  `candidate_pairs.is_empty()` fast-exit fires before the `nli_enabled` check and the new
  gate is never exercised. The empty-pairs path is tested by the existing fast-exit test.
- Provide a mock `NliServiceHandle` with a ready provider (so the test isolates the explicit
  `nli_enabled` gate from the implicit `get_provider()` Err path).

**Act**: Call `run_graph_inference_tick` (or the targeted Path B entry function) with
`nli_enabled=false` and the mock-ready provider.

**Assert**:
- No NLI Supports edges are written to the store (query store post-tick, verify no
  `EdgeType::Supports` edges from the NLI path).
- No rayon dispatch occurred (mock provider records call count; assert call count = 0 for
  the NLI scoring closure, or assert provider's inference method was never invoked).
- Function returns without panic.

**Coverage note**: This test distinguishes the explicit `nli_enabled` gate from the existing
`get_provider()` Err path. Without the mock-ready provider, a test that passes could be
explained by the Err path alone.

---

### T-02: `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` (AC-02, part 1)

**File**: `services/nli_detection_tick.rs` `#[cfg(test)]`
**Type**: `#[tokio::test]` async

**Arrange**:
- Open test store. Insert entries that would produce Informs edges in Phase A (entries with
  co-occurrence data or PPR scores sufficient to cross the Informs threshold).
- Build `InferenceConfig { nli_enabled: false, ..InferenceConfig::default() }`.

**Act**: Call `run_graph_inference_tick` with `nli_enabled=false`.

**Assert**:
- `EdgeType::Informs` edges are present in the store after the tick completes.
- At least one Informs edge is written, confirming Phase A executed.
- Function returns without panic.

**R-01/R-02 coverage**: This test, combined with T-03, is the structural proof that the gate
does not precede Phase A or Path C. Both tests are non-negotiable — passing T-01 alone is
insufficient.

---

### T-03: `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` (AC-02, part 2)

**File**: `services/nli_detection_tick.rs` `#[cfg(test)]`
**Type**: `#[tokio::test]` async

**Arrange**:
- Open test store. Insert two entries with categories. Ensure cosine similarity between them
  exceeds the `supports_cosine_threshold` default (0.70 or similar — use
  `InferenceConfig::default()` threshold value).
- Build `InferenceConfig { nli_enabled: false, ..InferenceConfig::default() }`.
- Populate `candidate_pairs` with this pair at a similarity above the cosine threshold.
- Populate `category_map` with both entry IDs.

**Act**: Call `run_graph_inference_tick` with `nli_enabled=false`.

**Assert**:
- At least one `EdgeType::Supports` edge is written to the store with source from Path C
  (cosine Supports, not NLI Supports).
- Function returns without panic.

**R-02 structural proof**: A Supports edge written while `nli_enabled=false` can only come
from `run_cosine_supports_path` (Path C), not from Path B. This proves Path C executed
after the `nli_enabled` gate, not before it, which in turn proves the gate is positioned
after `run_cosine_supports_path` completes.

Baseline reference: existing test `test_path_c_runs_unconditionally_nli_disabled` (TC-05,
line 2762) already tests `run_cosine_supports_path` directly with `nli_enabled=false`. T-03
extends this to test through `run_graph_inference_tick` so the gate position is verified
in the full call stack, not just the sub-function.

---

### T-04: `test_nli_gate_nli_enabled_path_not_regressed` (AC-03)

**File**: `services/nli_detection_tick.rs` `#[cfg(test)]`
**Type**: `#[tokio::test]` async

**Arrange**:
- Open test store. Insert entries.
- Build `InferenceConfig { nli_enabled: true, ..InferenceConfig::default() }`.
- Provide a mock `NliServiceHandle` that returns a ready provider and records calls.
- Provide candidate pairs sufficient to pass the NLI scoring pipeline.

**Act**: Call `run_graph_inference_tick` with `nli_enabled=true` and mock provider.

**Assert**:
- `get_provider()` is called (mock records invocation — assert call count >= 1).
- OR: NLI Supports edges are written (behavioral proxy if call recording is complex to arrange).
- Function returns without panic.
- No regression from the gate: the `if !config.nli_enabled` condition is false, so execution
  proceeds past the gate normally.

**R-09 coverage**: Verifies the gate condition is not inverted. A correctly written `if !nli_enabled`
returns early only when `nli_enabled=false`; this test confirms it does not fire when `nli_enabled=true`.

---

## Additional Behavioral Constraints (Non-Test)

The following are verified by code inspection, not test assertion, per ADR-001(c)/entry #4143:

1. **Log message text** (OQ-02 resolution): The debug! message on early return must be
   exactly `"graph inference tick: NLI disabled by config; Path B skipped"`. Code review
   confirms the string constant is distinct from the existing `get_provider()` Err message
   `"graph inference tick: NLI provider not ready; Supports path skipped"`.

2. **`background.rs` unchanged** (C-01): The caller in `background.rs` must remain
   unconditional. Stage 3c tester must confirm `background.rs` does not appear in the diff.

3. **Structural landmark** (R-02): Gate insertion is after the comment block
   `// === PATH B entry gate ===`, after `run_cosine_supports_path(...)` returns and after
   `if candidate_pairs.is_empty() { return; }`, before `nli_handle.get_provider().await`.
   Gate report must confirm this via code review.

---

## Gate Report Requirements

At Stage 3c, the gate report must include:
- "AC-01: test `test_nli_gate_path_b_skipped_nli_disabled` — PASS/FAIL. Note: uses non-empty
  candidate_pairs to exercise the explicit nli_enabled gate (not the empty-pairs fast-exit)."
- "AC-02: tests `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` (Path A) and
  `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` (Path C) — both
  required, both PASS/FAIL."
- "AC-03: test `test_nli_gate_nli_enabled_path_not_regressed` — PASS/FAIL."
- Code review confirmation: gate position at structural landmark, `background.rs` unchanged.
- "Log level for NLI gate debug! message verified by code review only per ADR-001(c)
  (entry #4143). No `tracing-test` harness used."
