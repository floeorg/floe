---
title: Type-Driven Features
---

Floe's compiler knows the full structure of your types at compile time. This powers features that would normally require runtime libraries in TypeScript -- validation, test data generation, and more. Everything is generated as plain code with zero runtime dependencies.

## The idea

In TypeScript, your types are erased at compile time. If you want to validate incoming JSON, you reach for Zod. If you want test fixtures, you reach for faker.js or write factories by hand. Every time you change a type, you update the schema and the factory too.

In Floe, the compiler already has the type information. It can generate validators and test data directly -- and they're always in sync because they come from the same source.

## `parse<T>` -- Runtime validation

`parse<T>` validates unknown data against a Floe type at runtime. The compiler generates the checking code inline -- no schema library needed.

```floe
// Validate JSON from an API
const user = json |> parse<User>?

// Validate with inline types
const point = data |> parse<{ x: number, y: number }>?

// Validate arrays
const items = raw |> parse<Array<Product>>?
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

For `parse<User>(json)` where `type User { name: string, age: number }`, the compiler emits type checks inline:

```typescript
(() => {
  const __v = json;
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
fn fetchUsers() -> async Result<Array<User>, Error> {
  const response = await Http.get("/api/users")?
  const data = await Http.json(response)?
  data |> parse<Array<User>>
}

// Form input validation
fn validateForm(data: unknown) -> Result<ContactForm, Error> {
  data |> parse<ContactForm>
}
```

---

## `mock<T>` -- Test data generation

`mock<T>` generates test data from a type definition. The compiler emits object literals directly -- no faker.js, no test factories, no runtime cost.

```floe
type User {
  id: string,
  name: string,
  age: number,
}

const testUser = mock<User>
// { id: "mock-id-1", name: "mock-name-2", age: 3 }
```

### Field overrides

Override specific fields when you need control over certain values:

```floe
const admin = mock<User>(name: "Alice", age: 30)
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
type Todo {
  id: string,
  text: string,
  done: boolean,
}

fn toggleDone(todo: Todo) -> Todo {
  Todo(..todo, done: !todo.done)
}

test "toggle flips done status" {
  const todo = mock<Todo>(done: false)
  const toggled = toggleDone(todo)
  assert toggled.done == true
}

test "toggle preserves other fields" {
  const todo = mock<Todo>
  const toggled = toggleDone(todo)
  assert toggled.id == todo.id
  assert toggled.text == todo.text
}
```

### Complex types

`mock<T>` handles nested and complex types recursively:

```floe
type Order {
  id: string,
  items: Array<OrderItem>,
  status: OrderStatus,
}

type OrderItem {
  productId: string,
  quantity: number,
}

type OrderStatus {
  | Pending
  | Shipped { trackingId: string }
  | Delivered
}

const testOrder = mock<Order>
// {
//   id: "mock-id-1",
//   items: [{ productId: "mock-productId-2", quantity: 3 }],
//   status: { tag: "Pending" }
// }
```

---

## Why this matters

In TypeScript, types and runtime behavior are separate worlds:

| Task | TypeScript | Floe |
|------|-----------|------|
| Validate API data | Zod, io-ts, or hand-written checks | `parse<T>` |
| Generate test data | faker.js, factories, or hand-written objects | `mock<T>` |
| Keep in sync | Manual -- update schema when type changes | Automatic -- same source |
| Runtime cost | Schema library bundled in production | Zero -- compiled away |

Floe's approach eliminates an entire category of boilerplate and bugs. The type definition is the single source of truth, and the compiler does the rest.
