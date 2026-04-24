import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
  router,
  get,
  post,
  put,
  patch,
  del,
  all,
  on,
  use,
  route,
  basePath,
  onError,
  notFound,
  mount,
  handle,
  request,
  type Handler,
} from "./index.ts";

type TestEnv = { readonly greeting: string };

describe("@floeorg/hono shim", () => {
  it("router() returns a fresh Router wrapping a Hono instance", () => {
    const r = router<TestEnv>();
    assert.ok(r.__inner);
    assert.equal(typeof r.__inner.fetch, "function");
  });

  it("router() exposes fetch at the top level so it works as a Workers default export", async () => {
    const app = get(router<TestEnv>(), "/ping", () => new Response("pong"));
    assert.equal(typeof app.fetch, "function");
    const res = await app.fetch(new Request("http://local/ping"), { greeting: "unused" });
    assert.equal(await res.text(), "pong");
  });

  it("get + post register routes and handle() dispatches by method and path", async () => {
    const app = post(
      get(router<TestEnv>(), "/hello", (c) => new Response(c.env.greeting)),
      "/echo",
      async (c) => new Response(await c.req.text()),
    );

    const helloRes = await handle(
      app,
      new Request("http://local/hello"),
      { greeting: "world" },
    );
    assert.equal(helloRes.status, 200);
    assert.equal(await helloRes.text(), "world");

    const echoRes = await handle(
      app,
      new Request("http://local/echo", { method: "POST", body: "ping" }),
      { greeting: "unused" },
    );
    assert.equal(await echoRes.text(), "ping");
  });

  it("unknown routes yield a 404", async () => {
    const app = get(router<TestEnv>(), "/hello", () => new Response("ok"));
    const res = await handle(
      app,
      new Request("http://local/missing"),
      { greeting: "unused" },
    );
    assert.equal(res.status, 404);
  });

  it("each helper returns the same Router identity so chaining is lossless", () => {
    const r = router<TestEnv>();
    const noop: Handler<TestEnv> = () => new Response();
    assert.equal(get(r, "/", noop), r);
    assert.equal(post(r, "/", noop), r);
    assert.equal(put(r, "/", noop), r);
    assert.equal(patch(r, "/", noop), r);
    assert.equal(del(r, "/", noop), r);
    assert.equal(all(r, "/a", noop), r);
    assert.equal(on(r, "PURGE", "/o", noop), r);
    assert.equal(use(r, "*", async (_c, next) => { await next(); }), r);
    assert.equal(route(r, "/sub", router<TestEnv>()), r);
    assert.equal(onError(r, (err) => new Response(err.message, { status: 500 })), r);
    assert.equal(notFound(r, () => new Response("nope", { status: 404 })), r);
    assert.equal(mount(r, "/ext", () => new Response("mounted")), r);
  });

  it("put / patch / del dispatch correctly", async () => {
    const app = del(
      patch(
        put(router<TestEnv>(), "/u", () => new Response("u")),
        "/p",
        () => new Response("p"),
      ),
      "/d",
      () => new Response("d"),
    );

    const u = await handle(app, new Request("http://local/u", { method: "PUT" }), { greeting: "" });
    const p = await handle(app, new Request("http://local/p", { method: "PATCH" }), { greeting: "" });
    const d = await handle(app, new Request("http://local/d", { method: "DELETE" }), { greeting: "" });

    assert.equal(await u.text(), "u");
    assert.equal(await p.text(), "p");
    assert.equal(await d.text(), "d");
  });

  it("routes see the env object passed to handle()", async () => {
    const app = get(router<TestEnv>(), "/env", (c) => new Response(c.env.greeting));
    const res = await handle(app, new Request("http://local/env"), { greeting: "hey" });
    assert.equal(await res.text(), "hey");
  });

  it("use() registers middleware that runs before the handler and can short-circuit", async () => {
    const trail: string[] = [];
    const app = get(
      use(router<TestEnv>(), "*", async (_c, next) => {
        trail.push("before");
        await next();
        trail.push("after");
      }),
      "/x",
      () => {
        trail.push("handler");
        return new Response("ok");
      },
    );
    const res = await handle(app, new Request("http://local/x"), { greeting: "" });
    assert.equal(await res.text(), "ok");
    assert.deepEqual(trail, ["before", "handler", "after"]);

    const gate = use(router<TestEnv>(), "*", () => new Response("denied", { status: 401 }));
    get(gate, "/private", () => new Response("secret"));
    const blocked = await handle(gate, new Request("http://local/private"), { greeting: "" });
    assert.equal(blocked.status, 401);
    assert.equal(await blocked.text(), "denied");
  });

  it("all() matches every HTTP method on the same path", async () => {
    const app = all(router<TestEnv>(), "/any", (c) => new Response(c.req.method));
    for (const method of ["GET", "POST", "PUT", "DELETE"]) {
      const res = await handle(app, new Request("http://local/any", { method }), { greeting: "" });
      assert.equal(await res.text(), method);
    }
  });

  it("on() matches custom HTTP methods", async () => {
    const app = on(router<TestEnv>(), "PURGE", "/cache", () => new Response("purged"));
    const res = await handle(app, new Request("http://local/cache", { method: "PURGE" }), { greeting: "" });
    assert.equal(await res.text(), "purged");
  });

  it("route() mounts a sub-router under a path prefix", async () => {
    const users = get(router<TestEnv>(), "/", () => new Response("list"));
    get(users, "/:id", (c) => new Response(`user ${c.req.param("id")}`));

    const app = route(router<TestEnv>(), "/users", users);
    const list = await handle(app, new Request("http://local/users"), { greeting: "" });
    assert.equal(await list.text(), "list");
    const one = await handle(app, new Request("http://local/users/42"), { greeting: "" });
    assert.equal(await one.text(), "user 42");
  });

  it("basePath() prefixes every route added after it", async () => {
    const app = get(basePath(router<TestEnv>(), "/api"), "/health", () => new Response("ok"));
    const hit = await handle(app, new Request("http://local/api/health"), { greeting: "" });
    assert.equal(await hit.text(), "ok");
    const miss = await handle(app, new Request("http://local/health"), { greeting: "" });
    assert.equal(miss.status, 404);
  });

  it("onError() catches thrown errors from handlers", async () => {
    const app = onError(
      get(router<TestEnv>(), "/boom", () => {
        throw new Error("kaboom");
      }),
      (err) => new Response(`caught: ${err.message}`, { status: 500 }),
    );
    const res = await handle(app, new Request("http://local/boom"), { greeting: "" });
    assert.equal(res.status, 500);
    assert.equal(await res.text(), "caught: kaboom");
  });

  it("notFound() customizes the response for unmatched paths", async () => {
    const app = notFound(
      get(router<TestEnv>(), "/real", () => new Response("ok")),
      () => new Response("custom 404", { status: 404 }),
    );
    const res = await handle(app, new Request("http://local/missing"), { greeting: "" });
    assert.equal(res.status, 404);
    assert.equal(await res.text(), "custom 404");
  });

  it("mount() forwards requests to a non-Hono handler", async () => {
    const app = mount(
      router<TestEnv>(),
      "/ext",
      (req) => new Response(`mounted ${new URL(req.url).pathname}`),
    );
    const res = await handle(app, new Request("http://local/ext/anything"), { greeting: "" });
    assert.equal(await res.text(), "mounted /anything");
  });

  it("request() is a test helper that dispatches without a real HTTP server", async () => {
    const app = get(router<TestEnv>(), "/hi", () => new Response("hey"));
    const res = await request(app, "/hi");
    assert.equal(await res.text(), "hey");
  });
});
