import {
  Hono,
  type Context,
  type ErrorHandler,
  type MiddlewareHandler,
  type NotFoundHandler,
} from "hono";

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

export type Middleware<E extends Env = Env> = MiddlewareHandler<{
  Bindings: E;
}>;

export type ErrorResponder<E extends Env = Env> = ErrorHandler<{
  Bindings: E;
}>;

export type NotFoundResponder<E extends Env = Env> = NotFoundHandler<{
  Bindings: E;
}>;

type MountApplicationHandler = Parameters<Hono["mount"]>[1];
type MountOptions = Parameters<Hono["mount"]>[2];

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

export function all<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.__inner.all(path, handler);
  return r;
}

export function on<E extends Env>(
  r: Router<E>,
  method: string,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.__inner.on(method, path, handler);
  return r;
}

export function use<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Middleware<E>,
): Router<E> {
  r.__inner.use(path, handler);
  return r;
}

export function route<E extends Env>(
  r: Router<E>,
  path: string,
  sub: Router<E>,
): Router<E> {
  r.__inner.route(path, sub.__inner);
  return r;
}

export function basePath<E extends Env>(
  r: Router<E>,
  path: string,
): Router<E> {
  const rebased = r.__inner.basePath(path) as Hono<{ Bindings: E }>;
  return { __inner: rebased, fetch: rebased.fetch.bind(rebased) };
}

export function onError<E extends Env>(
  r: Router<E>,
  handler: ErrorResponder<E>,
): Router<E> {
  r.__inner.onError(handler);
  return r;
}

export function notFound<E extends Env>(
  r: Router<E>,
  handler: NotFoundResponder<E>,
): Router<E> {
  r.__inner.notFound(handler);
  return r;
}

export function mount<E extends Env>(
  r: Router<E>,
  path: string,
  handler: MountApplicationHandler,
  options?: MountOptions,
): Router<E> {
  r.__inner.mount(path, handler, options);
  return r;
}

export function handle<E extends Env>(
  r: Router<E>,
  request: Request,
  env: E,
): Response | Promise<Response> {
  return r.__inner.fetch(request, env);
}

export function request<E extends Env>(
  r: Router<E>,
  input: Request | string | URL,
  init?: RequestInit,
  env?: E,
): Response | Promise<Response> {
  return r.__inner.request(input, init, env);
}
