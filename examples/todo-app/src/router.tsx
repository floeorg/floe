import {
  createRouter,
  createRoute,
  createRootRoute,
  Link,
  Outlet,
} from "@tanstack/react-router";
import { HomePage } from "./pages/home";
import { AboutPage } from "./pages/about";
import { PostsPage } from "./pages/posts";

const rootRoute = createRootRoute({
  component: () => (
    <div className="min-h-screen bg-zinc-950 text-zinc-100">
      <nav className="border-b border-zinc-800 px-6 py-4">
        <div className="mx-auto flex max-w-2xl items-center gap-6">
          <span className="text-lg font-bold text-indigo-400">Floe</span>
          <Link
            to="/"
            className="text-zinc-400 hover:text-zinc-100 transition-colors [&.active]:text-zinc-100"
          >
            Todos
          </Link>
          <Link
            to="/about"
            className="text-zinc-400 hover:text-zinc-100 transition-colors [&.active]:text-zinc-100"
          >
            About
          </Link>
          <Link
            to="/posts"
            className="text-zinc-400 hover:text-zinc-100 transition-colors [&.active]:text-zinc-100"
          >
            Posts
          </Link>
        </div>
      </nav>
      <main className="mx-auto max-w-2xl px-6 py-8">
        <Outlet />
      </main>
    </div>
  ),
});

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: HomePage,
});

const aboutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/about",
  component: AboutPage,
});

const postsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/posts",
  component: PostsPage,
});

const routeTree = rootRoute.addChildren([homeRoute, aboutRoute, postsRoute]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
