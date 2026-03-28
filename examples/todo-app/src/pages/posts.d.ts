import { useState, Suspense } from "react";
import { useSuspenseQuery, QueryClient, QueryClientProvider, QueryErrorResetBoundary } from "@tanstack/react-query";
import { ErrorBoundary } from "react-error-boundary";
type Post = { id: number; title: string; body: string; userId: number };
type User = { id: number; name: string; email: string; company: { name: string } };
export declare function PostsPage(): JSX.Element;
