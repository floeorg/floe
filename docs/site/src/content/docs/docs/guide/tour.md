---
title: Language Tour
sidebar:
  order: 1
---

## Basics

```floe
const name = "Alice"

fn greet(name: string) => string {
    `Hello, ${name}!`
}

fn identity<T>(x: T) => T { x }
```

## Pipes

```floe
const result = [1, 2, 3, 4, 5]
    |> filter((n) => n > 2)
    |> map((n) => n * 10)
    |> sort

users |> filter(.active) |> map(.name) |> sort

5 |> add(3, _)              // add(3, 5)
const addTen = add(10, _)   // partial application
```

## Pattern Matching

```floe
const label = match status {
    200..299 -> "success",
    404 -> "not found",
    _ -> "unknown",
}

match route {
    Home -> <HomePage />,
    Profile(id) -> <ProfilePage id={id} />,
    NotFound -> <NotFoundPage />,
}
```

## Types

```floe
type User { id: string, name: string, email: string }

type Shape {
    | Circle { radius: number }
    | Rectangle { width: number, height: number }
}

const u = User(name: "Alice", id: "1", email: "a@t.com")
const updated = User(..u, name: "Bob")
type UserId { string }   // newtype
```

## Error Handling

```floe
fn loadProfile(id: string) => Result<Profile, Error> {
    const user = fetchUser(id)?       // ? returns Err early
    const posts = fetchPosts(user.id)?

    Ok(Profile(user, posts))
}

match user.nickname {
    Some(nick) -> nick,   // Option<T> replaces null
    None -> user.name,
}
```

## For Blocks & Traits

```floe
for Array<Todo> {
    export fn remaining(self) => number {
        self |> filter(.done == false) |> length
    }
}

trait Display { fn display(self) => string }
for User: Display {
    fn display(self) => string { `${self.name} (${self.email})` }
}
```

## JSX

```floe
export fn Counter() => JSX.Element {
    const (count, setCount) = useState(0)

    <div>
        <h1>{`Count: ${count}`}</h1>
        <button onClick={() => setCount(count + 1)}>+</button>
    </div>
}
```

## Imports & Async

```floe
import { Todo } from "./types"
import { parseYaml } from "yaml-lib"           // untrusted by default
const data = parseYaml(input)?              // auto-wrapped in Result, ? unwraps
import trusted { useState } from "react"    // trusted = direct call, no wrapping
import { for Array } from "./helpers"       // import for-block extensions

fn fetchUser(id: string) => Promise<Result<User, Error>> {
    const response = Http.get(`/api/users/${id}`) |> Promise.await?
    const user = response |> Http.json |> Promise.await?

    Ok(user)
}
```

## Tests

```floe
fn add(a: number, b: number) => number { a + b }

test "addition" {
    assert add(1, 2) == 3
    assert add(-1, 1) == 0
}
```

## Placeholders

```floe
fn processPayment(order: Order) => Result<Receipt, Error> { todo }
match direction { "north" -> go(0, 1), _ -> unreachable }
```
