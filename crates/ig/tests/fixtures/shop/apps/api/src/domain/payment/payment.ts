import type { Order } from '../order/order';

export function total(items: Order[]) {
  return items.reduce((sum, o) => sum + o.amount, 0);
}
