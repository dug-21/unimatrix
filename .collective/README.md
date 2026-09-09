# Collective participation surface

This `.collective/` directory is the collective's projected participation surface
for this Program. Files marked `authority: baseline` are collective-owned contract
content; a Program changes that content only by adopting a new Release, never by
editing it in place (see `contract/OBLIGATIONS.yaml`, OBL-5E08). `EXTENSIONS.md` is
Program-owned and is not collective-authoritative.

Authoritative local source of each fact (path within this directory):

- Adopted Release identity and content digest: `ADOPTED.yaml` — the sole record of
  what this Program has adopted. No other file in this surface restates it.
- Per-file digests and authority marking of this surface: `MANIFEST.yaml`.
- Admitted participation vocabulary, term registers and remainder causes:
  `contract/VOCABULARY.yaml`.
- Retained obligations: `contract/OBLIGATIONS.yaml`.
- Operating contract prose: `contract/CONTRACT.md`.
- Classified Program registry subset: `contract/REGISTRATIONS.yaml`.

This surface carries only classified, publishable material. It states no currency,
drift, proposal, or adoption-status value; the adopted Release is read solely from
`ADOPTED.yaml`, and only a Program's own merge makes this surface an adoption record.

What you can verify locally, from this surface alone:

- That every file listed in `MANIFEST.yaml` matches its recorded per-file digest,
  byte for byte — the projected bytes are intact and unaltered.
- That the content digest string recorded in `ADOPTED.yaml` is the one carried by the
  cleared pre-admission verdict for this adoption.

What this surface does not let you recompute locally: the adopted Release content
digest itself. It is assigned collective-side when a candidate is promoted to a
Release, over the candidate's canonical form, which this surface does not carry.
Its authority therefore rests on the collective's pre-admission clearance of that
Release, not on local recomputation from these files. Local recomputation covers
per-file projection integrity (via `MANIFEST.yaml`), not the release-content binding.
