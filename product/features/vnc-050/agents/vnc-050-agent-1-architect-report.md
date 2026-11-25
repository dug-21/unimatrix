# vnc-050-agent-1-architect report (rework 1)

Revised ARCHITECTURE.md and ADR-001/003/004/005/006/007 per HUMAN-DECISIONS.md. No code changes.

Unimatrix: ADR-001 #5805->#5814, ADR-003 #5807->#5812, ADR-004 #5808->#5813, ADR-005 #5809->#5815, ADR-006 #5811->#5818, ADR-007 #5810->#5816 (written in error)->#5817 (restored, plus the bundle+headers note). ADR-002 #5806 unchanged.

Open: SPEC FR-14/AC-08, the MCP half of FR-15/AC-15, the write-rejected half of AC-13/FR-32, and RISK R-02/R-13 conflict with the accepted agent_id gap and need revision.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_get on #5805-#5811 to confirm the ADR ID mapping (briefing was skipped because this rework is driven by human decisions over existing ADRs)
- Stored: corrections #5812, #5813, #5814, #5815, #5817, #5818 via context_correct; nothing new to store as a pattern
