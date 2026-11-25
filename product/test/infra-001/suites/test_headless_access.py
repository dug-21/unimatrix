"""vnc-050: scoped headless tokens on the real HTTPS transport (#1010).

Drives a live `unimatrix serve --foreground` daemon (the `multi_slug_http_server`
fixture, slugs {arch-a, iso-b}) with tokens minted and revoked by the real
`client-bundle --headless` CLI against the running daemon's data dir, and reads
each slug's store back via sqlite. Every rejection has a success companion
(#4416). Covers the MCP-visible slice of R-02/R-03/R-04/R-05/R-13/R-15/R-17.
"""

from __future__ import annotations

import time
import uuid

import pytest

from harness.conftest import ISO_SLUG_A, ISO_SLUG_B  # noqa: F401

pytestmark = [pytest.mark.security]

A, B = ISO_SLUG_A, ISO_SLUG_B


def _label(prefix: str) -> str:
    return f"{prefix}-{uuid.uuid4().hex[:8]}"


def _tag() -> str:
    """Digit-free unique marker: random hex can trip the PII phone scanner."""
    return uuid.uuid4().hex.translate(str.maketrans("0123456789", "ghijklmnop"))


def _is_tool_error(msg: dict | None) -> bool:
    if msg is None:
        return True
    if "error" in msg:
        return True
    return bool(msg.get("result", {}).get("isError"))


def _text(msg: dict | None) -> str:
    if msg is None:
        return ""
    if "error" in msg:
        return str(msg["error"].get("message", ""))
    return " ".join(c.get("text", "") for c in msg.get("result", {}).get("content", []))


def _session(srv, slug, token):
    st, sid = srv.mcp_open(slug, token)
    assert st == 200 and sid, f"initialize on {slug} -> {st}"
    return sid


def _store(srv, slug, token, sid, content, agent_id="human"):
    st, msg, raw = srv.mcp_call(slug, token, sid, "context_store", {
        "content": content, "topic": "headless", "category": "convention",
        "agent_id": agent_id,
    })
    assert st == 200, raw[:300]
    return msg


def _entry_count(srv, slug, needle):
    return srv.query(slug, "SELECT count(*) FROM entries WHERE content LIKE ?",
                     (f"%{needle}%",))[0][0]


@pytest.fixture(scope="module")
def tokens(multi_slug_http_server):
    srv = multi_slug_http_server
    return {
        "read_a": srv.mint_token(A, "read", _label("ro")),
        "rw_a": srv.mint_token(A, "read-write", _label("rw")),
    }


def test_read_token_search_succeeds_store_denied(multi_slug_http_server, tokens):
    srv = multi_slug_http_server
    sid = _session(srv, A, tokens["read_a"])
    st, msg, raw = srv.mcp_call(A, tokens["read_a"], sid, "context_search",
                                {"query": "anything", "agent_id": "human"})
    assert st == 200 and not _is_tool_error(msg), raw[:300]
    marker = f"ro-write-{_tag()}"
    for agent in ("human", None, "system"):
        args = {"content": marker, "topic": "headless", "category": "convention"}
        if agent:
            args["agent_id"] = agent
        st, msg, raw = srv.mcp_call(A, tokens["read_a"], sid, "context_store", args)
        assert st == 200 and _is_tool_error(msg), f"read token stored as {agent}: {raw[:300]}"
    assert _entry_count(srv, A, marker) == 0


def test_read_write_token_store_succeeds_on_bound_slug(multi_slug_http_server, tokens):
    srv = multi_slug_http_server
    sid = _session(srv, A, tokens["rw_a"])
    marker = f"rw-write-{_tag()}"
    msg = _store(srv, A, tokens["rw_a"], sid, f"headless rw entry {marker}")
    assert not _is_tool_error(msg), _text(msg)[:300]
    assert _entry_count(srv, A, marker) == 1
    assert _entry_count(srv, B, marker) == 0


def test_scoped_token_other_slug_returns_403(multi_slug_http_server, tokens):
    srv = multi_slug_http_server
    for tok in (tokens["read_a"], tokens["rw_a"]):
        st, sid = srv.mcp_open(B, tok)
        assert st == 403 and sid is None
        st, _ = srv.observe_record_as(B, tok, f"s-{uuid.uuid4().hex[:8]}", "1010-headless-x")
        assert st == 403
    # Companion: same token accepted on its bound slug.
    st, sid = srv.mcp_open(A, tokens["rw_a"])
    assert st == 200 and sid


def test_cross_slug_403_body_identical_for_registered_and_unknown(multi_slug_http_server, tokens):
    """R-03/R-16: a scoped token's 403 must not reveal whether a slug exists."""
    srv = multi_slug_http_server
    tok = tokens["rw_a"]
    st_b, _, body_b = srv._post(f"{srv.base}/v1/{B}/mcp", {"jsonrpc": "2.0", "id": 1, "method": "ping"}, tok)
    st_x, _, body_x = srv._post(f"{srv.base}/v1/no-such-slug/mcp", {"jsonrpc": "2.0", "id": 1, "method": "ping"}, tok)
    assert st_b == 403
    assert (st_x, body_x) == (st_b, body_b)


def test_legacy_token_still_works_all_slugs(multi_slug_http_server):
    srv = multi_slug_http_server
    for slug in (A, B):
        sid = _session(srv, slug, srv.token)
        marker = f"legacy-{slug}-{_tag()}"
        msg = _store(srv, slug, srv.token, sid, f"legacy write {marker}")
        assert not _is_tool_error(msg), _text(msg)[:300]
        assert _entry_count(srv, slug, marker) == 1


def test_revoked_token_returns_401_without_restart(multi_slug_http_server):
    srv = multi_slug_http_server
    label = _label("rv")
    tok = srv.mint_token(A, "read-write", label)
    sid = _session(srv, A, tok)
    st, msg, _ = srv.mcp_call(A, tok, sid, "context_search", {"query": "x", "agent_id": "human"})
    assert st == 200 and not _is_tool_error(msg)
    r = srv.revoke_token(A, label)
    assert r.returncode == 0, r.stderr
    # Open session loses capability on the next request; new initialize refused.
    deadline = time.monotonic() + 10
    st = None
    while time.monotonic() < deadline:
        st, _, _ = srv.mcp_call(A, tok, sid, "context_search", {"query": "x", "agent_id": "human"})
        if st == 401:
            break
        time.sleep(0.2)
    assert st == 401
    assert srv.mcp_open(A, tok)[0] == 401
    # Tombstone: --list shows it; same-label re-mint fails.
    lst = srv.list_tokens(A)
    assert label in lst.stderr + lst.stdout and "revoked" in lst.stderr + lst.stdout
    with pytest.raises(RuntimeError):
        srv.mint_token(A, "read-write", label)
    # Secret never on the --list output (R-09).
    assert tok not in lst.stderr + lst.stdout


def test_read_token_observe_returns_403_zero_rows(multi_slug_http_server, tokens):
    """AC-18 server side at the real daemon (closes server_db_zero_observation_rows):
    read-token observe -> 403 'lacks SessionWrite', nothing persisted; the
    read-write companion lands (read-as-barrier) so the zero is not vacuous."""
    srv = multi_slug_http_server
    ro_marker = f"1010-ro{uuid.uuid4().hex[:6]}"
    rw_marker = f"1010-rw{uuid.uuid4().hex[:6]}"
    st, body = srv.observe_record_as(A, tokens["read_a"], f"s-{uuid.uuid4().hex[:8]}", ro_marker)
    assert st == 403 and "lacks SessionWrite" in body
    assert tokens["read_a"] not in body
    st, _ = srv.observe_record_as(A, tokens["rw_a"], f"s-{uuid.uuid4().hex[:8]}", rw_marker)
    assert st in (200, 202, 204)
    assert srv.wait_observation(A, rw_marker) >= 1
    assert srv.count_observations(A, ro_marker) == 0


def test_token_prefix_agent_id_rejected_over_http(multi_slug_http_server, tokens):
    srv = multi_slug_http_server
    sid = _session(srv, A, tokens["rw_a"])
    marker = f"forge-{_tag()}"
    msg = _store(srv, A, tokens["rw_a"], sid, marker, agent_id="token:ci")
    assert _is_tool_error(msg) and "reserved" in _text(msg).lower()
    assert _entry_count(srv, A, marker) == 0
    assert srv.query(A, "SELECT count(*) FROM agent_registry WHERE agent_id LIKE 'token:%'")[0][0] == 0


def test_read_token_leaves_learning_tables_unchanged(multi_slug_http_server, tokens):
    """AC-15/AC-26 at the real daemon: read-token search/get/briefing leave
    access counters and query_log untouched; a read-write get on another entry
    is the drain barrier, and a read-write search grows query_log (companion)."""
    srv = multi_slug_http_server
    sid_rw = _session(srv, A, tokens["rw_a"])
    ids = []
    bodies = ("Rust borrow checker lifetimes guide for async closures",
              "Kubernetes pod eviction thresholds and memory pressure tuning")
    for i, body in enumerate(bodies):
        m = f"learn-{i}-{_tag()}"
        msg = _store(srv, A, tokens["rw_a"], sid_rw, f"{body} {m}")
        assert not _is_tool_error(msg), _text(msg)[:300]
        ids.append(srv.query(A, "SELECT id FROM entries WHERE content LIKE ?", (f"%{m}%",))[0][0])
    x, y = ids

    def acc(eid):
        return srv.query(A, "SELECT access_count, last_accessed_at FROM entries WHERE id=?", (eid,))[0]

    ql = lambda: srv.query(A, "SELECT count(*) FROM query_log")[0][0]  # noqa: E731
    time.sleep(2)
    x0, y0, q0 = acc(x), acc(y), ql()

    sid_ro = _session(srv, A, tokens["read_a"])
    for tool, args in (
        ("context_search", {"query": "borrow checker lifetimes", "agent_id": "human"}),
        ("context_get", {"id": x, "agent_id": "human"}),
        ("context_lookup", {"id": x, "agent_id": "human"}),
        ("context_briefing", {"task": "borrow checker lifetimes", "agent_id": "human"}),
    ):
        st, msg, raw = srv.mcp_call(A, tokens["read_a"], sid_ro, tool, args)
        assert st == 200 and not _is_tool_error(msg), f"{tool}: {raw[:300]}"

    st, msg, _ = srv.mcp_call(A, tokens["rw_a"], sid_rw, "context_get", {"id": y, "agent_id": "human"})
    assert not _is_tool_error(msg)
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline and acc(y) == y0:
        time.sleep(0.5)
    assert acc(y) != y0, "rw companion never drained (barrier failed)"
    assert acc(x) == x0, f"read token moved access counters: {x0} -> {acc(x)}"
    assert ql() == q0, "read token wrote query_log"

    srv.mcp_call(A, tokens["rw_a"], sid_rw, "context_search", {"query": "borrow checker lifetimes", "agent_id": "human"})
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline and ql() == q0:
        time.sleep(0.5)
    assert ql() > q0, "rw search companion never logged a query"
