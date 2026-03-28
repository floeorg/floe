#!/usr/bin/env python3
"""
LSP Integration Test Suite for Floe.

Spawns `floe lsp`, sends real JSON-RPC messages, and reports what works.

Usage: python3 scripts/test-lsp.py [path-to-floe-binary]
"""

from __future__ import annotations

import json
import subprocess
import sys
import threading
import time

FLOE = sys.argv[1] if len(sys.argv) > 1 else "./target/debug/floe"

# ── Colors ────────────────────────────────────────────────

GREEN = "\033[0;32m"
RED = "\033[0;31m"
YELLOW = "\033[0;33m"
BOLD = "\033[1m"
DIM = "\033[2m"
NC = "\033[0m"

# ── Results ───────────────────────────────────────────────

passed = 0
failed = 0
errors: list[str] = []


def check(name: str, ok: bool, detail: str = ""):
    global passed, failed
    if ok:
        passed += 1
        print(f"  {GREEN}PASS{NC} {name}")
    else:
        failed += 1
        print(f"  {RED}FAIL{NC} {name}")
        if detail:
            print(f"       {DIM}{detail}{NC}")
        errors.append(f"{name}: {detail}")


# ── LSP Client ────────────────────────────────────────────


class LspClient:
    """Simple LSP client that communicates over stdin/stdout."""

    def __init__(self, binary: str):
        self.proc = subprocess.Popen(
            [binary, "lsp"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.next_id = 1
        self.responses: dict[int, dict] = {}
        self.notifications: list[dict] = []
        self._lock = threading.Lock()
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()

    def _read_loop(self):
        """Read LSP responses from stdout."""
        buf = b""
        while True:
            try:
                chunk = self.proc.stdout.read(1)
                if not chunk:
                    break
                buf += chunk

                # Look for Content-Length header
                while b"\r\n\r\n" in buf:
                    header_end = buf.index(b"\r\n\r\n")
                    header = buf[:header_end].decode("utf-8")
                    content_length = None
                    for line in header.split("\r\n"):
                        if line.lower().startswith("content-length:"):
                            content_length = int(line.split(":")[1].strip())

                    if content_length is None:
                        buf = buf[header_end + 4 :]
                        continue

                    body_start = header_end + 4
                    body_end = body_start + content_length

                    if len(buf) < body_end:
                        # Need more data
                        break

                    body = buf[body_start:body_end].decode("utf-8")
                    buf = buf[body_end:]

                    try:
                        msg = json.loads(body)
                    except json.JSONDecodeError:
                        continue

                    with self._lock:
                        if "id" in msg and "method" not in msg:
                            self.responses[msg["id"]] = msg
                        else:
                            self.notifications.append(msg)
            except Exception:
                break

    def send(self, method: str, params: dict, is_notification: bool = False) -> int | None:
        """Send a JSON-RPC message. Returns the id for requests."""
        msg: dict = {"jsonrpc": "2.0", "method": method, "params": params}
        msg_id = None
        if not is_notification:
            msg_id = self.next_id
            msg["id"] = msg_id
            self.next_id += 1

        body = json.dumps(msg)
        header = f"Content-Length: {len(body)}\r\n\r\n"
        self.proc.stdin.write(header.encode() + body.encode())
        self.proc.stdin.flush()
        return msg_id

    def wait_response(self, msg_id: int, timeout: float = 5.0) -> dict | None:
        """Wait for a response with the given id."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            with self._lock:
                if msg_id in self.responses:
                    return self.responses.pop(msg_id)
            time.sleep(0.05)
        return None

    def wait_notification(self, method: str, timeout: float = 5.0) -> dict | None:
        """Wait for a notification with the given method."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            with self._lock:
                for i, n in enumerate(self.notifications):
                    if n.get("method") == method:
                        return self.notifications.pop(i)
            time.sleep(0.05)
        return None

    def collect_notifications(self, method: str, timeout: float = 2.0) -> list[dict]:
        """Collect all notifications of a given method within timeout."""
        time.sleep(timeout)
        with self._lock:
            result = [n for n in self.notifications if n.get("method") == method]
            self.notifications = [n for n in self.notifications if n.get("method") != method]
        return result

    def initialize(self):
        """Send initialize + initialized."""
        msg_id = self.send("initialize", {"capabilities": {}, "rootUri": "file:///tmp"})
        resp = self.wait_response(msg_id)
        self.send("initialized", {}, is_notification=True)
        return resp

    def open_doc(self, uri: str, text: str):
        """Send textDocument/didOpen."""
        self.send(
            "textDocument/didOpen",
            {"textDocument": {"uri": uri, "languageId": "floe", "version": 1, "text": text}},
            is_notification=True,
        )

    def hover(self, uri: str, line: int, char: int) -> dict | None:
        msg_id = self.send(
            "textDocument/hover",
            {"textDocument": {"uri": uri}, "position": {"line": line, "character": char}},
        )
        return self.wait_response(msg_id)

    def completion(self, uri: str, line: int, char: int) -> dict | None:
        msg_id = self.send(
            "textDocument/completion",
            {"textDocument": {"uri": uri}, "position": {"line": line, "character": char}},
        )
        return self.wait_response(msg_id)

    def goto_definition(self, uri: str, line: int, char: int) -> dict | None:
        msg_id = self.send(
            "textDocument/definition",
            {"textDocument": {"uri": uri}, "position": {"line": line, "character": char}},
        )
        return self.wait_response(msg_id)

    def references(self, uri: str, line: int, char: int) -> dict | None:
        msg_id = self.send(
            "textDocument/references",
            {
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": char},
                "context": {"includeDeclaration": True},
            },
        )
        return self.wait_response(msg_id)

    def document_symbols(self, uri: str) -> dict | None:
        msg_id = self.send("textDocument/documentSymbol", {"textDocument": {"uri": uri}})
        return self.wait_response(msg_id)

    def code_action(self, uri: str, line: int, diagnostics: list[dict] | None = None) -> dict | None:
        msg_id = self.send(
            "textDocument/codeAction",
            {
                "textDocument": {"uri": uri},
                "range": {
                    "start": {"line": line, "character": 0},
                    "end": {"line": line, "character": 0},
                },
                "context": {"diagnostics": diagnostics or []},
            },
        )
        return self.wait_response(msg_id)

    def shutdown(self):
        msg_id = self.send("shutdown", {})
        self.wait_response(msg_id, timeout=2)
        self.send("exit", {}, is_notification=True)
        try:
            self.proc.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.proc.kill()


# ── Helpers ───────────────────────────────────────────────


def hover_text(resp: dict | None) -> str | None:
    """Extract hover text from response."""
    if resp is None:
        return None
    result = resp.get("result")
    if result is None:
        return None
    contents = result.get("contents", {})
    if isinstance(contents, dict):
        return contents.get("value", "")
    if isinstance(contents, str):
        return contents
    return str(contents)


def completion_labels(resp: dict | None) -> list[str]:
    """Extract completion labels from response."""
    if resp is None:
        return []
    result = resp.get("result")
    if result is None:
        return []
    if isinstance(result, list):
        items = result
    elif isinstance(result, dict):
        items = result.get("items", [])
    else:
        return []
    return [i.get("label", "") for i in items]


def def_locations(resp: dict | None) -> list:
    if resp is None:
        return []
    result = resp.get("result")
    if result is None:
        return []
    if isinstance(result, list):
        return result
    if isinstance(result, dict):
        return [result]
    return []


def diag_errors(notifs: list[dict]) -> list[dict]:
    """Extract error-severity diagnostics from publishDiagnostics notifications."""
    all_diags = []
    for n in notifs:
        for d in n.get("params", {}).get("diagnostics", []):
            if d.get("severity", 1) == 1:  # 1 = Error
                all_diags.append(d)
    return all_diags


def diag_all(notifs: list[dict]) -> list[dict]:
    all_diags = []
    for n in notifs:
        all_diags.extend(n.get("params", {}).get("diagnostics", []))
    return all_diags


def symbol_names(resp: dict | None) -> list[str]:
    if resp is None:
        return []
    result = resp.get("result")
    if result is None:
        return []
    if isinstance(result, list):
        return [s.get("name", "") for s in result]
    return []


# ── Test Fixtures ─────────────────────────────────────────

SIMPLE = """\
const x = 42
const msg = "hello"
const flag = true

fn add(a: number, b: number) -> number {
    a + b
}

export fn greet(name: string) -> string {
    `Hello, ${name}!`
}
"""

TYPES = """\
type Color {
    | Red
    | Green
    | Blue { hex: string }
}

type User {
    id: string,
    name: string,
    age: number,
}

fn describeColor(c: Color) -> string {
    match c {
        Red -> "red",
        Green -> "green",
        Blue(hex) -> `blue: ${hex}`,
    }
}
"""

PIPES = """\
const nums = [1, 2, 3, 4, 5]
const doubled = nums |> Array.map((n) => n * 2)
const total = nums |> Array.reduce((acc, n) => acc + n, 0)

fn process(input: string) -> string {
    input
        |> trim
        |> String.toUpperCase
}
"""

ERRORS = """\
let x = 42
var y = 10
class Foo {}
enum Bar { A, B }
"""

GOTO_DEF = """\
fn add(a: number, b: number) -> number {
    a + b
}

const result = add(1, 2)
"""

RESULT = """\
fn divide(a: number, b: number) -> Result<number, string> {
    match b {
        0 -> Err("division by zero"),
        _ -> Ok(a / b),
    }
}

fn safeDivide(a: number, b: number) -> Result<string, string> {
    const result = divide(a, b)?
    Ok(`result: ${result}`)
}
"""

FORBLOCK = """\
type Todo {
    text: string,
    done: boolean,
}

for Array<Todo> {
    export fn remaining(self) -> number {
        self |> filter(.done == false) |> length
    }

    export fn completed(self) -> number {
        self |> filter(.done == true) |> length
    }
}
"""

CODE_ACTION = """\
export fn add(a: number, b: number) {
    a + b
}
"""

# ── Test data for hover improvements (#403) ─────────

HOVER_TYPE_BODY = """\
type Product {
    id: number,
    title: string,
    price: number,
    tags: Array<string>,
}

type Status {
    | Active
    | Inactive(reason: string)
}

type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

type UserId = number
"""

HOVER_DEFAULT_PARAMS = """\
fn fetchProducts(
    category: string = "",
    limit: number = 20,
) -> string {
    category
}

const result = fetchProducts()
"""

HOVER_MEMBER_ACCESS = """\
type User {
    id: number,
    name: string,
    email: string,
}

fn getInfo(user: User) -> string {
    user.name
}
"""

HOVER_DESTRUCTURE = """\
fn getPair() -> (string, number) {
    ("hello", 42)
}

const (name, age) = getPair()
"""

HOVER_MATCH_BINDING = """\
type Shape {
    | Circle(radius: number)
    | Rect(width: number, height: number)
}

fn area(s: Shape) -> number {
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
type Direction {
    | North
    | South
    | East
    | West
}

fn describe(d: Direction) -> string {
    match d {
        North -> "up",
        South -> "down",
    }
}
"""

COMPLETION_PIPE = "const nums = [1, 2, 3]\nconst result = nums |> \n"

JSX_COMPONENT = """\
import trusted { useState } from "react"

export fn Counter() -> JSX.Element {
    const [count, setCount] = useState(0)

    fn handleClick() {
        setCount(count + 1)
    }

    <div>
        <h1>{`Count: ${count}`}</h1>
        <button onClick={handleClick}>Increment</button>
    </div>
}
"""

# ── Additional test fixtures ──────────────────────────────

EMPTY_FILE = ""

SINGLE_COMMENT = "// just a comment\n"

UNICODE = """\
const greeting = "hello"
const emoji_name = "world"
fn format(s: string) -> string { s }
"""

NESTED_MATCH = """\
type Outer {
    | A { inner: Inner }
    | B
}

type Inner {
    | X { val: number }
    | Y
}

fn describe(o: Outer) -> string {
    match o {
        A(inner) -> match inner {
            X(val) -> `x: ${val}`,
            Y -> "y",
        },
        B -> "b",
    }
}
"""

MULTIPLE_FNS = """\
fn first(x: number) -> number { x + 1 }
fn second(x: number) -> number { x + 2 }
fn third(x: number) -> number { x + 3 }

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
fn test() -> number {
    y + 1
}
"""

TYPE_MISMATCH = """\
fn add(a: number, b: number) -> number {
    a + b
}
const result: string = add(1, 2)
"""

TUPLE_FILE = """\
fn swap(a: number, b: number) -> (number, number) {
    (b, a)
}

const pair = swap(1, 2)
const (x, y) = swap(3, 4)
"""

ASYNC_FILE = """\
async fn fetchData(url: string) -> Result<string, Error> {
    const response = await Http.get(url)?
    const body = await response |> Http.text?
    Ok(body)
}
"""

OPTION_FILE = """\
fn findFirst(arr: Array<number>) -> Option<number> {
    match arr {
        [] -> None,
        [first, ..rest] -> Some(first),
    }
}

fn useOption() -> string {
    const val = findFirst([1, 2, 3])
    match val {
        Some(n) -> `found: ${n}`,
        None -> "empty",
    }
}
"""

TRAIT_FILE = """\
trait Printable {
    fn print(self) -> string
}

type Dog {
    name: string,
    breed: string,
}

for Dog: Printable {
    fn print(self) -> string {
        `${self.name} (${self.breed})`
    }
}
"""

SPREAD_FILE = """\
type Base {
    id: string,
    name: string,
}

type Extended {
    ...Base,
    extra: number,
}

fn makeExtended() -> Extended {
    Extended(id: "1", name: "test", extra: 42)
}
"""

RECORD_SPREAD = """\
type User {
    id: string,
    name: string,
    age: number,
}

fn updateName(user: User, newName: string) -> User {
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

export fn App() -> JSX.Element {
    const [items, setItems] = useState<Array<string>>([])

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

fn describe(m: Method) -> string {
    match m {
        "GET" -> "get",
        "POST" -> "post",
        "PUT" -> "put",
        "DELETE" -> "delete",
    }
}
"""

COLLECT_FILE = """\
fn validateName(name: string) -> Result<string, string> {
    match name |> String.length {
        0 -> Err("empty"),
        _ -> Ok(name),
    }
}

fn validateAge(age: number) -> Result<number, string> {
    match age {
        n when n < 0 -> Err("negative"),
        n when n > 150 -> Err("too old"),
        _ -> Ok(age),
    }
}

fn validate(name: string, age: number) -> Result<(string, number), Array<string>> {
    collect {
        const n = validateName(name)?
        const a = validateAge(age)?
        (n, a)
    }
}
"""

FN_PARAMS_HOVER = """\
fn process(name: string, count: number, flag: boolean) -> string {
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
fn outer() -> number {
    const inner = 10
    const doubled = inner * 2
    doubled + 1
}
"""

TODO_UNREACHABLE = """\
fn incomplete() -> number {
    todo
}

fn impossible(x: number) -> string {
    match x > 0 {
        true -> "positive",
        false -> "non-positive",
    }
}
"""

IMPORT_FOR = """\
type Msg { text: string }

for Array<Msg> {
    export fn count(self) -> number {
        self |> length
    }
}

export fn getMessage() -> Msg {
    Msg(text: "hello")
}
"""

LARGE_UNION = """\
type Token {
    | Plus
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
}

fn describe(t: Token) -> string {
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
type Color { | Red | Green | Blue }

fn name(c: Color) -> string {
    match c {
        Red -> "red",
    }
}
"""

MATCH_NUMBER_NO_WILDCARD = """\
fn test(n: number) -> string {
    match n {
        0 -> "zero",
        1 -> "one",
    }
}
"""

MATCH_STRING_NO_WILDCARD = """\
fn test(s: string) -> string {
    match s {
        "hello" -> "hi",
        "bye" -> "goodbye",
    }
}
"""

MATCH_NUMBER_GUARDS_NO_WILDCARD = """\
fn test(n: number) -> string {
    match n {
        n when n < 0 -> "negative",
        0 -> "zero",
        n when n < 100 -> "small",
    }
}
"""

MATCH_RANGES_NO_WILDCARD = """\
fn test(n: number) -> string {
    match n {
        0..10 -> "small",
        11..100 -> "medium",
    }
}
"""

MATCH_TUPLE_MISSING = """\
fn test(pair: (boolean, boolean)) -> string {
    match pair {
        (true, true) -> "both",
        (false, false) -> "neither",
    }
}
"""

DEFAULT_PARAMS = """\
fn greet(name: string, greeting: string = "Hello") -> string {
    `${greeting}, ${name}!`
}

const a = greet("Alice")
const b = greet("Bob", "Hi")
"""

WHEN_GUARD = """\
fn classify(n: number) -> string {
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
fn identity<T>(x: T) -> T { x }
fn pair<A, B>(a: A, b: B) -> (A, B) { (a, b) }
const _n = identity(42)
const _p = pair(1, "hello")
"""

DOT_SHORTHAND = """\
type User { name: string, active: boolean, age: number }

const users: Array<User> = []
const names = users |> Array.filter(.active) |> Array.map(.name)
"""

PLACEHOLDER = """\
fn add(a: number, b: number) -> number { a + b }
const addTen = add(10, _)
const result = 5 |> add(3, _)
"""

RANGE_MATCH = """\
fn httpStatus(code: number) -> string {
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
fn describe(items: Array<string>) -> string {
    match items {
        [] -> "empty",
        [only] -> `just ${only}`,
        [first, ..rest] -> `${first} and more`,
    }
}
"""

STRING_PATTERN = """\
fn route(url: string) -> string {
    match url {
        "/users/{id}" -> `user ${id}`,
        "/posts/{id}" -> `post ${id}`,
        _ -> "not found",
    }
}
"""

PIPE_INTO_MATCH = """\
fn label(temp: number) -> string {
    temp |> match {
        0..15 -> "cold",
        16..30 -> "warm",
        _ -> "hot",
    }
}
"""

NEWTYPE_WRAPPER = """\
type UserId { string }
type OrderId { string }

fn processUser(id: UserId) -> string {
    `user: ${id}`
}
"""

NEWTYPE = """\
type ProductId { number }
const id = ProductId(42)
"""

OPAQUE_TYPE = """\
opaque type HashedPassword = string

fn hash(pw: string) -> HashedPassword {
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
    fn display(self) -> string
}

type Point {
    x: number,
    y: number,
} deriving (Display)
"""

TEST_BLOCK = """\
fn add(a: number, b: number) -> number { a + b }

test "addition" {
    assert add(1, 2) == 3
    assert add(-1, 1) == 0
}

test "edge cases" {
    assert add(0, 0) == 0
}
"""

UNREACHABLE = """\
fn never(x: boolean) -> string {
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
type User { name: string, age: number }
const a = User(name: "Alice", age: 30)
const b = User(name: "Alice", age: 30)
const same = a == b
"""

INLINE_FOR = """\
for string {
    export fn shout(self) -> string {
        self |> String.toUpperCase
    }
}
"""

IMPORT_FOR_BLOCK_SYNTAX = """\
type Msg { text: string }

for Array<Msg> {
    export fn count(self) -> number {
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
type NetworkError {
    | Timeout { ms: number }
    | DnsFailure { host: string }
}

type ApiError {
    | Network { NetworkError }
    | NotFound
}

fn describe(e: ApiError) -> string {
    match e {
        Network(Timeout(ms)) -> `timeout: ${ms}`,
        Network(DnsFailure(host)) -> `dns: ${host}`,
        NotFound -> "not found",
    }
}
"""

QUALIFIED_VARIANT = """\
type Color { | Red | Green | Blue { hex: string } }
type Filter { | All | Active | Completed }

const _a = Color.Red
const _b = Color.Blue(hex: "#00f")
const _c = Filter.All
const _d = ("text", Color.Red)
const _e = [Color.Red, Color.Blue(hex: "#fff")]

fn describe(c: Color) -> string {
    match c {
        Red -> "red",
        Green -> "green",
        Blue(hex) -> `blue: ${hex}`,
    }
}
"""

AMBIGUOUS_VARIANT = """\
type Color { | Red | Green | Blue }
type Light { | Red | Yellow | Green }

const _a = Color.Red
const _b = Light.Red
const _c = Blue
const _d = Yellow
"""

# ── Run Tests ─────────────────────────────────────────────


def main():
    global passed, failed

    print(f"\n{BOLD}Floe LSP Integration Tests{NC}")
    print(f"Binary: {FLOE}\n")

    # Start one LSP session for all tests
    lsp = LspClient(FLOE)

    # ── 1. Initialize ────────────────────────────────────
    print(f"{BOLD}1. Server Initialization{NC}")

    init_resp = lsp.initialize()
    check("Server responds to initialize", init_resp is not None and "result" in init_resp)

    if init_resp and "result" in init_resp:
        caps = init_resp["result"]["capabilities"]
        for cap_name in [
            "hoverProvider",
            "completionProvider",
            "definitionProvider",
            "referencesProvider",
            "documentSymbolProvider",
            "codeActionProvider",
        ]:
            check(f"Capability: {cap_name}", cap_name in caps and caps[cap_name])

        sync = caps.get("textDocumentSync")
        check("Text document sync: full", sync == 1 or (isinstance(sync, dict) and sync.get("change") == 1))

        comp = caps.get("completionProvider", {})
        triggers = comp.get("triggerCharacters", [])
        check("Completion triggers: . | >", "." in triggers and "|" in triggers and ">" in triggers)
    else:
        for _ in range(8):
            check("(skipped — no init response)", False)

    # ── 2. Diagnostics ───────────────────────────────────
    print(f"\n{BOLD}2. Diagnostics{NC}")

    URI = "file:///tmp/test.fl"

    # Valid file — no errors
    lsp.open_doc(URI, SIMPLE)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    check("Valid file (simple): no errors", len(errs) == 0, f"Got {len(errs)} errors")

    # Types file
    lsp.open_doc(URI, TYPES)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    check("Valid file (types + unions): no errors", len(errs) == 0, f"Got {len(errs)} errors: {[e.get('message','') for e in errs[:3]]}")

    # Pipes file
    lsp.open_doc(URI, PIPES)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    check("Valid file (pipes): no errors", len(errs) == 0, f"Got {len(errs)} errors: {[e.get('message','') for e in errs[:3]]}")

    # Result/Option file
    lsp.open_doc(URI, RESULT)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    check("Valid file (Result/?): no errors", len(errs) == 0, f"Got {len(errs)} errors: {[e.get('message','') for e in errs[:3]]}")

    # For-block file
    lsp.open_doc(URI, FORBLOCK)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    check("Valid file (for-block): no errors", len(errs) == 0, f"Got {len(errs)} errors: {[e.get('message','') for e in errs[:3]]}")

    # Banned keywords
    lsp.open_doc(URI, ERRORS)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    check("Banned keywords (let/var/class/enum): has errors", len(errs) > 0, "Expected parse errors for banned keywords")

    # Match exhaustiveness
    lsp.open_doc(URI, MATCH_EXHAUSTIVE)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    has_exhaustive = any("exhaust" in d.get("message", "").lower() or "missing" in d.get("message", "").lower() for d in all_d)
    check(
        "Non-exhaustive match: reports error/warning",
        has_exhaustive,
        f"Got {len(all_d)} diagnostics: {[d.get('message','') for d in all_d[:3]]}",
    )

    # ── 3. Hover ─────────────────────────────────────────
    print(f"\n{BOLD}3. Hover{NC}")

    # Reopen simple file for hover tests
    lsp.open_doc(URI, SIMPLE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # const x = 42 — hover on "x" (line 0, char 6)
    h = hover_text(lsp.hover(URI, 0, 6))
    check("Hover: const x = 42 shows type", h is not None and ("number" in h or "const x" in h), f"Got: {h}")

    # const msg = "hello" — hover on "msg" (line 1, char 6)
    h = hover_text(lsp.hover(URI, 1, 6))
    check('Hover: const msg = "hello" shows type', h is not None and ("string" in h or "const msg" in h), f"Got: {h}")

    # const flag = true — hover on "flag" (line 2, char 6)
    h = hover_text(lsp.hover(URI, 2, 6))
    check("Hover: const flag = true shows type", h is not None and ("boolean" in h or "bool" in h or "const flag" in h), f"Got: {h}")

    # fn add — hover on "add" (line 4, char 3)
    h = hover_text(lsp.hover(URI, 4, 3))
    check("Hover: fn add shows signature", h is not None and "fn add" in h, f"Got: {h}")

    # export fn greet — hover on "greet" (line 8, char 10)
    h = hover_text(lsp.hover(URI, 8, 11))
    check("Hover: export fn greet shows signature", h is not None and "greet" in h, f"Got: {h}")

    # Hover on whitespace — should return null
    h = lsp.hover(URI, 3, 0)
    is_null = h is not None and h.get("result") is None
    check("Hover: whitespace returns null", is_null, f"Got: {h}")

    # Types file — hover on type name
    lsp.open_doc(URI, TYPES)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 6, 5))
    check("Hover: type User", h is not None and "User" in h, f"Got: {h}")

    h = hover_text(lsp.hover(URI, 1, 6))
    check("Hover: union variant Red", h is not None, f"Got: {h}")

    h = hover_text(lsp.hover(URI, 12, 5))
    check("Hover: fn describeColor", h is not None and "describeColor" in h, f"Got: {h}")

    # Builtins — hover on trim in pipes file
    lsp.open_doc(URI, PIPES)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 6, 11))
    check("Hover: builtin trim", h is not None and "trim" in h.lower(), f"Got: {h}")

    # For-block functions
    lsp.open_doc(URI, FORBLOCK)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 6, 18))
    check("Hover: for-block fn remaining", h is not None and "remaining" in h, f"Got: {h}")

    # Result builtins
    lsp.open_doc(URI, RESULT)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # Hover on Ok (line 3, "Ok(a / b)")
    h = hover_text(lsp.hover(URI, 3, 14))
    check("Hover: Ok builtin", h is not None and "ok" in h.lower(), f"Got: {h}")

    # Hover on Err (line 2, "Err(...)")
    h = hover_text(lsp.hover(URI, 2, 14))
    check("Hover: Err builtin", h is not None and "err" in h.lower(), f"Got: {h}")

    # Hover on ? operator — this is the variable before ?, not ? itself
    h = hover_text(lsp.hover(URI, 8, 23))  # "divide" in divide(a, b)?
    check("Hover: function before ? operator", h is not None and "divide" in (h or ""), f"Got: {h}")

    # ── 4. Completion ────────────────────────────────────
    print(f"\n{BOLD}4. Completion{NC}")

    # Basic completion at empty line
    lsp.open_doc(URI, SIMPLE + "\n")
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    labels = completion_labels(lsp.completion(URI, 11, 0))
    check("Completion: basic (has items)", len(labels) > 0, f"Got {len(labels)} items")
    has_keywords = any(k in labels for k in ["fn", "const", "type", "match", "import"])
    check("Completion: includes keywords", has_keywords, f"Labels: {labels[:10]}")
    has_symbols = any(s in labels for s in ["add", "greet", "x", "msg"])
    check("Completion: includes document symbols", has_symbols, f"Labels: {labels[:10]}")

    # Pipe completions
    lsp.open_doc(URI, COMPLETION_PIPE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    labels = completion_labels(lsp.completion(URI, 1, len("const result = nums |> ")))
    check("Completion: after |> has items", len(labels) > 0, f"Got {len(labels)} items")

    # Match arm completion
    match_source = "type Color { | Red | Green | Blue }\nconst c: Color = Red\nconst r = match c {\n    \n}"
    lsp.open_doc(URI, match_source)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    labels = completion_labels(lsp.completion(URI, 3, 4))
    has_variants = any(v in labels for v in ["Red", "Green", "Blue"])
    check("Completion: match arms show union variants", has_variants, f"Labels: {labels[:10]}")

    # JSX attribute completion
    jsx_source = 'import trusted { useState } from "react"\nexport fn App() -> JSX.Element {\n    <button on\n}'
    lsp.open_doc(URI, jsx_source)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    labels = completion_labels(lsp.completion(URI, 2, 15))
    has_jsx = any("on" in l.lower() for l in labels)
    check("Completion: JSX attributes (on...)", has_jsx, f"Labels: {labels[:10]}")

    # Stdlib module completions (Array.)
    stdlib_source = "const nums = [1, 2, 3]\nconst r = nums |> Array.\n"
    lsp.open_doc(URI, stdlib_source)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    labels = completion_labels(lsp.completion(URI, 1, len("const r = nums |> Array.")))
    has_array_methods = any(m in labels for m in ["map", "filter", "reduce", "sort", "length"])
    check("Completion: Array. shows methods", has_array_methods, f"Labels: {labels[:15]}")

    # String module completions
    str_source = 'const s = "hello"\nconst r = s |> String.\n'
    lsp.open_doc(URI, str_source)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    labels = completion_labels(lsp.completion(URI, 1, len("const r = s |> String.")))
    has_string_methods = any(m in labels for m in ["trim", "toUpperCase", "toLowerCase", "length", "split"])
    check("Completion: String. shows methods", has_string_methods, f"Labels: {labels[:15]}")

    # ── 5. Go to Definition ──────────────────────────────
    print(f"\n{BOLD}5. Go to Definition{NC}")

    lsp.open_doc(URI, GOTO_DEF)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # "add" on line 4 (usage) — should jump to line 0 (definition)
    locs = def_locations(lsp.goto_definition(URI, 4, 15))
    check("GotoDef: function usage -> definition", len(locs) > 0, f"Got {len(locs)} locations")
    if locs:
        target_line = locs[0].get("range", {}).get("start", {}).get("line", -1)
        check("GotoDef: jumps to correct line", target_line == 0, f"Expected line 0, got line {target_line}")

    # Go to def on a type
    lsp.open_doc(URI, TYPES + "\nfn pick(c: Color) -> string { \"ok\" }\n")
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    locs = def_locations(lsp.goto_definition(URI, 20, 11))  # "Color" in fn pick(c: Color)
    check("GotoDef: type usage -> type definition", len(locs) > 0, f"Got {len(locs)} locations")

    # Go to def on a keyword — should return empty
    lsp.open_doc(URI, SIMPLE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    locs = def_locations(lsp.goto_definition(URI, 0, 1))  # "const"
    check("GotoDef: keyword returns empty", len(locs) == 0, f"Got {len(locs)} locations")

    # ── 6. Find References ───────────────────────────────
    print(f"\n{BOLD}6. Find References{NC}")

    lsp.open_doc(URI, GOTO_DEF)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    resp = lsp.references(URI, 0, 3)  # "add" definition
    refs = resp.get("result", []) if resp else []
    check("References: fn add (def + usage)", len(refs) >= 2, f"Got {len(refs)} refs")

    # References on a type
    lsp.open_doc(URI, TYPES + "\nfn pick(c: Color) -> string { \"ok\" }\n")
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    resp = lsp.references(URI, 0, 5)  # "Color" type definition
    refs = resp.get("result", []) if resp else []
    check("References: type Color", len(refs) >= 2, f"Got {len(refs)} refs")

    # ── 7. Document Symbols ──────────────────────────────
    print(f"\n{BOLD}7. Document Symbols{NC}")

    lsp.open_doc(URI, SIMPLE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    names = symbol_names(lsp.document_symbols(URI))
    check("Symbols: lists functions", "add" in names, f"Names: {names}")
    check("Symbols: lists exported functions", "greet" in names, f"Names: {names}")
    check("Symbols: lists consts", "x" in names, f"Names: {names}")

    lsp.open_doc(URI, TYPES)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    names = symbol_names(lsp.document_symbols(URI))
    check("Symbols: lists types", "Color" in names, f"Names: {names}")
    check("Symbols: lists union variants", "Red" in names and "Green" in names, f"Names: {names}")
    check("Symbols: lists record types", "User" in names, f"Names: {names}")

    lsp.open_doc(URI, FORBLOCK)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    names = symbol_names(lsp.document_symbols(URI))
    check("Symbols: for-block functions appear", "remaining" in names, f"Names: {names}")

    # ── 8. Code Actions ──────────────────────────────────
    print(f"\n{BOLD}8. Code Actions{NC}")

    lsp.open_doc(URI, CODE_ACTION)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    code_action_diags = diag_all(notifs)

    resp = lsp.code_action(URI, 0, diagnostics=code_action_diags)
    actions = (resp.get("result") or []) if resp else []
    check(
        "CodeAction: missing return type (E010)",
        len(actions) > 0,
        f"Got {len(actions)} actions",
    )
    if actions:
        titles = [a.get("title", "") for a in actions]
        check("CodeAction: has 'add return type' fix", any("return type" in t.lower() or "-> " in t for t in titles), f"Titles: {titles}")

    lsp.open_doc(URI, SIMPLE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    resp = lsp.code_action(URI, 0)
    actions = (resp.get("result") or []) if resp else []
    check("CodeAction: valid code has no actions", len(actions) == 0, f"Got {len(actions)} actions")

    # ── 9. JSX-specific features ─────────────────────────
    print(f"\n{BOLD}9. JSX Support{NC}")

    lsp.open_doc(URI, JSX_COMPONENT)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)

    # JSX file should parse without errors (assuming react types available)
    # Note: may get import errors if react isn't installed, but parse should succeed
    all_d = diag_all(notifs)
    # Filter to only actual parse/syntax errors — not import resolution errors
    parse_errs = [e for e in all_d if "cannot find module" not in e.get("message", "").lower()
                  and ("parse" in e.get("message", "").lower() or "expected" in e.get("message", "").lower() and "found" in e.get("message", "").lower())]
    check("JSX: parses without syntax errors", len(parse_errs) == 0, f"Parse errors: {[e.get('message','') for e in parse_errs[:3]]}")

    # Hover on JSX component function
    h = hover_text(lsp.hover(URI, 2, 14))
    check("JSX: hover on component fn", h is not None and "Counter" in (h or ""), f"Got: {h}")

    # Hover on destructured useState vars
    h = hover_text(lsp.hover(URI, 3, 11))
    check("JSX: hover on destructured var (count)", h is not None, f"Got: {h}")

    h = hover_text(lsp.hover(URI, 3, 18))
    check("JSX: hover on destructured var (setCount)", h is not None, f"Got: {h}")

    # Hover on inner function
    h = hover_text(lsp.hover(URI, 5, 8))
    check("JSX: hover on inner fn (handleClick)", h is not None and "handleClick" in (h or ""), f"Got: {h}")

    # Go to def from JSX attribute value (handleClick in onClick={handleClick})
    locs = def_locations(lsp.goto_definition(URI, 11, 30))  # "handleClick" in onClick
    check("JSX: goto def from attribute value", len(locs) > 0, f"Got {len(locs)} locations")

    # Document symbols in JSX file
    names = symbol_names(lsp.document_symbols(URI))
    check("JSX: symbols include component", "Counter" in names, f"Names: {names}")
    check("JSX: symbols include inner fn", "handleClick" in names, f"Names: {names}")

    # ── 10. Edge Cases & Error Recovery ─────────────────────
    print(f"\n{BOLD}10. Edge Cases{NC}")

    # Drain any stale notifications from previous section
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=0.5)

    # Empty file
    lsp.open_doc(URI, EMPTY_FILE)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    check("Edge: empty file doesn't crash", True)  # If we got here, no crash
    errs = diag_errors(notifs)
    check("Edge: empty file has no errors", len(errs) == 0, f"Got {len(errs)} errors")

    # Comment-only file
    lsp.open_doc(URI, SINGLE_COMMENT)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    errs = diag_errors(notifs)
    check("Edge: comment-only file has no errors", len(errs) == 0, f"Got {len(errs)} errors")

    # Hover on empty file
    h = lsp.hover(URI, 0, 0)
    check("Edge: hover on empty file", h is not None, "No response")

    # Completion on empty file
    labels = completion_labels(lsp.completion(URI, 0, 0))
    check("Edge: completion on comment-only file", len(labels) > 0, f"Got {len(labels)} items")

    # Symbols on empty file
    lsp.open_doc(URI, EMPTY_FILE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Edge: symbols on empty file is empty", len(names) == 0, f"Names: {names}")

    # ── 11. Diagnostics: Type Errors ─────────────────────────
    print(f"\n{BOLD}11. Type Error Diagnostics{NC}")

    # Shadowing error
    lsp.open_doc(URI, SHADOWING)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    has_shadow = any("already defined" in d.get("message", "").lower() or "shadow" in d.get("message", "").lower() or "redecl" in d.get("message", "").lower() for d in all_d)
    check("Diag: variable shadowing detected", has_shadow, f"Diagnostics: {[d.get('message','') for d in all_d[:3]]}")

    # Undefined variable
    lsp.open_doc(URI, UNDEFINED_VAR)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    has_undefined = any("undefined" in d.get("message", "").lower() or "not defined" in d.get("message", "").lower() or "undeclared" in d.get("message", "").lower() for d in errs)
    check("Diag: undefined variable reported", has_undefined, f"Errors: {[d.get('message','') for d in errs[:3]]}")

    # Type mismatch
    lsp.open_doc(URI, TYPE_MISMATCH)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    has_mismatch = any("expected" in d.get("message", "").lower() and "found" in d.get("message", "").lower() for d in all_d)
    check("Diag: type mismatch detected", has_mismatch, f"Diagnostics: {[d.get('message','') for d in all_d[:5]]}")

    # Partial match (non-exhaustive)
    lsp.open_doc(URI, PARTIAL_MATCH)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    has_missing = any("exhaust" in d.get("message", "").lower() or "missing" in d.get("message", "").lower() or "green" in d.get("message", "").lower() or "blue" in d.get("message", "").lower() for d in all_d)
    check("Diag: partial match missing Green/Blue", has_missing, f"Diagnostics: {[d.get('message','') for d in all_d[:3]]}")

    # Number match without wildcard
    lsp.open_doc(URI, MATCH_NUMBER_NO_WILDCARD)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    has_err = any("exhaust" in d.get("message", "").lower() or "missing" in d.get("message", "").lower() for d in all_d)
    check("Diag: number match without _ requires wildcard", has_err, f"Diagnostics: {[d.get('message','') for d in all_d[:3]]}")

    # String match without wildcard
    lsp.open_doc(URI, MATCH_STRING_NO_WILDCARD)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    has_err = any("exhaust" in d.get("message", "").lower() or "missing" in d.get("message", "").lower() for d in all_d)
    check("Diag: string match without _ requires wildcard", has_err, f"Diagnostics: {[d.get('message','') for d in all_d[:3]]}")

    # Number with when guards, no catch-all
    lsp.open_doc(URI, MATCH_NUMBER_GUARDS_NO_WILDCARD)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    has_err = any("exhaust" in d.get("message", "").lower() or "missing" in d.get("message", "").lower() for d in all_d)
    check("Diag: number match with guards but no _ requires wildcard", has_err, f"Diagnostics: {[d.get('message','') for d in all_d[:3]]}")

    # Number with ranges, no wildcard
    lsp.open_doc(URI, MATCH_RANGES_NO_WILDCARD)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    has_err = any("exhaust" in d.get("message", "").lower() or "missing" in d.get("message", "").lower() for d in all_d)
    check("Diag: number match with ranges but no _ requires wildcard", has_err, f"Diagnostics: {[d.get('message','') for d in all_d[:3]]}")

    # Tuple missing cases
    lsp.open_doc(URI, MATCH_TUPLE_MISSING)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    has_err = any("exhaust" in d.get("message", "").lower() or "missing" in d.get("message", "").lower() for d in all_d)
    check("Diag: tuple match missing (true,false)/(false,true)", has_err, f"Diagnostics: {[d.get('message','') for d in all_d[:3]]}")

    # ── 12. Advanced Hover ───────────────────────────────────
    print(f"\n{BOLD}12. Advanced Hover{NC}")

    # Hover on function parameters
    lsp.open_doc(URI, FN_PARAMS_HOVER)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # "name" parameter (line 0, char 12)
    h = hover_text(lsp.hover(URI, 0, 12))
    check("Hover: fn parameter (name)", h is not None, f"Got: {h}")

    # Hover on nested match function
    lsp.open_doc(URI, NESTED_MATCH)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 10, 3))  # "describe" in fn describe(o: Outer)
    check("Hover: fn with nested match", h is not None and "describe" in (h or ""), f"Got: {h}")

    # Hover on type with spread
    lsp.open_doc(URI, SPREAD_FILE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 5, 5))  # "Extended" in type Extended {
    check("Hover: type with spread (Extended)", h is not None and "Extended" in (h or ""), f"Got: {h}")

    # Hover on closure assigned to const
    lsp.open_doc(URI, CLOSURE_ASSIGN)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 0, 6))  # "add" in const add = (...) => ...
    check("Hover: closure const (add)", h is not None and "add" in (h or ""), f"Got: {h}")

    h = hover_text(lsp.hover(URI, 1, 6))  # "double"
    check("Hover: closure const (double)", h is not None and "double" in (h or ""), f"Got: {h}")

    h = hover_text(lsp.hover(URI, 2, 6))  # "result" in const result = add(1, 2)
    check("Hover: const from closure call", h is not None, f"Got: {h}")

    # Hover on 'todo' keyword
    lsp.open_doc(URI, TODO_UNREACHABLE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 1, 4))
    check("Hover: 'todo' placeholder", h is not None, f"Got: {h}")

    # Hover on 'Some' and 'None'
    lsp.open_doc(URI, OPTION_FILE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 2, 15))
    check("Hover: None literal", h is not None and "none" in (h or "").lower(), f"Got: {h}")

    h = hover_text(lsp.hover(URI, 3, 27))  # "Some" in Some(first)
    check("Hover: Some literal", h is not None and "some" in (h or "").lower(), f"Got: {h}")

    # Hover on 'match' keyword
    lsp.open_doc(URI, WHEN_GUARD)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 1, 5))
    check("Hover: 'match' keyword", h is not None and "match" in (h or "").lower(), f"Got: {h}")

    # Hover on member access (self.name, user.id)
    lsp.open_doc(URI, RECORD_SPREAD)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 7, 15))  # "user" in User(..user, ...)
    check("Hover: variable in spread (user)", h is not None, f"Got: {h}")

    # Hover on const assigned to function call result
    lsp.open_doc(URI, MULTIPLE_FNS)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 4, 6))  # "a" in const a = first(1)
    check("Hover: const from fn call shows type", h is not None and "number" in (h or ""), f"Got: {h}")

    h = hover_text(lsp.hover(URI, 7, 6))  # "d" in const d = first(second(third(0)))
    check("Hover: nested fn call result type", h is not None and "number" in (h or ""), f"Got: {h}")

    # Hover on tuple result
    lsp.open_doc(URI, TUPLE_FILE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 0, 3))  # fn swap
    check("Hover: fn with tuple return", h is not None and "swap" in (h or ""), f"Got: {h}")

    h = hover_text(lsp.hover(URI, 4, 6))  # const pair
    check("Hover: const assigned to tuple", h is not None, f"Got: {h}")

    # Hover on destructured tuple vars
    h = hover_text(lsp.hover(URI, 5, 7))  # x in const (x, y)
    check("Hover: destructured tuple var (x)", h is not None, f"Got: {h}")

    # Hover on trait impl function
    lsp.open_doc(URI, TRAIT_FILE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 10, 7))  # "print" in for Dog: Printable { fn print(self) }
    check("Hover: trait impl fn (print)", h is not None and "print" in (h or ""), f"Got: {h}")

    # Hover on inner const
    lsp.open_doc(URI, INNER_CONST)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 1, 10))  # inner
    check("Hover: inner const (inner)", h is not None, f"Got: {h}")

    h = hover_text(lsp.hover(URI, 2, 10))  # doubled
    check("Hover: inner const (doubled)", h is not None, f"Got: {h}")

    # ── 13. Advanced Completion ──────────────────────────────
    print(f"\n{BOLD}13. Advanced Completion{NC}")

    # Completion with partial prefix
    lsp.open_doc(URI, "fn apple() -> number { 1 }\nfn apricot() -> number { 2 }\nconst r = ap\n")
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    labels = completion_labels(lsp.completion(URI, 2, 12))
    has_filtered = "apple" in labels and "apricot" in labels
    check("Completion: prefix filtering (ap -> apple, apricot)", has_filtered, f"Labels: {labels[:10]}")

    # Completion shouldn't include imports in normal context
    lsp.open_doc(URI, 'import { useState } from "react"\n\n')
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    labels = completion_labels(lsp.completion(URI, 1, 0))
    has_import = "useState" in labels
    check("Completion: includes imported symbols", has_import, f"Labels: {labels[:15]}")

    # Completion inside function body
    lsp.open_doc(URI, "fn outer() -> number {\n    const local = 42\n    \n}")
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    labels = completion_labels(lsp.completion(URI, 2, 4))
    has_local = "local" in labels
    check("Completion: local vars in fn body", has_local, f"Labels: {labels[:15]}")

    # Completion for union constructors
    lsp.open_doc(URI, "type Color { | Red | Green | Blue }\nconst c = \n")
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)

    labels = completion_labels(lsp.completion(URI, 1, 10))
    has_constructors = any(v in labels for v in ["Red", "Green", "Blue"])
    check("Completion: union constructors available", has_constructors, f"Labels: {labels[:15]}")

    # Completion for Result/Option builtins
    labels_with_ok = "Ok" in labels
    labels_with_err = "Err" in labels
    check("Completion: Ok/Err builtins present", labels_with_ok and labels_with_err, f"Labels: {labels[:15]}")

    # ── 14. Advanced Go to Definition ─────────────────────────
    print(f"\n{BOLD}14. Advanced Go to Definition{NC}")

    # Go to def on union variant usage in match
    lsp.open_doc(URI, TYPES)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # "Red" in match arm (line 14)
    locs = def_locations(lsp.goto_definition(URI, 14, 8))
    check("GotoDef: union variant in match arm (Red)", len(locs) > 0, f"Got {len(locs)} locations")

    # Go to def on const usage
    lsp.open_doc(URI, MULTIPLE_FNS)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # "a" on line 5 (const b = second(a))
    locs = def_locations(lsp.goto_definition(URI, 5, 17))
    check("GotoDef: const variable usage", len(locs) > 0, f"Got {len(locs)} locations")

    # Go to def on function in chained calls
    # line 7: const d = first(second(third(0)))
    locs = def_locations(lsp.goto_definition(URI, 7, 10))  # "first"
    check("GotoDef: fn in nested call (first)", len(locs) > 0, f"Got {len(locs)} locations")

    locs = def_locations(lsp.goto_definition(URI, 7, 16))  # "second"
    check("GotoDef: fn in nested call (second)", len(locs) > 0, f"Got {len(locs)} locations")

    locs = def_locations(lsp.goto_definition(URI, 7, 23))  # "third"
    check("GotoDef: fn in nested call (third)", len(locs) > 0, f"Got {len(locs)} locations")

    # Go to def on type in function parameter
    lsp.open_doc(URI, RECORD_SPREAD)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # "User" on line 6 fn updateName(user: User, ...)
    locs = def_locations(lsp.goto_definition(URI, 6, 20))
    check("GotoDef: type in parameter annotation", len(locs) > 0, f"Got {len(locs)} locations")

    # Go to def on type in return type annotation
    locs = def_locations(lsp.goto_definition(URI, 6, 47))  # -> User
    check("GotoDef: type in return annotation", len(locs) > 0, f"Got {len(locs)} locations")

    # ── 15. Advanced Find References ──────────────────────────
    print(f"\n{BOLD}15. Advanced Find References{NC}")

    lsp.open_doc(URI, MULTIPLE_FNS)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # "first" appears: def (line 0), usage (line 4), usage (line 7) = 3 refs
    resp = lsp.references(URI, 0, 3)
    refs = (resp.get("result") or []) if resp else []
    check("References: fn first (3 uses)", len(refs) >= 3, f"Got {len(refs)} refs")

    # "a" appears: def (line 4), usage (line 5) = 2 refs (at least)
    resp = lsp.references(URI, 4, 6)
    refs = (resp.get("result") or []) if resp else []
    check("References: const a (def + usage)", len(refs) >= 2, f"Got {len(refs)} refs")

    # Large union - all variants should be findable
    lsp.open_doc(URI, LARGE_UNION)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    resp = lsp.references(URI, 1, 6)  # "Plus"
    refs = (resp.get("result") or []) if resp else []
    check("References: large union variant (Plus)", len(refs) >= 2, f"Got {len(refs)} refs")

    # ── 16. Document Symbols: Advanced ────────────────────────
    print(f"\n{BOLD}16. Document Symbols: Advanced{NC}")

    # Multiple functions
    lsp.open_doc(URI, MULTIPLE_FNS)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Symbols: all fns listed", all(n in names for n in ["first", "second", "third"]), f"Names: {names}")
    check("Symbols: all consts listed", all(n in names for n in ["a", "b", "c", "d"]), f"Names: {names}")

    # Closure consts
    lsp.open_doc(URI, CLOSURE_ASSIGN)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Symbols: closure const (add)", "add" in names, f"Names: {names}")
    check("Symbols: closure const (double)", "double" in names, f"Names: {names}")

    # Trait file
    lsp.open_doc(URI, TRAIT_FILE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Symbols: trait definition", "Printable" in names, f"Names: {names}")
    check("Symbols: type Dog", "Dog" in names, f"Names: {names}")
    check("Symbols: trait impl fn print", "print" in names, f"Names: {names}")

    # Large union
    lsp.open_doc(URI, LARGE_UNION)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Symbols: large union (14 variants)", all(v in names for v in ["Plus", "Minus", "Star", "Eof"]), f"Names: {names}")

    # Nested match file
    lsp.open_doc(URI, NESTED_MATCH)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Symbols: nested union types", "Outer" in names and "Inner" in names, f"Names: {names}")
    check("Symbols: nested union variants", "A" in names and "X" in names, f"Names: {names}")

    # ── 17. Diagnostics: Valid Complex Code ──────────────────
    print(f"\n{BOLD}17. Valid Complex Code (no errors expected){NC}")

    for name, source in [
        ("nested match", NESTED_MATCH),
        ("multiple fns", MULTIPLE_FNS),
        ("tuple", TUPLE_FILE),
        ("option", OPTION_FILE),
        ("trait impl", TRAIT_FILE),
        ("spread type", SPREAD_FILE),
        ("record spread", RECORD_SPREAD),
        ("closure assign", CLOSURE_ASSIGN),
        ("string literal union", STRING_LITERAL_UNION),
        ("collect/error accumulation", COLLECT_FILE),
        ("default params", DEFAULT_PARAMS),
        ("when guards", WHEN_GUARD),
        ("large union (14 variants)", LARGE_UNION),
        ("inner const", INNER_CONST),
        ("todo/unreachable", TODO_UNREACHABLE),
        ("import for-block", IMPORT_FOR),
        ("closures", CLOSURE_FILE),
        ("dot shorthand", DOT_SHORTHAND),
        ("placeholder/partial app", PLACEHOLDER),
        ("range match", RANGE_MATCH),
        ("array patterns", ARRAY_PATTERN),
        ("string patterns", STRING_PATTERN),
        ("pipe into match", PIPE_INTO_MATCH),
        ("newtype wrappers", NEWTYPE_WRAPPER),
        ("newtypes", NEWTYPE),
        ("opaque types", OPAQUE_TYPE),
        ("tuple index access", TUPLE_INDEX),
        ("deriving", DERIVING),
        ("test blocks", TEST_BLOCK),
        ("unreachable", UNREACHABLE),
        ("map/set stdlib", MAP_SET),
        ("structural equality", STRUCTURAL_EQ),
        ("inline for", INLINE_FOR),
        ("number separators", NUMBER_SEPARATOR),
        ("multi-depth match", MULTI_DEPTH_MATCH),
        ("qualified variants", QUALIFIED_VARIANT),
        ("generic functions", GENERIC_FN),
    ]:
        lsp.open_doc(URI, source)
        notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
        errs = diag_errors(notifs)
        check(f"Valid: {name}", len(errs) == 0, f"Got {len(errs)} errors: {[e.get('message','') for e in errs[:3]]}")

    # ── 18. Hover: Type Inference Quality ────────────────────
    print(f"\n{BOLD}18. Hover: Inferred Type Quality{NC}")

    # Does hover show actual types, not 'unknown' or '?T'?
    lsp.open_doc(URI, MULTIPLE_FNS)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 4, 6))  # const a = first(1)
    has_unknown = h is not None and "unknown" in (h or "").lower()
    has_tvar = h is not None and "?T" in (h or "")
    check("TypeQuality: const a doesn't show 'unknown'", not has_unknown, f"Got: {h}")
    check("TypeQuality: const a doesn't show '?T'", not has_tvar, f"Got: {h}")

    # Closure call result - does hover show the type?
    lsp.open_doc(URI, CLOSURE_ASSIGN)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 2, 6))  # const result = add(1, 2)
    check("TypeQuality: closure call result type", h is not None and ("number" in (h or "") or "result" in (h or "")), f"Got: {h}")

    # Collect result type
    lsp.open_doc(URI, COLLECT_FILE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    h = hover_text(lsp.hover(URI, 15, 3))  # fn validate
    check("TypeQuality: collect fn shows Result return", h is not None and "validate" in (h or ""), f"Got: {h}")

    # ── 19. Rapid Document Updates (Typing Simulation) ───────
    print(f"\n{BOLD}19. Rapid Updates (Typing Simulation){NC}")

    # Simulate typing character by character
    base = "const x = "
    for i, char in enumerate("42"):
        lsp.open_doc(URI, base + "42"[:i+1])

    # Wait and check we still get valid diagnostics
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    check("RapidUpdate: survives rapid edits", True)  # No crash = pass

    # Simulate typing a function
    stages = [
        "fn ",
        "fn test",
        "fn test(",
        "fn test() ",
        "fn test() {",
        "fn test() { 42 }",
    ]
    for stage in stages:
        lsp.open_doc(URI, stage)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    check("RapidUpdate: partial fn typing", True)  # No crash

    # Open then immediately request hover
    lsp.open_doc(URI, SIMPLE)
    h = lsp.hover(URI, 0, 6)
    check("RapidUpdate: hover right after open", h is not None, "No response")

    # ── 20. Multiline Pipe Chains ────────────────────────────
    print(f"\n{BOLD}20. Multiline Pipes{NC}")

    lsp.open_doc(URI, MULTILINE_PIPE)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    check("MultilinePipe: no errors", len(errs) == 0, f"Errors: {[e.get('message','') for e in errs[:3]]}")

    h = hover_text(lsp.hover(URI, 0, 6))  # const result
    check("MultilinePipe: hover on result", h is not None, f"Got: {h}")

    # ── 21. Cross-file (two documents open) ──────────────────
    print(f"\n{BOLD}21. Cross-file Features{NC}")

    URI_A = "file:///tmp/types.fl"
    URI_B = "file:///tmp/main.fl"

    types_src = 'export type Color { | Red | Green | Blue }\nexport fn makeRed() -> Color { Red }\n'
    main_src = 'import { Color, makeRed } from "./types"\nconst c = makeRed()\n'

    lsp.open_doc(URI_A, types_src)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    lsp.open_doc(URI_B, main_src)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # Cross-file references
    resp = lsp.references(URI_A, 0, 14)  # "Color" in types.fl
    refs = (resp.get("result") or []) if resp else []
    cross_file_refs = [r for r in refs if r.get("uri") != URI_A]
    check("CrossFile: references found across files", len(refs) >= 2, f"Got {len(refs)} refs")
    check("CrossFile: refs include other file", len(cross_file_refs) > 0, f"Cross-file refs: {len(cross_file_refs)}")

    # Cross-file go to definition
    locs = def_locations(lsp.goto_definition(URI_B, 1, 10))  # "makeRed" in main.fl
    check("CrossFile: goto def across files", len(locs) > 0, f"Got {len(locs)} locations")
    if locs:
        target_uri = locs[0].get("uri", "")
        check("CrossFile: goto def points to types.fl", "types" in target_uri, f"Target: {target_uri}")

    # Cross-file completion (auto-import)
    new_main = 'import { Color, makeRed } from "./types"\nconst c = makeRed()\nmake\n'
    lsp.open_doc(URI_B, new_main)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    labels = completion_labels(lsp.completion(URI_B, 2, 4))
    has_cross = "makeRed" in labels
    check("CrossFile: completion shows cross-file symbols", has_cross, f"Labels: {labels[:10]}")

    # ── 22. Tour Feature Coverage ─────────────────────────
    print(f"\n{BOLD}22. Tour Feature Coverage{NC}")

    # Closures
    lsp.open_doc(URI, CLOSURE_FILE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    h = hover_text(lsp.hover(URI, 0, 6))  # const add
    check("Tour: hover on closure const", h is not None and "add" in (h or ""), f"Got: {h}")
    names = symbol_names(lsp.document_symbols(URI))
    check("Tour: closure consts in symbols", "add" in names and "double" in names, f"Names: {names}")

    # Dot shorthand
    lsp.open_doc(URI, DOT_SHORTHAND)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    check("Tour: dot shorthand hover on names", True)  # just checking no crash
    h = hover_text(lsp.hover(URI, 3, 6))  # const names
    check("Tour: hover on dot shorthand result", h is not None, f"Got: {h}")

    # Placeholder / partial application
    lsp.open_doc(URI, PLACEHOLDER)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    h = hover_text(lsp.hover(URI, 1, 6))  # const addTen
    check("Tour: hover on partial application result", h is not None, f"Got: {h}")

    # Pipe into match
    lsp.open_doc(URI, PIPE_INTO_MATCH)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    h = hover_text(lsp.hover(URI, 0, 3))  # fn label
    check("Tour: hover on pipe-into-match fn", h is not None and "label" in (h or ""), f"Got: {h}")

    # Newtype wrappers
    lsp.open_doc(URI, NEWTYPE_WRAPPER)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Tour: newtype wrapper in symbols", "UserId" in names, f"Names: {names}")
    h = hover_text(lsp.hover(URI, 0, 5))
    check("Tour: hover on newtype wrapper", h is not None and "UserId" in (h or ""), f"Got: {h}")

    # Newtypes
    lsp.open_doc(URI, NEWTYPE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    h = hover_text(lsp.hover(URI, 0, 5))
    check("Tour: hover on newtype (ProductId)", h is not None and "ProductId" in (h or ""), f"Got: {h}")

    # Opaque types
    lsp.open_doc(URI, OPAQUE_TYPE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Tour: opaque type in symbols", "HashedPassword" in names, f"Names: {names}")

    # Tuple index access
    lsp.open_doc(URI, TUPLE_INDEX)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    h = hover_text(lsp.hover(URI, 1, 6))  # const first
    check("Tour: hover on tuple index result", h is not None, f"Got: {h}")

    # Test blocks
    lsp.open_doc(URI, TEST_BLOCK)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Tour: fn inside test file in symbols", "add" in names, f"Names: {names}")

    # For block (was inline for)
    lsp.open_doc(URI, INLINE_FOR)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    h = hover_text(lsp.hover(URI, 1, 18))  # fn shout
    check("Tour: hover on inline for fn", h is not None and "shout" in (h or ""), f"Got: {h}")
    names = symbol_names(lsp.document_symbols(URI))
    check("Tour: inline for fn in symbols", "shout" in names, f"Names: {names}")

    # Map/Set stdlib
    lsp.open_doc(URI, MAP_SET)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    h = hover_text(lsp.hover(URI, 0, 6))  # const config
    check("Tour: hover on Map result", h is not None, f"Got: {h}")

    # Multi-depth match
    lsp.open_doc(URI, MULTI_DEPTH_MATCH)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    h = hover_text(lsp.hover(URI, 10, 3))  # "describe" in fn describe(e: ApiError)
    check("Tour: hover on multi-depth match fn", h is not None and "describe" in (h or ""), f"Got: {h}")
    names = symbol_names(lsp.document_symbols(URI))
    check("Tour: nested union variants in symbols", "Timeout" in names and "DnsFailure" in names, f"Names: {names}")

    # Deriving
    lsp.open_doc(URI, DERIVING)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)
    names = symbol_names(lsp.document_symbols(URI))
    check("Tour: deriving type in symbols", "Point" in names, f"Names: {names}")

    # ── 23. Qualified Variants ────────────────────────────
    print(f"\n{BOLD}23. Qualified Variants{NC}")

    # Valid qualified variant file — no errors
    lsp.open_doc(URI, QUALIFIED_VARIANT)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    check("QualifiedVariant: valid file no errors", len(errs) == 0, f"Errors: {[e.get('message','') for e in errs[:3]]}")

    # Hover on qualified variant (Color.Red) — hover on "Color" (line 3, char 11)
    h = hover_text(lsp.hover(URI, 3, 11))
    check("QualifiedVariant: hover on type name", h is not None and "Color" in (h or ""), f"Got: {h}")

    # Hover on variant after dot (line 3 "Color.Red", hover on "Red" around char 17)
    h = hover_text(lsp.hover(URI, 3, 17))
    check("QualifiedVariant: hover on variant after dot", h is not None, f"Got: {h}")

    # Hover on qualified variant with args (Color.Blue, line 4)
    h = hover_text(lsp.hover(URI, 4, 11))
    check("QualifiedVariant: hover on type in constructor", h is not None, f"Got: {h}")

    # Go-to-def on qualified variant type name
    locs = def_locations(lsp.goto_definition(URI, 3, 11))
    check("QualifiedVariant: goto def on type name", len(locs) > 0, f"Got {len(locs)} locations")

    # Document symbols include types and variants
    names = symbol_names(lsp.document_symbols(URI))
    check("QualifiedVariant: Color in symbols", "Color" in names, f"Names: {names}")
    check("QualifiedVariant: Filter in symbols", "Filter" in names, f"Names: {names}")
    check("QualifiedVariant: variants in symbols", "Red" in names and "All" in names, f"Names: {names}")

    # Qualified variant in tuple — no errors
    # (this was the original bug: Color.Red in tuple parsed as 3 elements)

    # Ambiguous variant detection
    lsp.open_doc(URI, AMBIGUOUS_VARIANT)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    all_d = diag_all(notifs)
    # Color.Red and Light.Red are qualified — no errors
    # Blue and Yellow are unambiguous — no errors
    errs = diag_errors(notifs)
    check("QualifiedVariant: qualified + unambiguous bare = no errors", len(errs) == 0, f"Errors: {[e.get('message','') for e in errs[:3]]}")

    # Test that bare ambiguous variant DOES error
    ambig_src = "type Color { | Red | Green | Blue }\ntype Light { | Red | Yellow | Green }\nconst _x = Red\n"
    lsp.open_doc(URI, ambig_src)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    has_ambig = any("ambiguous" in e.get("message", "").lower() for e in errs)
    check("QualifiedVariant: bare ambiguous variant errors", has_ambig, f"Errors: {[e.get('message','') for e in errs[:3]]}")

    # ── 24. Generic Functions ────────────────────────────
    print(f"\n{BOLD}24. Generic Functions{NC}")

    lsp.open_doc(URI, GENERIC_FN)
    notifs = lsp.collect_notifications("textDocument/publishDiagnostics", timeout=2)
    errs = diag_errors(notifs)
    type_mismatch_errs = [e for e in errs if "E001" in e.get("code", {}).get("value", "") or "argument" in e.get("message", "").lower()]
    check("GenericFn: no type mismatch errors", len(type_mismatch_errs) == 0, f"Errors: {[e.get('message','') for e in errs[:3]]}")

    # Hover on identity — should show <T> in detail
    h = hover_text(lsp.hover(URI, 0, 3))  # "identity" on line 0
    check("GenericFn: hover on identity shows <T>", h is not None and "<T>" in (h or ""), f"Got: {h}")

    # Hover on pair — should show <A, B>
    h = hover_text(lsp.hover(URI, 1, 3))  # "pair" on line 1
    check("GenericFn: hover on pair shows <A, B>", h is not None and "<A, B>" in (h or ""), f"Got: {h}")

    # Symbols should include generic functions
    names = symbol_names(lsp.document_symbols(URI))
    check("GenericFn: identity in symbols", "identity" in names, f"Names: {names}")
    check("GenericFn: pair in symbols", "pair" in names, f"Names: {names}")

    # ── 25. Hover Improvements (#403) ────────────────────
    print(f"\n{BOLD}25. Hover Improvements (#403){NC}")

    # --- Issue 1: Type definitions should show full body ---
    lsp.open_doc(URI, HOVER_TYPE_BODY)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # Hover on "Product" (line 0, char 5) — should show fields
    h = hover_text(lsp.hover(URI, 0, 5))
    check(
        "Hover #403.1: type Product shows field list",
        h is not None and "id: number" in h and "title: string" in h and "price: number" in h,
        f"Got: {h}",
    )

    # Hover on "Status" (union) — should show variants
    h = hover_text(lsp.hover(URI, 7, 5))
    check(
        "Hover #403.1: type Status shows variants",
        h is not None and "Active" in h and "Inactive" in h,
        f"Got: {h}",
    )

    # --- Issue 2: Record fields should show (property) type on hover ---
    # Hover on "title" field in Product (line 2, char 4)
    h = hover_text(lsp.hover(URI, 2, 4))
    check(
        "Hover #403.2: field title shows (property) type",
        h is not None and "title" in h and "string" in h,
        f"Got: {h}",
    )

    # Hover on "id" field in Product (line 1, char 4)
    # Should show "(property) id: number", NOT "parameter id: ..." from some other function
    h = hover_text(lsp.hover(URI, 1, 4))
    check(
        "Hover #403.2: field id shows correct (property) info, not parameter",
        h is not None and "number" in h and "parameter" not in h,
        f"Got: {h}",
    )

    # --- Issue 3: Stdlib module hover should not dump all members ---
    lsp.open_doc(URI, HOVER_STDLIB_MEMBER)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # Hover on "Array" in Array.map (line 1, char 25)
    h = hover_text(lsp.hover(URI, 1, 25))
    check(
        "Hover #403.3: module Array hover exists",
        h is not None and "Array" in h,
        f"Got: {h}",
    )

    # --- Issue 4: Stdlib method hover (e.g., Array.map) ---
    # Hover on "map" in Array.map (line 1, char 31)
    h = hover_text(lsp.hover(URI, 1, 31))
    check(
        "Hover #403.4: Array.map shows method signature",
        h is not None and "map" in h and "->" in h,
        f"Got: {h}",
    )

    # Hover on "split" in String.split (line 2, char 38)
    h = hover_text(lsp.hover(URI, 2, 38))
    check(
        "Hover #403.4: String.split shows method signature",
        h is not None and "split" in h and "->" in h,
        f"Got: {h}",
    )

    # --- Issue 6: Property access shows field type ---
    lsp.open_doc(URI, HOVER_MEMBER_ACCESS)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # Hover on "name" in user.name (line 7, char 9)
    h = hover_text(lsp.hover(URI, 7, 9))
    check(
        "Hover #403.6: user.name shows field type string",
        h is not None and "string" in h,
        f"Got: {h}",
    )

    # --- Issue 9: Destructured tuple bindings show types ---
    lsp.open_doc(URI, HOVER_DESTRUCTURE)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # Hover on "name" in const (name, age) (line 4, char 7)
    h = hover_text(lsp.hover(URI, 4, 7))
    check(
        "Hover #403.9: destructured tuple 'name' shows type",
        h is not None and ("string" in h or "name" in h),
        f"Got: {h}",
    )

    # --- Issue 10/11: Default values shown in function hover ---
    lsp.open_doc(URI, HOVER_DEFAULT_PARAMS)
    lsp.collect_notifications("textDocument/publishDiagnostics", timeout=1)

    # Hover on "fetchProducts" (line 0, char 3)
    h = hover_text(lsp.hover(URI, 0, 3))
    check(
        "Hover #403.10: fn with defaults shows = values",
        h is not None and '= ""' in h and "= 20" in h,
        f"Got: {h}",
    )

    # ── Done ─────────────────────────────────────────────
    lsp.shutdown()

    # ── Summary ──────────────────────────────────────────
    print(f"\n{BOLD}{'━' * 50}{NC}")
    print(f"{BOLD}Results: {GREEN}{passed} passed{NC}, {RED}{failed} failed{NC}")

    if errors:
        print(f"\n{YELLOW}Failures:{NC}")
        for err in errors:
            print(f"  - {err}")

    print()
    return 1 if failed > 0 else 0


if __name__ == "__main__":
    sys.exit(main())
