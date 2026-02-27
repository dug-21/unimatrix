# Test Plan: C7 — Static Fixtures

## Scope

JSON fixture files provide deterministic test data for security and edge case suites.

## Fixture Validation

| Fixture File | Consuming Suite | Validation |
|-------------|----------------|------------|
| injection_patterns.json | Security | Each "should_detect: true" pattern is rejected by server scanner |
| injection_patterns.json | Security | Each "should_detect: false" pattern is accepted by server |
| pii_samples.json | Security | Each "should_detect: true" sample is rejected by server scanner |
| pii_samples.json | Security | Each "should_detect: false" sample is accepted |
| unicode_corpus.json | Edge Cases | Each unicode string stores and retrieves intact |
| large_entries.json | Edge Cases | Large content entries handled (accepted or rejected with clear error) |

## Security Fixture Coverage

The injection_patterns.json covers 5 server scanner categories:
- InstructionOverride: 5 patterns
- RoleImpersonation: 5 patterns
- SystemPromptExtraction: 4 patterns
- DelimiterInjection: 4 patterns
- EncodingEvasion: 1 pattern
- SafeContent (false positives): 3 patterns

The pii_samples.json covers 4 server scanner categories:
- EmailAddress: 3 samples
- PhoneNumber: 3 samples
- SocialSecurityNumber: 2 samples
- ApiKey: 2 samples
- SafeContent (false positives): 2 samples

## Risk Coverage

| Risk | Static Fixture Role | Validation |
|------|-------------------|------------|
| R-06 | Provide known-good test vectors | Server detects all "should_detect: true" patterns |
| R-06 | False positive resistance | Server accepts all "should_detect: false" content |
