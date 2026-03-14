// Array stdlib
const _sorted = Array.sort([3, 1, 2])
const _mapped = Array.map([1, 2, 3], (n) => n * 2)
const _filtered = Array.filter([1, 2, 3, 4], (n) => n > 2)
const _found = Array.find([1, 2, 3], (n) => n == 2)
const _first = Array.head([1, 2, 3])
const _taken = Array.take([1, 2, 3, 4, 5], 3)
const _reversed = Array.reverse([1, 2, 3])
const _len = Array.length([1, 2, 3])

// Option stdlib
const _mapped_opt = Option.map(Some(42), (n) => n * 2)
const _unwrapped = Option.unwrapOr(None, 0)
const _is_some = Option.isSome(Some(1))

// Result stdlib
const _mapped_res = Result.map(Ok(42), (n) => n + 1)
const _is_ok = Result.isOk(Ok(1))
const _to_opt = Result.toOption(Ok(42))

// String stdlib
const _trimmed = String.trim("  hello  ")
const _upper = String.toUpper("hello")
const _contains = String.contains("hello world", "world")
const _split = String.split("a,b,c", ",")

// Number stdlib
const _parsed = Number.parse("42")
const _clamped = Number.clamp(15, 0, 10)

// Pipes with stdlib
const _piped = [3, 1, 2] |> Array.sort
const _pipe_chain = [1, 2, 3, 4, 5]
  |> Array.filter((n) => n > 2)
  |> Array.map((n) => n * 10)
  |> Array.reverse
