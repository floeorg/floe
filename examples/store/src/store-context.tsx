import { createContext, useContext, useState, type ReactNode } from "react";
import type { Product, ProductId, CartItem } from "./types";

type StoreContextType = {
  cart: CartItem[];
  addToCart: (product: Product) => void;
  updateQty: (productId: ProductId, qty: number) => void;
  removeFromCart: (productId: ProductId) => void;
  itemCount: number;
};

const StoreContext = createContext<StoreContextType | null>(null);

export function useStore(): StoreContextType {
  const ctx = useContext(StoreContext);
  if (!ctx) throw new Error("useStore must be inside StoreProvider");
  return ctx;
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [cart, setCart] = useState<CartItem[]>([]);

  function addToCart(product: Product) {
    setCart((prev) => {
      const idx = prev.findIndex(
        (item) => item.product.id.value === product.id.value,
      );
      if (idx === -1) return [...prev, { product, quantity: 1 }];
      return prev.map((item, i) =>
        i === idx ? { ...item, quantity: item.quantity + 1 } : item,
      );
    });
  }

  function updateQty(productId: ProductId, qty: number) {
    setCart((prev) =>
      qty <= 0
        ? prev.filter((item) => item.product.id.value !== productId.value)
        : prev.map((item) =>
            item.product.id.value === productId.value
              ? { ...item, quantity: qty }
              : item,
          ),
    );
  }

  function removeFromCart(productId: ProductId) {
    setCart((prev) =>
      prev.filter((item) => item.product.id.value !== productId.value),
    );
  }

  const itemCount = cart.reduce((acc, item) => acc + item.quantity, 0);

  return (
    <StoreContext.Provider
      value={{ cart, addToCart, updateQty, removeFromCart, itemCount }}
    >
      {children}
    </StoreContext.Provider>
  );
}
