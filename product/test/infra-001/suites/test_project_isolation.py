"""vnc-046 #800 — per-slug isolation over the true HTTPS/MCP wire.

The in-process Rust suite (`crates/unimatrix-server/tests/project_routing_
integration.rs`) proves per-slug observe isolation at the assembled `PathRouter`
edge; THIS suite proves the SAME INV-T2 invariant through the real HTTPS
transport (bearer + provisioned leaf cert) on ONE daemon hosting >=2 registered
slugs — the surface a future rewire would actually break, and C6's path to
`proven`.

Shape (every isolation test): BIDIRECTIONAL at N=2, marker-keyed read-as-barrier
(pattern #5347). Drive each slug's write ONLY through its own `/v1/{slug}/observe`
route; the own-read positive control is the durability barrier (retry-until-
present); the cross-read is a synchronous absence check gated behind it
(positive-gates-negative). Own-read timeout => the fixture/env is at fault
(fidelity/INFRA), never a silent isolation pass. Markers are feature-id-shaped
(so observe persists topic_signal, bugfix-832) and mutually non-substring (#5347).

Scope note (transparent, no fake-green): the OBSERVE surface is the robust,
sandbox-reachable wire proof and is what this suite asserts. The MCP-write
surface (`context_store` -> entries) and the `signal_class_counts` /
HTTPS==UDS-parity surfaces route through `cycle_review` + the embedding/serving
path, which is proven in the Docker infra-003 multi-tenant-isolation gate and by
the in-process Rust suite; those are NOT re-implemented here as flaky local
checks. See RISK-COVERAGE-REPORT.md.
"""

import pytest

from harness.conftest import ISO_SLUG_A, ISO_SLUG_B

# Feature-id-shaped, mutually non-substring markers (charset [a-z0-9-]).
_A = "arca-obs-a-1"
_B = "isob-obs-b-1"


def _assert_isolated(srv, writer, writer_marker, other, other_db_slug):
    """Drive `writer`'s observe, barrier on its own store (fidelity), then assert
    the OTHER slug's store folded nothing of it (isolation). Both directions are
    called as DISTINCT cases by the tests below — never inferred from each other."""
    status = srv.observe_record(writer, session_id=f"{writer}-sess", topic_signal=writer_marker)
    assert status == 204, (
        f"observe to /v1/{writer}/observe must be accepted (204) at the HTTPS edge; got {status}"
    )
    # Positive control / durability barrier: the write lands in the writer's own store.
    own = srv.wait_observation(writer, writer_marker)
    assert own >= 1, (
        f"fidelity/INFRA: {writer}'s own store never durably folded its observe "
        f"(marker {writer_marker!r}) within the barrier — env/fixture fault, not an isolation pass"
    )
    # Isolation: the other slug's store folded ZERO of the writer's marker.
    cross = srv.count_observations(other_db_slug, writer_marker)
    assert cross == 0, (
        f"ISOLATION BROKEN: {writer}'s marker {writer_marker!r} leaked into {other}'s store "
        f"(cross-slug observe contamination)"
    )


@pytest.mark.smoke
def test_observe_transcript_isolation_a_driver(multi_slug_http_server):
    """INV-T2 (A-driver): a delta to /v1/arch-a/observe lands in arch-a's store
    and NEVER in iso-b's — over the real HTTPS wire."""
    _assert_isolated(multi_slug_http_server, ISO_SLUG_A, _A, ISO_SLUG_B, ISO_SLUG_B)


@pytest.mark.smoke
def test_observe_transcript_isolation_b_driver(multi_slug_http_server):
    """INV-T2 (B-driver): the symmetric reverse mis-route guard (#5348) — a delta
    to /v1/iso-b/observe lands in iso-b's store and NEVER in arch-a's."""
    _assert_isolated(multi_slug_http_server, ISO_SLUG_B, _B, ISO_SLUG_A, ISO_SLUG_A)


def test_observe_isolation_matrix_bidirectional_2x2(multi_slug_http_server):
    """The full 2x2: after BOTH slugs write their own marker, each store holds
    ONLY its own (present-in-own AND absent-in-every-other), both directions."""
    srv = multi_slug_http_server
    ma, mb = "arca-mtx-a-1", "isob-mtx-b-1"
    assert srv.observe_record(ISO_SLUG_A, "arch-a-mtx", ma) == 204
    assert srv.observe_record(ISO_SLUG_B, "iso-b-mtx", mb) == 204
    # Own-read barriers (positive controls) first.
    assert srv.wait_observation(ISO_SLUG_A, ma) >= 1, "arch-a own-read (fidelity)"
    assert srv.wait_observation(ISO_SLUG_B, mb) >= 1, "iso-b own-read (fidelity)"
    # Cross cells absent in BOTH directions.
    assert srv.count_observations(ISO_SLUG_B, ma) == 0, "arch-a's marker must be absent from iso-b"
    assert srv.count_observations(ISO_SLUG_A, mb) == 0, "iso-b's marker must be absent from arch-a"


@pytest.mark.smoke
def test_unknown_slug_returns_404(multi_slug_http_server):
    """An unregistered slug 404s at the routing edge upstream of any write — never
    a default store, never a per-slug fold (R-07/R-09/R-10)."""
    srv = multi_slug_http_server
    code = srv.get_status("/v1/ghost-x/observe")
    assert code == 404, f"unregistered slug must 404 at the edge; got {code}"
