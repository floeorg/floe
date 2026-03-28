import { type CartItem, type ShippingInfo, type CheckoutError, type OrderId, type OrderStatus } from "./types";
import { sortProducts, matchesPrice, formatPrice, display, effectivePrice, savings, inStock, stockLabel, ratingStars, addItem, removeItem, updateQuantity, totals, itemCount, isEmpty } from "./product";
export declare function validateCheckout(cart: Array<CartItem>, shipping: ShippingInfo): { ok: true; value: ShippingInfo } | { ok: false; error: Array<CheckoutError> };
export declare function processCheckout(cart: Array<CartItem>, shipping: ShippingInfo): { ok: true; value: OrderStatus } | { ok: false; error: Array<CheckoutError> };
