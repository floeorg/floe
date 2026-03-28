function __floeEq(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a == null || b == null) return false;
  if (typeof a !== "object" || typeof b !== "object") return false;
  const ka = Object.keys(a as object);
  const kb = Object.keys(b as object);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => __floeEq((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
}

import { type Product, type ProductId, type CartItem, type SortOrder, type PriceRange, Display, Discountable } from "./types";

export function display(self: Product): string {
  return `${self.title} - $${self.price}`;
}

export function effectivePrice(self: Product): number {
  return self.price * (1 - self.discountPercentage / 100);
}

export function savings(self: Product): number {
  return self.price - (effectivePrice(self));
}
export function inStock(self: Product): boolean {
  return self.stock > 0;
}
export function stockLabel(self: Product): string {
  return self.stock === 0 ? "Out of stock" : true ? (() => { const n = self.stock; if (n < 5) { return `Only ${n} left!`; } return true ? (() => { const n = self.stock; if (n < 20) { return "In stock"; } return "Plenty in stock"; })() : "Plenty in stock"; })() : true ? (() => { const n = self.stock; if (n < 20) { return "In stock"; } return "Plenty in stock"; })() : "Plenty in stock";
}
export function ratingStars(self: Product): string {
  const full = Math.floor(self.rating);
  return "*".repeat(full);
}

export function addItem(self: Array<CartItem>, product: Product, qty: number = 1): Array<CartItem> {
  const existing = (() => { const _i = self.findIndex((item) => __floeEq(item.product.id, product.id)); return _i === -1 ? undefined : _i; })();
  return existing === -1 ? [...self, { product: product, quantity: qty }] : self.map((item) => __floeEq(item.product.id, product.id) === true ? { ...item, quantity: item.quantity + qty } : __floeEq(item.product.id, product.id) === false ? item : (() => { throw new Error("non-exhaustive match"); })());
}
export function removeItem(self: Array<CartItem>, productId: ProductId): Array<CartItem> {
  return self.filter((item) => !__floeEq(item.product.id, productId));
}
export function updateQuantity(self: Array<CartItem>, productId: ProductId, qty: number): Array<CartItem> {
  return qty === 0 ? removeItem(self, productId) : self.map((item) => __floeEq(item.product.id, productId) === true ? { ...item, quantity: qty } : __floeEq(item.product.id, productId) === false ? item : (() => { throw new Error("non-exhaustive match"); })());
}
export function totals(self: Array<CartItem>): readonly [number, number, number] {
  const subtotal = self.reduce((acc: number, item: CartItem) => acc + item.product.price * item.quantity, 0);
  const discounted = self.reduce((acc: number, item: CartItem) => acc + (effectivePrice(item.product)) * item.quantity, 0);
  return [subtotal, subtotal - discounted, discounted];
}
export function itemCount(self: Array<CartItem>): number {
  return self.reduce((acc, item) => acc + item.quantity, 0);
}
export function isEmpty(self: Array<CartItem>): boolean {
  return self.length === 0;
}

function compareBy(order: SortOrder, a: Product, b: Product): number {
  return order.tag === "PriceLow" ? (effectivePrice(a)) - (effectivePrice(b)) : order.tag === "PriceHigh" ? (effectivePrice(b)) - (effectivePrice(a)) : order.tag === "Rating" ? b.rating - a.rating : order.tag === "Name" ? a.title.localeCompare(b.title) : (() => { throw new Error("non-exhaustive match"); })();
}

export function sortProducts(products: Array<Product>, order: SortOrder): Array<Product> {
  return [...products].sort((a, b) => a - b);
}

export function matchesPrice(product: Product, range: PriceRange): boolean {
  const price = effectivePrice(product);
  return __floeEq(range, { tag: "Any" }) ? true : true ? (() => { const p = price; if (__floeEq(range, { tag: "Under", max: 25 })) { return p < 25; } return (() => { const p = price; return range.tag === "Any" ? true : range.tag === "Under" ? (() => { const max = range.max; return p < max; })() : range.tag === "Between" ? (() => { const min = range.min; const max = range.max; return p >= min && p < max; })() : range.tag === "Over" ? (() => { const min = range.min; return p >= min; })() : (() => { throw new Error("non-exhaustive match"); })(); })(); })() : (() => { const p = price; return range.tag === "Any" ? true : range.tag === "Under" ? (() => { const max = range.max; return p < max; })() : range.tag === "Between" ? (() => { const min = range.min; const max = range.max; return p >= min && p < max; })() : range.tag === "Over" ? (() => { const min = range.min; return p >= min; })() : (() => { throw new Error("non-exhaustive match"); })(); })();
}

export function formatPrice(amount: number): string {
  return `$${amount.toFixed(2)}`;
}




