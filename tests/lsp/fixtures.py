"""Floe source fixtures for LSP tests."""

SIMPLE = """\
const x = 42
const msg = "hello"
const flag = true

fn add(a: number, b: number) => number {
    a + b
}

export fn greet(name: string) => string {
    `Hello, ${name}!`
}
"""

TYPES = """\
type Color = Red | Green | Blue { hex: string }

type User = { id: string, name: string, age: number }

fn describeColor(c: Color) => string {
    match c {
        Red -> "red",
        Green -> "green",
        Blue { hex } -> `blue: ${hex}`,
    }
}
"""

PIPES = """\
const nums = [1, 2, 3, 4, 5]
const doubled = nums |> Array.map((n) => n * 2)
const total = nums |> Array.reduce((acc, n) => acc + n, 0)

fn process(input: string) => string {
    input
        |> trim
        |> String.toUpperCase
}
"""

TAGGED_TEMPLATE = """\
fn sql(strings: Array<string>, values: Array<string>) => string {
    ""
}

const id = "42"
const q = sql`select * from users where id = ${id}`
"""

ERRORS_BANNED_KEYWORDS = """\
let x = 42
var y = 10
class Foo {}
enum Bar { A, B }
"""

GOTO_DEF = """\
fn add(a: number, b: number) => number {
    a + b
}

const result = add(1, 2)
"""

RESULT = """\
fn divide(a: number, b: number) => Result<number, string> {
    match b {
        0 -> Err("division by zero"),
        _ -> Ok(a / b),
    }
}

fn safeDivide(a: number, b: number) => Result<string, string> {
    const result = divide(a, b)?
    Ok(`result: ${result}`)
}
"""

FORBLOCK = """\
type Todo = {
    text: string,
    done: boolean,
}

for Array<Todo> {
    export fn remaining(self) => number {
        self |> filter(.done == false) |> length
    }

    export fn completed(self) => number {
        self |> filter(.done == true) |> length
    }
}
"""

CODE_ACTION = """\
export fn add(a: number, b: number) {
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
fn fetchProducts(
    category: string = "",
    limit: number = 20,
) => string {
    category
}

const result = fetchProducts()
"""

HOVER_MEMBER_ACCESS = """\
type User = {
    id: number,
    name: string,
    email: string,
}

fn getInfo(user: User) => string {
    user.name
}
"""

HOVER_DESTRUCTURE = """\
fn getPair() => (string, number) {
    ("hello", 42)
}

const (name, age) = getPair()
"""

HOVER_MATCH_BINDING = """\
type Shape = | Circle(number)
    | Rect(number, number)

fn area(s: Shape) => number {
    match s {
        Circle(r) -> r * r,
        Rect(w, h) -> w * h,
    }
}
"""

HOVER_STDLIB_MEMBER = """\
const nums = [1, 2, 3]
const doubled = nums |> Array.map((n) => n * 2)
const joined = "a,b,c" |> String.split(",")
"""

MATCH_EXHAUSTIVE = """\
type Direction = | North
    | South
    | East
    | West

fn describe(d: Direction) => string {
    match d {
        North -> "up",
        South -> "down",
    }
}
"""

COMPLETION_PIPE = "const nums = [1, 2, 3]\nconst result = nums |> \n"

JSX_COMPONENT = """\
import trusted { useState } from "react"

export fn Counter() => JSX.Element {
    const (count, setCount) = useState(0)

    fn handleClick() {
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

fn describe(o: Outer) => string {
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
fn first(x: number) => number { x + 1 }
fn second(x: number) => number { x + 2 }
fn third(x: number) => number { x + 3 }

const a = first(1)
const b = second(a)
const c = third(b)
const d = first(second(third(0)))
"""

SHADOWING = """\
const x = 5
const x = 10
"""

UNDEFINED_VAR = """\
fn test() => number {
    y + 1
}
"""

TYPE_MISMATCH = """\
fn add(a: number, b: number) => number {
    a + b
}
const result: string = add(1, 2)
"""

TUPLE_FILE = """\
fn swap(a: number, b: number) => (number, number) {
    (b, a)
}

const pair = swap(1, 2)
const (x, y) = swap(3, 4)
"""

OPTION_FILE = """\
fn findFirst(arr: Array<number>) => Option<number> {
    match arr {
        [] -> None,
        [first, ..rest] -> Some(first),
    }
}

fn useOption() => string {
    const val = findFirst([1, 2, 3])
    match val {
        Some(n) -> `found: ${n}`,
        None -> "empty",
    }
}
"""

TRAIT_FILE = """\
trait Printable {
    fn print(self) => string
}

type Dog = {
    name: string,
    breed: string,
}

for Dog: Printable {
    fn print(self) => string {
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

fn makeExtended() => Extended {
    Extended(id: "1", name: "test", extra: 42)
}
"""

RECORD_SPREAD = """\
type User = {
    id: string,
    name: string,
    age: number,
}

fn updateName(user: User, newName: string) => User {
    User(..user, name: newName)
}
"""

CLOSURE_ASSIGN = """\
const add = (a: number, b: number) => a + b
const double = (n: number) => n * 2
const result = add(1, 2)
"""

DEEPLY_NESTED_JSX = """\
import trusted { useState } from "react"

export fn App() => JSX.Element {
    const (items, setItems) = useState<Array<string>>([])

    <div className="container">
        <div className="header">
            <h1>Title</h1>
        </div>
        <div className="body">
            <ul>
                {items |> map((item) =>
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

fn describe(m: Method) => string {
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

fn describe(m: Method) => string {
    match m {
        Get -> "get",
        Post -> "post",
        Put -> "put",
        Delete -> "delete",
    }
}
"""

COLLECT_FILE = """\
fn validateName(name: string) => Result<string, string> {
    match name |> String.length {
        0 -> Err("empty"),
        _ -> Ok(name),
    }
}

fn validateAge(age: number) => Result<number, string> {
    match age {
        n when n < 0 -> Err("negative"),
        n when n > 150 -> Err("too old"),
        _ -> Ok(age),
    }
}

fn validate(name: string, age: number) => Result<(string, number), Array<string>> {
    collect {
        const n = validateName(name)?
        const a = validateAge(age)?
        (n, a)
    }
}
"""

FN_PARAMS_HOVER = """\
fn process(name: string, count: number, flag: boolean) => string {
    `${name}: ${count}`
}
"""

MULTILINE_PIPE = """\
const result = [1, 2, 3, 4, 5]
    |> Array.filter((n) => n > 2)
    |> Array.map((n) => n * 10)
    |> Array.reduce((acc, n) => acc + n, 0)
"""

INNER_CONST = """\
fn outer() => number {
    const inner = 10
    const doubled = inner * 2
    doubled + 1
}
"""

TODO_UNREACHABLE = """\
fn incomplete() => number {
    todo
}

fn impossible(x: number) => string {
    match x > 0 {
        true -> "positive",
        false -> "non-positive",
    }
}
"""

IMPORT_FOR = """\
type Msg = { text: string }

for Array<Msg> {
    export fn count(self) => number {
        self |> length
    }
}

export fn getMessage() => Msg {
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

fn describe(t: Token) => string {
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

fn name(c: Color) => string {
    match c {
        Red -> "red",
    }
}
"""

MATCH_NUMBER_NO_WILDCARD = """\
fn test(n: number) => string {
    match n {
        0 -> "zero",
        1 -> "one",
    }
}
"""

MATCH_STRING_NO_WILDCARD = """\
fn test(s: string) => string {
    match s {
        "hello" -> "hi",
        "bye" -> "goodbye",
    }
}
"""

MATCH_NUMBER_GUARDS_NO_WILDCARD = """\
fn test(n: number) => string {
    match n {
        n when n < 0 -> "negative",
        0 -> "zero",
        n when n < 100 -> "small",
    }
}
"""

MATCH_RANGES_NO_WILDCARD = """\
fn test(n: number) => string {
    match n {
        0..10 -> "small",
        11..100 -> "medium",
    }
}
"""

MATCH_TUPLE_MISSING = """\
fn test(pair: (boolean, boolean)) => string {
    match pair {
        (true, true) -> "both",
        (false, false) -> "neither",
    }
}
"""

DEFAULT_PARAMS = """\
fn greet(name: string, greeting: string = "Hello") => string {
    `${greeting}, ${name}!`
}

const a = greet("Alice")
const b = greet("Bob", greeting: "Hi")
"""

WHEN_GUARD = """\
fn classify(n: number) => string {
    match n {
        x when x < 0 -> "negative",
        0 -> "zero",
        x when x > 100 -> "big",
        _ -> "normal",
    }
}
"""

CLOSURE_FILE = """\
const add = (a: number, b: number) => a + b
const double = (n: number) => n * 2
const greet = () => "hello"
const result = add(1, 2)
"""

GENERIC_FN = """\
fn identity<T>(x: T) => T { x }
fn pair<A, B>(a: A, b: B) => (A, B) { (a, b) }
const _n = identity(42)
const _p = pair(1, "hello")
"""

DOT_SHORTHAND = """\
type User = { name: string, active: boolean, age: number }

const users: Array<User> = []
const names = users |> Array.filter(.active) |> Array.map(.name)
"""

PLACEHOLDER = """\
fn add(a: number, b: number) => number { a + b }
const addTen = add(10, _)
const result = 5 |> add(3, _)
"""

RANGE_MATCH = """\
fn httpStatus(code: number) => string {
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
fn describe(items: Array<string>) => string {
    match items {
        [] -> "empty",
        [only] -> `just ${only}`,
        [first, ..rest] -> `${first} and more`,
    }
}
"""

STRING_PATTERN = """\
fn route(url: string) => string {
    match url {
        "/users/{id}" -> `user ${id}`,
        "/posts/{id}" -> `post ${id}`,
        _ -> "not found",
    }
}
"""

PIPE_INTO_MATCH = """\
fn label(temp: number) => string {
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

fn processUser(id: UserId) => string {
    `user: ${id}`
}
"""

NEWTYPE = """\
type ProductId = ProductId(number)
const id = ProductId(42)
"""

OPAQUE_TYPE = """\
opaque type HashedPassword = string

fn hash(pw: string) => HashedPassword {
    pw
}
"""

TUPLE_INDEX = """\
const pair = ("hello", 42)
const first = pair.0
const second = pair.1
"""

DERIVING = """\
trait Display {
    fn display(self) => string
}

type Point = {
    x: number,
    y: number,
} deriving (Display)
"""

TEST_BLOCK = """\
fn add(a: number, b: number) => number { a + b }

test "addition" {
    assert add(1, 2) == 3
    assert add(-1, 1) == 0
}

test "edge cases" {
    assert add(0, 0) == 0
}
"""

UNREACHABLE = """\
fn never(x: boolean) => string {
    match x {
        true -> "yes",
        false -> "no",
    }
}
"""

MAP_SET = """\
const config = Map.fromArray([("host", "localhost"), ("port", "8080")])
const updated = config |> Map.set("port", "3000")
const tags = Set.fromArray(["urgent", "bug"])
const withNew = tags |> Set.add("frontend")
"""

STRUCTURAL_EQ = """\
type User = { name: string, age: number }
const a = User(name: "Alice", age: 30)
const b = User(name: "Alice", age: 30)
const same = a == b
"""

INLINE_FOR = """\
for string {
    export fn shout(self) => string {
        self |> String.toUpperCase
    }
}
"""

IMPORT_FOR_BLOCK_SYNTAX = """\
type Msg = { text: string }

for Array<Msg> {
    export fn count(self) => number {
        self |> length
    }
}
"""

NUMBER_SEPARATOR = """\
const million = 1_000_000
const pi = 3.141_592
const hex = 0xFF_FF
"""

MULTI_DEPTH_MATCH = """\
type NetworkError = Timeout { ms: number } | DnsFailure { host: string }

type ApiError = Network(NetworkError) | NotFound

fn describe(e: ApiError) => string {
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

const _a = Color.Red
const _b = Color.Blue(hex: "#00f")
const _c = Filter.All
const _d = ("text", Color.Red)
const _e = [Color.Red, Color.Blue(hex: "#fff")]

fn describe(c: Color) => string {
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

const _a = Color.Red
const _b = Light.Red
const _c = Blue
const _d = Yellow
"""

PIPE_MAP_INFERENCE = """\
type Accent = { id: number, name: string }
type Row = { id: number, rawName: string }

for Row {
    export fn toAccent(self) => Accent {
        Accent(id: self.id, name: self.rawName)
    }
}

fn convert(rows: Array<Row>) => Array<Accent> {
    const accents = rows |> map((r) => r |> toAccent)
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
    onClick: () => (),
    label: string,
}
"""

# ── Member access hover ─────────────────────────────────────

MEMBER_ACCESS = """\
type User = {
    name: string,
    age: number,
}

const user = User(name: "Ryan", age: 30)
const _name = user.name
"""

# ── Match pattern bindings ──────────────────────────────────

MATCH_PATTERN_BINDING = """\
type User = {
    name: string,
}

fn _test(x: Option<User>) => string {
    match x {
        Some(u) -> u.name,
        None -> "unknown",
    }
}
"""

LAMBDA_PARAM = """\
const items = [1, 2, 3]
const _result = items |> map((item) => item + 1)
"""

JSX_RENDER_PROP_PARAM = """\
import trusted { Draggable } from "@hello-pangea/dnd"

type Props = { id: string }

export fn Card(props: Props) => JSX.Element {
    <Draggable draggableId={props.id} index={0}>
        {(provided, snapshot) =>
            <div />
        }
    </Draggable>
}
"""

MATCH_PATTERN_LITERAL = """\
fn _test(x: boolean) => string {
    match x {
        true -> "yes",
        false -> "no",
    }
}
"""

PIPE_HOVER = """\
const items = [1, 2, 3]
const _result = items |> map((x) => x + 1) |> Array.length
"""
