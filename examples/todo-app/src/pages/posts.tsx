import { Suspense, useState } from "react";
import {
  QueryClient,
  QueryClientProvider,
  QueryErrorResetBoundary,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { ErrorBoundary } from "react-error-boundary";

type Post = {
  id: number;
  title: string;
  body: string;
  userId: number;
};

type User = {
  id: number;
  name: string;
  email: string;
  company: { name: string };
};

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 60 * 1000,
      retry: 1,
    },
  },
});

function PostAuthor({ userId }: { userId: number }) {
  const { data: user } = useSuspenseQuery<User>({
    queryKey: ["user", userId],
    queryFn: async () => {
      const res = await fetch(
        `https://jsonplaceholder.typicode.com/users/${userId}`,
      );
      if (!res.ok) throw new Error("Failed to fetch user");
      return res.json();
    },
  });

  return (
    <span className="text-indigo-400">
      {user.name} ({user.company.name})
    </span>
  );
}

function PostList() {
  const [selectedUserId, setSelectedUserId] = useState<number | null>(null);

  const { data: posts } = useSuspenseQuery<Post[]>({
    queryKey: ["posts", selectedUserId],
    queryFn: async () => {
      const url = selectedUserId
        ? `https://jsonplaceholder.typicode.com/posts?userId=${selectedUserId}`
        : "https://jsonplaceholder.typicode.com/posts?_limit=10";
      const res = await fetch(url);
      if (!res.ok) throw new Error("Failed to fetch posts");
      return res.json();
    },
  });

  return (
    <div>
      <div className="mb-6 flex flex-wrap gap-2">
        <button
          onClick={() => setSelectedUserId(null)}
          className={`rounded px-3 py-1 text-sm transition-colors ${
            selectedUserId === null
              ? "bg-indigo-600 text-white"
              : "bg-zinc-800 text-zinc-400 hover:text-zinc-200"
          }`}
        >
          All
        </button>
        {[1, 2, 3, 4, 5].map((id) => (
          <button
            key={id}
            onClick={() => setSelectedUserId(id)}
            className={`rounded px-3 py-1 text-sm transition-colors ${
              selectedUserId === id
                ? "bg-indigo-600 text-white"
                : "bg-zinc-800 text-zinc-400 hover:text-zinc-200"
            }`}
          >
            User {id}
          </button>
        ))}
      </div>

      <div className="space-y-4">
        {posts.map((post) => (
          <article
            key={post.id}
            className="rounded-lg border border-zinc-800 bg-zinc-900/50 p-5"
          >
            <h3 className="mb-1 text-lg font-semibold capitalize text-zinc-100">
              {post.title}
            </h3>
            <Suspense
              fallback={
                <span className="text-sm text-zinc-600">Loading author...</span>
              }
            >
              <p className="mb-3 text-sm">
                by <PostAuthor userId={post.userId} />
              </p>
            </Suspense>
            <p className="text-sm leading-relaxed text-zinc-400">{post.body}</p>
          </article>
        ))}
      </div>
    </div>
  );
}

function LoadingSkeleton() {
  return (
    <div className="space-y-4">
      {Array.from({ length: 3 }).map((_, i) => (
        <div
          key={i}
          className="animate-pulse rounded-lg border border-zinc-800 bg-zinc-900/50 p-5"
        >
          <div className="mb-2 h-5 w-3/4 rounded bg-zinc-800" />
          <div className="mb-3 h-4 w-1/4 rounded bg-zinc-800" />
          <div className="space-y-2">
            <div className="h-3 w-full rounded bg-zinc-800" />
            <div className="h-3 w-5/6 rounded bg-zinc-800" />
          </div>
        </div>
      ))}
    </div>
  );
}

export function PostsPage() {
  return (
    <QueryClientProvider client={queryClient}>
      <h1 className="mb-2 text-3xl font-bold">Posts</h1>
      <p className="mb-6 text-zinc-400">
        TanStack Query + Suspense demo using JSONPlaceholder API.
      </p>

      <QueryErrorResetBoundary>
        {({ reset }) => (
          <ErrorBoundary
            onReset={reset}
            fallbackRender={({ resetErrorBoundary, error }) => (
              <div className="rounded-lg border border-red-900/50 bg-red-950/30 p-6 text-center">
                <p className="mb-3 text-red-400">
                  Failed to load posts: {error.message}
                </p>
                <button
                  onClick={resetErrorBoundary}
                  className="rounded bg-red-600 px-4 py-2 text-sm text-white hover:bg-red-500"
                >
                  Retry
                </button>
              </div>
            )}
          >
            <Suspense fallback={<LoadingSkeleton />}>
              <PostList />
            </Suspense>
          </ErrorBoundary>
        )}
      </QueryErrorResetBoundary>
    </QueryClientProvider>
  );
}
