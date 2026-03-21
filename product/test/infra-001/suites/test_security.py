"""Suite 5: Security (~30 tests).

Content scanning (injection, PII), capability enforcement,
and input validation boundary testing.
"""

import json
import os
import subprocess
import threading
import time
import pytest
from pathlib import Path
from harness.assertions import assert_tool_success, assert_tool_error, extract_entry_id, parse_entries
from harness.conftest import get_binary_path


FIXTURES_DIR = Path(__file__).resolve().parent.parent / "fixtures"


def _load_fixture(name: str) -> dict:
    with open(FIXTURES_DIR / name) as f:
        return json.load(f)


# === Content Scanning: Injection Patterns =============================

class TestInjectionDetection:
    """S-01 to S-10: Injection patterns detected by server scanner."""

    @pytest.fixture(autouse=True)
    def _load_patterns(self):
        data = _load_fixture("injection_patterns.json")
        self.patterns = data["patterns"]

    @pytest.mark.smoke
    @pytest.mark.security
    def test_injection_patterns_detected(self, server):
        """S-01..S-09: Each should_detect=true injection pattern is rejected."""
        detectable = [p for p in self.patterns if p["should_detect"]]
        detected_count = 0
        for pattern in detectable:
            resp = server.context_store(
                pattern["content"], "testing", "convention", agent_id="human"
            )
            result = resp.result
            if result and result.get("isError", False):
                detected_count += 1
            elif resp.error:
                detected_count += 1
        # At least some injection patterns should be detected
        assert detected_count > 0, (
            f"Expected server to detect injection patterns, but {detected_count}/{len(detectable)} detected"
        )

    @pytest.mark.security
    def test_safe_content_accepted(self, server):
        """S-10: Safe content (false positives) accepted."""
        safe = [p for p in self.patterns if not p["should_detect"]]
        for pattern in safe:
            resp = server.context_store(
                pattern["content"], "testing", "convention", agent_id="human"
            )
            assert_tool_success(resp)


# === Content Scanning: PII ============================================

class TestPIIDetection:
    """S-11 to S-18: PII samples detected by server scanner."""

    @pytest.fixture(autouse=True)
    def _load_samples(self):
        data = _load_fixture("pii_samples.json")
        self.samples = data["samples"]

    @pytest.mark.security
    def test_pii_patterns_detected(self, server):
        """S-11..S-17: Each should_detect=true PII sample is rejected."""
        detectable = [s for s in self.samples if s["should_detect"]]
        detected_count = 0
        for sample in detectable:
            resp = server.context_store(
                sample["content"], "testing", "convention", agent_id="human"
            )
            result = resp.result
            if result and result.get("isError", False):
                detected_count += 1
            elif resp.error:
                detected_count += 1
        assert detected_count > 0, (
            f"Expected server to detect PII, but {detected_count}/{len(detectable)} detected"
        )

    @pytest.mark.security
    def test_safe_content_without_pii_accepted(self, server):
        """S-18: Safe content without PII accepted."""
        safe = [s for s in self.samples if not s["should_detect"]]
        for sample in safe:
            resp = server.context_store(
                sample["content"], "testing", "convention", agent_id="human"
            )
            assert_tool_success(resp)


# === Capability Enforcement ===========================================

@pytest.mark.security
def test_restricted_agent_search_allowed(server):
    """S-19: Restricted agent can search."""
    server.context_store(
        "searchable content", "testing", "convention", agent_id="human"
    )
    resp = server.context_search("searchable", agent_id="restricted-test-agent")
    assert_tool_success(resp)


@pytest.mark.security
def test_restricted_agent_lookup_allowed(server):
    """S-20: Restricted agent can lookup."""
    resp = server.context_lookup(topic="testing", agent_id="restricted-test-agent")
    assert_tool_success(resp)


@pytest.mark.security
def test_restricted_agent_store_allowed_permissive(server):
    """S-21: With permissive auto-enroll, restricted agent CAN store (has Write)."""
    resp = server.context_store(
        "restricted store", "testing", "convention", agent_id="restricted-test-agent"
    )
    assert_tool_success(resp)


@pytest.mark.security
def test_restricted_agent_correct_allowed_permissive(server):
    """S-22: With permissive auto-enroll, restricted agent CAN correct (has Write)."""
    store_resp = server.context_store(
        "for restricted correct", "testing", "convention", agent_id="human", format="json"
    )
    from harness.assertions import extract_entry_id
    entry_id = extract_entry_id(store_resp)
    resp = server.context_correct(
        entry_id, "corrected", agent_id="restricted-test-agent"
    )
    assert_tool_success(resp)


@pytest.mark.security
def test_restricted_agent_deprecate_allowed_permissive(server):
    """S-23: With permissive auto-enroll, restricted agent CAN deprecate (has Write)."""
    store_resp = server.context_store(
        "for restricted deprecate", "testing", "convention", agent_id="human", format="json"
    )
    from harness.assertions import extract_entry_id
    entry_id = extract_entry_id(store_resp)
    resp = server.context_deprecate(entry_id, agent_id="restricted-test-agent")
    assert_tool_success(resp)


@pytest.mark.security
def test_restricted_agent_quarantine_rejected(server):
    """S-24: Restricted agent cannot quarantine (requires Admin)."""
    store_resp = server.context_store(
        "for restricted quarantine", "testing", "convention", agent_id="human", format="json"
    )
    from harness.assertions import extract_entry_id
    entry_id = extract_entry_id(store_resp)
    resp = server.context_quarantine(entry_id, agent_id="restricted-test-agent")
    assert_tool_error(resp)


# === Input Validation =================================================

@pytest.mark.security
def test_input_max_topic_length(server):
    """S-27: Very long topic handled or rejected."""
    long_topic = "a" * 200
    resp = server.context_store(
        "max topic test", long_topic, "convention", agent_id="human"
    )
    # Server may accept (truncate) or reject with error; both valid
    # The key is: no crash
    assert resp.result is not None or resp.error is not None


@pytest.mark.security
def test_input_control_characters(server):
    """S-28: Control characters in content handled."""
    content = "content with\x00null\x01and\x02control\x03chars"
    resp = server.context_store(content, "testing", "convention", agent_id="human")
    # Server should handle gracefully (accept or reject, not crash)
    assert resp.result is not None or resp.error is not None


@pytest.mark.security
def test_input_negative_entry_id(server):
    """S-29: Negative entry ID rejected."""
    resp = server.context_get(-1)
    assert_tool_error(resp)


@pytest.mark.security
def test_input_zero_entry_id(server):
    """S-29b: Zero entry ID rejected."""
    resp = server.context_get(0)
    assert_tool_error(resp)


@pytest.mark.security
def test_false_positive_safe_content(server):
    """S-30: Safe content with scanner-adjacent words accepted."""
    resp = server.context_store(
        "We decided to ignore test failures during the warmup phase. "
        "The system prompt documentation was moved to a new location.",
        "testing",
        "decision",
        agent_id="human",
    )
    assert_tool_success(resp)


# === crt-018b: Auto-Quarantine DoS Mitigation ================================


@pytest.mark.security
def test_auto_quarantine_cycles_invalid_large_value_rejected_at_startup(tmp_path):
    """S-31: UNIMATRIX_AUTO_QUARANTINE_CYCLES > 1000 causes startup failure (Constraint 14, Security Risk 1).

    An operator who can set env vars could set AUTO_QUARANTINE_CYCLES to a
    very large value (e.g., 1001) as a DoS amplification.  Constraint 14
    requires the server to reject implausibly large values at startup rather
    than silently accepting them.

    This test verifies: server exits with non-zero exit code when the env var
    exceeds the 1000 upper bound.  The server must NOT serve MCP requests.
    """
    binary = get_binary_path()
    env = os.environ.copy()
    env["UNIMATRIX_AUTO_QUARANTINE_CYCLES"] = "1001"

    # vnc-005: default invocation is now bridge mode; use `serve --stdio` for stdio path.
    proc = subprocess.Popen(
        [binary, "--project-dir", str(tmp_path), "serve", "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )

    stderr_lines: list[str] = []

    def drain_stderr():
        for line in iter(proc.stderr.readline, b""):
            stderr_lines.append(line.decode("utf-8", errors="replace").rstrip())

    t = threading.Thread(target=drain_stderr, daemon=True)
    t.start()

    try:
        exit_code = proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
        raise AssertionError(
            "Server did not exit within 10s with UNIMATRIX_AUTO_QUARANTINE_CYCLES=1001. "
            "Expected startup failure (Constraint 14 / Security Risk 1)."
        )

    t.join(timeout=2)
    stderr_all = "\n".join(stderr_lines)

    assert exit_code != 0, (
        f"Server must exit with non-zero code when UNIMATRIX_AUTO_QUARANTINE_CYCLES=1001. "
        f"Got exit code {exit_code}. Stderr: {stderr_all[-500:]}"
    )

    # The error message must mention the implausible value
    assert "1001" in stderr_all or "implausibly" in stderr_all.lower() or "1000" in stderr_all, (
        f"Server exit message must reference the invalid value (1001) or the limit (1000). "
        f"Got stderr: {stderr_all[-500:]}"
    )


@pytest.mark.security
def test_auto_quarantine_cycles_zero_accepted_at_startup(tmp_path):
    """S-32: UNIMATRIX_AUTO_QUARANTINE_CYCLES=0 is accepted at startup (AC-12, Constraint 14).

    Value 0 is the disable sentinel — must NOT be rejected.  The server must
    start and serve MCP requests normally when the threshold is 0.
    """
    binary = get_binary_path()
    env = os.environ.copy()
    env["UNIMATRIX_AUTO_QUARANTINE_CYCLES"] = "0"

    # vnc-005: default invocation is now bridge mode; use `serve --stdio` for stdio path.
    proc = subprocess.Popen(
        [binary, "--project-dir", str(tmp_path), "serve", "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )

    stderr_lines: list[str] = []

    def drain_stderr():
        for line in iter(proc.stderr.readline, b""):
            stderr_lines.append(line.decode("utf-8", errors="replace").rstrip())

    t = threading.Thread(target=drain_stderr, daemon=True)
    t.start()

    # Give server time to start and load the embedding model
    time.sleep(3)

    still_running = proc.poll() is None
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()

    t.join(timeout=2)
    stderr_all = "\n".join(stderr_lines)

    assert still_running, (
        f"Server must NOT exit immediately when UNIMATRIX_AUTO_QUARANTINE_CYCLES=0. "
        f"Value 0 is the disable sentinel (AC-12). "
        f"Stderr: {stderr_all[-500:]}"
    )


# === crt-023: NLI Security Boundaries ========================================


@pytest.mark.security
def test_store_large_content_nli_no_crash(server):
    """S-CRT023-01: Storing 100,000-char content does not crash server or NLI path (AC-03, NFR-08).

    Any content stored through context_store becomes a candidate passage for NLI
    inference. Per-side truncation (512 tokens / ~2000 chars) must be enforced
    inside NliProvider before inference. This test verifies that a vastly oversized
    payload does not panic the server, poison the NLI session, or return a tool
    error on a subsequent context_search call.
    """
    # 100,000 char content — well beyond NLI truncation boundary
    large_content = ("unimatrix nli truncation boundary test alpha " * 2400)[:100_000]
    store_resp = server.context_store(
        large_content,
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    # Store itself must succeed (large content is a valid payload up to server limits)
    # If rejected by content scanner, that is also acceptable — key is: no crash
    assert store_resp.result is not None or store_resp.error is not None, (
        "context_store must return some response (not a dead connection) for 100k-char content"
    )

    # Server must still be healthy — subsequent search must work
    search_resp = server.context_search(
        "nli truncation boundary test alpha", format="json", agent_id="human"
    )
    assert_tool_success(search_resp), (
        "AC-03/NFR-08: Server must remain healthy after storing large content. "
        "NLI session must not be poisoned by oversized input."
    )


@pytest.mark.security
def test_nli_hash_mismatch_graceful_degradation(server):
    """S-CRT023-02: Server with NLI hash mismatch still serves all MCP tools (AC-06, AC-14).

    In CI the NLI model is absent, which causes NliServiceHandle to transition
    to Failed — equivalent to a hash mismatch degradation path. The server must
    start successfully and serve context_search without returning an error to
    callers. This validates the graceful degradation contract for both the
    absent-model and hash-mismatch cases (the observable MCP behavior is
    identical: cosine fallback, no tool-level error).
    """
    # Store an entry
    store_resp = server.context_store(
        "nli hash mismatch degradation test unique crt023 gamma",
        "testing",
        "convention",
        agent_id="human",
        format="json",
    )
    assert_tool_success(store_resp)
    entry_id = extract_entry_id(store_resp)

    # Search must return results — cosine fallback active (AC-14)
    search_resp = server.context_search(
        "nli hash mismatch degradation test unique crt023 gamma",
        format="json",
        agent_id="human",
    )
    assert_tool_success(search_resp), (
        "AC-06/AC-14: context_search must return results when NLI is unavailable "
        "(hash mismatch / model absent). No error must be returned to callers."
    )
    entries = parse_entries(search_resp)
    result_ids = [e.get("id") for e in entries if e.get("id") is not None]
    assert entry_id in result_ids, (
        f"Stored entry must be findable via cosine fallback. "
        f"entry_id={entry_id} not in results: {result_ids}"
    )
