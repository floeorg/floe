import { Suspense } from "react";

import { useSuspenseQuery, QueryErrorResetBoundary } from "@tanstack/react-query";

import { ErrorBoundary } from "react-error-boundary";

import { Link } from "@tanstack/react-router";

import { type Product, type ProductId, type Review } from "../types";

import { formatPrice, display, effectivePrice, savings, inStock, stockLabel, ratingStars } from "../product";

import { fetchProduct } from "../api";

function Stars(props: { rating: number }): JSX.Element {
  const filled = Math.floor(props.rating);
  const filledStars = "*".repeat(filled);
  const emptyStars = "*".repeat(5 - filled);
  return <div className={"flex gap-0.5"}><span className={"text-amber-400"}>{filledStars}</span><span className={"text-zinc-700"}>{emptyStars}</span></div>;
}

function ReviewCard(props: { review: Review }): JSX.Element {
  const review = props.review;
  return <div className={"rounded-lg border border-zinc-800 bg-zinc-900/50 p-4"}><div className={"mb-2 flex items-center justify-between"}><span className={"font-medium text-zinc-200"}>{review.reviewerName}</span><Stars rating={review.rating} /></div><p className={"text-sm text-zinc-400"}>{review.comment}</p><p className={"mt-2 text-xs text-zinc-600"}>{review.date}</p></div>;
}

function ProductDetailContent(props: { productId: ProductId; onAddToCart: (_p0: Product) => void }): JSX.Element {
  const { data } = useSuspenseQuery({ queryKey: ["product", props.productId], queryFn: async () => fetchProduct(props.productId) });
  const [product, reviews] = data.ok === true ? (() => { const result = data.value; return result; })() : data.ok === false ? (() => { throw new Error("unreachable"); })() : (() => { throw new Error("non-exhaustive match"); })();
  const hasDiscount = product.discountPercentage > 0;
  return <div><div className={"mb-8 flex gap-8"}><div className={"w-1/2"}><img src={product.thumbnail} alt={product.title} className={"w-full rounded-xl object-cover"} /><div className={"mt-3 flex gap-2 overflow-x-auto"}>{product.images.map((img) => <img key={img} src={img} alt={product.title} className={"h-20 w-20 shrink-0 rounded-lg object-cover border border-zinc-800"} />)}</div></div><div className={"w-1/2"}><span className={"mb-2 inline-block rounded-full bg-zinc-800 px-3 py-1 text-xs text-zinc-400"}>{product.category}</span><h1 className={"mb-2 text-3xl font-bold text-zinc-100"}>{product.title}</h1>{product.brand === "" ? <span /> : (() => { const brand = product.brand; return <p className={"mb-4 text-sm text-zinc-500"}>{`by ${brand}`}</p>; })()}<div className={"mb-4 flex items-center gap-3"}><Stars rating={product.rating} /><span className={"text-sm text-zinc-400"}>{`${product.rating}/5`}</span><span className={"text-sm text-zinc-600"}>{`(${reviews.length} reviews)`}</span></div><div className={"mb-4 flex items-baseline gap-3"}><span className={"text-3xl font-bold text-indigo-400"}>{formatPrice(effectivePrice(product))}</span>{hasDiscount === true ? <span className={"text-lg text-zinc-500 line-through"}>{formatPrice(product.price)}</span> : hasDiscount === false ? <span /> : (() => { throw new Error("non-exhaustive match"); })()}{hasDiscount === true ? <span className={"rounded bg-emerald-900/50 px-2 py-1 text-sm text-emerald-400"}>{`Save ${formatPrice(savings(product))}`}</span> : hasDiscount === false ? <span /> : (() => { throw new Error("non-exhaustive match"); })()}</div><p className={"mb-6 text-zinc-400"}>{product.description}</p><div className={"mb-6 flex items-center gap-3"}><span className={product.stock === 0 ? "rounded-full bg-red-900/30 px-3 py-1 text-sm text-red-400" : true ? (() => { const n = product.stock; if (n < 5) { return "rounded-full bg-amber-900/30 px-3 py-1 text-sm text-amber-400"; } return "rounded-full bg-emerald-900/30 px-3 py-1 text-sm text-emerald-400"; })() : "rounded-full bg-emerald-900/30 px-3 py-1 text-sm text-emerald-400"}>{stockLabel(product)}</span></div><div className={"mb-6 flex flex-wrap gap-2"}>{product.tags.map((tag) => <span key={tag} className={"rounded-full bg-zinc-800 px-2.5 py-1 text-xs text-zinc-400"}>{tag}</span>)}</div><button onClick={() => props.onAddToCart(product)} disabled={!(inStock(product))} className={inStock(product) === true ? "w-full rounded-xl bg-indigo-600 py-3 text-lg font-semibold text-white transition-colors hover:bg-indigo-500" : inStock(product) === false ? "w-full rounded-xl bg-zinc-800 py-3 text-lg font-semibold text-zinc-600 cursor-not-allowed" : (() => { throw new Error("non-exhaustive match"); })()}>{inStock(product) === true ? "Add to Cart" : inStock(product) === false ? "Out of Stock" : (() => { throw new Error("non-exhaustive match"); })()}</button></div></div><div><h2 className={"mb-4 text-xl font-semibold"}>Reviews</h2><div className={"space-y-3"}>{reviews.length === 0 ? <p className={"text-zinc-500"}>No reviews yet.</p> : (() => { const revs = reviews; return <div className={"space-y-3"}>{revs.map((review) => <ReviewCard key={review.reviewerName} review={review} />)}</div>; })()}</div></div></div>;
}

function DetailSkeleton(): JSX.Element {
  return <div className={"animate-pulse"}><div className={"mb-8 flex gap-8"}><div className={"w-1/2"}><div className={"h-80 rounded-xl bg-zinc-800"} /></div><div className={"w-1/2 space-y-4"}><div className={"h-4 w-20 rounded bg-zinc-800"} /><div className={"h-8 w-3/4 rounded bg-zinc-800"} /><div className={"h-4 w-1/4 rounded bg-zinc-800"} /><div className={"h-10 w-1/3 rounded bg-zinc-800"} /><div className={"space-y-2"}><div className={"h-4 w-full rounded bg-zinc-800"} /><div className={"h-4 w-5/6 rounded bg-zinc-800"} /><div className={"h-4 w-4/6 rounded bg-zinc-800"} /></div><div className={"h-12 rounded-xl bg-zinc-800"} /></div></div></div>;
}

export function ProductDetailPage(props: { productId: ProductId; onAddToCart: (_p0: Product) => void }): JSX.Element {
  return <div><Link to={"/"} className={"mb-6 inline-block text-sm text-zinc-400 hover:text-zinc-200 transition-colors"}>{"<- Back to catalog"}</Link><QueryErrorResetBoundary>{({ reset }) => <ErrorBoundary onReset={reset} fallbackRender={({ resetErrorBoundary, error }) => <div className={"rounded-lg border border-red-900/50 bg-red-950/30 p-6 text-center"}><p className={"mb-3 text-red-400"}>{`Failed to load product: ${error.message}`}</p><button onClick={resetErrorBoundary} className={"rounded bg-red-600 px-4 py-2 text-sm text-white hover:bg-red-500"}>Retry</button></div>}><Suspense fallback={<DetailSkeleton />}><ProductDetailContent productId={props.productId} onAddToCart={props.onAddToCart} /></Suspense></ErrorBoundary>}</QueryErrorResetBoundary></div>;
}
