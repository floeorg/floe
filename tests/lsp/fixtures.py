"""Floe source fixtures for LSP tests."""

SIMPLE = """\
let x = 42
let msg = "hello"
let flag = true

let add(a: number, b: number) -> number = {
    a + b
}

export let greet(name: string) -> string = {
    `Hello, ${name}!`
}
"""

TYPES = """\
type Color = Red | Green | Blue { hex: string }

type User = { id: string, name: string, age: number }

let describeColor(c: Color) -> string = {
    match c {
        Red -> "red",
        Green -> "green",
        Blue { hex } -> `blue: ${hex}`,
    }
}
"""

PIPES = """\
let nums = [1, 2, 3, 4, 5]
let doubled = nums |> Array.map((n) -> n * 2)
let total = nums |> Array.reduce((acc, n) -> acc + n, 0)

let process(input: string) -> string = {
    input
        |> trim
        |> String.toUpperCase
}
"""

TAGGED_TEMPLATE = """\
let sql(strings: Array<string>, values: Array<string>) -> string = {
    ""
}

let id = "42"
let q = sql`select * from users where id = ${id}`
"""

ERRORS_BANNED_KEYWORDS = """\
let x = 42
var y = 10
class Foo {}
enum Bar { A, B }
"""

GOTO_DEF = """\
let add(a: number, b: number) -> number = {
    a + b
}

let result = add(1, 2)
"""

RESULT = """\
let divide(a: number, b: number) -> Result<number, string> = {
    match b {
        0 -> Err("division by zero"),
        _ -> Ok(a / b),
    }
}

let safeDivide(a: number, b: number) -> Result<string, string> = {
    let result = divide(a, b)?
    Ok(`result: ${result}`)
}
"""

FORBLOCK = """\
type Todo = {
    text: string,
    done: boolean,
}

for Array<Todo> {
    export let remaining(self) -> number = {
        self |> filter(.done == false) |> length
    }

    export let completed(self) -> number = {
        self |> filter(.done == true) |> length
    }
}
"""

CODE_ACTION = """\
export let add(a: number, b: number) = {
    a + b
}
"""

HOVER_TYPE_BODY = """\
type Product = {
    id: number,
    title: string,
    price: number,
    tags: Array<string>,
}

type Status = | Active
    | Inactive { reason: string }

type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

type UserId = number
"""

HOVER_DEFAULT_PARAMS = """\
let fetchProducts(
    category: string = "",
    limit: number = 20,
) -> string = {
    category
}

let result = fetchProducts()
"""

HOVER_MEMBER_ACCESS = """\
type User = {
    id: number,
    name: string,
    email: string,
}

let getInfo(user: User) -> string = {
    user.name
}
"""

HOVER_DESTRUCTURE = """\
let getPair() -> (string, number) = {
    ("hello", 42)
}

let (name, age) = getPair()
"""

HOVER_MATCH_BINDING = """\
type Shape = | Circle(number)
    | Rect(number, number)

let area(s: Shape) -> number = {
    match s {
        Circle(r) -> r * r,
        Rect(w, h) -> w * h,
    }
}
"""

HOVER_STDLIB_MEMBER = """\
let nums = [1, 2, 3]
let doubled = nums |> Array.map((n) -> n * 2)
let joined = "a,b,c" |> String.split(",")
"""

MATCH_EXHAUSTIVE = """\
type Direction = | North
    | South
    | East
    | West

let describe(d: Direction) -> string = {
    match d {
        North -> "up",
        South -> "down",
    }
}
"""

COMPLETION_PIPE = "let nums = [1, 2, 3]\nlet result = nums |> \n"

JSX_COMPONENT = """\
import trusted { useState } from "react"

export let Counter() -> JSX.Element = {
    let (count, setCount) = useState(0)

    let handleClick() = {
        setCount(count + 1)
    }

    <div>
        <h1>{`Count: ${count}`}</h1>
        <button onClick={handleClick}>Increment</button>
    </div>
}
"""

EMPTY_FILE = ""

SINGLE_COMMENT = "// just a comment\n"

NESTED_MATCH = """\
type Outer = A { inner: Inner } | B

type Inner = X { val: number } | Y

let describe(o: Outer) -> string = {
    match o {
        A { inner } -> match inner {
            X { val } -> `x: ${val}`,
            Y -> "y",
        },
        B -> "b",
    }
}
"""

MULTIPLE_FNS = """\
let first(x: number) -> number = { x + 1 }
let second(x: number) -> number = { x + 2 }
let third(x: number) -> number = { x + 3 }

let a = first(1)
let b = second(a)
let c = third(b)
let d = first(second(third(0)))
"""

SHADOWING = """\
let x = 5
let x = 10
"""

UNDEFINED_VAR = """\
let test() -> number = {
    y + 1
}
"""

TYPE_MISMATCH = """\
let add(a: number, b: number) -> number = {
    a + b
}
let result: string = add(1, 2)
"""

TUPLE_FILE = """\
let swap(a: number, b: number) -> (number, number) = {
    (b, a)
}

let pair = swap(1, 2)
let (x, y) = swap(3, 4)
"""

OPTION_FILE = """\
let findFirst(arr: Array<number>) -> Option<number> = {
    match arr {
        [] -> None,
        [first, ..rest] -> Some(first),
    }
}

let useOption() -> string = {
    let val = findFirst([1, 2, 3])
    match val {
        Some(n) -> `found: ${n}`,
        None -> "empty",
    }
}
"""

TRAIT_FILE = """\
trait Printable {
    let print(self) -> string
}

type Dog = {
    name: string,
    breed: string,
}

for Dog: Printable {
    let print(self) -> string = {
        `${self.name} (${self.breed})`
    }
}
"""

SPREAD_FILE = """\
type Base = {
    id: string,
    name: string,
}

type Extended = {
    ...Base,
    extra: number,
}

let makeExtended() -> Extended = {
    Extended(id: "1", name: "test", extra: 42)
}
"""

RECORD_SPREAD = """\
type User = {
    id: string,
    name: string,
    age: number,
}

let updateName(user: User, newName: string) -> User = {
    User(name: newName, ..user)
}
"""

CLOSURE_ASSIGN = """\
let add = (a: number, b: number) -> a + b
let double = (n: number) -> n * 2
let result = add(1, 2)
"""

DEEPLY_NESTED_JSX = """\
import trusted { useState } from "react"

export let App() -> JSX.Element = {
    let (items, setItems) = useState<Array<string>>([])

    <div className="container">
        <div className="header">
            <h1>Title</h1>
        </div>
        <div className="body">
            <ul>
                {items |> map((item) ->
                    <li key={item}>
                        <span>{item}</span>
                    </li>
                )}
            </ul>
        </div>
        <div className="footer">
            <p>Footer</p>
        </div>
    </div>
}
"""

STRING_LITERAL_UNION = """\
type Method = "GET" | "POST" | "PUT" | "DELETE"

let describe(m: Method) -> string = {
    match m {
        "GET" -> "get",
        "POST" -> "post",
        "PUT" -> "put",
        "DELETE" -> "delete",
    }
}
"""

STRING_LITERAL_UNION_NATIVE = """\
type Method = | Get
    | Post
    | Put
    | Delete

let describe(m: Method) -> string = {
    match m {
        Get -> "get",
        Post -> "post",
        Put -> "put",
        Delete -> "delete",
    }
}
"""

COLLECT_FILE = """\
let validateName(name: string) -> Result<string, string> = {
    match name |> String.length {
        0 -> Err("empty"),
        _ -> Ok(name),
    }
}

let validateAge(age: number) -> Result<number, string> = {
    match age {
        n when n < 0 -> Err("negative"),
        n when n > 150 -> Err("too old"),
        _ -> Ok(age),
    }
}

let validate(name: string, age: number) -> Result<(string, number), Array<string>> = {
    collect {
        let n = validateName(name)?
        let a = validateAge(age)?
        (n, a)
    }
}
"""

FN_PARAMS_HOVER = """\
let process(name: string, count: number, flag: boolean) -> string = {
    `${name}: ${count}`
}
"""

MULTILINE_PIPE = """\
let result = [1, 2, 3, 4, 5]
    |> Array.filter((n) -> n > 2)
    |> Array.map((n) -> n * 10)
    |> Array.reduce((acc, n) -> acc + n, 0)
"""

INNER_CONST = """\
let outer() -> number = {
    let inner = 10
    let doubled = inner * 2
    doubled + 1
}
"""

TODO_UNREACHABLE = """\
let incomplete() -> number = {
    todo
}

let impossible(x: number) -> string = {
    match x > 0 {
        true -> "positive",
        false -> "non-positive",
    }
}
"""

IMPORT_FOR = """\
type Msg = { text: string }

for Array<Msg> {
    export let count(self) -> number = {
        self |> length
    }
}

export let getMessage() -> Msg = {
    Msg(text: "hello")
}
"""

LARGE_UNION = """\
type Token = | Plus
    | Minus
    | Star
    | Slash
    | Equals
    | Bang
    | LeftParen
    | RightParen
    | LeftBrace
    | RightBrace
    | Comma
    | Dot
    | Semicolon
    | Eof

let describe(t: Token) -> string = {
    match t {
        Plus -> "+",
        Minus -> "-",
        Star -> "*",
        Slash -> "/",
        Equals -> "=",
        Bang -> "!",
        LeftParen -> "(",
        RightParen -> ")",
        LeftBrace -> "{",
        RightBrace -> "}",
        Comma -> ",",
        Dot -> ".",
        Semicolon -> ";",
        Eof -> "EOF",
    }
}
"""

PARTIAL_MATCH = """\
type Color = | Red | Green | Blue

let name(c: Color) -> string = {
    match c {
        Red -> "red",
    }
}
"""

MATCH_NUMBER_NO_WILDCARD = """\
let test(n: number) -> string = {
    match n {
        0 -> "zero",
        1 -> "one",
    }
}
"""

MATCH_STRING_NO_WILDCARD = """\
let test(s: string) -> string = {
    match s {
        "hello" -> "hi",
        "bye" -> "goodbye",
    }
}
"""

MATCH_NUMBER_GUARDS_NO_WILDCARD = """\
let test(n: number) -> string = {
    match n {
        n when n < 0 -> "negative",
        0 -> "zero",
        n when n < 100 -> "small",
    }
}
"""

MATCH_RANGES_NO_WILDCARD = """\
let test(n: number) -> string = {
    match n {
        0..10 -> "small",
        11..100 -> "medium",
    }
}
"""

MATCH_TUPLE_MISSING = """\
let test(pair: (boolean, boolean)) -> string = {
    match pair {
        (true, true) -> "both",
        (false, false) -> "neither",
    }
}
"""

DEFAULT_PARAMS = """\
let greet(name: string, greeting: string = "Hello") -> string = {
    `${greeting}, ${name}!`
}

let a = greet("Alice")
let b = greet("Bob", greeting: "Hi")
"""

WHEN_GUARD = """\
let classify(n: number) -> string = {
    match n {
        x when x < 0 -> "negative",
        0 -> "zero",
        x when x > 100 -> "big",
        _ -> "normal",
    }
}
"""

CLOSURE_FILE = """\
let add = (a: number, b: number) -> a + b
let double = (n: number) -> n * 2
let greet = () -> "hello"
let result = add(1, 2)
"""

GENERIC_FN = """\
let identity<T>(x: T) -> T = { x }
let pair<A, B>(a: A, b: B) -> (A, B) = { (a, b) }
let _n = identity(42)
let _p = pair(1, "hello")
"""

DOT_SHORTHAND = """\
type User = { name: string, active: boolean, age: number }

let users: Array<User> = []
let names = users |> Array.filter(.active) |> Array.map(.name)
"""

PLACEHOLDER = """\
let add(a: number, b: number) -> number = { a + b }
let addTen = add(10, _)
let result = 5 |> add(3, _)
"""

RANGE_MATCH = """\
let httpStatus(code: number) -> string = {
    match code {
        200..299 -> "success",
        300..399 -> "redirect",
        400..499 -> "client error",
        500..599 -> "server error",
        _ -> "unknown",
    }
}
"""

ARRAY_PATTERN = """\
let describe(items: Array<string>) -> string = {
    match items {
        [] -> "empty",
        [only] -> `just ${only}`,
        [first, ..rest] -> `${first} and more`,
    }
}
"""

STRING_PATTERN = """\
let route(url: string) -> string = {
    match url {
        "/users/{id}" -> `user ${id}`,
        "/posts/{id}" -> `post ${id}`,
        _ -> "not found",
    }
}
"""

PIPE_INTO_MATCH = """\
let label(temp: number) -> string = {
    temp |> match {
        0..15 -> "cold",
        16..30 -> "warm",
        _ -> "hot",
    }
}
"""

NEWTYPE_WRAPPER = """\
type UserId = UserId(string)
type OrderId = OrderId(string)

let processUser(id: UserId) -> string = {
    `user: ${id}`
}
"""

NEWTYPE = """\
type ProductId = ProductId(number)
let id = ProductId(42)
"""

OPAQUE_TYPE = """\
opaque type HashedPassword = string

let hash(pw: string) -> HashedPassword = {
    pw
}
"""

TUPLE_INDEX = """\
let pair = ("hello", 42)
let first = pair.0
let second = pair.1
"""

DERIVING = """\
trait Display {
    let display(self) -> string
}

type Point = {
    x: number,
    y: number,
} deriving (Display)
"""

TEST_BLOCK = """\
let add(a: number, b: number) -> number = { a + b }

test "addition" {
    assert add(1, 2) == 3
    assert add(-1, 1) == 0
}

test "edge cases" {
    assert add(0, 0) == 0
}
"""

UNREACHABLE = """\
let never(x: boolean) -> string = {
    match x {
        true -> "yes",
        false -> "no",
    }
}
"""

MAP_SET = """\
let config = Map.fromArray([("host", "localhost"), ("port", "8080")])
let updated = config |> Map.set("port", "3000")
let tags = Set.fromArray(["urgent", "bug"])
let withNew = tags |> Set.add("frontend")
"""

STRUCTURAL_EQ = """\
type User = { name: string, age: number }
let a = User(name: "Alice", age: 30)
let b = User(name: "Alice", age: 30)
let same = a == b
"""

INLINE_FOR = """\
for string {
    export let shout(self) -> string = {
        self |> String.toUpperCase
    }
}
"""

UNION_SHORT = """\
export type Filter = All | Active | Completed
"""

UNION_LONG = """\
export type CheckoutError =
    | EmptyCart
    | InvalidEmail { email: string, reason: string }
    | InvalidPhone { phone: string, reason: string }
    | OutOfStock { productId: number, name: string }
"""

IMPORT_FOR_BLOCK_SYNTAX = """\
type Msg = { text: string }

for Array<Msg> {
    export let count(self) -> number = {
        self |> length
    }
}
"""

NUMBER_SEPARATOR = """\
let million = 1_000_000
let pi = 3.141_592
let hex = 0xFF_FF
"""

MULTI_DEPTH_MATCH = """\
type NetworkError = Timeout { ms: number } | DnsFailure { host: string }

type ApiError = Network(NetworkError) | NotFound

let describe(e: ApiError) -> string = {
    match e {
        Network(Timeout { ms }) -> `timeout: ${ms}`,
        Network(DnsFailure { host }) -> `dns: ${host}`,
        NotFound -> "not found",
    }
}
"""

QUALIFIED_VARIANT = """\
type Color = | Red | Green | Blue { hex: string }
type Filter = | All | Active | Completed

let _a = Color.Red
let _b = Color.Blue(hex: "#00f")
let _c = Filter.All
let _d = ("text", Color.Red)
let _e = [Color.Red, Color.Blue(hex: "#fff")]

let describe(c: Color) -> string = {
    match c {
        Red -> "red",
        Green -> "green",
        Blue { hex } -> `blue: ${hex}`,
    }
}
"""

AMBIGUOUS_VARIANT = """\
type Color = | Red | Green | Blue
type Light = | Red | Yellow | Green

let _a = Color.Red
let _b = Light.Red
let _c = Blue
let _d = Yellow
"""

PIPE_MAP_INFERENCE = """\
type Accent = { id: number, name: string }
type Row = { id: number, rawName: string }

for Row {
    export let toAccent(self) -> Accent = {
        Accent(id: self.id, name: self.rawName)
    }
}

let convert(rows: Array<Row>) -> Array<Accent> = {
    let accents = rows |> map((r) -> r |> toAccent)
    accents
}
"""

# ── Record spread hover ─────────────────────────────────────

RECORD_SPREAD_HOVER = """\
type BaseProps = {
    className: string,
    disabled: boolean,
}

type ButtonProps = {
    ...BaseProps,
    onClick: () -> (),
    label: string,
}
"""

# ── Member access hover ─────────────────────────────────────

MEMBER_ACCESS = """\
type User = {
    name: string,
    age: number,
}

let user = User(name: "Ryan", age: 30)
let _name = user.name
"""

# ── Match pattern bindings ──────────────────────────────────

MATCH_PATTERN_BINDING = """\
type User = {
    name: string,
}

let _test(x: Option<User>) -> string = {
    match x {
        Some(u) -> u.name,
        None -> "unknown",
    }
}
"""

LAMBDA_PARAM = """\
let items = [1, 2, 3]
let _result = items |> map((item) -> item + 1)
"""

JSX_RENDER_PROP_PARAM = """\
type DragProvided = { draggableProps: string, innerRef: string }
type DragSnapshot = { isDragging: boolean }
type DraggableProps = {
    draggableId: string,
    index: number,
    children: (DragProvided, DragSnapshot) -> JSX.Element,
}

let Draggable(props: DraggableProps) -> JSX.Element = {
    <div />
}

type Props = { id: string }

export let Card(props: Props) -> JSX.Element = {
    <Draggable draggableId={props.id} index={0}>
        {(provided, snapshot) ->
            <div />
        }
    </Draggable>
}
"""

MATCH_PATTERN_LITERAL = """\
let _test(x: boolean) -> string = {
    match x {
        true -> "yes",
        false -> "no",
    }
}
"""

PIPE_HOVER = """\
let items = [1, 2, 3]
let _result = items |> map((x) -> x + 1) |> Array.length
"""

USE_BIND = """\
let provideValues(cb: ({ a: number, b: number }) -> number) -> number = {
    cb({ a: 1, b: 2 })
}

let withPair(cb: (number, number) -> number) -> number = {
    cb(1, 2)
}

let callback(cb: (number) -> number) -> number = {
    cb(42)
}

let sumDestructure() -> number = {
    use { a, b } <- provideValues
    a + b
}

let renameDestructure() -> number = {
    use { a: x, b: y } <- provideValues
    x + y
}

let sumPair() -> number = {
    use (first, second) <- withPair
    first + second
}

let bindOne() -> number = {
    use bound <- callback
    bound + 1
}

let zeroBind() -> string = {
    use <- Bool.guard(true, "bail")
    "ok"
}
"""

USE_AS_IDENT = """\
let use(p: number) -> number = { p + 1 }

let caller() -> number = {
    use(42)
}
"""

USE_BARE = """\
let test() -> number = {
    use
    42
}
"""
