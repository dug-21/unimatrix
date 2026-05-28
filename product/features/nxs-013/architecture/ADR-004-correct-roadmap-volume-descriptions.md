## ADR-004: Correct WAVE2-ROADMAP.md and PRODUCT-VISION.md Volume Descriptions

### Context

OQ-02 asks whether WAVE2-ROADMAP.md should be corrected or left as a historical document.

PRODUCT-VISION.md (line 439) references WAVE2-ROADMAP.md as "the authoritative Wave 2 planning document." Both documents describe W2-1 with a multi-volume model (`unimatrix-data`, `unimatrix-analytics`, `unimatrix-shared`) that was never shipped. nan-014 delivered a single `unimatrix-data` volume with ONNX models baked into the image and config inside the data volume.

Leaving factually incorrect volume descriptions in a document labeled "authoritative" causes downstream agents and humans to design against the wrong model.

### Decision

Correct the W2-1 sections in both documents to reflect the shipped single-volume design:
- Single `unimatrix-data` named volume containing databases, vector indexes, config, and logs.
- ONNX models baked into the image (no separate shared volume).
- Add a one-line annotation to each edit: "Updated to reflect nan-014 shipped design."

Edits are constrained to the W2-1 volume description only. No other sections are modified in either document.

### Consequences

- **Easier**: Both documents are factually correct. Agents and humans reading W2-1 get the shipped design.
- **Easier**: Eliminates the risk of future features being designed against a multi-volume model that does not exist.
- **Harder**: The documents no longer preserve the original design intent for the volume separation. This is acceptable — the original design was superseded by nan-014's implementation, and ADR-005 (Unimatrix #4573) documents the rationale.
