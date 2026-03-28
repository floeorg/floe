import { type Product, type ProductId, type Review, type ApiError } from "./types";
type ProductDetailResponse = { id: number; title: string; description: string; category: string; price: number; discountPercentage: number; rating: number; stock: number; tags: Array<string>; brand: string; thumbnail: string; images: Array<string>; reviews: Array<Review> };
type ProductListResponse = { products: Array<Product>; total: number };
export type CategoryResponse = { slug: string; name: string };
export declare async function fetchProduct(id: ProductId): { ok: true; value: readonly [Product, Array<Review>] } | { ok: false; error: ApiError };
export declare async function fetchProducts(category: string = "", search: string = "", limit: number = 20, skip: number = 0): { ok: true; value: readonly [Array<Product>, number] } | { ok: false; error: ApiError };
export declare async function fetchCategories(): { ok: true; value: Array<CategoryResponse> } | { ok: false; error: ApiError };
export declare async function fetchStoreDashboard(category: string = ""): { ok: true; value: readonly [Array<Product>, Array<CategoryResponse>] } | { ok: false; error: Array<ApiError> };
