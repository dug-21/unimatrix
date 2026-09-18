# `.collective-delivery/` — Program-owned adoption-rail receiver (col-007)

This directory is Unimatrix's own copy of the collective's col-007 adoption-rail **receiver**
tooling. It is the one-time bootstrap substrate that lets an approved collective release reach
this repository as a **reviewable pull request** that Unimatrix — and only Unimatrix — decides
whether to merge.

- `delivery/`, `project/`, `boundary/`, `release/`, `contract/schema/` — vendored **verbatim**
  from the collective's `tools/` tree (the receiver entry points and their reuse closure). Do
  not edit these; update them only by re-vendoring from the collective.
- `INSTALLATION.json` (`collective.delivery-installation/1`) — this Program's own declaration:
  its opaque identity, the public mirror repository/ref it pulls from, and its canonical and
  proposal-base branches.
- `propose.js` — P1 job wrapper (opens a proposal PR; fail-closed).
- `preadoption-check.js` — P2 required pre-adoption check wrapper (blocks merge on non-zero).
- `package.json` — the three runtime dependencies (`ajv`, `ajv-formats`, `yaml`), pinned.

The receiver holds **no merge, admin, or collective grant**. Both entry points are fail-closed:
absent an approved, independently verifiable mirror source they hold/refuse and change nothing.
The collective **proposes**; Unimatrix alone adopts by merging.
