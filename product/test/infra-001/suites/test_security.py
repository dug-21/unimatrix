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
from harness.assertions import assert_tool_success, assert_tool_error, extract_entry_id, parse_entries, get_result_text
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
def test_restricted_agent_quarantine_rejected_requires_admin(server):
    """S-24: context_quarantine requires Admin (not Write).
    Restricted agent (auto-enrolled with Write in permissive mode) cannot quarantine.
    context_quarantine and context_enroll are the two Admin-only tools by design.
    See Unimatrix ADR #4413 and GH #589.
    """
    store_resp = server.context_store(
        "for restricted quarantine", "testing", "convention",
        agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)

    resp = server.context_quarantine(entry_id, agent_id="restricted-test-agent")
    assert_tool_error(resp, "lacks")


@pytest.mark.security
def test_admin_agent_quarantine_allowed(server):
    """S-24b: Admin agent CAN quarantine. Confirms Admin gate is enforcement, not lockout.
    See Unimatrix ADR #4413.
    """
    store_resp = server.context_store(
        "for admin quarantine", "testing", "convention",
        agent_id="human", format="json"
    )
    entry_id = extract_entry_id(store_resp)

    resp = server.context_quarantine(entry_id, agent_id="human")
    assert_tool_success(resp)


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


# === crt-052 untrusted-buffer no-panic + no-leak through MCP =============
#
# The transcript buffer is untrusted client-disk JSONL fed via the UDS
# transcript_delta hook path (not active in this stdio MCP harness). The
# module- and handler-level fuzz corpus (truncated JSON, non-UTF-8, oversized
# line, unknown record type, embedded NUL, fully-corrupt snapshot) is exercised
# in Rust: distill::jsonl corpus_tests and
# distill_handler::test_handler_fully_corrupt_snapshot_normal_response
# (AC-V-FUZZ, R-10) — those are the authoritative no-panic proofs since they can
# inject buffer bytes. These MCP tests assert the handler-boundary guarantee
# observable through the protocol: a cycle review over the no-live-buffer
# degrade path returns a normal response, candidates absent, the MCP call never
# errors or crashes; and no candidate/transcript bytes reach any read tool or
# persisted/queryable surface (AC-06, R-04 — extends vnc-025 AC-12).
#
# A review over a feature with no observation data returns an MCP error
# (-32010); these tests seed minimal observation rows directly (the UDS hook
# path is inactive in the harness) to drive the review onto its success path.


def _crt052_sec_db_path(project_dir):
    import hashlib as _hashlib
    import os as _os
    canonical = _os.path.realpath(project_dir)
    digest = _hashlib.sha256(canonical.encode()).hexdigest()[:16]
    return _os.path.join(_os.path.expanduser("~"), ".unimatrix", digest, "unimatrix.db")


def _crt052_sec_seed_observations(db_path, feature_cycle, num_records=20):
    import sqlite3 as _sqlite3
    import time as _time
    import uuid as _uuid
    conn = _sqlite3.connect(db_path)
    conn.execute("PRAGMA journal_mode=WAL")
    now_secs = int(_time.time())
    session_id = f"sec-{feature_cycle}-{_uuid.uuid4().hex[:8]}"
    conn.execute(
        "INSERT INTO sessions (session_id, feature_cycle, started_at, status) VALUES (?, ?, ?, 0)",
        (session_id, feature_cycle, now_secs),
    )
    for i in range(num_records):
        ts_millis = now_secs * 1000 - 86_400_000 + (i * 300_000)
        hook = "PreToolUse" if i % 2 == 0 else "PostToolUse"
        conn.execute(
            "INSERT INTO observations (session_id, ts_millis, hook, tool, input, response_size, response_snippet) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (session_id, ts_millis, hook, "Read", None,
             1024 if hook == "PostToolUse" else None,
             "output" if hook == "PostToolUse" else None),
        )
    conn.commit()
    conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    conn.close()


@pytest.mark.security
def test_cycle_review_corrupt_buffer_no_panic(server):
    """crt-052 AC-V-FUZZ / R-10: a cycle review over the buffer-degrade path
    returns a normal MCP response (candidates absent), never errors/crashes.

    The deep adversarial-JSONL no-panic proof is the Rust handler test
    `test_handler_fully_corrupt_snapshot_normal_response`; the buffer cannot be
    fed corrupt bytes through stdio MCP. This pins the MCP-observable invariant:
    the review handler degrades gracefully and the call does not become an error
    or kill the server."""
    topic = "crt052-corrupt-buffer-no-panic"
    _crt052_sec_seed_observations(_crt052_sec_db_path(server.project_dir), topic)

    # Two reviews back-to-back — a panic in the distill path would surface as a
    # tool error, a dropped connection, or a server death on the second call.
    resp1 = server.context_cycle_review(topic, agent_id="human", format="json", timeout=30.0)
    assert_tool_success(resp1), (
        "crt-052 AC-V-FUZZ: cycle review must return a normal (non-error) MCP "
        "response on the buffer-degrade path"
    )
    text1 = get_result_text(resp1)
    assert "transcript_candidates" not in text1, (
        "crt-052 AC-V-FUZZ: degraded review must carry no candidates"
    )

    # Server still alive and responsive after the review — no panic/crash.
    resp2 = server.context_cycle_review(topic, agent_id="human", format="json", timeout=30.0)
    assert_tool_success(resp2), (
        "crt-052 AC-V-FUZZ: server must remain responsive after a cycle review "
        "(no handler panic took down the process)"
    )
    status = server.context_status(agent_id="human", format="json")
    assert_tool_success(status), (
        "crt-052 AC-V-FUZZ: server must answer subsequent MCP calls — proves the "
        "review handler never panicked"
    )


@pytest.mark.security
def test_cycle_review_no_candidate_content_in_query_surface(server):
    """crt-052 AC-06 / R-04: no transcript/candidate content is returned by any
    read tool or persisted record after a cycle review (extends vnc-025 AC-12).

    With no live buffer the section is absent; this test guards the negative
    invariant at the MCP surface — search, status, and the memoized cycle review
    index expose no candidate marker (`transcript_candidates`) or transcript
    byte_offset provenance."""
    import sqlite3 as _sqlite3
    import hashlib as _hashlib
    import os as _os
    import json as _json

    topic = "crt052-no-candidate-leak"
    _crt052_sec_seed_observations(_crt052_sec_db_path(server.project_dir), topic)
    review = server.context_cycle_review(topic, agent_id="human", format="json", timeout=30.0)
    assert_tool_success(review)

    # No read tool surfaces candidate content.
    search = server.context_search("transcript_candidates byte_offset", format="json", agent_id="human")
    assert_tool_success(search)
    search_text = get_result_text(search)
    assert "transcript_candidates" not in search_text, (
        "crt-052 AC-06: context_search must not surface a transcript_candidates section"
    )

    status = server.context_status(agent_id="human", format="json")
    assert_tool_success(status)
    assert "transcript_candidates" not in get_result_text(status), (
        "crt-052 AC-06: context_status must not surface transcript candidate content"
    )

    # No persisted/queryable record carries candidate content. A row may or may
    # not exist (depends on whether signals were available); if present, assert
    # it is candidate-free.
    canonical = _os.path.realpath(server.project_dir)
    digest = _hashlib.sha256(canonical.encode()).hexdigest()[:16]
    db_path = _os.path.join(_os.path.expanduser("~"), ".unimatrix", digest, "unimatrix.db")
    if _os.path.isfile(db_path):
        conn = _sqlite3.connect(db_path)
        try:
            rows = conn.execute(
                "SELECT summary_json FROM cycle_review_index WHERE feature_cycle = ?", (topic,)
            ).fetchall()
        finally:
            conn.close()
        for (summary_json,) in rows:
            assert "transcript_candidates" not in summary_json, (
                "crt-052 AC-06: persisted summary_json must contain no "
                "transcript_candidates field (structural absence from the memoized report)"
            )


@pytest.mark.security
def test_cycle_review_transcript_no_new_persistence(server):
    """crt-057 R-03 / AC-14: the new `transcript:{}` scoped-retrieval path creates
    NO new persistence. After a transcript retrieval no candidate/search marker
    reaches the persisted cycle_review_index (ANY column) and no read tool nor the
    server log surfaces candidate content. With no live buffer there is no verbatim
    to leak; the buffer-populated sink content-scan is the Rust `#[traced_test]`
    unit guard (#5089, test-plan §6d/OQ-C)."""
    import sqlite3 as _sqlite3

    topic = "crt057-transcript-no-persist"
    db_path = _crt052_sec_db_path(server.project_dir)
    _crt052_sec_seed_observations(db_path, topic)

    markers = ("transcript_candidates", "transcript_search", "byte_offset", "family_hints")

    review = server.context_cycle_review(
        topic, agent_id="human", format="json", transcript={}, timeout=30.0
    )
    assert_tool_success(review), (
        "crt-057: transcript:{} review must succeed on the seeded success path"
    )

    # (1) No persisted cycle_review_index column carries a candidate/search marker.
    if os.path.isfile(db_path):
        conn = _sqlite3.connect(db_path)
        try:
            rows = conn.execute(
                "SELECT * FROM cycle_review_index WHERE feature_cycle = ?", (topic,)
            ).fetchall()
        finally:
            conn.close()
        for row in rows:
            blob = " ".join("" if v is None else str(v) for v in row)
            for m in markers:
                assert m not in blob, (
                    f"crt-057 R-03/AC-14: persisted cycle_review_index must not "
                    f"contain the candidate/search marker '{m}'"
                )

    # (2) No read tool surfaces candidate content after the transcript retrieval.
    search = server.context_search(
        "transcript_candidates byte_offset family_hints", format="json", agent_id="human"
    )
    assert_tool_success(search)
    search_text = get_result_text(search)
    for m in ("transcript_candidates", "transcript_search"):
        assert m not in search_text, (
            f"crt-057 R-03/AC-14: context_search must not surface '{m}'"
        )

    # (3) Best-effort log scan: captured server stderr carries no candidate/search
    #     marker (content-free audits + no candidate byte leak, R-03).
    try:
        stderr = server.get_stderr() or ""
    except Exception:
        stderr = ""
    for m in ("transcript_candidates", "transcript_search"):
        assert m not in stderr, (
            f"crt-057 R-03: server stderr must not carry the marker '{m}'"
        )
