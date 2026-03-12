# Test Plan: CLAUDE.md Block Template (Component 3)

## Content Review Checks

### CR-01: Sentinel Markers
- [ ] Open sentinel: `<!-- unimatrix-init v1: DO NOT REMOVE THIS LINE -->`
- [ ] Close sentinel: `<!-- end unimatrix-init v1 -->`
- [ ] Open sentinel is the FIRST line of the block
- [ ] Close sentinel is the LAST line of the block
- [ ] Version number "v1" present in both markers

### CR-02: Skills Table (AC-01a, ADR-005)
- [ ] Table header: "Skill | When to Use"
- [ ] Row: `/unimatrix-init` with description
- [ ] Row: `/unimatrix-seed` with description
- [ ] Only unimatrix-* prefixed skills listed (no store-adr, retro, etc.)
- [ ] Exactly 2 skills in table

### CR-03: Category Guide (AC-01b)
- [ ] Table lists 5 categories: decision, pattern, procedure, convention, lesson-learned
- [ ] No `outcome` category (per SCOPE.md and ARCHITECTURE.md)
- [ ] Each category has a "What Goes Here" description
- [ ] Skill references where applicable (store-adr, store-pattern, store-procedure, store-lesson)

### CR-04: Usage Triggers (AC-01c)
- [ ] "When to Invoke" section present
- [ ] Covers: before implementing, after decisions, after shipping, when technique evolves
- [ ] Actionable guidance for each trigger

### CR-05: Self-Containment (AC-11)
- [ ] Block is readable without consulting other docs
- [ ] A newcomer can answer: "what skills exist and when to use each?"
- [ ] No broken references or undefined terms

### CR-06: Append Safety
- [ ] Block is separated from preceding content by blank lines
- [ ] Block does not assume any specific position in CLAUDE.md
- [ ] No markdown syntax that could break preceding content (e.g., unclosed code blocks)
