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

import { type Todo, type Filter, type Validation, Display } from "./types";

export function validate(self: string): Validation {
  const trimmed = self.trim();
  const len = trimmed.length;
  return len === 0 ? { tag: "Empty" } : len === 1 ? { tag: "TooShort" } : len > 100 ? { tag: "TooLong" } : { tag: "Valid", text: trimmed };
}

export function display(self: Todo): string {
  return self.done === true ? `[x] ${self.text}` : self.done === false ? `[ ] ${self.text}` : (() => { throw new Error("non-exhaustive match"); })();
}

export function filterBy(self: Array<Todo>, f: Filter): Array<Todo> {
  return f.tag === "All" ? self : f.tag === "Active" ? self.filter((_x) => __floeEq(_x.done, false)) : f.tag === "Completed" ? self.filter((_x) => __floeEq(_x.done, true)) : (() => { throw new Error("non-exhaustive match"); })();
}
export function remaining(self: Array<Todo>): number {
  return (() => { const _v = self.filter((_x) => __floeEq(_x.done, false)); ((active) => console.log("active todos:"))(_v); return _v; })().length;
}
export function stats(self: Array<Todo>): readonly [number, number] {
  return [self.length, self.filter((_x) => __floeEq(_x.done, true)).length];
}
export function search(self: Array<Todo>, _query: string): Array<Todo> {
  return (() => { throw new Error("not implemented"); })();
}

function toResult(v: Validation): { ok: true; value: string } | { ok: false; error: string } {
  return v.tag === "Valid" ? (() => { const text = v.text; return { ok: true as const, value: text }; })() : v.tag === "TooShort" ? { ok: false as const, error: "Text too short" } : v.tag === "TooLong" ? { ok: false as const, error: "Text too long" } : v.tag === "Empty" ? { ok: false as const, error: "Text is empty" } : (() => { throw new Error("non-exhaustive match"); })();
}

function validateId(id: string): { ok: true; value: string } | { ok: false; error: string } {
  return id.length === 0 ? { ok: false as const, error: "ID is empty" } : { ok: true as const, value: id };
}

export function validateTodo(text: string, id: string): { ok: true; value: Todo } | { ok: false; error: Array<string> } {
  return (() => {
    const __errors: Array<any> = [];
    const _r0 = toResult(validate(text));
    if (!_r0.ok) __errors.push(_r0.error);
    const validText = _r0.ok ? _r0.value : undefined as any;
    const _r1 = validateId(id);
    if (!_r1.ok) __errors.push(_r1.error);
    const validId = _r1.ok ? _r1.value : undefined as any;
    if (__errors.length > 0) return { ok: false as const, error: __errors };
    return { ok: true as const, value: { id: validId, text: validText, done: false } };
  })();
}

export function parseTodo(data: unknown): { ok: true; value: Todo } | { ok: false; error: Error } {
  return (() => { const __v = data; if (typeof __v !== "object" || __v === null) return { ok: false as const, error: new Error("expected object, got " + typeof __v) }; return { ok: true as const, value: __v as Todo }; })();
}

export function parseTodos(data: unknown): { ok: true; value: Array<Todo> } | { ok: false; error: Error } {
  return (() => { const __v = data; if (!Array.isArray(__v)) return { ok: false as const, error: new Error("expected array, got " + typeof __v) }; for (let __i3 = 0; __i3 < __v.length; __i3++) { if (typeof __v[__i3] !== "object" || __v[__i3] === null) return { ok: false as const, error: new Error("element [" + __i3 + "]: expected object, got " + typeof __v[__i3]) }; } return { ok: true as const, value: __v as Array<Todo> }; })();
}




