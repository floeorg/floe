import { useState } from "react";
import type { CartItem, OrderStatus } from "../types";
import { OrderId } from "../types";
import { totals, itemCount as getItemCount, formatPrice } from "../product";

// ── Cart item row ──────────────────────────────────────

function CartItemRow(props: {
  item: CartItem;
  onUpdateQty: (productId: number, qty: number) => void;
  onRemove: (productId: number) => void;
}) {
  const item = props.item;
  const product = item.product;
  const lineTotal =
    product.price * (1 - product.discountPercentage / 100) * item.quantity;

  return (
    <div className="flex items-center gap-4 rounded-lg border border-zinc-800 bg-zinc-900/50 p-4">
      <img
        src={product.thumbnail}
        alt={product.title}
        className="h-20 w-20 rounded-lg object-cover"
      />
      <div className="flex-1">
        <h3 className="font-medium text-zinc-100">{product.title}</h3>
        <p className="text-sm text-zinc-500">{product.category}</p>
        <p className="text-sm text-indigo-400">
          {formatPrice(product.price * (1 - product.discountPercentage / 100))}
        </p>
      </div>
      <div className="flex items-center gap-2">
        <button
          onClick={() => props.onUpdateQty(product.id, item.quantity - 1)}
          className="flex h-8 w-8 items-center justify-center rounded bg-zinc-800 text-zinc-300 hover:bg-zinc-700"
        >
          {"-"}
        </button>
        <span className="w-8 text-center text-zinc-100">{item.quantity}</span>
        <button
          onClick={() => props.onUpdateQty(product.id, item.quantity + 1)}
          className="flex h-8 w-8 items-center justify-center rounded bg-zinc-800 text-zinc-300 hover:bg-zinc-700"
        >
          {"+"}
        </button>
      </div>
      <span className="w-24 text-right font-semibold text-zinc-100">
        {formatPrice(lineTotal)}
      </span>
      <button
        onClick={() => props.onRemove(product.id)}
        className="text-zinc-600 hover:text-red-400 transition-colors"
      >
        {"x"}
      </button>
    </div>
  );
}

// ── Order summary ───────────────────────────────────────

function OrderSummary(props: {
  cart: CartItem[];
  orderStatus: OrderStatus;
  onCheckout: () => void;
}) {
  const [subtotal, discount, total] = totals(props.cart);

  function statusLabel(status: OrderStatus): string {
    switch (status.tag) {
      case "Pending":
        return "Checkout";
      case "Confirmed":
        return `Order #${status.orderId} confirmed!`;
      case "Shipped":
        return `Shipped: ${status.tracking}`;
      case "Failed":
        return `Failed: ${status.reason}`;
    }
  }

  return (
    <div className="rounded-xl border border-zinc-800 bg-zinc-900/50 p-6">
      <h3 className="mb-4 text-lg font-semibold">Order Summary</h3>

      <div className="space-y-2 text-sm">
        <div className="flex justify-between">
          <span className="text-zinc-400">Subtotal</span>
          <span className="text-zinc-200">{formatPrice(subtotal)}</span>
        </div>
        {discount > 0 && (
          <div className="flex justify-between">
            <span className="text-emerald-400">Discount</span>
            <span className="text-emerald-400">{`-${formatPrice(discount)}`}</span>
          </div>
        )}
        <div className="my-3 border-t border-zinc-800" />
        <div className="flex justify-between text-base font-semibold">
          <span className="text-zinc-200">Total</span>
          <span className="text-indigo-400">{formatPrice(total)}</span>
        </div>
      </div>

      <button
        onClick={props.onCheckout}
        disabled={props.orderStatus.tag !== "Pending"}
        className={
          props.orderStatus.tag === "Pending"
            ? "mt-6 w-full rounded-xl bg-indigo-600 py-3 font-semibold text-white transition-colors hover:bg-indigo-500"
            : "mt-6 w-full rounded-xl bg-zinc-800 py-3 font-semibold text-zinc-600 cursor-not-allowed"
        }
      >
        {statusLabel(props.orderStatus)}
      </button>
    </div>
  );
}

// ── Cart page ──────────────────────────────────────────

export function CartPage(props: {
  cart: CartItem[];
  onUpdateQty: (productId: number, qty: number) => void;
  onRemove: (productId: number) => void;
}) {
  const [orderStatus, setOrderStatus] = useState<OrderStatus>({
    tag: "Pending",
  });

  function handleCheckout() {
    const orderId = OrderId(Math.floor(Math.random() * 10_000));
    setOrderStatus({ tag: "Confirmed", orderId });
  }

  const count = getItemCount(props.cart);

  return (
    <div>
      <h1 className="mb-6 text-3xl font-bold">
        {`Cart (${count} item${count === 1 ? "" : "s"})`}
      </h1>

      {props.cart.length === 0 ? (
        <div className="rounded-xl border border-zinc-800 bg-zinc-900/50 p-12 text-center">
          <p className="text-lg text-zinc-500">Your cart is empty</p>
          <p className="mt-2 text-sm text-zinc-600">
            Browse the store to add some products!
          </p>
        </div>
      ) : (
        <div className="flex gap-8">
          <div className="flex-1 space-y-3">
            {props.cart.map((item) => (
              <CartItemRow
                key={item.product.id}
                item={item}
                onUpdateQty={props.onUpdateQty}
                onRemove={props.onRemove}
              />
            ))}
          </div>
          <div className="w-80 shrink-0">
            <OrderSummary
              cart={props.cart}
              orderStatus={orderStatus}
              onCheckout={handleCheckout}
            />
          </div>
        </div>
      )}
    </div>
  );
}
