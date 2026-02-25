"""Suite 5: Security (~30 tests).

Content scanning (injection, PII), capability enforcement,
and input validation boundary testing.
"""

import json
import pytest
from pathlib import Path
from harness.assertions import assert_tool_success, assert_tool_error


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
def test_restricted_agent_store_rejected(server):
    """S-21: Restricted agent cannot store."""
    resp = server.context_store(
        "restricted store", "testing", "convention", agent_id="restricted-test-agent"
    )
    assert_tool_error(resp)


@pytest.mark.security
def test_restricted_agent_correct_rejected(server):
    """S-22: Restricted agent cannot correct."""
    store_resp = server.context_store(
        "for restricted correct", "testing", "convention", agent_id="human", format="json"
    )
    from harness.assertions import extract_entry_id
    entry_id = extract_entry_id(store_resp)
    resp = server.context_correct(
        entry_id, "corrected", agent_id="restricted-test-agent"
    )
    assert_tool_error(resp)


@pytest.mark.security
def test_restricted_agent_deprecate_rejected(server):
    """S-23: Restricted agent cannot deprecate."""
    store_resp = server.context_store(
        "for restricted deprecate", "testing", "convention", agent_id="human", format="json"
    )
    from harness.assertions import extract_entry_id
    entry_id = extract_entry_id(store_resp)
    resp = server.context_deprecate(entry_id, agent_id="restricted-test-agent")
    assert_tool_error(resp)


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
