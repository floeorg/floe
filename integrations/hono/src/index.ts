import { Hono, type Context } from "hono";

type Env = Record<string, unknown>;

/**
 * An immutable-looking handle around a Hono instance. Every routing
 * helper below returns a `Router<E>` so Floe code can chain them with
 * the pipe operator. The underlying Hono instance is stored on a
 * double-underscore field to signal "compiler fingerprint — do not
 * reach into this directly".
 *
 * `fetch` is exposed at the top level so the router value satisfies
 * runtimes like Cloudflare Workers directly — `export default app`
 * works without a wrapping `{ fetch: ... }` adapter.
 */
export type Router<E extends Env = Env> = {
  readonly __inner: Hono<{ Bindings: E }>;
  fetch: Hono<{ Bindings: E }>["fetch"];
};

export type Handler<E extends Env = Env> = (
  c: Context<{ Bindings: E }>,
) => Response | Promise<Response>;

export function router<E extends Env = Env>(): Router<E> {
  const inner = new Hono<{ Bindings: E }>();
  return { __inner: inner, fetch: inner.fetch.bind(inner) };
}

export function get<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.__inner.get(path, handler);
  return r;
}

export function post<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.__inner.post(path, handler);
  return r;
}

export function put<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.__inner.put(path, handler);
  return r;
}

export function patch<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.__inner.patch(path, handler);
  return r;
}

// `delete` is reserved in JS/TS identifier position, so this function is
// named `del`. Floe users write `del(router, "/x", handler)`.
export function del<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.__inner.delete(path, handler);
  return r;
}

export function handle<E extends Env>(
  r: Router<E>,
  request: Request,
  env: E,
): Response | Promise<Response> {
  return r.__inner.fetch(request, env);
}
