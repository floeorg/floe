import { Suspense } from "react";
import { useSuspenseQuery, QueryErrorResetBoundary } from "@tanstack/react-query";
import { ErrorBoundary } from "react-error-boundary";
import { Link } from "@tanstack/react-router";
import { type Product, type ProductId, type Review } from "../types";
import { formatPrice, display, effectivePrice, savings, inStock, stockLabel, ratingStars } from "../product";
import { fetchProduct } from "../api";
export declare function ProductDetailPage(props: { productId: ProductId; onAddToCart: (_p0: Product) => void }): JSX.Element;
