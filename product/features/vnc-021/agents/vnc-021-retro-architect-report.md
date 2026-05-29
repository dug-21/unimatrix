# Agent Report: vnc-021-retro-architect

## Task
Retrospective architecture review of shipped vnc-021 (HTTPS transport + static bearer token auth). Extract reusable knowledge, validate ADRs, identify lessons from hotspots and security findings.

## Findings

### Patterns
- **Updated #319 -> #4680**: CallerId pattern now includes HttpBearer(String) variant shipped by vnc-021. Added rate-limiting exemption behavior note.
- **Skipped**: Tower middleware composition (StaticTokenAuth -> PathRouter -> McpAdapter -> StreamableHttpService). Standard tower usage, not project-specific. Future HTTP features extend this stack, not replicate it.
- **Skipped**: BearerValidator trait pattern. One-off until W2-3 adds a second implementation.
- **Confirmed #4661**: rmcp HTTP transport dependency pattern. Still accurate -- no new transitive deps were needed.

### Procedures
- **Updated #3934 -> #4681**: Config section procedure now documents standalone validate function approach (vnc-021) alongside method approach (crt-036). Added Step 3a for auto-detect logic (TlsConfig.is_enabled() with Option<bool>). Updated Step 6 for non-tick config threading.

### ADR Validation
All 6 ADRs validated by successful implementation:
- ADR-001 (#4665): Constant-time validation. CR-01 through CR-04 passed.
- ADR-002 (#4666): Health auth bypass. 4 exact-match tests confirm.
- ADR-003 (#4667): Thin adapter boundary. R-01 spike confirmed extensions propagate; adapter still valuable for body size and error mapping.
- ADR-004 (#4668): Pre-TLS semaphore. RAII guard, no permit leaks.
- ADR-005 (#4669): rustls with configurable bypass. Auto-detect implemented correctly.
- ADR-006 (#4670): credential_type = "static_token". Constant asserted in tests.
- **No ADRs flagged for supersession.**

### Lessons
- **New #4682**: Atomic file creation with permissions -- use OpenOptions.mode() not create-then-chmod. From security finding #662 (TOCTOU).
- **New #4683**: HTTP body size limits must enforce on body stream, not Content-Length header. Chunked TE bypass. From security finding #663.

### Hotspot Analysis
- **compile_cycles (143)**: Covered by existing lessons #3439, #4593, #3815. vnc-021 adds a fourth data point at the lower end of the range. No new lesson.
- **file_breadth (155 files)**: Covered by existing lesson #3818 (rmcp source navigation). R-01 spike was the OQ driving exploration. Expected.
- **bash_for_search (686, 2.0σ)**: Known recurring issue covered by #2578, #3545, #4530. No new lesson.
- **context_load (295 KB before first write)**: 8-component feature with 6 ADRs and extensive pseudocode. Expected for design+delivery scope.
- **tool_failure_hotspot (33 failures at +155m)**: Agents navigating rmcp source in cargo registry. Expected per #3818.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 10 entries; mcp__unimatrix__context_search (5 queries) -- 6 ADRs + 1 pattern for vnc-021; existing lessons on compile cycles, bash-for-search, rmcp navigation all confirmed still accurate
- Stored: #4680 (corrected CallerId pattern), #4681 (corrected config procedure), #4682 (TOCTOU file creation lesson), #4683 (chunked TE body size lesson)
