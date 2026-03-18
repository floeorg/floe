---
title: Testing
---

Floe supports inline test blocks that live alongside the code they test. Tests are type-checked with the rest of your code but stripped from production output.

## Writing tests

Use the `test` keyword followed by a name and a block of `assert` statements:

```floe
fn add(a: number, b: number) -> number { a + b }

test "addition" {
  assert add(1, 2) == 3
  assert add(-1, 1) == 0
  assert add(0, 0) == 0
}
```

## Assert

`assert` takes any expression that evaluates to `boolean`. If the expression is false, the test fails.

```floe
test "comparisons" {
  assert 1 < 2
  assert "hello" == "hello"
  assert true != false
}
```

The compiler enforces that assert expressions are boolean at compile time.

## Co-located tests

Tests live in the same file as the code they test. This makes it easy to keep tests in sync with the implementation:

```floe
type Validation {
  | Valid { string }
  | Empty
  | TooShort
  | TooLong
}

fn validate(input: string) -> Validation {
  const len = input |> String.length
  match len {
    0 -> Empty,
    1 -> TooShort,
    _ -> match len > 100 {
      true -> TooLong,
      false -> Valid(input),
    },
  }
}

test "validation" {
  assert validate("") == Empty
  assert validate("a") == TooShort
  assert validate("hello") == Valid("hello")
}
```

## Running tests

```bash
# Run all tests in a directory
floe test src/

# Run tests in a specific file
floe test src/math.fl
```

## Behavior in different commands

| Command | Test blocks |
|---------|-------------|
| `floe test` | Compiled and executed |
| `floe check` | Type-checked, not executed |
| `floe build` | Stripped from output |

## Rules

- `test` is a contextual keyword - it only starts a test block when followed by a string literal. You can still use `test` as a function or variable name.
- `assert` is a keyword that is only valid inside test blocks.
- Test blocks cannot be exported.
- Multiple test blocks per file are allowed.
