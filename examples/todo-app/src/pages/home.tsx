// @ts-nocheck
function __floeEq(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a == null || b == null) return false;
  if (typeof a !== "object" || typeof b !== "object") return false;
  const ka = Object.keys(a as object);
  const kb = Object.keys(b as object);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => __floeEq((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
}

import { useState } from "react";

import { v4 } from "uuid";

import { type Todo, type Filter, type Validation } from "../types";

import { validate, filterBy, remaining, stats, search } from "../todo";

export function HomePage(): JSX.Element {
  const [todos, setTodos] = useState([]);
  const [input, setInput] = useState("");
  const [filter, setFilter] = useState({ tag: "All" });
  const [error, setError] = useState("");
  const visible = filterBy(todos, filter);
  const remainingCount = remaining(todos);
  function handleAdd() {
    return validate(input).tag === "Empty" ? setError("Please enter a todo") : validate(input).tag === "TooShort" ? setError("Todo must be at least 2 characters") : validate(input).tag === "TooLong" ? setError("Todo must be under 100 characters") : validate(input).tag === "Valid" ? (() => { const text = validate(input).text;     setTodos([...todos, { id: v4(), text: text, done: false }]);     setInput("");     setError("");  })() : (() => { throw new Error("non-exhaustive match"); })();
  }
  function handleToggle(id: string) {
    return setTodos(todos.map((t) => __floeEq(t.id, id) === true ? { ...t, done: !t.done } : __floeEq(t.id, id) === false ? t : (() => { throw new Error("non-exhaustive match"); })()));
  }
  function handleDelete(id: string) {
    return setTodos(todos.filter((_x) => !__floeEq(_x.id, id)));
  }
  return <div><h1 className={"text-3xl font-bold mb-6"}>Todos</h1><div className={"flex gap-2 mb-4"}><input value={input} onChange={(e) => setInput(e.target.value)} onKeyDown={(e) => e.key === "Enter" ? handleAdd() : undefined} placeholder={"What needs to be done?"} className={"flex-1 rounded-lg bg-zinc-800 px-4 py-2 text-zinc-100 placeholder-zinc-500 outline-none ring-1 ring-zinc-700 focus:ring-indigo-500"} /><button onClick={handleAdd} className={"rounded-lg bg-indigo-600 px-4 py-2 font-medium text-white hover:bg-indigo-500 transition-colors"}>Add</button></div>{error === "" ? <span /> : <p className={"text-red-400 text-sm mb-4"}>{error}</p>}<div className={"flex gap-2 mb-6"}><button onClick={() => setFilter({ tag: "All" })} className={filter.tag === "All" ? "rounded-full px-3 py-1 text-sm font-medium bg-indigo-600 text-white" : "rounded-full px-3 py-1 text-sm font-medium bg-zinc-800 text-zinc-400 hover:text-zinc-100"}>All</button><button onClick={() => setFilter({ tag: "Active" })} className={filter.tag === "Active" ? "rounded-full px-3 py-1 text-sm font-medium bg-indigo-600 text-white" : "rounded-full px-3 py-1 text-sm font-medium bg-zinc-800 text-zinc-400 hover:text-zinc-100"}>Active</button><button onClick={() => setFilter({ tag: "Completed" })} className={filter.tag === "Completed" ? "rounded-full px-3 py-1 text-sm font-medium bg-indigo-600 text-white" : "rounded-full px-3 py-1 text-sm font-medium bg-zinc-800 text-zinc-400 hover:text-zinc-100"}>Completed</button></div><ul className={"space-y-2"}>{visible.map((item) => <li key={item.id} className={"flex items-center gap-3 rounded-lg bg-zinc-800/50 px-4 py-3"}><button onClick={() => handleToggle(item.id)} className={item.done === true ? "h-5 w-5 rounded-full border-2 border-indigo-500 bg-indigo-500 flex items-center justify-center" : item.done === false ? "h-5 w-5 rounded-full border-2 border-zinc-600" : (() => { throw new Error("non-exhaustive match"); })()}>{item.done === true ? <span className={"text-white text-xs"}>✓</span> : item.done === false ? <span /> : (() => { throw new Error("non-exhaustive match"); })()}</button><span className={item.done === true ? "flex-1 text-zinc-500 line-through" : item.done === false ? "flex-1 text-zinc-100" : (() => { throw new Error("non-exhaustive match"); })()}>{item.text}</span><button onClick={() => handleDelete(item.id)} className={"text-zinc-600 hover:text-red-400 transition-colors"}>✕</button></li>)}</ul><p className={"mt-6 text-sm text-zinc-500"}>{`${remainingCount} item(s) remaining`}</p></div>;
}
