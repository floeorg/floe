function __floeEq(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a == null || b == null) return false;
  if (typeof a !== "object" || typeof b !== "object") return false;
  const ka = Object.keys(a as object);
  const kb = Object.keys(b as object);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => __floeEq((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
}

import { type Product, type ProductId, type Review, type ApiError } from "./types";

type ProductDetailResponse = { id: number; title: string; description: string; category: string; price: number; discountPercentage: number; rating: number; stock: number; tags: Array<string>; brand: string; thumbnail: string; images: Array<string>; reviews: Array<Review> };

type ProductListResponse = { products: Array<Product>; total: number };

export type CategoryResponse = { slug: string; name: string };

export async function fetchProduct(id: ProductId): { ok: true; value: readonly [Product, Array<Review>] } | { ok: false; error: ApiError } {
  const _r0 = await (async () => { try { const _r = await fetch(`https://dummyjson.com/products/${id}`); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })();
  if (!_r0.ok) return _r0;
  const _r1 = await (async () => { try { const _r = await _r0.value.json(); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })();
  if (!_r1.ok) return _r1;
  const _r2 = (() => { const __v = _r1.value; if (typeof __v !== "object" || __v === null) return { ok: false as const, error: new Error("expected object, got " + typeof __v) }; return { ok: true as const, value: __v as ProductDetailResponse }; })();
  if (!_r2.ok) return _r2;
  const data = _r2.value;
  const product = { id: { tag: "ProductId", value: data.id }, title: data.title, description: data.description, category: data.category, price: data.price, discountPercentage: data.discountPercentage, rating: data.rating, stock: data.stock, tags: data.tags, brand: data.brand, thumbnail: data.thumbnail, images: data.images };
  return { ok: true as const, value: [product, data.reviews] };
}

export async function fetchProducts(category: string = "", search: string = "", limit: number = 20, skip: number = 0): { ok: true; value: readonly [Array<Product>, number] } | { ok: false; error: ApiError } {
  const url = [category, search][0] === "" && [category, search][1] === "" ? `https://dummyjson.com/products?limit=${limit}&skip=${skip}` : [category, search][1] === "" ? (() => { const cat = [category, search][0]; if (!__floeEq(cat, "")) { return `https://dummyjson.com/products/category/${cat}?limit=${limit}&skip=${skip}`; } return true ? (() => { const q = [category, search][1]; return `https://dummyjson.com/products/search?q=${q}&limit=${limit}&skip=${skip}`; })() : (() => { throw new Error("non-exhaustive match"); })(); })() : true ? (() => { const q = [category, search][1]; return `https://dummyjson.com/products/search?q=${q}&limit=${limit}&skip=${skip}`; })() : (() => { throw new Error("non-exhaustive match"); })();
  const _r3 = await (async () => { try { const _r = await fetch(url); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })();
  if (!_r3.ok) return _r3;
  const _r4 = await (async () => { try { const _r = await _r3.value.json(); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })();
  if (!_r4.ok) return _r4;
  const _r5 = (() => { const __v = _r4.value; if (typeof __v !== "object" || __v === null) return { ok: false as const, error: new Error("expected object, got " + typeof __v) }; return { ok: true as const, value: __v as ProductListResponse }; })();
  if (!_r5.ok) return _r5;
  const data = _r5.value;
  return { ok: true as const, value: [data.products, data.total] };
}

export async function fetchCategories(): { ok: true; value: Array<CategoryResponse> } | { ok: false; error: ApiError } {
  const _r6 = await (async () => { try { const _r = await fetch("https://dummyjson.com/products/categories"); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })();
  if (!_r6.ok) return _r6;
  const _r7 = await (async () => { try { const _r = await _r6.value.json(); return { ok: true as const, value: _r }; } catch (_e) { return { ok: false as const, error: _e instanceof Error ? _e : new Error(String(_e)) }; } })();
  if (!_r7.ok) return _r7;
  const _r8 = (() => { const __v = _r7.value; if (!Array.isArray(__v)) return { ok: false as const, error: new Error("expected array, got " + typeof __v) }; for (let __i3 = 0; __i3 < __v.length; __i3++) { if (typeof __v[__i3] !== "object" || __v[__i3] === null) return { ok: false as const, error: new Error("element [" + __i3 + "]: expected object, got " + typeof __v[__i3]) }; } return { ok: true as const, value: __v as Array<CategoryResponse> }; })();
  if (!_r8.ok) return _r8;
  const data = _r8.value;
  return { ok: true as const, value: data };
}

export async function fetchStoreDashboard(category: string = ""): { ok: true; value: readonly [Array<Product>, Array<CategoryResponse>] } | { ok: false; error: Array<ApiError> } {
  return (async () => {
    const __errors: Array<any> = [];
    const _r9 = await fetchProducts(category);
    if (!_r9.ok) return _r9;
    const [products, _total] = _r9.value;
    const _r10 = await fetchCategories();
    if (!_r10.ok) return _r10;
    const categories = _r10.value;
    if (__errors.length > 0) return { ok: false as const, error: __errors };
    return { ok: true as const, value: [products, categories] };
  })();
}


