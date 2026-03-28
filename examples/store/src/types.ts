export type ProductId = { tag: "ProductId"; value: number };

export type OrderId = { tag: "OrderId"; value: number };

type WithRating = { rating: number };

export type Product = WithRating & { id: ProductId; title: string; description: string; category: string; price: number; discountPercentage: number; stock: number; tags: Array<string>; brand: string; thumbnail: string; images: Array<string> };

function display(self: Product): string {
  return `Product(id: ${self.id}, title: ${self.title}, description: ${self.description}, category: ${self.category}, price: ${self.price}, discountPercentage: ${self.discountPercentage}, stock: ${self.stock}, tags: ${self.tags}, brand: ${self.brand}, thumbnail: ${self.thumbnail}, images: ${self.images})`;
}

export type Review = WithRating & { comment: string; date: string; reviewerName: string };

export type CartItem = { product: Product; quantity: number };

export type NetworkError = { tag: "Timeout"; ms: number } | { tag: "DnsFailure"; host: string } | { tag: "ConnectionRefused" };

export type ApiError = { tag: "Network"; value: NetworkError } | { tag: "NotFound"; id: ProductId } | { tag: "BadResponse"; status: number; body: string } | { tag: "ParseError"; message: string };

export type SortOrder = { tag: "PriceLow" } | { tag: "PriceHigh" } | { tag: "Rating" } | { tag: "Name" };

export type PriceRange = { tag: "Any" } | { tag: "Under"; max: number } | { tag: "Between"; min: number; max: number } | { tag: "Over"; min: number };

export type OrderStatus = { tag: "Pending" } | { tag: "Confirmed"; orderId: OrderId } | { tag: "Shipped"; tracking: string } | { tag: "Failed"; reason: string };

export type CheckoutError = { tag: "EmptyCart" } | { tag: "InvalidEmail"; email: string } | { tag: "InvalidPhone"; phone: string } | { tag: "OutOfStock"; productId: ProductId };

export type ShippingInfo = { name: string; email: string; phone: string; address: string };




