# Test Plan: feedback-hooks (Wave 4)

## Tests

### T-R04-01: Helpful vote on agent entry does NOT generate signal
- **Setup**: Mock entry with trust_source = "agent"
- **Action**: Run trust_source filter check
- **Assert**: No FeedbackSignal emitted

### T-R04-02: Helpful vote on auto entry DOES generate signal
- **Setup**: Mock entry with trust_source = "auto"
- **Action**: Run trust_source filter and signal generation
- **Assert**: FeedbackSignal::HelpfulVote emitted with correct fields

### T-R04-03: Deprecation on neural entry DOES generate signal
- **Setup**: Mock entry with trust_source = "neural"
- **Action**: Run trust_source filter and signal generation
- **Assert**: FeedbackSignal::Deprecation emitted with correct fields

## Notes

These tests verify the trust_source filtering logic, not the full MCP handler flow. They can be unit tests on the filter function or helper that builds signals from entries.
