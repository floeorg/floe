---
title: Type-Driven Features
---

Floe's compiler knows the full structure of your types at compile time. This powers features that would normally require runtime libraries in TypeScript -- validation, test data generation, and more. Everything is generated as plain code with zero runtime dependencies.

## Why This Exists

In TypeScript, types are erased at compile time. Validating incoming JSON requires Zod or io-ts. Test fixtures require faker.js or hand-written factories. Every time you change a type, you update the schema and the factory too.

Floe's compiler already has the type information. It generates validators and test data directly -- always in sync because they come from the same source.

## `parse<T>` -- Runtime validation

`parse<T>` validates unknown data against a Floe type at runtime. The compiler generates the checking code inline -- no schema library needed.

```floe
// Validate JSON from an API
let user = json |> parse<User>?

// Validate with inline types
let point = data |> parse<{ x: number, y: number }>?

// Validate arrays
let items = raw |> parse<Array<Product>>?
```

### Return type

`parse<T>` always returns `Result<T, Error>`. Use `?` to unwrap or `match` to handle errors:

```floe
match data |> parse<User> {
  Ok(user) -> Console.log(user.name),
  Err(e) -> Console.error(e.message),
}
```

### What it generates

For `parse<User>(json)` where `type User = { name: string, age: number }`, the compiler emits type checks inline:

```typescript
(() => {
  let __v = json;
  if (typeof __v !== "object" || __v === null)
    return { ok: false, error: new Error("expected object, got " + typeof __v) };
  if (typeof (__v as any).name !== "string")
    return { ok: false, error: new Error("field 'name': expected string, got " + ...) };
  if (typeof (__v as any).age !== "number")
    return { ok: false, error: new Error("field 'age': expected number, got " + ...) };
  return { ok: true, value: __v as { name: string; age: number } };
})()
```

No runtime dependency. No schema definition to maintain. Change the type, the validation updates automatically.

### Supported types

| Type | Validation |
|------|-----------|
| `string`, `number`, `boolean` | `typeof` check |
| Record types | Object check + recursive field validation |
| `Array<T>` | `Array.isArray` + element validation loop |
| `Option<T>` | Allow `undefined` or validate inner type |
| Named types | Object structure check |

### Common patterns

```floe
// API response validation
let fetchUsers() -> Promise<Result<Array<User>, Error>> = {
  let response = Http.get("/api/users") |> Promise.await?
  let data = Http.json(response) |> Promise.await?
  data |> parse<Array<User>>
}

// Form input validation
let validateForm(data: unknown) -> Result<ContactForm, Error> = {
  data |> parse<ContactForm>
}
```

---

## `mock<T>` -- Test data generation

`mock<T>` generates test data from a type definition. The compiler emits object literals directly -- no faker.js, no test factories, no runtime cost.

```floe
type User = {
  id: string,
  name: string,
  age: number,
}

let testUser = mock<User>
// { id: "mock-id-1", name: "mock-name-2", age: 3 }
```

### Field overrides

Override specific fields when you need control over certain values:

```floe
let admin = mock<User>(name: "Alice", age: 30)
// { id: "mock-id-1", name: "Alice", age: 30 }
```

Non-overridden fields are still auto-generated. This is useful when your test cares about specific values but not others.

### Generation rules

| Type | Generated Value |
|------|----------------|
| `string` | `"mock-fieldname-N"` (uses the field name for context) |
| `number` | Sequential integers (1, 2, 3, ...) |
| `boolean` | Alternates true/false |
| `Array<T>` | Array with 1 mock element |
| Record types | All fields mocked recursively |
| Unions | First variant |
| `Option<T>` | The inner value (not undefined) |
| String literal unions | First variant |
| Newtypes | Mock the inner type |

### Using with tests

`mock<T>` pairs naturally with Floe's inline test blocks:

```floe
type Todo = {
  id: string,
  text: string,
  done: boolean,
}

let toggleDone(todo: Todo) -> Todo = {
  Todo(..todo, done: !todo.done)
}

test "toggle flips done status" {
  let todo = mock<Todo>(done: false)
  let toggled = toggleDone(todo)
  assert toggled.done == true
}

test "toggle preserves other fields" {
  let todo = mock<Todo>
  let toggled = toggleDone(todo)
  assert toggled.id == todo.id
  assert toggled.text == todo.text
}
```

### Complex types

`mock<T>` handles nested and complex types recursively:

```floe
type Order = {
  id: string,
  items: Array<OrderItem>,
  status: OrderStatus,
}

type OrderItem = {
  productId: string,
  quantity: number,
}

type OrderStatus = | Pending
  | Shipped { trackingId: string }
  | Delivered

let testOrder = mock<Order>
// {
//   id: "mock-id-1",
//   items: [{ productId: "mock-productId-2", quantity: 3 }],
//   status: { __tag: "Pending" }
// }
```

---

## Comparison with TypeScript

| Task | TypeScript | Floe |
|------|-----------|------|
| Validate API data | Zod, io-ts, or hand-written checks | `parse<T>` |
| Generate test data | faker.js, factories, or hand-written objects | `mock<T>` |
| Keep in sync | Manual -- update schema when type changes | Automatic -- same source |
| Runtime cost | Schema library bundled in production | Zero -- compiled away |

---

## Testing

Floe supports inline test blocks that live alongside the code they test. Tests are type-checked with the rest of your code but stripped from production output.

### Writing Tests

Use the `test` keyword followed by a name and a block of `assert` statements:

```floe
let add(a: number, b: number) -> number = { a + b }

test "addition" {
  assert add(1, 2) == 3
  assert add(-1, 1) == 0
  assert add(0, 0) == 0
}
```

`assert` takes any expression that evaluates to `boolean`. The compiler enforces this at compile time.

### Co-located Tests

Tests live in the same file as the code they test:

```floe
type Validation = | Valid { string }
  | Empty
  | TooShort
  | TooLong

let validate(input: string) -> Validation = {
  let len = input |> String.length
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

### Running Tests

```bash
floe test src/          # all tests in a directory
floe test src/math.fl   # tests in a specific file
```

| Command | Test blocks |
|---------|-------------|
| `floe test` | Compiled and executed |
| `floe check` | Type-checked, not executed |
| `floe build` | Stripped from output |

### Test Rules

- `test` is a contextual keyword -- it only starts a test block when followed by a string literal
- `assert` is only valid inside test blocks
- Test blocks cannot be exported
- Multiple test blocks per file are allowed
