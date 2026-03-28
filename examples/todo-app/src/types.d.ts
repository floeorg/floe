export type Todo = { id: string; text: string; done: boolean };
export type Filter = { tag: "All" } | { tag: "Active" } | { tag: "Completed" };
export type Validation = { tag: "Valid"; text: string } | { tag: "TooShort" } | { tag: "TooLong" } | { tag: "Empty" };
export type Timestamped = { createdAt: number; updatedAt: number };
export type TodoWithTimestamp = Todo & Timestamped;
