// ── Newtype wrappers ───────────────────────────────────
// In pure TS we use branded types for nominal safety.

export type ProductId = number & { readonly __brand: "ProductId" };
export type OrderId = number & { readonly __brand: "OrderId" };

export function ProductId(n: number): ProductId {
  return n as ProductId;
}

export function OrderId(n: number): OrderId {
  return n as OrderId;
}

// ── Product (from DummyJSON API) ───────────────────────

export type Product = {
  id: ProductId;
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
};

export type Review = {
  rating: number;
  comment: string;
  date: string;
  reviewerName: string;
};

// ── Cart ───────────────────────────────────────────────

export type CartItem = {
  product: Product;
  quantity: number;
};

// ── Nested error unions ────────────────────────────────

export type NetworkError =
  | { tag: "Timeout"; ms: number }
  | { tag: "DnsFailure"; host: string }
  | { tag: "ConnectionRefused" };

export type ApiError =
  | { tag: "Network"; error: NetworkError }
  | { tag: "NotFound"; id: ProductId }
  | { tag: "BadResponse"; status: number; body: string }
  | { tag: "ParseError"; message: string };

// ── Sort + Filter ──────────────────────────────────────

export type SortOrder = "PriceLow" | "PriceHigh" | "Rating" | "Name";

export type PriceRange =
  | { tag: "Any" }
  | { tag: "Under"; max: number }
  | { tag: "Between"; min: number; max: number }
  | { tag: "Over"; min: number };

// ── Order ──────────────────────────────────────────────

export type OrderStatus =
  | { tag: "Pending" }
  | { tag: "Confirmed"; orderId: OrderId }
  | { tag: "Shipped"; tracking: string }
  | { tag: "Failed"; reason: string };

// ── Checkout validation ────────────────────────────────

export type CheckoutError =
  | { tag: "EmptyCart" }
  | { tag: "InvalidEmail"; email: string }
  | { tag: "InvalidPhone"; phone: string }
  | { tag: "OutOfStock"; productId: ProductId };

export type ShippingInfo = {
  name: string;
  email: string;
  phone: string;
  address: string;
};

// ── Result type ────────────────────────────────────────

export type Result<T, E> =
  | { ok: true; value: T }
  | { ok: false; error: E };

export function Ok<T, E>(value: T): Result<T, E> {
  return { ok: true, value };
}

export function Err<T, E>(error: E): Result<T, E> {
  return { ok: false, error };
}
