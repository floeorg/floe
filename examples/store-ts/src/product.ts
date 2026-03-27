import type { Product, CartItem, SortOrder, PriceRange } from "./types";

// ── Display ──────────────────────────────────────────────

export function display(product: Product): string {
  return `${product.title} - $${product.price}`;
}

// ── Product helpers ────────────────────────────────────

export function effectivePrice(product: Product): number {
  return product.price * (1 - product.discountPercentage / 100);
}

export function savings(product: Product): number {
  return product.price - effectivePrice(product);
}

export function inStock(product: Product): boolean {
  return product.stock > 0;
}

export function stockLabel(product: Product): string {
  if (product.stock === 0) return "Out of stock";
  if (product.stock < 5) return `Only ${product.stock} left!`;
  if (product.stock < 20) return "In stock";
  return "Plenty in stock";
}

export function ratingStars(product: Product): string {
  const full = Math.floor(product.rating);
  return "*".repeat(full);
}

// ── Cart operations ────────────────────────────────────

export function addItem(
  cart: CartItem[],
  product: Product,
  qty: number = 1,
): CartItem[] {
  const idx = cart.findIndex((item) => item.product.id === product.id);
  if (idx === -1) return [...cart, { product, quantity: qty }];
  return cart.map((item) =>
    item.product.id === product.id
      ? { ...item, quantity: item.quantity + qty }
      : item,
  );
}

export function removeItem(cart: CartItem[], productId: number): CartItem[] {
  return cart.filter((item) => item.product.id !== productId);
}

export function updateQuantity(
  cart: CartItem[],
  productId: number,
  qty: number,
): CartItem[] {
  if (qty === 0) return removeItem(cart, productId);
  return cart.map((item) =>
    item.product.id === productId ? { ...item, quantity: qty } : item,
  );
}

// Returns [subtotal, discount, total]
export function totals(cart: CartItem[]): [number, number, number] {
  const subtotal = cart.reduce(
    (acc, item) => acc + item.product.price * item.quantity,
    0,
  );
  const discounted = cart.reduce(
    (acc, item) => acc + effectivePrice(item.product) * item.quantity,
    0,
  );
  return [subtotal, subtotal - discounted, discounted];
}

export function itemCount(cart: CartItem[]): number {
  return cart.reduce((acc, item) => acc + item.quantity, 0);
}

export function isEmpty(cart: CartItem[]): boolean {
  return cart.length === 0;
}

// ── Sorting ─────────────────────────────────────────────

function compareBy(order: SortOrder, a: Product, b: Product): number {
  switch (order) {
    case "PriceLow":
      return effectivePrice(a) - effectivePrice(b);
    case "PriceHigh":
      return effectivePrice(b) - effectivePrice(a);
    case "Rating":
      return b.rating - a.rating;
    case "Name":
      return a.title.localeCompare(b.title);
  }
}

export function sortProducts(
  products: Product[],
  order: SortOrder,
): Product[] {
  return [...products].sort((a, b) => compareBy(order, a, b));
}

// ── Price filtering ─────────────────────────────────────

export function matchesPrice(product: Product, range: PriceRange): boolean {
  const price = effectivePrice(product);
  switch (range.tag) {
    case "Any":
      return true;
    case "Under":
      return price < range.max;
    case "Between":
      return price >= range.min && price < range.max;
    case "Over":
      return price >= range.min;
  }
}

// ── Formatting ─────────────────────────────────────────

export function formatPrice(amount: number): string {
  return `$${amount.toFixed(2)}`;
}
