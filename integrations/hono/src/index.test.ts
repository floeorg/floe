import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { router, get, post, put, patch, del, handle, type Handler } from "./index.ts";

type TestEnv = { readonly greeting: string };

describe("@floeorg/hono shim", () => {
  it("router() returns a fresh Router wrapping a Hono instance", () => {
    const r = router<TestEnv>();
    assert.ok(r.__inner);
    assert.equal(typeof r.__inner.fetch, "function");
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
});
