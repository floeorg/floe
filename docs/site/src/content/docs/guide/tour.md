---
title: Language Tour
sidebar:
  order: 1
---

## Basics

```floe
// All bindings are immutable
const name = "Alice"
const age = 30
const scores: Array<number> = [95, 87, 92]

// Named functions
fn greet(name: string) -> string {
    `Hello, ${name}!`
}

// Implicit return — last expression is the return value
fn double(n: number) -> number {
    n * 2
}

// Closures use fn() — same keyword, no name
const add = fn(a, b) a + b
const log = fn() Console.log("clicked")
```

## Pipes

```floe
// Pipe value as first argument
const result = [1, 2, 3, 4, 5]
    |> filter(fn(n) n > 2)
    |> map(fn(n) n * 10)
    |> sort

// Dot shorthand — even shorter than closures
users
    |> filter(.active)
    |> map(.name)
    |> sort

// Module-qualified calls also work: Array.filter, Array.map, etc.
// Useful when the function name alone would be ambiguous.

// Placeholder _ for non-first position
5 |> add(3, _)          // add(3, 5)

// _ outside pipes creates partial application
const addTen = add(10, _)   // fn(x) add(10, x)

// Tap for side effects mid-pipeline
data
    |> transform
    |> tap(Console.log)
    |> save
```

## Pattern Matching

```floe
// Replaces if/else, switch, and ternary
const label = match status {
    200..299 -> "success",
    404 -> "not found",
    500 -> "server error",
    _ -> "unknown",
}

// Destructure union variants
match route {
    Home -> <HomePage />,
    Profile(id) -> <ProfilePage id={id} />,
    NotFound -> <NotFoundPage />,
}

// Guards
match user.age {
    _ when user.age >= 18 -> "adult",
    _ -> "minor",
}

// Multi-depth destructuring
match error {
    Network(Timeout(ms)) -> `Timed out after ${ms}ms`,
    Network(DnsFailure(host)) -> `DNS failed: ${host}`,
    NotFound -> "not found",
    _ -> "unknown error",
}

// Array patterns
match items {
    [] -> "empty",
    [only] -> `just ${only}`,
    [first, ..rest] -> `${first} and ${rest |> Array.length} more`,
}

// String patterns
match url {
    "/users/{id}" -> fetchUser(id),
    "/users/{id}/posts" -> fetchPosts(id),
    _ -> notFound(),
}

// Pipe into match
const icon = temperature |> match {
    _ when _ < 0 -> "snowflake",
    0..15 -> "cloud",
    16..30 -> "sun",
    _ -> "fire",
}
```

## Types

```floe
// Records
type User {
    id: string,
    name: string,
    email: string,
}

// Union types (discriminated, exhaustive)
type Shape {
    | Circle { radius: number }
    | Rectangle { width: number, height: number }
    | Triangle { base: number, height: number }
}

fn area(shape: Shape) -> number {
    match shape {
        Circle(r) -> Math.PI * r * r,
        Rectangle(w, h) -> w * h,
        Triangle(b, h) -> 0.5 * b * h,
    }
}

// Constructors — positional or named
const u = User("1", "Alice", "alice@test.com")
const u = User(name: "Alice", id: "1", email: "alice@test.com")

// Record spread
const updated = User(..u, name: "Bob")

// Tuples
const point: (number, number) = (10, 20)
const (x, y) = point

// Newtypes — prevent mixing IDs
type UserId { string }
type OrderId { number }
const id = OrderId(42)
const OrderId(n) = id   // destructure to get inner value

// Opaque types — only the defining module can create/read
opaque type HashedPassword = string

// String literal unions (for npm interop)
type Method = "GET" | "POST" | "PUT" | "DELETE"

// Record composition
type ButtonProps {
    ...BaseProps,
    onClick: fn() -> (),
    label: string,
}

// Tuple index access
const pair = ("hello", 42)
const first = pair.0    // "hello"
const second = pair.1   // 42

// Default parameter values
fn greet(name: string, greeting: string = "Hello") -> string {
    `${greeting}, ${name}!`
}
greet("Alice")                // "Hello, Alice!"
greet("Alice", "Hey")         // "Hey, Alice!"
```

## Error Handling

```floe
// Result<T, E> replaces exceptions
fn divide(a: number, b: number) -> Result<number, string> {
    match b {
        0 -> Err("division by zero"),
        _ -> Ok(a / b),
    }
}

// ? operator for early return
fn loadProfile(id: string) -> Result<Profile, Error> {
    const user = fetchUser(id)?
    const posts = fetchPosts(user.id)?
    Ok(Profile(user, posts))
}

// Option<T> replaces null/undefined
match user.nickname {
    Some(nick) -> nick,
    None -> user.name,
}

// collect — accumulate all errors instead of failing fast
fn validateForm(input: FormInput) -> Result<ValidForm, Array<string>> {
    collect {
        const name = validateName(input.name)?
        const email = validateEmail(input.email)?
        const age = validateAge(input.age)?
        ValidForm(name, email, age)
    }
}

// parse<T> — compiler-generated runtime validation
const user = json |> parse<User>?
const items = data |> parse<Array<Product>>?
```

## Async

```floe
// Async functions
async fn fetchUser(id: string) -> Result<User, Error> {
    const response = await Http.get(`/api/users/${id}`)?
    const user = await response |> Http.json?
    Ok(user)
}

// npm functions might throw — wrap with try
import { parseYaml } from "yaml-lib"
const config = try parseYaml(rawText)   // Result<T, Error>

// Async closures
const data = useSuspenseQuery({
    queryKey: ["users"],
    queryFn: async fn() fetchUsers(),
})
```

## Stdlib

```floe
// Map<K, V>
const config = Map.fromArray([("host", "localhost"), ("port", "8080")])
const updated = config |> Map.set("port", "3000")
const port = config |> Map.get("port")   // Option<string>

// Set<T>
const tags = Set.fromArray(["urgent", "bug"])
const withNew = tags |> Set.add("frontend")
const common = Set.intersect(teamA, teamB)

// Http — pipe-friendly fetch returning Result
const users = await Http.get("https://api.example.com/users")? |> Http.json?
const result = await Http.post(url, { name: "Alice" })?

// Structural equality — == does deep comparison
const a = User(name: "Alice", age: 30)
const b = User(name: "Alice", age: 30)
const same = a == b   // true (compares fields, not references)
```

## For Blocks

```floe
// Attach functions to types (like extension methods)
for Array<Todo> {
    export fn remaining(self) -> number {
        self |> Array.filter(.done == false) |> Array.length
    }
}

for string {
    export fn shout(self) -> string {
        self |> String.toUpperCase
    }
}

// Use in pipes — self is the piped value
todos |> remaining
"hello" |> shout          // "HELLO"
```

## Traits

```floe
trait Display {
    fn display(self) -> string
}

for User: Display {
    fn display(self) -> string {
        `${self.name} (${self.email})`
    }
}

// Auto-derive for records
type Point {
    x: number,
    y: number,
} deriving (Display)
```

## JSX

```floe
import trusted { useState } from "react"

export fn Counter() -> JSX.Element {
    const [count, setCount] = useState(0)

    <div>
        <h1>{`Count: ${count}`}</h1>
        <button onClick={fn() setCount(count + 1)}>
            Increment
        </button>
        {count |> match {
            0 -> <p>Click the button!</p>,
            _ -> <p>{`Clicked ${count} times`}</p>,
        }}
    </div>
}
```

## Imports

```floe
// Standard import
import { Todo, Filter } from "./types"

// npm imports are unsafe by default
import { parseYaml } from "yaml-lib"
const result = try parseYaml(input)    // wraps in Result

// trusted imports skip the try requirement
import trusted { useState } from "react"
import trusted { clsx } from "clsx"

// Import for-block extensions
import { for Array, for string } from "./helpers"
```

## Tests

```floe
fn add(a: number, b: number) -> number { a + b }

test "addition" {
    assert add(1, 2) == 3
    assert add(-1, 1) == 0
}
```

## Placeholders

```floe
// todo — compile-time warning, throws at runtime
fn processPayment(order: Order) -> Result<Receipt, Error> {
    todo
}

// unreachable — asserts a code path is impossible
match direction {
    "north" -> go(0, 1),
    "south" -> go(0, -1),
    "east" -> go(1, 0),
    "west" -> go(-1, 0),
    _ -> unreachable,
}
```
