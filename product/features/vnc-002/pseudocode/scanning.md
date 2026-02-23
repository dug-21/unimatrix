# Pseudocode: scanning (C2)

## File: `crates/unimatrix-server/src/scanning.rs`

### Types

```
enum PatternCategory:
    InstructionOverride
    RoleImpersonation
    SystemPromptExtraction
    DelimiterInjection
    EncodingEvasion
    EmailAddress
    PhoneNumber
    SocialSecurityNumber
    ApiKey

struct CompiledPattern:
    category: PatternCategory
    description: &'static str
    regex: Regex

struct ContentScanner:
    injection_patterns: Vec<CompiledPattern>
    pii_patterns: Vec<CompiledPattern>

struct ScanResult:
    category: PatternCategory
    description: &'static str
    matched_text: String
```

### Display for PatternCategory

```
fn fmt(self) -> &str:
    match self:
        InstructionOverride => "InstructionOverride"
        RoleImpersonation => "RoleImpersonation"
        SystemPromptExtraction => "SystemPromptExtraction"
        DelimiterInjection => "DelimiterInjection"
        EncodingEvasion => "EncodingEvasion"
        EmailAddress => "EmailAddress"
        PhoneNumber => "PhoneNumber"
        SocialSecurityNumber => "SocialSecurityNumber"
        ApiKey => "ApiKey"
```

### Pattern Definitions

```
fn injection_patterns() -> Vec<CompiledPattern>:
    // InstructionOverride patterns (~8)
    "(?i)ignore\\s+(all\\s+)?previous\\s+instructions"
    "(?i)disregard\\s+(all\\s+)?(above|prior|previous)"
    "(?i)forget\\s+(all\\s+)?your\\s+(previous\\s+)?instructions"
    "(?i)override\\s+(all\\s+)?(previous|prior|above)\\s+(instructions|rules|guidelines)"
    "(?i)do\\s+not\\s+follow\\s+(the\\s+)?(above|previous|prior)\\s+(instructions|rules)"
    "(?i)new\\s+instructions?\\s*:"
    "(?i)system\\s*:\\s*you\\s+are"
    "(?i)\\[\\s*system\\s*\\]"

    // RoleImpersonation patterns (~6)
    "(?i)you\\s+are\\s+now\\s+(?:a\\s+|an\\s+)?(?!going|ready|able|about)"
    "(?i)act\\s+as\\s+(?:a\\s+|an\\s+)?(?:root|admin|superuser|developer|hacker)"
    "(?i)pretend\\s+(?:to\\s+be|you\\s+are)\\s+(?:a\\s+|an\\s+)?"
    "(?i)assume\\s+the\\s+(?:role|identity|persona)\\s+of"
    "(?i)you\\s+must\\s+(?:now\\s+)?(?:be|become|act\\s+as)"
    "(?i)switch\\s+(?:to|into)\\s+(?:a\\s+|an\\s+)?(?:new\\s+)?(?:role|mode|persona)"

    // SystemPromptExtraction patterns (~5)
    "(?i)(?:show|display|reveal|output|print|repeat)\\s+(?:your\\s+)?(?:system\\s+)?(?:prompt|instructions|rules)"
    "(?i)what\\s+(?:are|were)\\s+your\\s+(?:initial|original|system)?\\s*instructions"
    "(?i)(?:copy|paste|echo)\\s+(?:the\\s+)?(?:above|previous)\\s+(?:text|instructions|prompt)"
    "(?i)dump\\s+(?:your\\s+)?(?:system\\s+)?(?:prompt|config|instructions)"
    "(?i)tell\\s+me\\s+(?:your\\s+)?(?:system\\s+)?(?:prompt|instructions)"

    // DelimiterInjection patterns (~5)
    "(?i)<\\s*/\\s*(?:system|instruction|prompt|context|message)\\s*>"
    "(?i)<\\s*(?:system|instruction|prompt)\\s*>"
    "(?i)```\\s*(?:system|instruction|prompt)"
    "(?i)---\\s*(?:SYSTEM|END|BEGIN)\\s*---"
    "(?i)\\[\\s*(?:INST|SYS|END)\\s*\\]"

    // EncodingEvasion patterns (~4)
    "(?i)(?:base64|b64)\\s*(?:decode|encoded?)\\s*:"
    "(?i)\\\\u[0-9a-fA-F]{4}\\s*\\\\u[0-9a-fA-F]{4}\\s*\\\\u[0-9a-fA-F]{4}"
    "(?i)&#x?[0-9a-fA-F]+;\\s*&#x?[0-9a-fA-F]+;\\s*&#x?[0-9a-fA-F]+;"
    "(?i)%[0-9a-fA-F]{2}%[0-9a-fA-F]{2}%[0-9a-fA-F]{2}"

fn pii_patterns() -> Vec<CompiledPattern>:
    // EmailAddress (~1)
    "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}"

    // PhoneNumber (~2)
    "(?:\\+?1[\\s.-]?)?\\(?[2-9]\\d{2}\\)?[\\s.-]?\\d{3}[\\s.-]?\\d{4}"
    "(?:\\+?1[\\s.-]?)?[2-9]\\d{2}[\\s.-]?\\d{3}[\\s.-]?\\d{4}"

    // SocialSecurityNumber (~1)
    "\\b\\d{3}-\\d{2}-\\d{4}\\b"

    // ApiKey (~4)
    "(?i)bearer\\s+[a-zA-Z0-9._~+/=-]{20,}"
    "AKIA[A-Z0-9]{16}"
    "ghp_[a-zA-Z0-9]{36}"
    "gho_[a-zA-Z0-9]{36}"
    "ghs_[a-zA-Z0-9]{36}"
```

### Implementation

```
static SCANNER: OnceLock<ContentScanner> = OnceLock::new()

impl ContentScanner:
    fn global() -> &'static ContentScanner:
        SCANNER.get_or_init(|| ContentScanner::new())

    fn new() -> Self:
        injection = injection_patterns()  // compile all regex
        pii = pii_patterns()             // compile all regex
        ContentScanner { injection_patterns: injection, pii_patterns: pii }

    fn scan(&self, content: &str) -> Result<(), ScanResult>:
        // Check injection patterns first (higher priority)
        for pattern in &self.injection_patterns:
            if let Some(m) = pattern.regex.find(content):
                return Err(ScanResult {
                    category: pattern.category,
                    description: pattern.description,
                    matched_text: m.as_str().to_string(),
                })
        // Then PII patterns
        for pattern in &self.pii_patterns:
            if let Some(m) = pattern.regex.find(content):
                return Err(ScanResult {
                    category: pattern.category,
                    description: pattern.description,
                    matched_text: m.as_str().to_string(),
                })
        Ok(())

    fn scan_title(&self, title: &str) -> Result<(), ScanResult>:
        // Title gets injection patterns only, NOT PII
        for pattern in &self.injection_patterns:
            if let Some(m) = pattern.regex.find(title):
                return Err(ScanResult {
                    category: pattern.category,
                    description: pattern.description,
                    matched_text: m.as_str().to_string(),
                })
        Ok(())
```

### Key Constraints
- OnceLock ensures patterns compiled exactly once
- Injection patterns checked before PII patterns
- Title only gets injection patterns (not PII)
- ~50 total patterns across all categories
- ScanResult.matched_text is NOT included in the error returned to the client
- Pattern specificity: "act as" requires additional context (admin/root/etc) to avoid false positives on "act as a proxy"
