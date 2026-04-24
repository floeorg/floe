---
title: Language Tour
sidebar:
  order: 1
---

## Basics

```floe
let name = "Alice"

let greet(name: string) -> string = {
    `Hello, ${name}!`
}

let identity<T>(x: T) -> T = { x }
```

## Pipes

```floe
let result = [1, 2, 3, 4, 5]
    |> filter((n) -> n > 2)
    |> map((n) -> n * 10)
    |> sort

users |> filter(.active) |> map(.name) |> sort

5 |> add(3, _)              // add(3, 5)
let addTen = add(10, _)   // partial application
```

## Pattern Matching

```floe
let label = match status {
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
type User = { id: string, name: string, email: string }

type Shape = | Circle { radius: number }
    | Rectangle { width: number, height: number }

let u = User(name: "Alice", id: "1", email: "a@t.com")
let updated = User(name: "Bob", ..u)
type UserId = UserId(string)   // newtype
```

## Error Handling

```floe
let loadProfile(id: string) -> Result<Profile, Error> = {
    let user = fetchUser(id)?       // ? returns Err early
    let posts = fetchPosts(user.id)?

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
    export let remaining(self) -> number = {
        self |> filter(.done == false) |> length
    }
}

trait Display { let display(self) -> string }
impl Display for User {
    let display(self) -> string = { `${self.name} (${self.email})` }
}
```

## JSX

```floe
export let Counter() -> JSX.Element = {
    let (count, setCount) = useState(0)

    <div>
        <h1>{`Count: ${count}`}</h1>
        <button onClick={() -> setCount(count + 1)}>+</button>
    </div>
}
```

## Imports & Async

```floe
import { Todo } from "./types"
import { parseYaml } from "yaml-lib"           // untrusted by default
let data = parseYaml(input)?              // auto-wrapped in Result, ? unwraps
import trusted { useState } from "react"    // trusted = direct call, no wrapping
import { for Array } from "./helpers"       // import for-block extensions

let fetchUser(id: string) -> Promise<Result<User, Error>> = {
    let response = Http.get(`/api/users/${id}`) |> Promise.await?
    let user = response |> Http.json |> Promise.await?

    Ok(user)
}
```

## Tests

```floe
let add(a: number, b: number) -> number = { a + b }

test "addition" {
    assert add(1, 2) == 3
    assert add(-1, 1) == 0
}
```

## Placeholders

```floe
let processPayment(order: Order) -> Result<Receipt, Error> = { todo }
match direction { "north" -> go(0, 1), _ -> unreachable }
```
