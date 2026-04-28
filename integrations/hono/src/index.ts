import {
  Hono,
  type Context,
  type ErrorHandler,
  type MiddlewareHandler,
  type NotFoundHandler,
} from "hono";

type Env = Record<string, unknown>;

/**
 * Pipe-friendly alias for Hono. Floe code calls standalone helpers
 * (`get`, `post`, `route`, …) on a `Router<E>`, which is just a real
 * `Hono<{ Bindings: E }>` so it interops cleanly with stock Hono code:
 *
 * ```ts
 * const app = new Hono();
 * app.route("/x", createFloeProvider());   // works — no wrapper unwrap
 * ```
 *
 * Earlier versions wrapped Hono in `{ __inner, fetch }` to "abstract
 * over internals." That broke `app.route(path, sub)` because stock
 * Hono reads `sub.routes` and the wrapper hid it. The wrapper bought
 * nothing real and cost interop, so it's gone.
 */
export type Router<E extends Env = Env> = Hono<{ Bindings: E }>;

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
  return new Hono<{ Bindings: E }>();
}

export function get<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.get(path, handler);
  return r;
}

export function post<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.post(path, handler);
  return r;
}

export function put<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.put(path, handler);
  return r;
}

export function patch<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.patch(path, handler);
  return r;
}

// `delete` is reserved in JS/TS identifier position, so this function is
// named `del`. Floe users write `del(router, "/x", handler)`.
export function del<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.delete(path, handler);
  return r;
}

export function all<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.all(path, handler);
  return r;
}

export function on<E extends Env>(
  r: Router<E>,
  method: string,
  path: string,
  handler: Handler<E>,
): Router<E> {
  r.on(method, path, handler);
  return r;
}

export function use<E extends Env>(
  r: Router<E>,
  path: string,
  handler: Middleware<E>,
): Router<E> {
  r.use(path, handler);
  return r;
}

export function route<E extends Env>(
  r: Router<E>,
  path: string,
  sub: Router<E>,
): Router<E> {
  r.route(path, sub);
  return r;
}

export function basePath<E extends Env>(
  r: Router<E>,
  path: string,
): Router<E> {
  return r.basePath(path) as Router<E>;
}

export function onError<E extends Env>(
  r: Router<E>,
  handler: ErrorResponder<E>,
): Router<E> {
  r.onError(handler);
  return r;
}

export function notFound<E extends Env>(
  r: Router<E>,
  handler: NotFoundResponder<E>,
): Router<E> {
  r.notFound(handler);
  return r;
}

export function mount<E extends Env>(
  r: Router<E>,
  path: string,
  handler: MountApplicationHandler,
  options?: MountOptions,
): Router<E> {
  r.mount(path, handler, options);
  return r;
}

export function handle<E extends Env>(
  r: Router<E>,
  request: Request,
  env: E,
): Response | Promise<Response> {
  return r.fetch(request, env);
}

export function request<E extends Env>(
  r: Router<E>,
  input: Request | string | URL,
  init?: RequestInit,
  env?: E,
): Response | Promise<Response> {
  return r.request(input, init, env);
}
