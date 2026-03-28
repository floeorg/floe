import { useState } from "react";
import { type CartItem, type Product, type ProductId, type OrderStatus, type OrderId } from "../types";
import { formatPrice, addItem, removeItem, updateQuantity, totals, itemCount, isEmpty } from "../product";
export declare function CartPage(props: { cart: Array<CartItem>; onUpdateQty: (_p0: ProductId, _p1: number) => void; onRemove: (_p0: ProductId) => void }): JSX.Element;
