import { useState, Suspense } from "react";
import { useSuspenseQuery, QueryErrorResetBoundary } from "@tanstack/react-query";
import { ErrorBoundary } from "react-error-boundary";
import { Link } from "@tanstack/react-router";
import type { Product, SortOrder, PriceRange } from "../types";
import {
  effectivePrice,
  inStock,
  stockLabel,
  ratingStars,
  sortProducts,
  matchesPrice,
  formatPrice,
} from "../product";
import { fetchProducts, fetchCategories, type CategoryResponse } from "../api";

// ── Product card ───────────────────────────────────────

function ProductCard(props: {
  product: Product;
  onAddToCart: (product: Product) => void;
}) {
  const product = props.product;
  const hasDiscount = product.discountPercentage > 0;

  return (
    <div className="group rounded-xl border border-zinc-800 bg-zinc-900/50 p-4 transition-all hover:border-zinc-700 hover:bg-zinc-900">
      <Link to="/product/$productId" params={{ productId: String(product.id) }} className="block">
        <img
          src={product.thumbnail}
          alt={product.title}
          className="mb-3 h-48 w-full rounded-lg object-cover"
        />
      </Link>
      <div className="mb-2 flex items-start justify-between gap-2">
        <Link
          to="/product/$productId"
          params={{ productId: String(product.id) }}
          className="font-semibold text-zinc-100 line-clamp-2 hover:text-indigo-300 transition-colors"
        >
          {product.title}
        </Link>
        <span className="shrink-0 rounded-full bg-zinc-800 px-2 py-0.5 text-xs text-zinc-400">
          {product.category}
        </span>
      </div>

      <div className="mb-2 flex items-center gap-2">
        <span className="text-lg font-bold text-indigo-400">
          {formatPrice(effectivePrice(product))}
        </span>
        {hasDiscount && (
          <span className="text-sm text-zinc-500 line-through">
            {formatPrice(product.price)}
          </span>
        )}
        {hasDiscount && (
          <span className="rounded bg-emerald-900/50 px-1.5 py-0.5 text-xs text-emerald-400">
            {`-${Math.round(product.discountPercentage)}%`}
          </span>
        )}
      </div>

      <div className="mb-3 flex items-center gap-2 text-sm">
        <span className="text-amber-400">{ratingStars(product)}</span>
        <span className="text-zinc-500">{`${product.rating}/5`}</span>
        <span
          className={
            product.stock === 0
              ? "ml-auto text-red-400"
              : product.stock < 5
                ? "ml-auto text-amber-400"
                : "ml-auto text-zinc-500"
          }
        >
          {stockLabel(product)}
        </span>
      </div>

      <button
        onClick={() => props.onAddToCart(product)}
        disabled={!inStock(product)}
        className={
          inStock(product)
            ? "w-full rounded-lg bg-indigo-600 py-2 text-sm font-medium text-white transition-colors hover:bg-indigo-500"
            : "w-full rounded-lg bg-zinc-800 py-2 text-sm font-medium text-zinc-600 cursor-not-allowed"
        }
      >
        {inStock(product) ? "Add to Cart" : "Out of Stock"}
      </button>
    </div>
  );
}

// ── Filter sidebar ─────────────────────────────────────

function FilterSidebar(props: {
  categories: CategoryResponse[];
  selectedCategory: string;
  onCategoryChange: (cat: string) => void;
  priceRange: PriceRange;
  onPriceRangeChange: (range: PriceRange) => void;
  sortOrder: SortOrder;
  onSortChange: (order: SortOrder) => void;
}) {
  const priceRanges: [string, PriceRange][] = [
    ["All Prices", { tag: "Any" }],
    ["Under $25", { tag: "Under", max: 25 }],
    ["$25 - $50", { tag: "Between", min: 25, max: 50 }],
    ["$50 - $100", { tag: "Between", min: 50, max: 100 }],
    ["$100 - $500", { tag: "Between", min: 100, max: 500 }],
    ["Over $500", { tag: "Over", min: 500 }],
  ];

  const sortOptions: [string, SortOrder][] = [
    ["Price: Low to High", "PriceLow"],
    ["Price: High to Low", "PriceHigh"],
    ["Top Rated", "Rating"],
    ["Name", "Name"],
  ];

  const activeBtn =
    "block w-full rounded px-3 py-1.5 text-left text-sm bg-indigo-600/20 text-indigo-300";
  const inactiveBtn =
    "block w-full rounded px-3 py-1.5 text-left text-sm text-zinc-400 hover:bg-zinc-800 hover:text-zinc-200";

  return (
    <aside className="w-64 shrink-0 space-y-6 sticky top-0 self-start max-h-screen overflow-y-auto">
      <div>
        <h3 className="mb-3 text-sm font-semibold uppercase tracking-wider text-zinc-400">
          Category
        </h3>
        <div className="space-y-1">
          <button
            onClick={() => props.onCategoryChange("")}
            className={props.selectedCategory === "" ? activeBtn : inactiveBtn}
          >
            All
          </button>
          {props.categories.map((cat) => (
            <button
              key={cat.slug}
              onClick={() => props.onCategoryChange(cat.slug)}
              className={
                props.selectedCategory === cat.slug ? activeBtn : inactiveBtn
              }
            >
              {cat.name}
            </button>
          ))}
        </div>
      </div>

      <div>
        <h3 className="mb-3 text-sm font-semibold uppercase tracking-wider text-zinc-400">
          Price
        </h3>
        <div className="space-y-1">
          {priceRanges.map(([label, range]) => (
            <button
              key={label}
              onClick={() => props.onPriceRangeChange(range)}
              className={
                JSON.stringify(props.priceRange) === JSON.stringify(range)
                  ? activeBtn
                  : inactiveBtn
              }
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div>
        <h3 className="mb-3 text-sm font-semibold uppercase tracking-wider text-zinc-400">
          Sort By
        </h3>
        <div className="space-y-1">
          {sortOptions.map(([label, order]) => (
            <button
              key={label}
              onClick={() => props.onSortChange(order)}
              className={props.sortOrder === order ? activeBtn : inactiveBtn}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
    </aside>
  );
}

// ── Category loader ────────────────────────────────────

function CategoryList(props: {
  selectedCategory: string;
  onCategoryChange: (cat: string) => void;
  priceRange: PriceRange;
  onPriceRangeChange: (range: PriceRange) => void;
  sortOrder: SortOrder;
  onSortChange: (order: SortOrder) => void;
}) {
  const { data } = useSuspenseQuery({
    queryKey: ["categories"],
    queryFn: () => fetchCategories(),
  });

  const categories = data.ok ? data.value : [];

  return (
    <FilterSidebar
      categories={categories}
      selectedCategory={props.selectedCategory}
      onCategoryChange={props.onCategoryChange}
      priceRange={props.priceRange}
      onPriceRangeChange={props.onPriceRangeChange}
      sortOrder={props.sortOrder}
      onSortChange={props.onSortChange}
    />
  );
}

// ── Product grid ───────────────────────────────────────

function ProductGrid(props: {
  category: string;
  search: string;
  sortOrder: SortOrder;
  priceRange: PriceRange;
  onAddToCart: (product: Product) => void;
}) {
  const { data } = useSuspenseQuery({
    queryKey: ["products", props.category, props.search],
    queryFn: () => fetchProducts(props.category, props.search, 30),
  });

  const [products] = data.ok ? data.value : [[] as Product[], 0];

  const filtered = products.filter((p) => matchesPrice(p, props.priceRange));
  const sorted = sortProducts(filtered, props.sortOrder);

  return (
    <div>
      <p className="mb-4 text-sm text-zinc-500">{`${sorted.length} products`}</p>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {sorted.map((product) => (
          <ProductCard
            key={product.id}
            product={product}
            onAddToCart={props.onAddToCart}
          />
        ))}
      </div>
    </div>
  );
}

// ── Skeletons ──────────────────────────────────────────

function CatalogSkeleton() {
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {[1, 2, 3, 4, 5, 6].map((i) => (
        <div
          key={i}
          className="animate-pulse rounded-xl border border-zinc-800 bg-zinc-900/50 p-4"
        >
          <div className="mb-3 h-48 rounded-lg bg-zinc-800" />
          <div className="mb-2 h-5 w-3/4 rounded bg-zinc-800" />
          <div className="mb-2 h-6 w-1/3 rounded bg-zinc-800" />
          <div className="mb-3 h-4 w-1/2 rounded bg-zinc-800" />
          <div className="h-10 rounded-lg bg-zinc-800" />
        </div>
      ))}
    </div>
  );
}

function SidebarSkeleton() {
  return (
    <div className="w-64 shrink-0 animate-pulse space-y-4">
      <div className="h-4 w-20 rounded bg-zinc-800" />
      <div className="space-y-2">
        <div className="h-8 rounded bg-zinc-800" />
        <div className="h-8 rounded bg-zinc-800" />
        <div className="h-8 rounded bg-zinc-800" />
      </div>
    </div>
  );
}

// ── Main catalog page ──────────────────────────────────

export function CatalogPage(props: {
  onAddToCart: (product: Product) => void;
}) {
  const [category, setCategory] = useState("");
  const [search, setSearch] = useState("");
  const [sortOrder, setSortOrder] = useState<SortOrder>("Rating");
  const [priceRange, setPriceRange] = useState<PriceRange>({ tag: "Any" });

  return (
    <div>
      <div className="mb-6 flex items-center gap-4">
        <h1 className="text-3xl font-bold">Store</h1>
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search products..."
          className="flex-1 rounded-lg bg-zinc-800 px-4 py-2 text-zinc-100 placeholder-zinc-500 outline-none ring-1 ring-zinc-700 focus:ring-indigo-500"
        />
      </div>

      <div className="flex gap-8">
        <Suspense fallback={<SidebarSkeleton />}>
          <CategoryList
            selectedCategory={category}
            onCategoryChange={setCategory}
            priceRange={priceRange}
            onPriceRangeChange={setPriceRange}
            sortOrder={sortOrder}
            onSortChange={setSortOrder}
          />
        </Suspense>

        <div className="flex-1">
          <QueryErrorResetBoundary>
            {({ reset }) => (
              <ErrorBoundary
                onReset={reset}
                fallbackRender={({ resetErrorBoundary, error }) => (
                  <div className="rounded-lg border border-red-900/50 bg-red-950/30 p-6 text-center">
                    <p className="mb-3 text-red-400">{`Failed to load products: ${(error as Error).message}`}</p>
                    <button
                      onClick={resetErrorBoundary}
                      className="rounded bg-red-600 px-4 py-2 text-sm text-white hover:bg-red-500"
                    >
                      Retry
                    </button>
                  </div>
                )}
              >
                <Suspense fallback={<CatalogSkeleton />}>
                  <ProductGrid
                    category={category}
                    search={search}
                    sortOrder={sortOrder}
                    priceRange={priceRange}
                    onAddToCart={props.onAddToCart}
                  />
                </Suspense>
              </ErrorBoundary>
            )}
          </QueryErrorResetBoundary>
        </div>
      </div>
    </div>
  );
}
