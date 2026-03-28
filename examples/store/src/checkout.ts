import { type CartItem, type ShippingInfo, type CheckoutError, type OrderId, type OrderStatus } from "./types";

import { sortProducts as __sortProducts, matchesPrice as __matchesPrice, formatPrice as __formatPrice, display as __display, effectivePrice as __effectivePrice, savings as __savings, inStock as __inStock, stockLabel as __stockLabel, ratingStars as __ratingStars, addItem as __addItem, removeItem as __removeItem, updateQuantity as __updateQuantity, totals as __totals, itemCount as __itemCount, isEmpty as __isEmpty } from "./product";

function validateEmail(email: string): { ok: true; value: string } | { ok: false; error: CheckoutError } {
  const trimmed = email.trim();
  return trimmed.includes("@") === true ? { ok: true as const, value: trimmed } : trimmed.includes("@") === false ? { ok: false as const, error: { tag: "InvalidEmail", email: trimmed } } : (() => { throw new Error("non-exhaustive match"); })();
}

function validatePhone(phone: string): { ok: true; value: string } | { ok: false; error: CheckoutError } {
  const trimmed = phone.trim();
  return true ? (() => { const n = trimmed.length; if (n >= 7) { return { ok: true as const, value: trimmed }; } return { ok: false as const, error: { tag: "InvalidPhone", phone: trimmed } }; })() : { ok: false as const, error: { tag: "InvalidPhone", phone: trimmed } };
}

function validateStock(cart: Array<CartItem>): { ok: true; value: Array<CartItem> } | { ok: false; error: CheckoutError } {
  const outOfStock = cart.filter((item) => item.quantity > item.product.stock);
  return outOfStock.length === 0 ? { ok: true as const, value: cart } : outOfStock.length >= 1 ? (() => { const first = outOfStock[0]; return { ok: false as const, error: { tag: "OutOfStock", productId: first.product.id } }; })() : (() => { throw new Error("non-exhaustive match"); })();
}

export function validateCheckout(cart: Array<CartItem>, shipping: ShippingInfo): { ok: true; value: ShippingInfo } | { ok: false; error: Array<CheckoutError> } {
  return cart.length === 0 === true ? { ok: false as const, error: [{ tag: "EmptyCart" }] } : cart.length === 0 === false ? (() => {
    const __errors: Array<any> = [];
    const _r0 = validateEmail(shipping.email);
    if (!_r0.ok) __errors.push(_r0.error);
    const email = _r0.ok ? _r0.value : undefined as any;
    const _r1 = validatePhone(shipping.phone);
    if (!_r1.ok) __errors.push(_r1.error);
    const phone = _r1.ok ? _r1.value : undefined as any;
    const _r2 = validateStock(cart);
    if (!_r2.ok) __errors.push(_r2.error);
    const _stock = _r2.ok ? _r2.value : undefined as any;
    if (__errors.length > 0) return { ok: false as const, error: __errors };
    return { ok: true as const, value: { ...shipping, email: email, phone: phone } };
  })() : (() => { throw new Error("non-exhaustive match"); })();
}

export function processCheckout(cart: Array<CartItem>, shipping: ShippingInfo): { ok: true; value: OrderStatus } | { ok: false; error: Array<CheckoutError> } {
  const _r0 = validateCheckout(cart, shipping);
  if (!_r0.ok) return _r0;
  const _validated = _r0.value;
  const orderId = { tag: "OrderId", value: Math.floor(Math.random() * 10_000) };
  return { ok: true as const, value: { tag: "Confirmed", orderId: orderId } };
}




