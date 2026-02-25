"""Unimatrix integration test harness.

Provides MCP client, test data generators, and assertion helpers
for black-box testing of the unimatrix-server binary.
"""

from harness.client import UnimatrixClient, MCPResponse, ClientError, TimeoutError, ServerDied
from harness.assertions import (
    assert_tool_success,
    assert_tool_error,
    assert_jsonrpc_error,
    parse_entry,
    parse_entries,
    parse_status_report,
    assert_entry_has,
    assert_search_contains,
    assert_search_not_contains,
    extract_entry_id,
    get_result_text,
)
from harness.generators import (
    make_entry,
    make_entries,
    make_contradicting_pair,
    make_correction_chain,
    make_injection_payloads,
    make_pii_content,
    make_unicode_edge_cases,
    make_bulk_dataset,
    log_seed_on_failure,
    CATEGORIES,
    TOPICS,
)

__all__ = [
    "UnimatrixClient",
    "MCPResponse",
    "ClientError",
    "TimeoutError",
    "ServerDied",
    "assert_tool_success",
    "assert_tool_error",
    "assert_jsonrpc_error",
    "parse_entry",
    "parse_entries",
    "parse_status_report",
    "assert_entry_has",
    "assert_search_contains",
    "assert_search_not_contains",
    "extract_entry_id",
    "get_result_text",
    "make_entry",
    "make_entries",
    "make_contradicting_pair",
    "make_correction_chain",
    "make_injection_payloads",
    "make_pii_content",
    "make_unicode_edge_cases",
    "make_bulk_dataset",
    "log_seed_on_failure",
    "CATEGORIES",
    "TOPICS",
]
