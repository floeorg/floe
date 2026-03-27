import {
  createRouter,
  createRoute,
  createRootRoute,
  Link,
  Outlet,
} from "@tanstack/react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StoreProvider, useStore } from "./store-context";
import { CatalogPage } from "./pages/catalog";
import { ProductDetailPage } from "./pages/product-detail";
import { CartPage } from "./pages/cart";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 60000,
      retry: 1,
    },
  },
});

function RootComponent() {
  const { itemCount } = useStore();

  return (
    <QueryClientProvider client={queryClient}>
      <div className="flex h-screen flex-col bg-zinc-950 text-zinc-100">
        <nav className="shrink-0 border-b border-zinc-800 px-6 py-4">
          <div className="mx-auto flex max-w-6xl items-center gap-6">
            <span className="text-lg font-bold text-indigo-400">
              Floe Store (TS)
            </span>
            <Link
              to="/"
              className="text-zinc-400 hover:text-zinc-100 transition-colors [&.active]:text-zinc-100"
            >
              Catalog
            </Link>
            <Link
              to="/cart"
              className="ml-auto flex items-center gap-2 text-zinc-400 hover:text-zinc-100 transition-colors [&.active]:text-zinc-100"
            >
              Cart
              {itemCount > 0 && (
                <span className="rounded-full bg-indigo-600 px-2 py-0.5 text-xs text-white">
                  {itemCount}
                </span>
              )}
            </Link>
          </div>
        </nav>
        <main className="mx-auto max-w-6xl flex-1 overflow-y-auto px-6 py-8">
          <Outlet />
        </main>
      </div>
    </QueryClientProvider>
  );
}

function RootWrapper() {
  return (
    <StoreProvider>
      <RootComponent />
    </StoreProvider>
  );
}

const rootRoute = createRootRoute({
  component: RootWrapper,
});

const catalogRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: () => {
    const { addToCart } = useStore();
    return <CatalogPage onAddToCart={addToCart} />;
  },
});

const productRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/product/$productId",
  component: () => {
    const { productId } = productRoute.useParams();
    const { addToCart } = useStore();
    return (
      <ProductDetailPage
        productId={Number(productId)}
        onAddToCart={addToCart}
      />
    );
  },
});

const cartRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/cart",
  component: () => {
    const { cart, updateQty, removeFromCart } = useStore();
    return (
      <CartPage
        cart={cart}
        onUpdateQty={updateQty}
        onRemove={removeFromCart}
      />
    );
  },
});

const routeTree = rootRoute.addChildren([
  catalogRoute,
  productRoute,
  cartRoute,
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
