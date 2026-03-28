import { useState, Suspense } from "react";
import { useSuspenseQuery, QueryErrorResetBoundary } from "@tanstack/react-query";
import { ErrorBoundary } from "react-error-boundary";
import { Link } from "@tanstack/react-router";
import { type Product, type SortOrder, type PriceRange, type ApiError } from "../types";
import { sortProducts, matchesPrice, formatPrice, display, effectivePrice, savings, inStock, stockLabel, ratingStars, addItem, removeItem, updateQuantity, totals, itemCount, isEmpty } from "../product";
import { display, isRetryable } from "../errors";
import { fetchProducts, fetchCategories, type CategoryResponse } from "../api";
export declare function CatalogPage(props: { onAddToCart: (_p0: Product) => void }): JSX.Element;
