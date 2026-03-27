import type {
  CartItem,
  ShippingInfo,
  CheckoutError,
  OrderStatus,
  Result,
} from "./types";
import { OrderId, Ok, Err } from "./types";

// ── Validation helpers ─────────────────────────────────

function validateEmail(email: string): Result<string, CheckoutError> {
  const trimmed = email.trim();
  if (trimmed.includes("@")) return Ok(trimmed);
  return Err({ tag: "InvalidEmail", email: trimmed });
}

function validatePhone(phone: string): Result<string, CheckoutError> {
  const trimmed = phone.trim();
  if (trimmed.length >= 7) return Ok(trimmed);
  return Err({ tag: "InvalidPhone", phone: trimmed });
}

function validateStock(
  cart: CartItem[],
): Result<CartItem[], CheckoutError> {
  const outOfStock = cart.filter((item) => item.quantity > item.product.stock);
  if (outOfStock.length === 0) return Ok(cart);
  return Err({ tag: "OutOfStock", productId: outOfStock[0].product.id });
}

// ── Checkout validation ────────────────────────────────
// Collects all validation errors at once.

export function validateCheckout(
  cart: CartItem[],
  shipping: ShippingInfo,
): Result<ShippingInfo, CheckoutError[]> {
  if (cart.length === 0) return Err([{ tag: "EmptyCart" }]);

  const errors: CheckoutError[] = [];

  const emailResult = validateEmail(shipping.email);
  const phoneResult = validatePhone(shipping.phone);
  const stockResult = validateStock(cart);

  if (!emailResult.ok) errors.push(emailResult.error);
  if (!phoneResult.ok) errors.push(phoneResult.error);
  if (!stockResult.ok) errors.push(stockResult.error);

  if (errors.length > 0) return Err(errors);

  return Ok({
    ...shipping,
    email: emailResult.ok ? emailResult.value : shipping.email,
    phone: phoneResult.ok ? phoneResult.value : shipping.phone,
  });
}

// ── Mock checkout ──────────────────────────────────────

export function processCheckout(
  cart: CartItem[],
  shipping: ShippingInfo,
): Result<OrderStatus, CheckoutError[]> {
  const validated = validateCheckout(cart, shipping);
  if (!validated.ok) return Err(validated.error);

  const orderId = OrderId(Math.floor(Math.random() * 10_000));
  return Ok({ tag: "Confirmed", orderId });
}
