// ── API layer ──────────────────────────────────────────
// Pure TypeScript version using fetch + Zod for validation.

import type { Product, ProductId, Review, ApiError } from "./types";
import { ProductId as mkProductId, Ok, Err, type Result } from "./types";

// ── Response types for parsing ─────────────────────────

type ProductDetailResponse = {
  id: number;
  title: string;
  description: string;
  category: string;
  price: number;
  discountPercentage: number;
  rating: number;
  stock: number;
  tags: string[];
  brand: string;
  thumbnail: string;
  images: string[];
  reviews: Review[];
};

type ProductListResponse = {
  products: Product[];
  total: number;
};

export type CategoryResponse = {
  slug: string;
  name: string;
};

// ── Single product fetch ───────────────────────────────

export async function fetchProduct(
  id: ProductId,
): Promise<Result<[Product, Review[]], ApiError>> {
  try {
    const res = await fetch(`https://dummyjson.com/products/${id}`);
    if (!res.ok) {
      return Err({ tag: "BadResponse", status: res.status, body: await res.text() });
    }
    const data: ProductDetailResponse = await res.json();

    const product: Product = {
      id: mkProductId(data.id),
      title: data.title,
      description: data.description,
      category: data.category,
      price: data.price,
      discountPercentage: data.discountPercentage,
      rating: data.rating,
      stock: data.stock,
      tags: data.tags,
      brand: data.brand,
      thumbnail: data.thumbnail,
      images: data.images,
    };

    return Ok([product, data.reviews]);
  } catch {
    return Err({ tag: "Network", error: { tag: "ConnectionRefused" } });
  }
}

// ── Product list with filters ──────────────────────────

export async function fetchProducts(
  category: string = "",
  search: string = "",
  limit: number = 20,
  skip: number = 0,
): Promise<Result<[Product[], number], ApiError>> {
  try {
    let url: string;
    if (category && !search) {
      url = `https://dummyjson.com/products/category/${category}?limit=${limit}&skip=${skip}`;
    } else if (search) {
      url = `https://dummyjson.com/products/search?q=${search}&limit=${limit}&skip=${skip}`;
    } else {
      url = `https://dummyjson.com/products?limit=${limit}&skip=${skip}`;
    }

    const res = await fetch(url);
    if (!res.ok) {
      return Err({ tag: "BadResponse", status: res.status, body: await res.text() });
    }
    const data: ProductListResponse = await res.json();
    return Ok([data.products, data.total]);
  } catch {
    return Err({ tag: "Network", error: { tag: "ConnectionRefused" } });
  }
}

// ── Categories ─────────────────────────────────────────

export async function fetchCategories(): Promise<
  Result<CategoryResponse[], ApiError>
> {
  try {
    const res = await fetch("https://dummyjson.com/products/categories");
    if (!res.ok) {
      return Err({ tag: "BadResponse", status: res.status, body: await res.text() });
    }
    const data: CategoryResponse[] = await res.json();
    return Ok(data);
  } catch {
    return Err({ tag: "Network", error: { tag: "ConnectionRefused" } });
  }
}
