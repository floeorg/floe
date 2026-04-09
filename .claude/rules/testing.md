<!-- managed by milky-kit | DO NOT EDIT — changes will be overwritten on next sync -->

# Testing

**Write tests for any logic you add or change.** Don't ask — just write them. If a function matches the "worth testing" criteria below, it must have tests before shipping.

## When to write tests

**Test logic, not plumbing.** If a function makes decisions, transforms data, or enforces invariants — test it. If it mostly calls an external API and passes through results — don't.

### Worth testing (unit tests)
- State machines and lifecycle transitions
- Parsers, extractors, and data transformers
- Ranking/scoring/prioritization logic
- Context assembly and truncation
- Anything with clear invariants or edge cases

### Worth testing (integration tests)
- Database queries — does the query return what you expect against a real test database?
- Multi-module flows — message in -> task created -> queryable
- Pipeline composition — do chained steps produce correct output?

### Schema and constraint tests (recommended for schema changes)
- **UNIQUE indexes**: test that duplicates are rejected, and that allowed combinations succeed
- **CHECK constraints**: test valid and invalid values
- **DEFAULT values**: test that records created without the field get the expected default
- **Complex queries**: test that JOINs, CTEs, or window functions return expected results

### Not worth testing
- Thin API wrappers — you'd be testing your mock
- Single-line delegations or trivial getters
- Serialization/deserialization already covered by derives

## How to write tests

### Unit tests live next to the code
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_cannot_transition_from_idle_to_done() {
        // ...
    }
}
```

### Test naming
- Name tests after the behavior, not the function: `task_with_past_due_date_is_overdue` not `test_is_overdue`
- Group related tests in the same `mod tests` block

### Keep tests fast
- Use a dedicated test database for integration tests
- No network calls in tests — if you must, use a mock server behind a feature flag
- No `sleep` in tests — use `tokio::time::pause()` for time-dependent logic

## Property testing
Use `proptest` for functions with wide input spaces (parsers, serialization roundtrips, state machines). Add when a unit test feels like it's only covering the happy path.
